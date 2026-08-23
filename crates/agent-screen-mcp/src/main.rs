// Agent Screen's local stdio MCP server, also available as `jjaeng-mcp`.
//
// The MCP process intentionally has no GTK or OCR dependency. It talks to
// Hyprland and Wayland capture utilities directly for typed observation and
// capture tools, and uses the existing daemon socket for UI actions.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ErrorData, GetPromptRequestParams, GetPromptResponse,
        GetPromptResult, Implementation, ListPromptsResult, ListResourcesResult,
        PaginatedRequestParams, Prompt, PromptMessage, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, Role,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, Json, RoleServer, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::sleep;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const RECORDING_STARTUP_GRACE: Duration = Duration::from_millis(350);
const DEFAULT_MAX_RECORDING_SECONDS: u64 = 3_600;
const MAX_INLINE_IMAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MonitorParams {
    /// Monitor name. Omit to use the focused monitor.
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegionParams {
    /// Wayland geometry in `x,y widthxheight` form. Omit to show slurp.
    pub geometry: Option<String>,
    /// When true, show an interactive region picker if geometry is omitted.
    #[serde(default)]
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WindowParams {
    /// Hyprland window address, preferred when known.
    pub address: Option<String>,
    /// Exact or partial window title match.
    pub title: Option<String>,
    /// Exact or partial window class match.
    pub class: Option<String>,
    /// Use the visible window picker when no stable selector is supplied.
    #[serde(default)]
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HistoryParams {
    /// Maximum number of entries to return.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactParams {
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartRecordingParams {
    /// `monitor`, `region`, `window`, `workspace`, or `all_outputs`.
    pub target: String,
    pub monitor: Option<String>,
    pub geometry: Option<String>,
    pub address: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    /// `off`, `desktop`, `microphone`, or `both`.
    #[serde(default = "default_audio_mode")]
    pub audio: String,
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
    pub max_duration_seconds: Option<u64>,
    /// GSR encoding size such as `1920x1080`.
    pub encode_resolution: Option<String>,
    pub fps: Option<u32>,
}

fn default_audio_mode() -> String {
    "off".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub compositor: String,
    pub monitors: bool,
    pub workspaces: bool,
    pub windows: bool,
    pub screenshots: bool,
    pub interactive_selection: bool,
    pub gpu_screen_recorder: bool,
    pub wl_screenrec: bool,
    pub audio_sources: bool,
    pub microphone_recording: bool,
    pub ui_daemon_socket: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactInfo {
    pub artifact_id: String,
    pub kind: String,
    pub media_path: String,
    pub thumbnail_path: Option<String>,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub file_size_bytes: Option<u64>,
    pub created_at: u64,
    pub target: Option<String>,
    pub audio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Monitor {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    width: i32,
    #[serde(default)]
    height: i32,
    #[serde(default, rename = "activeWorkspace")]
    active_workspace: Option<WorkspaceRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorkspaceRef {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workspace {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default)]
    windows: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Window {
    #[serde(default)]
    address: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    class: String,
    #[serde(default)]
    mapped: bool,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    at: [i32; 2],
    #[serde(default)]
    size: [i32; 2],
    #[serde(default)]
    workspace: WorkspaceRef,
    #[serde(default)]
    monitor: i32,
    #[serde(default)]
    pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SelectedWindow {
    pub address: String,
    pub title: String,
    pub class: String,
    pub workspace_id: i32,
    pub geometry: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryManifest {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    #[serde(default)]
    kind: String,
    #[serde(alias = "capture_id", alias = "recording_id")]
    entry_id: String,
    #[serde(alias = "image_path")]
    media_path: PathBuf,
    thumbnail_path: PathBuf,
    width: u32,
    height: u32,
    created_at: u64,
    #[serde(default)]
    saved_path: Option<PathBuf>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    file_size_bytes: Option<u64>,
}

#[derive(Debug)]
struct RecordingSession {
    child: Child,
    artifact_id: String,
    output_path: PathBuf,
    started_at: u64,
    width: u32,
    height: u32,
    target: String,
    audio: String,
}

#[derive(Debug, Default)]
struct Runtime {
    recording: Option<RecordingSession>,
    last_recording: Option<ArtifactInfo>,
}

#[derive(Debug, Clone)]
pub struct AgentScreenServer {
    tool_router: ToolRouter<Self>,
    runtime: Arc<Mutex<Runtime>>,
    server_name: &'static str,
}

impl Default for AgentScreenServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentScreenServer {
    pub fn new() -> Self {
        Self::with_server_name("agent-screen")
    }

    pub fn with_server_name(server_name: &'static str) -> Self {
        Self {
            tool_router: Self::tool_router(),
            runtime: Arc::new(Mutex::new(Runtime::default())),
            server_name,
        }
    }

    fn resource_scheme(&self) -> &'static str {
        if self.server_name == "agent-screen" {
            "agent-screen"
        } else {
            "jjaeng"
        }
    }
}

#[tool_router]
impl AgentScreenServer {
    #[tool(
        name = "status",
        description = "Return Agent Screen daemon and MCP recording status"
    )]
    pub async fn status(&self, _params: Parameters<EmptyParams>) -> Result<CallToolResult, String> {
        let daemon = read_daemon_status().unwrap_or_else(|_| json!({"state": "offline"}));
        let recording = recording_status_value(&self.runtime)?;
        Ok(CallToolResult::structured(
            json!({"daemon": daemon, "mcp": {"recording": recording}}),
        ))
    }

    #[tool(
        name = "capabilities",
        description = "List available Hyprland, Wayland, recording, and UI capabilities"
    )]
    pub async fn capabilities(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<Json<Capabilities>, String> {
        Ok(Json(current_capabilities()))
    }

    #[tool(
        name = "list_monitors",
        description = "List Hyprland monitors with geometry and focused workspace"
    )]
    pub async fn list_monitors(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::structured(json!(hyprland_monitors()?)))
    }

    #[tool(name = "list_workspaces", description = "List Hyprland workspaces")]
    pub async fn list_workspaces(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::structured(json!(hyprland_workspaces()?)))
    }

    #[tool(
        name = "list_windows",
        description = "List visible Hyprland windows, including address, class, workspace, and geometry"
    )]
    pub async fn list_windows(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::structured(json!(hyprland_windows()?)))
    }

    #[tool(
        name = "active_window",
        description = "Return the currently active Hyprland window"
    )]
    pub async fn active_window(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        let output = command_output("hyprctl", &["activewindow", "-j"])?;
        serde_json::from_slice(&output)
            .map(CallToolResult::structured)
            .map_err(|err| format!("invalid active window JSON: {err}"))
    }

    #[tool(
        name = "select_window",
        description = "Open an interactive Wayland window picker and return the selected window"
    )]
    pub async fn select_window(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<Json<SelectedWindow>, String> {
        select_window().map(Json)
    }

    #[tool(
        name = "screenshot_fullscreen",
        description = "Capture the focused monitor or a named monitor as an MCP image"
    )]
    pub async fn screenshot_fullscreen(
        &self,
        params: Parameters<MonitorParams>,
    ) -> Result<CallToolResult, String> {
        let monitor = params.0.monitor;
        let artifact = capture_monitor(monitor.as_deref())?;
        image_result(&artifact)
    }

    #[tool(
        name = "screenshot_all_outputs",
        description = "Capture all visible Wayland outputs as one MCP image"
    )]
    pub async fn screenshot_all_outputs(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        let path = new_image_path("screenshot")?;
        run_capture_to_file("grim", &[], &path)?;
        let info = persist_screenshot(path, "all_outputs")?;
        image_result(&ArtifactFile {
            path: PathBuf::from(&info.media_path),
            info,
        })
    }

    #[tool(
        name = "screenshot_workspace",
        description = "Capture the currently visible workspace on a monitor; hidden workspaces are not switched implicitly"
    )]
    pub async fn screenshot_workspace(
        &self,
        params: Parameters<MonitorParams>,
    ) -> Result<CallToolResult, String> {
        let monitor = params.0.monitor;
        let artifact = capture_monitor(monitor.as_deref())?;
        image_result(&artifact)
    }

    #[tool(
        name = "screenshot_region",
        description = "Capture a supplied Wayland geometry or use an interactive region picker"
    )]
    pub async fn screenshot_region(
        &self,
        params: Parameters<RegionParams>,
    ) -> Result<CallToolResult, String> {
        let geometry = match params.0.geometry {
            Some(geometry) => geometry,
            None => select_region()?,
        };
        let artifact = capture_geometry(&geometry, "region")?;
        image_result(&artifact)
    }

    #[tool(
        name = "screenshot_window",
        description = "Capture a selected Hyprland window by address/title/class or interactively"
    )]
    pub async fn screenshot_window(
        &self,
        params: Parameters<WindowParams>,
    ) -> Result<CallToolResult, String> {
        let window = resolve_window(&params.0)?;
        let artifact = capture_geometry(&window.geometry, "window")?;
        image_result_with_metadata(&artifact, Some(json!({"window": window})))
    }

    #[tool(
        name = "history",
        description = "List Agent Screen screenshot and recording history"
    )]
    pub async fn history(
        &self,
        params: Parameters<HistoryParams>,
    ) -> Result<CallToolResult, String> {
        let mut entries = load_history()?.entries;
        let limit = params.0.limit.unwrap_or(48).clamp(1, 200);
        entries.truncate(limit);
        Ok(CallToolResult::structured(json!(entries
            .into_iter()
            .map(history_info)
            .collect::<Vec<_>>())))
    }

    #[tool(
        name = "get_artifact",
        description = "Return artifact metadata and inline a screenshot when it is an image"
    )]
    pub async fn get_artifact(
        &self,
        params: Parameters<ArtifactParams>,
    ) -> Result<CallToolResult, String> {
        let entry = find_history_entry(&params.0.artifact_id)?;
        if entry.kind.eq_ignore_ascii_case("screenshot")
            || entry.media_path.extension().and_then(|x| x.to_str()) == Some("png")
        {
            let artifact = ArtifactFile {
                info: history_info(entry.clone()),
                path: entry.media_path,
            };
            image_result(&artifact)
        } else {
            Ok(CallToolResult::structured(json!(history_info(entry))))
        }
    }

    #[tool(
        name = "list_audio_sources",
        description = "List PipeWire/PulseAudio sources and defaults"
    )]
    pub async fn list_audio_sources(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        let sources = command_output("pactl", &["list", "short", "sources"])
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let default_source = command_output("pactl", &["get-default-source"])
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string());
        let default_sink = command_output("pactl", &["get-default-sink"])
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string());
        Ok(CallToolResult::structured(
            json!({"sources": sources, "default_source": default_source, "default_sink": default_sink}),
        ))
    }

    #[tool(
        name = "start_recording",
        description = "Start a bounded monitor, region, window, workspace, or all-output recording"
    )]
    pub async fn start_recording(
        &self,
        params: Parameters<StartRecordingParams>,
    ) -> Result<CallToolResult, String> {
        let request = params.0;
        let max_duration = request
            .max_duration_seconds
            .unwrap_or(DEFAULT_MAX_RECORDING_SECONDS)
            .clamp(1, DEFAULT_MAX_RECORDING_SECONDS);
        {
            let runtime = self
                .runtime
                .lock()
                .map_err(|_| "recording state lock poisoned".to_string())?;
            if runtime.recording.is_some() {
                return Err("a recording is already active".to_string());
            }
        }
        let session = start_recording_session(&request)?;
        let result = json!({
            "recording_id": session.artifact_id.clone(),
            "path": session.output_path.display().to_string(),
            "target": session.target.clone(),
            "audio": session.audio.clone(),
            "started_at": session.started_at,
            "max_duration_seconds": max_duration,
            "approval_note": "Recording is active; microphone capture should be confirmed by the MCP client."
        });
        {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| "recording state lock poisoned".to_string())?;
            runtime.recording = Some(session);
        }
        let runtime = Arc::clone(&self.runtime);
        tokio::spawn(async move {
            sleep(Duration::from_secs(max_duration)).await;
            let _ = stop_recording_runtime(&runtime);
        });
        Ok(CallToolResult::structured(result))
    }

    #[tool(
        name = "recording_status",
        description = "Return active recording state and elapsed duration"
    )]
    pub async fn recording_status(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        recording_status_value(&self.runtime).map(CallToolResult::structured)
    }

    #[tool(
        name = "stop_recording",
        description = "Stop the active recording, finalize its thumbnail, and persist it to history"
    )]
    pub async fn stop_recording(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<Json<ArtifactInfo>, String> {
        stop_recording_runtime(&self.runtime).map(Json)
    }

    #[tool(
        name = "pause_recording",
        description = "Pause the active recording process"
    )]
    pub async fn pause_recording(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        signal_recording(&self.runtime, "-STOP")?;
        Ok(CallToolResult::structured(
            json!({"ok": true, "state": "paused"}),
        ))
    }

    #[tool(
        name = "resume_recording",
        description = "Resume the active recording process"
    )]
    pub async fn resume_recording(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        signal_recording(&self.runtime, "-CONT")?;
        Ok(CallToolResult::structured(
            json!({"ok": true, "state": "recording"}),
        ))
    }

    #[tool(
        name = "open_history",
        description = "Open the existing Agent Screen history window"
    )]
    pub async fn open_history(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("OpenHistory").map(CallToolResult::structured)
    }

    #[tool(
        name = "open_preview",
        description = "Open the latest Agent Screen preview"
    )]
    pub async fn open_preview(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("OpenPreview").map(CallToolResult::structured)
    }

    #[tool(
        name = "open_editor",
        description = "Open the latest Agent Screen capture in the editor"
    )]
    pub async fn open_editor(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("OpenEditor").map(CallToolResult::structured)
    }

    #[tool(
        name = "copy_latest",
        description = "Copy the latest Agent Screen capture to the clipboard"
    )]
    pub async fn copy_latest(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("CopyLatest").map(CallToolResult::structured)
    }

    #[tool(
        name = "save_latest",
        description = "Save the latest Agent Screen capture"
    )]
    pub async fn save_latest(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("SaveLatest").map(CallToolResult::structured)
    }

    #[tool(
        name = "dismiss_latest",
        description = "Dismiss the latest Agent Screen preview"
    )]
    pub async fn dismiss_latest(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, String> {
        daemon_action("DismissLatest").map(CallToolResult::structured)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AgentScreenServer {
    fn get_info(&self) -> ServerInfo {
        let product_name = "Agent Screen";
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
            .with_server_info(Implementation::new(self.server_name, env!("CARGO_PKG_VERSION")))
            .with_instructions(format!("{product_name} is a local Hyprland screen capture and recording server. Read-only observation is safe by default; recording and UI actions require client approval."))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(resource_definitions(
            self.resource_scheme(),
        )))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        read_resource_contents(&request.uri, self.resource_scheme())
            .map(|contents| ReadResourceResult::new(contents).into())
            .map_err(|message| ErrorData::invalid_params(message, None))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult::with_all_items(prompt_definitions()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        let text = match request.name.as_str() {
            "inspect_screen" => "Inspect the current Hyprland monitor, workspace, and active window. Capture only the requested visible target and describe any Wayland visibility limitations.",
            "capture_evidence" => "Capture reproducible visual evidence. Preserve artifact IDs, target geometry, timestamp, and media metadata.",
            "record_demo" => "Record a bounded desktop demonstration. Confirm the target, duration, and audio mode before starting; microphone audio requires explicit approval.",
            "compare_captures" => "Capture a before and after view using the same target and compare artifact metadata and visible changes.",
            "research" => "Research online with the host web tools while keeping local Agent Screen screenshots and recordings private. Preserve URLs and dates.",
            _ => return Err(ErrorData::invalid_params("unknown Agent Screen prompt", None)),
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)]).into())
    }
}

fn resource_definitions(scheme: &str) -> Vec<Resource> {
    let mut resources = [
        ("status", "status", "application/json"),
        ("capabilities", "capabilities", "application/json"),
        ("monitors", "monitors", "application/json"),
        ("workspaces", "workspaces", "application/json"),
        ("windows", "windows", "application/json"),
        ("history", "history", "application/json"),
    ]
    .into_iter()
    .map(|(path, name, mime)| {
        Resource::new(format!("{scheme}://{path}"), name).with_mime_type(mime)
    })
    .collect::<Vec<_>>();
    resources.push(
        Resource::new(format!("{scheme}://artifact/{{artifact_id}}"), "artifact")
            .with_description("An Agent Screen screenshot or recording artifact")
            .with_mime_type("application/octet-stream"),
    );
    resources
}

fn prompt_definitions() -> Vec<Prompt> {
    [
        (
            "inspect_screen",
            "Inspect the current visible screen safely",
        ),
        ("capture_evidence", "Capture reproducible visual evidence"),
        ("record_demo", "Record a bounded desktop demonstration"),
        ("compare_captures", "Compare before and after captures"),
        (
            "research",
            "Research online while keeping local media private",
        ),
    ]
    .into_iter()
    .map(|(name, description)| Prompt::new(name, Some(description), None))
    .collect()
}

fn read_resource_contents(uri: &str, scheme: &str) -> Result<Vec<ResourceContents>, String> {
    let (uri_scheme, resource_path) = uri
        .split_once("://")
        .ok_or_else(|| format!("invalid resource URI: {uri}"))?;
    if uri_scheme != "jjaeng" && uri_scheme != "chalkak" && uri_scheme != "agent-screen" {
        return Err(format!("unknown resource scheme: {uri_scheme}"));
    }
    let (resource_path, artifact_id) = resource_path
        .strip_prefix("artifact/")
        .map_or((resource_path, None), |id| {
            ("artifact/{artifact_id}", Some(id))
        });
    let canonical_uri = format!("{scheme}://{resource_path}");
    let (text, mime) = match resource_path {
        "status" => (read_daemon_status()?.to_string(), "application/json"),
        "capabilities" => (
            serde_json::to_string(&current_capabilities()).map_err(|err| err.to_string())?,
            "application/json",
        ),
        "monitors" => (
            serde_json::to_string(&hyprland_monitors()?).map_err(|err| err.to_string())?,
            "application/json",
        ),
        "workspaces" => (
            serde_json::to_string(&hyprland_workspaces()?).map_err(|err| err.to_string())?,
            "application/json",
        ),
        "windows" => (
            serde_json::to_string(&hyprland_windows()?).map_err(|err| err.to_string())?,
            "application/json",
        ),
        "history" => (
            serde_json::to_string(
                &load_history()?
                    .entries
                    .into_iter()
                    .map(history_info)
                    .collect::<Vec<_>>(),
            )
            .map_err(|err| err.to_string())?,
            "application/json",
        ),
        "artifact/{artifact_id}" => {
            let id = artifact_id.ok_or_else(|| "artifact URI is missing an ID".to_string())?;
            let entry = find_history_entry(id)?;
            let path = entry.media_path.clone();
            let bytes = fs::read(&path).map_err(|err| format!("failed to read artifact: {err}"))?;
            if bytes.len() > MAX_INLINE_IMAGE_BYTES {
                return Ok(vec![ResourceContents::text(
                    serde_json::to_string(&history_info(entry)).map_err(|err| err.to_string())?,
                    format!("{scheme}://artifact/{id}"),
                )]);
            }
            let mime = history_info(entry).mime_type;
            return Ok(vec![ResourceContents::blob(
                BASE64.encode(bytes),
                format!("{scheme}://artifact/{id}"),
            )
            .with_mime_type(mime)]);
        }
        _ => return Err(format!("unknown screen resource: {uri}")),
    };
    Ok(vec![
        ResourceContents::text(text, canonical_uri).with_mime_type(mime)
    ])
}

fn binary_name() -> &'static str {
    let is_canonical = std::env::args_os()
        .next()
        .map(|path| {
            Path::new(&path).file_stem().and_then(|name| name.to_str()) == Some("agent-screen-mcp")
        })
        .unwrap_or(true);
    if is_canonical {
        "agent-screen-mcp"
    } else {
        "jjaeng-mcp"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binary_name = binary_name();
    let server_name = if binary_name == "agent-screen-mcp" {
        "agent-screen"
    } else {
        "jjaeng"
    };
    if let Some(argument) = std::env::args().nth(1) {
        match argument.as_str() {
            "--version" | "-V" => {
                println!("{binary_name} {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                let product_name = if binary_name == "agent-screen-mcp" {
                    "Agent Screen"
                } else {
                    "Agent Screen (compatibility mode)"
                };
                println!(
                    "{product_name} MCP server\n\nUsage: {binary_name} [--version|--help]\n\nWith no arguments, serve MCP over stdio."
                );
                return Ok(());
            }
            _ => {}
        }
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let server = AgentScreenServer::with_server_name(server_name);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn current_capabilities() -> Capabilities {
    Capabilities {
        compositor: "hyprland".to_string(),
        monitors: command_available("hyprctl"),
        workspaces: command_available("hyprctl"),
        windows: command_available("hyprctl"),
        screenshots: command_available("grim"),
        interactive_selection: command_available("slurp"),
        gpu_screen_recorder: command_available("gpu-screen-recorder"),
        wl_screenrec: command_available("wl-screenrec"),
        audio_sources: command_available("pactl"),
        microphone_recording: command_available("pactl")
            && command_available("gpu-screen-recorder"),
        ui_daemon_socket: daemon_socket_path().exists(),
    }
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .args([
            "-c",
            "command -v \"$1\" >/dev/null 2>&1",
            "jjaeng-mcp",
            command,
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_output(program: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to start {program}: {err}"))?;
    wait_for_child(&mut child, COMMAND_TIMEOUT)?;
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to collect {program} output: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|err| format!("failed to poll process: {err}"))?
        {
            Some(_) => return Ok(()),
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "process exceeded {} second timeout",
                    timeout.as_secs()
                ));
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn hyprland_monitors() -> Result<Vec<Monitor>, String> {
    let bytes = command_output("hyprctl", &["monitors", "-j"])?;
    serde_json::from_slice(&bytes).map_err(|err| format!("invalid monitor JSON: {err}"))
}

fn hyprland_workspaces() -> Result<Vec<Workspace>, String> {
    let bytes = command_output("hyprctl", &["workspaces", "-j"])?;
    serde_json::from_slice(&bytes).map_err(|err| format!("invalid workspace JSON: {err}"))
}

fn hyprland_windows() -> Result<Vec<Window>, String> {
    let bytes = command_output("hyprctl", &["clients", "-j"])?;
    let windows: Vec<Window> =
        serde_json::from_slice(&bytes).map_err(|err| format!("invalid client JSON: {err}"))?;
    Ok(windows
        .into_iter()
        .filter(|window| window.mapped && !window.hidden)
        .collect())
}

fn select_region() -> Result<String, String> {
    let output = command_output("slurp", &[])?;
    let geometry = String::from_utf8_lossy(&output).trim().to_string();
    parse_geometry(&geometry)?;
    Ok(geometry)
}

fn select_window() -> Result<SelectedWindow, String> {
    let monitors = hyprland_monitors()?;
    let focused_workspace = monitors
        .iter()
        .find(|monitor| monitor.focused)
        .and_then(|monitor| monitor.active_workspace.as_ref())
        .map(|workspace| workspace.id)
        .ok_or_else(|| "focused monitor has no active workspace".to_string())?;
    let windows = hyprland_windows()?;
    let candidates: Vec<&Window> = windows
        .iter()
        .filter(|window| window.workspace.id == focused_workspace)
        .collect();
    if candidates.is_empty() {
        return Err("no selectable windows on the active workspace".to_string());
    }
    let menu = candidates
        .iter()
        .enumerate()
        .map(|(index, window)| {
            let title = if window.title.trim().is_empty() {
                "window"
            } else {
                window.title.trim()
            };
            format!(
                "{},{} {}x{} {:02}. {} [{}]",
                window.at[0],
                window.at[1],
                window.size[0],
                window.size[1],
                index + 1,
                title.replace(['\n', '\r'], " "),
                window.class.replace(['\n', '\r'], " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let selected = run_slurp_with_input(&menu)?;
    let geometry = selected.trim();
    let (x, y, width, height) = parse_geometry(geometry)?;
    let candidate = candidates
        .iter()
        .find(|window| window.at == [x, y] && window.size == [width as i32, height as i32])
        .copied()
        .ok_or_else(|| "selected geometry no longer matches a visible window".to_string())?;
    Ok(selected_window(candidate, geometry, x, y, width, height))
}

fn run_slurp_with_input(menu: &str) -> Result<String, String> {
    let mut child = Command::new("slurp")
        .arg("-r")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to start slurp: {err}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "slurp stdin unavailable".to_string())?
        .write_all(menu.as_bytes())
        .map_err(|err| format!("failed to write slurp candidates: {err}"))?;
    wait_for_child(&mut child, COMMAND_TIMEOUT)?;
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to collect slurp output: {err}"))?;
    if !output.status.success() {
        return Err("window selection cancelled".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn resolve_window(params: &WindowParams) -> Result<SelectedWindow, String> {
    if params.interactive
        || (params.address.is_none() && params.title.is_none() && params.class.is_none())
    {
        return select_window();
    }
    let windows = hyprland_windows()?;
    let address = params.address.as_deref().map(normalize_address);
    let title = params.title.as_deref().map(|value| value.to_lowercase());
    let class = params.class.as_deref().map(|value| value.to_lowercase());
    let window = windows
        .iter()
        .find(|window| {
            address
                .as_deref()
                .is_some_and(|needle| normalize_address(&window.address) == needle)
                || title
                    .as_deref()
                    .is_some_and(|needle| window.title.to_lowercase().contains(needle))
                || class
                    .as_deref()
                    .is_some_and(|needle| window.class.to_lowercase().contains(needle))
        })
        .ok_or_else(|| "no visible window matched the supplied selector".to_string())?;
    if window.size[0] <= 0 || window.size[1] <= 0 {
        return Err("window has invalid geometry".to_string());
    }
    let geometry = format!(
        "{},{} {}x{}",
        window.at[0], window.at[1], window.size[0], window.size[1]
    );
    Ok(selected_window(
        window,
        &geometry,
        window.at[0],
        window.at[1],
        window.size[0] as u32,
        window.size[1] as u32,
    ))
}

fn selected_window(
    window: &Window,
    geometry: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> SelectedWindow {
    SelectedWindow {
        address: window.address.clone(),
        title: window.title.clone(),
        class: window.class.clone(),
        workspace_id: window.workspace.id,
        geometry: geometry.to_string(),
        x,
        y,
        width,
        height,
    }
}

fn normalize_address(address: &str) -> String {
    address.trim().trim_start_matches("0x").to_lowercase()
}

fn capture_monitor(monitor: Option<&str>) -> Result<ArtifactFile, String> {
    let selected = match monitor {
        Some(name) => name.to_string(),
        None => hyprland_monitors()?
            .into_iter()
            .find(|monitor| monitor.focused)
            .map(|monitor| monitor.name)
            .ok_or_else(|| "no focused monitor found".to_string())?,
    };
    let path = new_image_path("screenshot")?;
    run_capture_to_file("grim", &["-o", selected.as_str()], &path)?;
    let info = persist_screenshot(path, "monitor")?;
    let media_path = PathBuf::from(&info.media_path);
    Ok(ArtifactFile {
        info,
        path: media_path,
    })
}

fn capture_geometry(geometry: &str, target: &str) -> Result<ArtifactFile, String> {
    parse_geometry(geometry)?;
    let path = new_image_path("screenshot")?;
    run_capture_to_file("grim", &["-g", geometry], &path)?;
    let info = persist_screenshot(path.clone(), target)?;
    let media_path = PathBuf::from(&info.media_path);
    Ok(ArtifactFile {
        info,
        path: media_path,
    })
}

fn run_capture_to_file(program: &str, args: &[&str], path: &PathBuf) -> Result<(), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to start {program}: {err}"))?;
    wait_for_child(&mut child, COMMAND_TIMEOUT)?;
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to collect {program} output: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if !path.exists() || fs::metadata(path).map_err(|err| err.to_string())?.len() == 0 {
        return Err(format!("{program} produced an empty capture"));
    }
    Ok(())
}

fn persist_screenshot(path: PathBuf, target: &str) -> Result<ArtifactInfo, String> {
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("screenshot")
        .to_string();
    let media_path = history_image_path(&id)?;
    if path != media_path {
        fs::rename(&path, &media_path)
            .map_err(|err| format!("failed to move screenshot into history: {err}"))?;
    }
    let thumbnail_path = history_thumbnail_path(&id)?;
    let thumbnail_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            media_path.to_string_lossy().as_ref(),
            "-vf",
            "scale=320:-2",
            "-frames:v",
            "1",
            thumbnail_path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !thumbnail_status
        .map(|status| status.success())
        .unwrap_or(false)
    {
        fs::copy(&media_path, &thumbnail_path)
            .map_err(|err| format!("failed to create screenshot thumbnail: {err}"))?;
    }
    let size = fs::metadata(&media_path)
        .map_err(|err| format!("failed to stat screenshot: {err}"))?
        .len();
    let (width, height) = png_dimensions(&media_path).unwrap_or((0, 0));
    let info = ArtifactInfo {
        artifact_id: id.clone(),
        kind: "screenshot".to_string(),
        media_path: media_path.display().to_string(),
        thumbnail_path: Some(thumbnail_path.display().to_string()),
        mime_type: "image/png".to_string(),
        width: (width > 0).then_some(width),
        height: (height > 0).then_some(height),
        duration_ms: None,
        file_size_bytes: Some(size),
        created_at: now_millis(),
        target: Some(target.to_string()),
        audio: None,
    };
    append_history(&info)?;
    Ok(info)
}

fn png_dimensions(path: &PathBuf) -> Option<(u32, u32)> {
    let mut header = [0_u8; 24];
    let mut file = fs::File::open(path).ok()?;
    file.read_exact(&mut header).ok()?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(header[16..20].try_into().ok()?),
        u32::from_be_bytes(header[20..24].try_into().ok()?),
    ))
}

#[derive(Debug, Clone)]
struct ArtifactFile {
    info: ArtifactInfo,
    path: PathBuf,
}

fn image_result(artifact: &ArtifactFile) -> Result<CallToolResult, String> {
    image_result_with_metadata(artifact, None)
}

fn image_result_with_metadata(
    artifact: &ArtifactFile,
    extra: Option<Value>,
) -> Result<CallToolResult, String> {
    let bytes =
        fs::read(&artifact.path).map_err(|err| format!("failed to read screenshot: {err}"))?;
    if bytes.len() > MAX_INLINE_IMAGE_BYTES {
        return Ok(CallToolResult::structured(
            json!({"artifact": artifact.info, "message": "screenshot exceeds inline MCP size; use the media_path resource"}),
        ));
    }
    let mut metadata = json!({"artifact": artifact.info});
    if let Some(extra) = extra {
        if let (Some(base), Some(extra)) = (metadata.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(CallToolResult::success(vec![
        ContentBlock::text(metadata.to_string()),
        ContentBlock::image(BASE64.encode(bytes), "image/png"),
    ]))
}

fn parse_geometry(geometry: &str) -> Result<(i32, i32, u32, u32), String> {
    let mut parts = geometry.split_whitespace();
    let position = parts
        .next()
        .ok_or_else(|| format!("invalid geometry: {geometry}"))?;
    let size = parts
        .next()
        .ok_or_else(|| format!("invalid geometry: {geometry}"))?;
    if parts.next().is_some() {
        return Err(format!("invalid geometry: {geometry}"));
    }
    let (x, y) = position
        .split_once(',')
        .ok_or_else(|| format!("invalid geometry: {geometry}"))?;
    let (width, height) = size
        .split_once('x')
        .ok_or_else(|| format!("invalid geometry: {geometry}"))?;
    let x = x
        .parse::<i32>()
        .map_err(|_| format!("invalid geometry: {geometry}"))?;
    let y = y
        .parse::<i32>()
        .map_err(|_| format!("invalid geometry: {geometry}"))?;
    let width = width
        .parse::<u32>()
        .map_err(|_| format!("invalid geometry: {geometry}"))?;
    let height = height
        .parse::<u32>()
        .map_err(|_| format!("invalid geometry: {geometry}"))?;
    if width == 0 || height == 0 {
        return Err(format!("invalid geometry: {geometry}"));
    }
    Ok((x, y, width, height))
}

fn start_recording_session(request: &StartRecordingParams) -> Result<RecordingSession, String> {
    let target = request.target.to_lowercase();
    if target == "all_outputs" {
        return Err("all_outputs recording is not supported by the installed GSR capture options; select a monitor explicitly".to_string());
    }
    let (capture_target, width, height) = match target.as_str() {
        "monitor" | "workspace" | "fullscreen" => {
            let monitor_name = request
                .monitor
                .clone()
                .or_else(|| {
                    hyprland_monitors().ok().and_then(|items| {
                        items
                            .into_iter()
                            .find(|item| item.focused)
                            .map(|item| item.name)
                    })
                })
                .ok_or_else(|| "no focused monitor found".to_string())?;
            let monitor = hyprland_monitors()?
                .into_iter()
                .find(|item| item.name == monitor_name)
                .ok_or_else(|| format!("monitor not found: {monitor_name}"))?;
            (
                monitor_name,
                monitor.width.max(1) as u32,
                monitor.height.max(1) as u32,
            )
        }
        "region" => {
            let geometry = request.geometry.clone().unwrap_or(select_region()?);
            let (_, _, width, height) = parse_geometry(&geometry)?;
            (format!("region:{geometry}"), width, height)
        }
        "window" => {
            let params = WindowParams {
                address: request.address.clone(),
                title: request.title.clone(),
                class: request.class.clone(),
                interactive: request.address.is_none()
                    && request.title.is_none()
                    && request.class.is_none(),
            };
            let window = resolve_window(&params)?;
            (
                format!("region:{}", window.geometry),
                window.width,
                window.height,
            )
        }
        other => return Err(format!("unsupported recording target: {other}")),
    };
    let id = format!("recording-{}", now_millis());
    let output_path = history_video_path(&id, "mkv")?;
    let mut command = if command_available("gpu-screen-recorder") {
        let mut command = Command::new("gpu-screen-recorder");
        command.args(["-o", output_path.to_string_lossy().as_ref()]);
        command
            .arg("-w")
            .arg(if capture_target.starts_with("region:") {
                "region"
            } else {
                capture_target.as_str()
            });
        if let Some(encoding) = request.encode_resolution.as_deref() {
            command.args(["-s", encoding]);
        }
        if let Some(fps) = request.fps {
            command.args(["-f", &fps.to_string()]);
        }
        if let Some(region) = capture_target.strip_prefix("region:") {
            command.args(["-region", region]);
        }
        if let Some(audio) = resolve_audio(
            &request.audio,
            request.system_audio.as_deref(),
            request.microphone.as_deref(),
        )? {
            command.args(["-a", &audio]);
        }
        command
    } else if command_available("wl-screenrec") {
        if request.audio.eq_ignore_ascii_case("both") {
            return Err("combined audio requires gpu-screen-recorder".to_string());
        }
        let mut command = Command::new("wl-screenrec");
        command.args(["-f", output_path.to_string_lossy().as_ref()]);
        if let Some(region) = capture_target.strip_prefix("region:") {
            command.args(["-g", region]);
        } else {
            command.args(["-o", capture_target.as_str()]);
        }
        command
    } else {
        return Err("recording requires gpu-screen-recorder or wl-screenrec".to_string());
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to start recording: {err}"))?;
    thread::sleep(RECORDING_STARTUP_GRACE);
    if let Some(status) = child
        .try_wait()
        .map_err(|err| format!("failed to check recorder: {err}"))?
    {
        return Err(format!("recorder exited immediately with status {status}"));
    }
    Ok(RecordingSession {
        child,
        artifact_id: id,
        output_path,
        started_at: now_millis(),
        width,
        height,
        target,
        audio: request.audio.clone(),
    })
}

fn resolve_audio(
    mode: &str,
    system: Option<&str>,
    microphone: Option<&str>,
) -> Result<Option<String>, String> {
    match mode.to_lowercase().as_str() {
        "off" => Ok(None),
        "desktop" => Ok(Some(
            system
                .map(str::to_string)
                .or_else(default_system_audio)
                .ok_or_else(|| "no desktop audio source found".to_string())?,
        )),
        "microphone" | "mic" => Ok(Some(
            microphone
                .map(str::to_string)
                .or_else(default_microphone)
                .ok_or_else(|| "no microphone source found".to_string())?,
        )),
        "both" => {
            let desktop = system
                .map(str::to_string)
                .or_else(default_system_audio)
                .ok_or_else(|| "no desktop audio source found".to_string())?;
            let microphone = microphone
                .map(str::to_string)
                .or_else(default_microphone)
                .ok_or_else(|| "no microphone source found".to_string())?;
            Ok(Some(format!("{desktop}|{microphone}")))
        }
        other => Err(format!("unsupported audio mode: {other}")),
    }
}

fn default_system_audio() -> Option<String> {
    let sink = command_output("pactl", &["get-default-sink"]).ok()?;
    let sink = String::from_utf8_lossy(&sink).trim().to_string();
    (!sink.is_empty()).then(|| format!("{sink}.monitor"))
}

fn default_microphone() -> Option<String> {
    let source = command_output("pactl", &["get-default-source"]).ok()?;
    let source = String::from_utf8_lossy(&source).trim().to_string();
    (!source.is_empty() && !source.ends_with(".monitor")).then_some(source)
}

fn recording_status_value(runtime: &Arc<Mutex<Runtime>>) -> Result<Value, String> {
    let runtime = runtime
        .lock()
        .map_err(|_| "recording state lock poisoned".to_string())?;
    let Some(recording) = runtime.recording.as_ref() else {
        return Ok(json!({"active": false, "last_recording": runtime.last_recording}));
    };
    Ok(
        json!({"active": true, "recording_id": recording.artifact_id, "target": recording.target, "audio": recording.audio, "output_path": recording.output_path, "started_at": recording.started_at, "elapsed_ms": now_millis().saturating_sub(recording.started_at), "width": recording.width, "height": recording.height}),
    )
}

fn signal_recording(runtime: &Arc<Mutex<Runtime>>, signal: &str) -> Result<(), String> {
    let runtime = runtime
        .lock()
        .map_err(|_| "recording state lock poisoned".to_string())?;
    let recording = runtime
        .recording
        .as_ref()
        .ok_or_else(|| "no active recording".to_string())?;
    send_signal(recording.child.id(), signal)
}

fn stop_recording_runtime(runtime: &Arc<Mutex<Runtime>>) -> Result<ArtifactInfo, String> {
    let mut runtime = runtime
        .lock()
        .map_err(|_| "recording state lock poisoned".to_string())?;
    let mut recording = runtime
        .recording
        .take()
        .ok_or_else(|| "no active recording".to_string())?;
    send_signal(recording.child.id(), "-INT")?;
    wait_for_child(&mut recording.child, Duration::from_secs(15))?;
    let status = recording
        .child
        .wait()
        .map_err(|err| format!("failed to wait for recorder: {err}"))?;
    if !status.success() {
        tracing::warn!(?status, "recorder exited non-zero after stop");
    }
    let size = fs::metadata(&recording.output_path)
        .map_err(|err| format!("recording output missing: {err}"))?
        .len();
    if size == 0 {
        return Err("recording output is empty".to_string());
    }
    let thumbnail_path = recording.output_path.with_extension("thumb.png");
    let _ = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            recording.output_path.to_string_lossy().as_ref(),
            "-vf",
            "scale=320:-2",
            "-frames:v",
            "1",
            thumbnail_path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let info = ArtifactInfo {
        artifact_id: recording.artifact_id.clone(),
        kind: "recording".to_string(),
        media_path: recording.output_path.display().to_string(),
        thumbnail_path: thumbnail_path
            .exists()
            .then(|| thumbnail_path.display().to_string()),
        mime_type: "video/x-matroska".to_string(),
        width: Some(recording.width),
        height: Some(recording.height),
        duration_ms: Some(now_millis().saturating_sub(recording.started_at)),
        file_size_bytes: Some(size),
        created_at: recording.started_at,
        target: Some(recording.target),
        audio: Some(recording.audio),
    };
    append_history(&info)?;
    runtime.last_recording = Some(info.clone());
    Ok(info)
}

fn send_signal(pid: u32, signal: &str) -> Result<(), String> {
    let status = Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .map_err(|err| format!("failed to signal recorder: {err}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("kill {signal} failed with {status}"))
}

fn daemon_action(variant: &str) -> Result<Value, String> {
    let encoded = serde_json::to_string(variant).map_err(|err| err.to_string())? + "\n";
    let mut stream = connect_daemon_socket()
        .map_err(|err| format!("Agent Screen daemon socket unavailable: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|err| err.to_string())?;
    stream
        .write_all(encoded.as_bytes())
        .map_err(|err| err.to_string())?;
    stream.flush().map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok();
    Ok(json!({"ok": true, "daemon_response": response.trim(), "action": variant}))
}

fn read_daemon_status() -> Result<Value, String> {
    let path = [
        runtime_dir().join("agent-screen-status.json"),
        runtime_dir().join("jjaeng-status.json"),
        PathBuf::from("/tmp/chalkak/chalkak-status.json"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .ok_or_else(|| "status snapshot not found".to_string())?;
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&contents).map_err(|err| format!("invalid daemon status: {err}"))
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/agent-screen"))
}

fn daemon_socket_path() -> PathBuf {
    runtime_dir().join("agent-screen.sock")
}

fn connect_daemon_socket() -> std::io::Result<UnixStream> {
    let mut last_error = None;
    for path in [
        daemon_socket_path(),
        runtime_dir().join("jjaeng.sock"),
        PathBuf::from("/tmp/chalkak/chalkak.sock"),
    ] {
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no daemon socket")))
}

fn state_root() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn cache_root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn history_root() -> PathBuf {
    state_root().join("agent-screen/history")
}
fn history_image_path(id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let path = history_root().join("images").join(format!("{id}.png"));
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    Ok(path)
}
fn history_video_path(id: &str, ext: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let path = history_root().join("videos").join(format!("{id}.{ext}"));
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    Ok(path)
}
fn history_thumbnail_path(id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let path = cache_root()
        .join("agent-screen/thumbnails")
        .join(format!("{id}.png"));
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    Ok(path)
}
fn new_image_path(prefix: &str) -> Result<PathBuf, String> {
    let path = history_root()
        .join("incoming")
        .join(format!("{prefix}-{}.png", now_millis()));
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    Ok(path)
}
fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err("invalid artifact id".to_string())
    } else {
        Ok(())
    }
}
fn manifest_path() -> PathBuf {
    state_root().join("agent-screen/history.json")
}

fn load_history() -> Result<HistoryManifest, String> {
    let path = [
        manifest_path(),
        state_root().join("jjaeng/history.json"),
        state_root().join("chalkak/history.json"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .unwrap_or_else(manifest_path);
    if !path.exists() {
        return Ok(HistoryManifest {
            entries: Vec::new(),
        });
    }
    serde_json::from_slice(&fs::read(path).map_err(|err| err.to_string())?)
        .map_err(|err| format!("invalid history manifest: {err}"))
}

fn append_history(info: &ArtifactInfo) -> Result<(), String> {
    let mut manifest = load_history()?;
    manifest
        .entries
        .retain(|entry| entry.entry_id != info.artifact_id);
    manifest.entries.insert(
        0,
        HistoryEntry {
            kind: info.kind.clone(),
            entry_id: info.artifact_id.clone(),
            media_path: PathBuf::from(&info.media_path),
            thumbnail_path: info
                .thumbnail_path
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&info.media_path)),
            width: info.width.unwrap_or_default(),
            height: info.height.unwrap_or_default(),
            created_at: info.created_at,
            saved_path: None,
            duration_ms: info.duration_ms,
            file_size_bytes: info.file_size_bytes,
        },
    );
    manifest.entries.truncate(48);
    let path = manifest_path();
    fs::create_dir_all(path.parent().unwrap()).map_err(|err| err.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    fs::rename(tmp, path).map_err(|err| err.to_string())
}

fn find_history_entry(id: &str) -> Result<HistoryEntry, String> {
    validate_id(id)?;
    let entry = load_history()?
        .entries
        .into_iter()
        .find(|entry| entry.entry_id == id)
        .ok_or_else(|| format!("artifact not found: {id}"))?;
    let state_roots = [
        state_root().join("agent-screen"),
        state_root().join("jjaeng"),
        state_root().join("chalkak"),
    ];
    let cache_roots = [
        cache_root().join("agent-screen"),
        cache_root().join("jjaeng"),
        cache_root().join("chalkak"),
    ];
    if !state_roots
        .iter()
        .any(|root| entry.media_path.starts_with(root))
        || (!cache_roots
            .iter()
            .any(|root| entry.thumbnail_path.starts_with(root))
            && !state_roots
                .iter()
                .any(|root| entry.thumbnail_path.starts_with(root)))
    {
        return Err("artifact path is outside Agent Screen-managed storage".to_string());
    }
    Ok(entry)
}

fn history_info(entry: HistoryEntry) -> ArtifactInfo {
    let media_path = entry.media_path.clone();
    let thumbnail_path = entry.thumbnail_path.clone();
    let kind = entry.kind.clone();
    let entry_id = entry.entry_id.clone();
    let mime_type = if entry.kind.eq_ignore_ascii_case("recording") {
        match media_path.extension().and_then(|value| value.to_str()) {
            Some("mkv") => "video/x-matroska",
            Some("webm") => "video/webm",
            _ => "video/mp4",
        }
    } else {
        "image/png"
    };
    ArtifactInfo {
        artifact_id: entry_id,
        kind,
        media_path: media_path.display().to_string(),
        thumbnail_path: Some(thumbnail_path.display().to_string()),
        mime_type: mime_type.to_string(),
        width: Some(entry.width),
        height: Some(entry.height),
        duration_ms: entry.duration_ms,
        file_size_bytes: entry
            .file_size_bytes
            .or_else(|| fs::metadata(&media_path).ok().map(|meta| meta.len())),
        created_at: entry.created_at,
        target: None,
        audio: None,
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_parser_accepts_slurp_form() {
        assert_eq!(parse_geometry("10,20 300x200").unwrap(), (10, 20, 300, 200));
    }

    #[test]
    fn geometry_parser_rejects_zero_dimensions() {
        assert!(parse_geometry("0,0 0x200").is_err());
    }

    #[test]
    fn address_normalization_is_stable() {
        assert_eq!(normalize_address("0xABC"), "abc");
    }

    #[test]
    fn artifact_ids_are_path_safe() {
        assert!(validate_id("recording-123").is_ok());
        assert!(validate_id("../secret").is_err());
    }

    #[test]
    fn agent_screen_resources_use_agent_screen_scheme() {
        let resources = resource_definitions("agent-screen");
        assert_eq!(
            resources.first().map(|resource| resource.uri.as_str()),
            Some("agent-screen://status")
        );
        assert_eq!(
            resources.last().map(|resource| resource.uri.as_str()),
            Some("agent-screen://artifact/{artifact_id}")
        );
    }

    #[test]
    fn resource_reader_rejects_unknown_schemes() {
        assert!(read_resource_contents("https://status", "agent-screen").is_err());
    }
}
