use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ui::{install_lucide_icon_theme, StyleTokens};
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Button};
use jjaeng_core::capture;
use jjaeng_core::clipboard::{ClipboardBackend, WlCopyBackend};
use jjaeng_core::editor::tools::CropElement;
use jjaeng_core::editor::{self, EditorAction, ToolKind};
use jjaeng_core::error::AppResult;
use jjaeng_core::history::HistoryService;
use jjaeng_core::identity::{APP_CSS_ROOT, APP_ID, APP_NAME};
use jjaeng_core::input::ShortcutAction;
use jjaeng_core::preview::PreviewAction;
use jjaeng_core::recording::{
    self, AudioMode, RecordingEncodingPreset, RecordingRequest, RecordingSelection, RecordingSize,
};
use jjaeng_core::service::{RemoteCommand, StatusSnapshot};
use jjaeng_core::state::{AppEvent, AppState, StateMachine};
use jjaeng_core::storage::StorageService;

mod actions;
mod adaptive;
mod bootstrap;
mod editor_history;
mod editor_popup;
mod editor_runtime;
mod editor_text_runtime;
mod editor_viewport;
mod history_runtime;
mod hover_controls;
mod hypr;
mod input_bridge;
mod launchpad;
mod launchpad_actions;
mod layout;
mod lifecycle;
mod ocr_support;
mod preview_runtime;
mod recording_prompt;
mod recording_result;
mod runtime_css;
mod runtime_support;
mod window_state;
mod worker;

use self::bootstrap::*;
use self::editor_popup::*;
use self::editor_runtime::*;
use self::history_runtime::*;
use self::launchpad::*;
use self::launchpad_actions::*;
use self::lifecycle::*;
use self::preview_runtime::*;
use self::recording_prompt::*;
use self::recording_result::*;
use self::runtime_support::*;
pub use self::runtime_support::{StartupCaptureMode, StartupConfig};
use self::window_state::*;
use self::worker::spawn_worker_action;

const EDITOR_PEN_ICON_NAME: &str = "pencil-symbolic";
type ToolOptionsRefresh = Rc<dyn Fn(ToolKind)>;
type ToolOptionsRefreshSlot = RefCell<Option<ToolOptionsRefresh>>;
type SharedToolOptionsRefresh = Rc<ToolOptionsRefreshSlot>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextInputActivation {
    Auto,
    ForceOn,
    ForceOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolSwitchConfig {
    clear_pending_crop_when_not_crop: bool,
    text_input: TextInputActivation,
}

impl ToolSwitchConfig {
    const fn auto(clear_pending_crop_when_not_crop: bool) -> Self {
        Self {
            clear_pending_crop_when_not_crop,
            text_input: TextInputActivation::Auto,
        }
    }
}

fn text_input_should_be_active(selected_tool: ToolKind, activation: TextInputActivation) -> bool {
    match activation {
        TextInputActivation::Auto => selected_tool == ToolKind::Text,
        TextInputActivation::ForceOn => true,
        TextInputActivation::ForceOff => false,
    }
}

fn set_editor_pan_cursor<W: IsA<gtk4::Widget>>(
    widget: &W,
    active_editor_tool: &Cell<ToolKind>,
    space_pan_pressed: &Cell<bool>,
    drag_pan_active: &Cell<bool>,
) {
    let cursor = if drag_pan_active.get() {
        Some("grabbing")
    } else if active_editor_tool.get() == ToolKind::Pan || space_pan_pressed.get() {
        Some("grab")
    } else {
        None
    };

    widget.set_cursor_from_name(cursor);
}

fn set_active_editor_tool(
    active_editor_tool: &Cell<ToolKind>,
    tool: ToolKind,
    refresh_editor_cursor: &dyn Fn(),
) {
    active_editor_tool.set(tool);
    refresh_editor_cursor();
}

fn start_editor_text_input(
    editor_input_mode: &RefCell<editor::EditorInputMode>,
    text_im_context: &gtk4::IMMulticontext,
) {
    editor_input_mode.borrow_mut().start_text_input();
    text_im_context.focus_in();
}

fn stop_editor_text_input(
    editor_input_mode: &RefCell<editor::EditorInputMode>,
    text_im_context: &gtk4::IMMulticontext,
    text_preedit_state: &RefCell<TextPreeditState>,
) {
    editor_input_mode.borrow_mut().end_text_input();
    text_im_context.reset();
    text_im_context.focus_out();
    *text_preedit_state.borrow_mut() = TextPreeditState::default();
}

fn sync_editor_tool_controls(
    tool_buttons: &RefCell<Vec<(ToolKind, Button)>>,
    refresh_tool_options: &ToolOptionsRefreshSlot,
    selected_tool: ToolKind,
) {
    sync_active_tool_buttons(tool_buttons.borrow().as_slice(), selected_tool);
    if let Some(refresh) = refresh_tool_options.borrow().as_ref() {
        refresh(selected_tool);
    }
}

#[derive(Clone)]
struct EditorToolSwitchContext {
    active_editor_tool: Rc<Cell<ToolKind>>,
    editor_tools: Rc<RefCell<editor::EditorTools>>,
    editor_input_mode: Rc<RefCell<editor::EditorInputMode>>,
    tool_drag_preview: Rc<RefCell<Option<ToolDragPreview>>>,
    pending_crop: Rc<RefCell<Option<CropElement>>>,
    text_im_context: Rc<gtk4::IMMulticontext>,
    text_preedit_state: Rc<RefCell<TextPreeditState>>,
    tool_buttons: Rc<RefCell<Vec<(ToolKind, Button)>>>,
    refresh_tool_options: SharedToolOptionsRefresh,
    refresh_editor_cursor: Rc<dyn Fn()>,
}

impl EditorToolSwitchContext {
    fn switch_to(&self, selected_tool: ToolKind, clear_pending_crop_when_not_crop: bool) {
        self.switch_to_with_config(
            selected_tool,
            ToolSwitchConfig::auto(clear_pending_crop_when_not_crop),
        );
    }

    fn switch_to_with_config(&self, selected_tool: ToolKind, config: ToolSwitchConfig) {
        set_active_editor_tool(
            self.active_editor_tool.as_ref(),
            selected_tool,
            self.refresh_editor_cursor.as_ref(),
        );
        self.editor_tools.borrow_mut().select_tool(selected_tool);
        self.tool_drag_preview.borrow_mut().take();

        if config.clear_pending_crop_when_not_crop && selected_tool != ToolKind::Crop {
            self.pending_crop.borrow_mut().take();
        }

        if selected_tool == ToolKind::Crop {
            self.editor_input_mode.borrow_mut().activate_crop();
        } else {
            self.editor_input_mode.borrow_mut().deactivate_crop();
        }

        if text_input_should_be_active(selected_tool, config.text_input) {
            start_editor_text_input(
                self.editor_input_mode.as_ref(),
                self.text_im_context.as_ref(),
            );
        } else {
            stop_editor_text_input(
                self.editor_input_mode.as_ref(),
                self.text_im_context.as_ref(),
                self.text_preedit_state.as_ref(),
            );
        }

        sync_editor_tool_controls(
            self.tool_buttons.as_ref(),
            self.refresh_tool_options.as_ref(),
            selected_tool,
        );
    }
}

fn shortcut_editor_tool_switch(action: ShortcutAction) -> Option<(ToolKind, &'static str)> {
    match action {
        ShortcutAction::EditorEnterSelect => Some((ToolKind::Select, "editor select tool armed")),
        ShortcutAction::EditorEnterPan => Some((ToolKind::Pan, "editor pan tool armed")),
        ShortcutAction::EditorEnterBlur => Some((ToolKind::Blur, "editor blur tool armed")),
        ShortcutAction::EditorEnterPen => Some((ToolKind::Pen, "editor pen tool armed")),
        ShortcutAction::EditorEnterArrow => Some((ToolKind::Arrow, "editor arrow tool armed")),
        ShortcutAction::EditorEnterRectangle => {
            Some((ToolKind::Rectangle, "editor rectangle tool armed"))
        }
        ShortcutAction::EditorEnterCrop => Some((ToolKind::Crop, "editor crop interaction armed")),
        ShortcutAction::EditorEnterText => Some((ToolKind::Text, "editor text tool armed")),
        ShortcutAction::EditorEnterOcr => Some((ToolKind::Ocr, "editor OCR tool armed")),
        _ => None,
    }
}

fn editor_window_default_geometry(style_tokens: StyleTokens) -> RuntimeWindowGeometry {
    RuntimeWindowGeometry::new(
        style_tokens.editor_initial_width,
        style_tokens.editor_initial_height,
    )
}

fn editor_window_min_geometry(style_tokens: StyleTokens) -> RuntimeWindowGeometry {
    RuntimeWindowGeometry::new(
        style_tokens.editor_min_width,
        style_tokens.editor_min_height,
    )
}

#[derive(Clone)]
struct EditorRuntimeState {
    capture_id: Rc<RefCell<Option<String>>>,
    has_unsaved_changes: Rc<RefCell<bool>>,
    close_dialog_open: Rc<RefCell<bool>>,
    toast: Rc<RefCell<Option<ToastRuntime>>>,
    input_mode: Rc<RefCell<editor::EditorInputMode>>,
}

impl EditorRuntimeState {
    fn new() -> Self {
        Self {
            capture_id: Rc::new(RefCell::new(None)),
            has_unsaved_changes: Rc::new(RefCell::new(false)),
            close_dialog_open: Rc::new(RefCell::new(false)),
            toast: Rc::new(RefCell::new(None)),
            input_mode: Rc::new(RefCell::new(editor::EditorInputMode::new())),
        }
    }

    fn reset_session_state(&self) {
        *self.has_unsaved_changes.borrow_mut() = false;
        *self.close_dialog_open.borrow_mut() = false;
        self.input_mode.borrow_mut().reset();
    }

    fn clear_runtime_state(&self) {
        *self.capture_id.borrow_mut() = None;
        *self.toast.borrow_mut() = None;
        self.reset_session_state();
    }
}

fn reset_editor_session_state(editor_runtime: &EditorRuntimeState) {
    editor_runtime.reset_session_state();
}

fn clear_editor_runtime_state(editor_runtime: &EditorRuntimeState) {
    editor_runtime.clear_runtime_state();
}

struct ActiveRecording {
    handle: recording::RecordingHandle,
    timer_source_id: gtk4::glib::SourceId,
    started_at: Instant,
    paused_at: Option<Instant>,
    paused_total_ms: u64,
}

impl ActiveRecording {
    fn elapsed_ms(&self) -> u64 {
        let paused_ms = self.paused_total_ms.saturating_add(
            self.paused_at
                .map(|paused_at| paused_at.elapsed().as_millis() as u64)
                .unwrap_or(0),
        );
        (self.started_at.elapsed().as_millis() as u64).saturating_sub(paused_ms)
    }

    fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    fn mark_paused(&mut self) {
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
        }
    }

    fn mark_resumed(&mut self) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_total_ms = self
                .paused_total_ms
                .saturating_add(paused_at.elapsed().as_millis() as u64);
        }
    }
}

#[derive(Clone, Default)]
struct RecordingRuntimeState {
    active: Rc<RefCell<Option<ActiveRecording>>>,
    elapsed_ms: Rc<Cell<u64>>,
}

impl RecordingRuntimeState {
    fn is_active(&self) -> bool {
        self.active.borrow().is_some()
    }

    fn active_recording_id(&self) -> Option<String> {
        self.active
            .borrow()
            .as_ref()
            .map(|active| active.handle.recording_id.clone())
    }

    fn is_paused(&self) -> bool {
        self.active
            .borrow()
            .as_ref()
            .is_some_and(ActiveRecording::is_paused)
    }
}

fn close_editor_if_open_and_clear(
    editor_window: &Rc<RefCell<Option<ApplicationWindow>>>,
    runtime_window_state: &Rc<RefCell<RuntimeWindowState>>,
    editor_close_guard: &Rc<Cell<bool>>,
    editor_runtime: &EditorRuntimeState,
    style_tokens: StyleTokens,
) -> bool {
    if close_editor_window_if_open(
        editor_window,
        runtime_window_state,
        editor_close_guard,
        editor_window_default_geometry(style_tokens),
        editor_window_min_geometry(style_tokens),
    ) {
        clear_editor_runtime_state(editor_runtime);
        true
    } else {
        false
    }
}

#[derive(Clone)]
struct EditorOutputActionRuntime {
    runtime_session: Rc<RefCell<RuntimeSession>>,
    shared_machine: Rc<RefCell<StateMachine>>,
    storage_service: Rc<Option<StorageService>>,
    history_service: Rc<Option<HistoryService>>,
    status_log: Rc<RefCell<String>>,
    editor_toast: ToastRuntime,
    editor_tools: Rc<RefCell<editor::EditorTools>>,
    pending_crop: Rc<RefCell<Option<CropElement>>>,
    editor_source_pixbuf: Option<gtk4::gdk_pixbuf::Pixbuf>,
    editor_has_unsaved_changes: Rc<RefCell<bool>>,
    editor_output_format: Rc<Cell<EditorOutputFormat>>,
    toast_duration_ms: u32,
}

impl EditorOutputActionRuntime {
    fn run(&self, action: EditorAction, action_label: &'static str) -> bool {
        let active_capture = match self.runtime_session.borrow().active_capture().cloned() {
            Some(artifact) => artifact,
            None => {
                *self.status_log.borrow_mut() =
                    format!("{action_label} requires an active capture");
                return false;
            }
        };

        if !matches!(self.shared_machine.borrow().state(), AppState::Editor) {
            *self.status_log.borrow_mut() = format!("editor {action_label} requires editor state");
            return false;
        }

        let Some(service) = self.storage_service.as_ref().as_ref() else {
            *self.status_log.borrow_mut() = "storage service unavailable".to_string();
            return false;
        };

        let Some(source_pixbuf) = self.editor_source_pixbuf.as_ref() else {
            *self.status_log.borrow_mut() = "editor source image unavailable".to_string();
            self.editor_toast
                .show("Source image unavailable", self.toast_duration_ms);
            return false;
        };

        let tools = self.editor_tools.borrow();
        let completed = execute_editor_output_action(EditorOutputActionContext {
            action,
            active_capture: &active_capture,
            output_format: self.editor_output_format.get(),
            editor_tools: &tools,
            pending_crop: self.pending_crop.borrow().as_ref().copied(),
            source_pixbuf,
            storage_service: service,
            status_log: &self.status_log,
            editor_toast: &self.editor_toast,
            toast_duration_ms: self.toast_duration_ms,
            editor_has_unsaved_changes: &self.editor_has_unsaved_changes,
        });

        if !completed {
            return false;
        }

        if let Some(history_service) = self.history_service.as_ref().as_ref() {
            if let Err(err) = history_service.record_capture(&active_capture) {
                tracing::debug!(
                    capture_id = %active_capture.capture_id,
                    ?err,
                    "failed to refresh history after editor output action"
                );
            }
            if matches!(action, EditorAction::Save) {
                match service.allocate_target_path_with_extension(
                    &active_capture.capture_id,
                    self.editor_output_format.get().file_extension(),
                ) {
                    Ok(saved_path) => {
                        if let Err(err) =
                            history_service.mark_saved(&active_capture.capture_id, &saved_path)
                        {
                            tracing::debug!(
                                capture_id = %active_capture.capture_id,
                                ?err,
                                "failed to update history saved path after editor save"
                            );
                        }
                    }
                    Err(err) => {
                        tracing::debug!(
                            capture_id = %active_capture.capture_id,
                            ?err,
                            "failed to resolve saved path after editor save"
                        );
                    }
                }
            }
        }

        true
    }
}

fn run_startup_capture<R: Fn() + 'static>(
    launchpad_actions: &LaunchpadActionExecutor,
    startup_capture: StartupCaptureMode,
    on_complete: R,
) {
    match startup_capture {
        StartupCaptureMode::Full => launchpad_actions.capture_and_open_preview_async(
            capture::capture_full,
            "Captured full screen",
            "full capture failed",
            "Full capture failed",
            on_complete,
        ),
        StartupCaptureMode::Region => launchpad_actions.capture_and_open_preview_async(
            capture::capture_region,
            "Captured selected region",
            "region capture failed",
            "Region capture failed",
            on_complete,
        ),
        StartupCaptureMode::Window => launchpad_actions.capture_and_open_preview_async(
            capture::capture_window,
            "Captured selected window",
            "window capture failed",
            "Window capture failed",
            on_complete,
        ),
        StartupCaptureMode::None => {}
    }
}

fn startup_capture_from_remote_command(command: &RemoteCommand) -> Option<StartupCaptureMode> {
    match command {
        RemoteCommand::CaptureFull => Some(StartupCaptureMode::Full),
        RemoteCommand::CaptureRegion => Some(StartupCaptureMode::Region),
        RemoteCommand::CaptureWindow => Some(StartupCaptureMode::Window),
        _ => None,
    }
}

fn should_show_launchpad_for_remote_fallback(command: &RemoteCommand) -> bool {
    !matches!(
        command,
        RemoteCommand::OpenHistory
            | RemoteCommand::ToggleHistory
            | RemoteCommand::StartRecording(_)
            | RemoteCommand::PromptRecording(_)
            | RemoteCommand::StopRecording
    )
}

fn is_history_remote_command(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::OpenHistory | RemoteCommand::ToggleHistory
    )
}

fn is_recording_remote_command(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::StartRecording(_)
            | RemoteCommand::PromptRecording(_)
            | RemoteCommand::StopRecording
    )
}

#[allow(clippy::too_many_arguments)]
fn dispatch_remote_command(
    launchpad_actions: &LaunchpadActionExecutor,
    open_history_window: &Rc<dyn Fn()>,
    toggle_history_window: &Rc<dyn Fn()>,
    start_recording: &Rc<dyn Fn(RecordingRequest)>,
    prompt_recording: &Rc<dyn Fn(RecordingRequest)>,
    stop_recording: &Rc<dyn Fn()>,
    remote_command: RemoteCommand,
    render: &Rc<dyn Fn()>,
) {
    match remote_command {
        RemoteCommand::CaptureFull => {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_full,
                "Captured full screen",
                "full capture failed",
                "Full capture failed",
                move || (render.as_ref())(),
            );
        }
        RemoteCommand::CaptureRegion => {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_region,
                "Captured selected region",
                "region capture failed",
                "Region capture failed",
                move || (render.as_ref())(),
            );
        }
        RemoteCommand::CaptureWindow => {
            let render = render.clone();
            launchpad_actions.capture_and_open_preview_async(
                capture::capture_window,
                "Captured selected window",
                "window capture failed",
                "Window capture failed",
                move || (render.as_ref())(),
            );
        }
        RemoteCommand::OpenPreview => {
            launchpad_actions.open_preview();
            (render.as_ref())();
        }
        RemoteCommand::OpenHistory => {
            (open_history_window.as_ref())();
            (render.as_ref())();
        }
        RemoteCommand::ToggleHistory => {
            (toggle_history_window.as_ref())();
            (render.as_ref())();
        }
        RemoteCommand::OpenEditor => {
            launchpad_actions.open_editor();
            (render.as_ref())();
        }
        RemoteCommand::SaveLatest => {
            let render = render.clone();
            launchpad_actions.run_preview_action_async(PreviewAction::Save, move || {
                (render.as_ref())();
            });
        }
        RemoteCommand::CopyLatest => {
            let render = render.clone();
            launchpad_actions.run_preview_action_async(PreviewAction::Copy, move || {
                (render.as_ref())();
            });
        }
        RemoteCommand::DismissLatest => {
            launchpad_actions.close_preview();
            (render.as_ref())();
        }
        RemoteCommand::StartRecording(request) => {
            (start_recording.as_ref())(request);
        }
        RemoteCommand::PromptRecording(request) => {
            (prompt_recording.as_ref())(request);
        }
        RemoteCommand::StopRecording => {
            (stop_recording.as_ref())();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn should_release_headless_startup_hold(
    hold_active: bool,
    startup_capture_completed: bool,
    state: AppState,
    has_active_capture: bool,
    preview_window_count: usize,
    editor_window_open: bool,
    recording_active: bool,
    recording_prompt_open: bool,
    recording_result_open: bool,
) -> bool {
    hold_active
        && startup_capture_completed
        && matches!(state, AppState::Idle)
        && !has_active_capture
        && preview_window_count == 0
        && !editor_window_open
        && !recording_active
        && !recording_prompt_open
        && !recording_result_open
}

fn should_restore_active_recording_on_stop_error(err: &recording::RecordError) -> bool {
    matches!(err, recording::RecordError::StopFailed { .. })
        || matches!(
            err,
            recording::RecordError::CommandFailed { command, .. } if command == "kill -INT"
        )
}

pub struct App {
    machine: StateMachine,
}

impl App {
    pub fn new() -> Self {
        Self {
            machine: StateMachine::new(),
        }
    }

    pub fn start(&mut self, startup_override: Option<StartupConfig>) -> AppResult<()> {
        let bootstrap = bootstrap_app_runtime(startup_override);
        let mut startup_capture = bootstrap.startup_config.capture;
        let mut show_launchpad = bootstrap.startup_config.show_launchpad;
        let mut show_history = bootstrap.startup_config.show_history;
        let daemon_mode = bootstrap.startup_config.daemon_mode;
        let startup_remote_command = bootstrap.startup_config.remote_command;
        let print_status_json = bootstrap.startup_config.print_status_json;
        let theme_config = bootstrap.theme_config;
        let editor_navigation_bindings = bootstrap.editor_navigation_bindings;
        let app_config = jjaeng_core::config::load_app_config();

        if print_status_json {
            match jjaeng_core::service::read_status_snapshot_json() {
                Ok(json) => println!("{json}"),
                Err(err) => println!(
                    "{{\"state\":\"error\",\"message\":{}}}",
                    serde_json::to_string(&err.to_string())
                        .unwrap_or_else(|_| "\"failed to read status\"".to_string())
                ),
            }
            return Ok(());
        }

        let local_startup_remote_command = if daemon_mode {
            None
        } else if let Some(command) = startup_remote_command {
            match jjaeng_core::service::try_send_command(&command) {
                Ok(response) if response.ok => {
                    tracing::info!(message = %response.message, "dispatched remote command");
                    return Ok(());
                }
                Ok(response) => {
                    tracing::warn!(
                        message = %response.message,
                        "remote command rejected; falling back locally"
                    );
                    if startup_capture_from_remote_command(&command).is_none() {
                        show_launchpad = should_show_launchpad_for_remote_fallback(&command);
                        show_history = matches!(command, RemoteCommand::OpenHistory);
                        Some(command)
                    } else {
                        None
                    }
                }
                Err(err) => {
                    tracing::debug!(?err, "remote daemon unavailable; falling back locally");
                    if let Some(capture_mode) = startup_capture_from_remote_command(&command) {
                        startup_capture = capture_mode;
                        None
                    } else {
                        show_launchpad = should_show_launchpad_for_remote_fallback(&command);
                        show_history = matches!(command, RemoteCommand::OpenHistory);
                        Some(command)
                    }
                }
            }
        } else {
            None
        };

        tracing::info!(event = "start", from = ?self.machine.state());
        let _ = self.machine.transition(AppEvent::Start)?;

        let runtime_session = Rc::new(RefCell::new(RuntimeSession::default()));
        let shared_machine = Rc::new(RefCell::new(std::mem::take(&mut self.machine)));
        let storage_service = initialize_storage_service();
        let history_service = initialize_history_service();
        let (remote_command_tx, remote_command_rx) = mpsc::channel::<RemoteCommand>();
        let remote_command_rx = Rc::new(RefCell::new(remote_command_rx));

        tracing::info!("starting gtk runtime");
        let application = Application::new(Some(APP_ID), gtk4::gio::ApplicationFlags::NON_UNIQUE);

        let status_log = Rc::new(RefCell::new(String::from(
            "Ready. Capture to open preview/editor flow.",
        )));
        let status_log_for_activate = status_log.clone();
        let runtime_session_for_activate = runtime_session.clone();
        let machine_for_activate = shared_machine.clone();
        let runtime_window_state = Rc::new(RefCell::new(RuntimeWindowState::default()));
        let storage_service = Rc::new(storage_service);
        let storage_service_for_activate = storage_service.clone();
        let history_service = Rc::new(history_service);
        let history_service_for_activate = history_service.clone();
        let preview_windows = Rc::new(RefCell::new(HashMap::<String, PreviewWindowRuntime>::new()));
        let preview_action_target_capture_id = Rc::new(RefCell::new(None::<String>));
        let editor_runtime = Rc::new(EditorRuntimeState::new());
        let editor_window = Rc::new(RefCell::new(None::<ApplicationWindow>));
        let history_window = Rc::new(RefCell::new(None::<HistoryWindowRuntime>));
        let editor_capture_id = editor_runtime.capture_id.clone();
        let editor_has_unsaved_changes = editor_runtime.has_unsaved_changes.clone();
        let editor_close_dialog_open = editor_runtime.close_dialog_open.clone();
        let editor_input_mode = editor_runtime.input_mode.clone();
        let editor_toast = editor_runtime.toast.clone();
        let editor_close_guard = Rc::new(Cell::new(false));
        let editor_navigation_bindings = Rc::new(editor_navigation_bindings);
        let app_config_for_activate = app_config.clone();
        let headless_startup_capture = !show_launchpad
            && !show_history
            && !matches!(startup_capture, StartupCaptureMode::None);
        let headless_history_startup = !show_launchpad
            && local_startup_remote_command
                .as_ref()
                .is_some_and(is_history_remote_command);
        let headless_recording_startup = !show_launchpad
            && local_startup_remote_command
                .as_ref()
                .is_some_and(is_recording_remote_command);
        let activate_once = Rc::new(Cell::new(false));

        application.connect_activate(move |app| {
            if activate_once.replace(true) {
                tracing::debug!("ignoring duplicate gtk activate signal");
                return;
            }
            install_lucide_icon_theme();
            let headless_hold_guard =
                Rc::new(RefCell::new(None::<gtk4::gio::ApplicationHoldGuard>));
            let history_startup_hold_guard =
                Rc::new(RefCell::new(None::<gtk4::gio::ApplicationHoldGuard>));
            let startup_capture_completed = Rc::new(Cell::new(!headless_startup_capture));
            if headless_startup_capture || headless_recording_startup || daemon_mode {
                tracing::info!("holding app lifecycle for headless startup capture");
                let hold_guard =
                    <gtk4::Application as gtk4::gio::prelude::ApplicationExtManual>::hold(app);
                headless_hold_guard.borrow_mut().replace(hold_guard);
            }
            if headless_history_startup {
                tracing::info!("holding app lifecycle for headless history startup");
                let hold_guard =
                    <gtk4::Application as gtk4::gio::prelude::ApplicationExtManual>::hold(app);
                history_startup_hold_guard.borrow_mut().replace(hold_guard);
            }
            let gtk_settings = gtk4::Settings::default();
            let theme_mode = resolve_runtime_theme_mode(theme_config.mode, gtk_settings.as_ref());
            let resolved_theme_runtime = resolve_theme_runtime(&theme_config, theme_mode);
            let style_tokens = resolved_theme_runtime.style_tokens;
            let color_tokens = resolved_theme_runtime.color_tokens;
            let text_input_palette = resolved_theme_runtime.text_input_palette;
            let rectangle_border_radius_override = resolved_theme_runtime
                .editor_theme_overrides
                .rectangle_border_radius;
            let editor_selection_palette = resolved_theme_runtime
                .editor_theme_overrides
                .selection_palette;
            let default_tool_color_override = resolved_theme_runtime
                .editor_theme_overrides
                .default_tool_color;
            let default_text_size_override = resolved_theme_runtime
                .editor_theme_overrides
                .default_text_size;
            let default_stroke_width_override = resolved_theme_runtime
                .editor_theme_overrides
                .default_stroke_width;
            let editor_tool_option_presets = resolved_theme_runtime.editor_tool_option_presets;
            let ocr_language = resolved_theme_runtime.ocr_language;
            tracing::info!(
                requested_mode = ?theme_config.mode,
                resolved_mode = ?theme_mode,
                "resolved runtime theme mode"
            );
            let motion_enabled = gtk_settings
                .as_ref()
                .map(|settings| settings.is_gtk_enable_animations())
                .unwrap_or(true);
            let motion_hover_ms = if motion_enabled {
                style_tokens.motion_hover_ms
            } else {
                0
            };
            install_runtime_css(style_tokens, &color_tokens, motion_enabled);
            let window = ApplicationWindow::new(app);
            window.add_css_class(APP_CSS_ROOT);
            window.set_title(Some(APP_NAME));
            window.set_default_size(760, 520);

            let settings_info = {
                let theme_label = format!("{:?} → {:?}", theme_config.mode, theme_mode);
                let ocr_language_label = format!(
                    "{} ({})",
                    ocr_language.display_name(),
                    ocr_language.as_str()
                );
                let ocr_model_dir_label = jjaeng_core::ocr::resolve_model_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not found".to_string());
                let (xdg_config, home_dir) = jjaeng_core::config::config_env_dirs();
                let theme_config_path = jjaeng_core::config::existing_app_config_path(
                    "theme.json",
                    xdg_config.as_deref(),
                    home_dir.as_deref(),
                )
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unavailable".to_string());
                let config_path = jjaeng_core::config::existing_app_config_path(
                    "config.json",
                    xdg_config.as_deref(),
                    home_dir.as_deref(),
                )
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unavailable".to_string());
                let keybinding_config_path = jjaeng_core::config::existing_app_config_path(
                    "keybindings.json",
                    xdg_config.as_deref(),
                    home_dir.as_deref(),
                )
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unavailable".to_string());
                LaunchpadSettingsInfo {
                    theme_label,
                    ocr_language_label,
                    ocr_model_dir_label,
                    config_path,
                    theme_config_path,
                    keybinding_config_path,
                }
            };
            let recording_defaults = LaunchpadRecordingDefaults {
                size: app_config_for_activate
                    .recording_size
                    .unwrap_or(RecordingSize::Native),
                encoding: app_config_for_activate
                    .recording_encoding_preset
                    .unwrap_or(RecordingEncodingPreset::Standard),
                audio_mode: app_config_for_activate
                    .recording_audio_mode
                    .unwrap_or(AudioMode::Off),
                microphone_device: app_config_for_activate.recording_mic_device.clone(),
            };
            let launchpad = build_launchpad_ui(
                style_tokens,
                show_launchpad,
                &settings_info,
                recording_defaults,
            );
            let launchpad_toast_runtime = ToastRuntime::new(&launchpad.toast_label);
            let close_editor_button = launchpad.close_editor_button.clone();

            window.set_child(Some(&launchpad.root));
            let ocr_available = jjaeng_core::ocr::resolve_model_dir().is_some();
            let ocr_engine: Rc<RefCell<Option<jjaeng_core::ocr::OcrEngine>>> =
                Rc::new(RefCell::new(None));
            let ocr_in_progress = Rc::new(Cell::new(false));
            let recording_runtime = RecordingRuntimeState::default();
            let recording_prompt = Rc::new(RefCell::new(None::<RecordingPromptRuntime>));
            let recording_result = Rc::new(RefCell::new(None::<RecordingResultRuntime>));
            let recording_flow_pending = Rc::new(Cell::new(false));
            let app_for_preview = app.clone();
            let app_for_lifecycle = app.clone();
            let render_handle = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));
            let launchpad_actions = LaunchpadActionExecutor::new(
                runtime_session_for_activate.clone(),
                preview_action_target_capture_id.clone(),
                machine_for_activate.clone(),
                storage_service_for_activate.clone(),
                history_service_for_activate.clone(),
                status_log_for_activate.clone(),
                preview_windows.clone(),
                runtime_window_state.clone(),
                launchpad_toast_runtime.clone(),
                style_tokens.toast_duration_ms,
                ocr_engine.clone(),
                ocr_language,
                ocr_in_progress.clone(),
            );
            let preview_render_context = PreviewRenderContext::new(
                app_for_preview.clone(),
                style_tokens,
                motion_hover_ms,
                status_log_for_activate.clone(),
                preview_windows.clone(),
                preview_action_target_capture_id.clone(),
                launchpad_actions.clone(),
                render_handle.clone(),
                runtime_window_state.clone(),
                editor_window.clone(),
                editor_close_guard.clone(),
                editor_runtime.clone(),
                ocr_available,
            );
            let editor_render_context = EditorRenderContext {
                preview_windows: preview_windows.clone(),
                runtime_window_state: runtime_window_state.clone(),
                editor_window: editor_window.clone(),
                editor_capture_id: editor_capture_id.clone(),
                editor_close_guard: editor_close_guard.clone(),
                editor_runtime: editor_runtime.clone(),
                app_for_preview: app_for_preview.clone(),
                motion_hover_ms,
                runtime_session: runtime_session_for_activate.clone(),
                style_tokens,
                theme_mode,
                editor_selection_palette,
                text_input_palette,
                rectangle_border_radius_override,
                default_tool_color_override,
                default_text_size_override,
                default_stroke_width_override,
                editor_tool_option_presets: editor_tool_option_presets.clone(),
                editor_navigation_bindings: editor_navigation_bindings.clone(),
                status_log_for_render: status_log_for_activate.clone(),
                editor_input_mode: editor_input_mode.clone(),
                editor_has_unsaved_changes: editor_has_unsaved_changes.clone(),
                editor_close_dialog_open: editor_close_dialog_open.clone(),
                editor_toast: editor_toast.clone(),
                close_editor_button: close_editor_button.clone(),
                storage_service: storage_service_for_activate.clone(),
                history_service: history_service_for_activate.clone(),
                shared_machine: machine_for_activate.clone(),
                ocr_engine: ocr_engine.clone(),
                ocr_language,
                ocr_in_progress: ocr_in_progress.clone(),
                ocr_available,
            };
            let history_render_context = HistoryRenderContext {
                app: app_for_preview.clone(),
                style_tokens,
                status_log: status_log_for_activate.clone(),
                runtime_session: runtime_session_for_activate.clone(),
                shared_machine: machine_for_activate.clone(),
                storage_service: storage_service_for_activate.clone(),
                history_service: history_service_for_activate.clone(),
                editor_has_unsaved_changes: editor_has_unsaved_changes.clone(),
                history_window: history_window.clone(),
            };

            let render: Rc<dyn Fn()> = {
                let runtime_session = runtime_session_for_activate.clone();
                let shared_machine = machine_for_activate.clone();
                let launchpad = launchpad.clone();
                let runtime_window_state = runtime_window_state.clone();
                let preview_windows = preview_windows.clone();
                let preview_render_context = preview_render_context.clone();
                let editor_render_context = editor_render_context.clone();
                let editor_runtime = editor_runtime.clone();
                let editor_window = editor_window.clone();
                let editor_close_guard = editor_close_guard.clone();
                let status_log_for_render = status_log_for_activate.clone();
                let app_for_lifecycle = app_for_lifecycle.clone();
                let headless_hold_guard = headless_hold_guard.clone();
                let startup_capture_completed = startup_capture_completed.clone();
                let history_render_context = history_render_context.clone();
                let render_handle = render_handle.clone();
                let recording_runtime = recording_runtime.clone();
                let recording_prompt = recording_prompt.clone();
                let recording_result = recording_result.clone();
                let recording_flow_pending = recording_flow_pending.clone();

                Rc::new(move || {
                    let runtime = runtime_session.borrow();
                    let state = shared_machine.borrow().state();
                    let has_capture = runtime.active_capture().is_some();
                    let active_capture = runtime.active_capture().cloned();
                    let captures = runtime.captures_for_display();
                    let ids = runtime.ids_for_display();
                    let recording_available = recording::recording_backend_available();
                    let active_capture_id = active_capture
                        .as_ref()
                        .map(|artifact| artifact.capture_id.clone())
                        .unwrap_or_else(|| "none".to_string());

                    launchpad.update_overview(
                        state,
                        &active_capture_id,
                        &runtime.latest_label_text(),
                        &ids,
                    );
                    launchpad.set_action_availability(
                        state,
                        has_capture,
                        ocr_available,
                        recording_available,
                    );

                    match state {
                        AppState::Preview => {
                            render_preview_state(&preview_render_context, &captures);
                        }
                        AppState::Editor => {
                            render_editor_state(&editor_render_context, active_capture.clone());
                        }
                        _ => {
                            close_all_preview_windows(&preview_windows, &runtime_window_state);
                            close_editor_if_open_and_clear(
                                &editor_window,
                                &runtime_window_state,
                                &editor_close_guard,
                                &editor_runtime,
                                style_tokens,
                            );
                        }
                    }

                    launchpad.set_status_text(status_log_for_render.borrow().as_str());
                    if let Some(render) = render_handle.borrow().as_ref() {
                        refresh_history_window_if_open(&history_render_context, render);
                    }

                    let preview_window_count = preview_windows.borrow().len();
                    let editor_window_open = editor_window.borrow().is_some();
                    let recording_active = recording_runtime.is_active();
                    let recording_paused = recording_runtime.is_paused();
                    launchpad.update_recording_overview(
                        recording_available,
                        recording_active,
                        recording_paused,
                        recording_runtime.elapsed_ms.get(),
                    );
                    sync_recording_prompt(
                        &recording_prompt,
                        recording_active,
                        recording_paused,
                        recording_runtime.elapsed_ms.get(),
                    );
                    let recording_result_window_open =
                        recording_result_open(&recording_result);
                    let recording_prompt_open =
                        recording_flow_pending.get() || recording_prompt_open(&recording_prompt);
                    let status_snapshot = StatusSnapshot {
                        state: format!("{state:?}").to_ascii_lowercase(),
                        active_capture_id: active_capture
                            .as_ref()
                            .map(|artifact| artifact.capture_id.clone()),
                        latest_label: runtime.latest_label_text(),
                        capture_count: captures.len(),
                        preview_count: preview_window_count,
                        editor_open: editor_window_open,
                        recording: recording_active,
                        recording_duration_ms: recording_active
                            .then_some(recording_runtime.elapsed_ms.get()),
                        recording_id: recording_runtime.active_recording_id(),
                    };
                    if let Err(err) = jjaeng_core::service::write_status_snapshot(&status_snapshot)
                    {
                        tracing::debug!(?err, "failed to write status snapshot");
                    }
                    if !daemon_mode
                        && should_release_headless_startup_hold(
                            headless_hold_guard.borrow().is_some(),
                            startup_capture_completed.get(),
                            state,
                            has_capture,
                            preview_window_count,
                            editor_window_open,
                            recording_active,
                            recording_prompt_open,
                            recording_result_window_open,
                        )
                    {
                        tracing::info!("releasing headless startup capture hold");
                        let _ = headless_hold_guard.borrow_mut().take();
                        app_for_lifecycle.quit();
                    }
                })
            };
            render_handle.borrow_mut().replace(render.clone());

            let open_history_window: Rc<dyn Fn()> = {
                let history_render_context = history_render_context.clone();
                let render_handle = render_handle.clone();
                Rc::new(move || {
                    if let Some(render) = render_handle.borrow().as_ref() {
                        present_history_window(&history_render_context, render);
                    }
                })
            };

            let toggle_history_window: Rc<dyn Fn()> = {
                let history_render_context = history_render_context.clone();
                let render_handle = render_handle.clone();
                Rc::new(move || {
                    if let Some(render) = render_handle.borrow().as_ref() {
                        toggle_history_window(&history_render_context, render);
                    }
                })
            };

            let begin_recording: Rc<dyn Fn(RecordingRequest, Option<RecordingSelection>, bool)> = {
                let machine = machine_for_activate.clone();
                let status_log = status_log_for_activate.clone();
                let recording_runtime = recording_runtime.clone();
                let recording_prompt = recording_prompt.clone();
                let recording_result = recording_result.clone();
                let recording_flow_pending = recording_flow_pending.clone();
                let render_handle = render_handle.clone();
                Rc::new(
                    move |request: RecordingRequest,
                          selection: Option<RecordingSelection>,
                          preserve_prompt: bool| {
                        if recording_runtime.is_active() {
                            recording_flow_pending.set(false);
                            *status_log.borrow_mut() = "recording already active".to_string();
                            if preserve_prompt {
                                set_recording_prompt_error(
                                    &recording_prompt,
                                    "Recording already active",
                                );
                            }
                            if let Some(render) = render_handle.borrow().as_ref() {
                                (render.as_ref())();
                            }
                            return;
                        }
                        if !matches!(machine.borrow().state(), AppState::Idle) {
                            recording_flow_pending.set(false);
                            *status_log.borrow_mut() =
                                "recording can only start from idle state".to_string();
                            if preserve_prompt {
                                set_recording_prompt_error(
                                    &recording_prompt,
                                    "Recording can only start from idle",
                                );
                            }
                            if let Some(render) = render_handle.borrow().as_ref() {
                                (render.as_ref())();
                            }
                            return;
                        }
                        if !recording::recording_backend_available() {
                            recording_flow_pending.set(false);
                            let message = recording::recording_backend_requirement_message();
                            *status_log.borrow_mut() = message.clone();
                            if preserve_prompt {
                                set_recording_prompt_error(&recording_prompt, &message);
                            }
                            jjaeng_core::notification::send(message);
                            if let Some(render) = render_handle.borrow().as_ref() {
                                (render.as_ref())();
                            }
                            return;
                        }

                        dismiss_recording_result(&recording_result);
                        if preserve_prompt {
                            set_recording_prompt_starting(&recording_prompt);
                        } else {
                            dismiss_recording_prompt(&recording_prompt);
                        }
                        recording_flow_pending.set(true);

                        *status_log.borrow_mut() =
                            format!("starting {:?} recording", request.target);
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }

                        let machine = machine.clone();
                        let status_log = status_log.clone();
                        let recording_runtime = recording_runtime.clone();
                        let recording_prompt = recording_prompt.clone();
                        let recording_flow_pending = recording_flow_pending.clone();
                        let render_handle = render_handle.clone();
                        let target = request.target;
                        spawn_worker_action(
                            move || match selection.as_ref() {
                                Some(selection) => {
                                    recording::start_recording_selected(&request, selection)
                                }
                                None => recording::start_recording(&request),
                            },
                            move |result| {
                                match result {
                                    Ok(handle) => {
                                        recording_flow_pending.set(false);
                                        if let Err(err) =
                                            machine.borrow_mut().transition(AppEvent::StartRecording)
                                        {
                                            *status_log.borrow_mut() =
                                                format!("cannot enter recording state: {err}");
                                            if preserve_prompt {
                                                set_recording_prompt_error(
                                                    &recording_prompt,
                                                    "Failed to enter recording state",
                                                );
                                            }
                                        } else {
                                            recording_runtime.elapsed_ms.set(0);
                                            let active_recording = recording_runtime.active.clone();
                                            let elapsed_ms =
                                                recording_runtime.elapsed_ms.clone();
                                            let render =
                                                render_handle.borrow().as_ref().cloned();
                                            let timer_source_id = gtk4::glib::timeout_add_local(
                                                Duration::from_millis(250),
                                                move || {
                                                    let active_slot = active_recording.borrow();
                                                    let Some(active) = active_slot.as_ref() else {
                                                        return gtk4::glib::ControlFlow::Break;
                                                    };
                                                    elapsed_ms.set(active.elapsed_ms());
                                                    if let Some(render) = render.as_ref() {
                                                        (render.as_ref())();
                                                    }
                                                    gtk4::glib::ControlFlow::Continue
                                                },
                                            );
                                            recording_runtime.active.borrow_mut().replace(
                                                ActiveRecording {
                                                    handle,
                                                    timer_source_id,
                                                    started_at: Instant::now(),
                                                    paused_at: None,
                                                    paused_total_ms: 0,
                                                },
                                            );
                                            *status_log.borrow_mut() =
                                                format!("{target:?} recording active");
                                            jjaeng_core::notification::send("Recording started");
                                        }
                                    }
                                    Err(err) => {
                                        recording_flow_pending.set(false);
                                        *status_log.borrow_mut() =
                                            format!("recording failed to start: {err}");
                                        if preserve_prompt {
                                            set_recording_prompt_error(
                                                &recording_prompt,
                                                &format!("Failed to start: {err}"),
                                            );
                                        }
                                        jjaeng_core::notification::send(format!(
                                            "Recording failed: {err}"
                                        ));
                                    }
                                }
                                if let Some(render) = render_handle.borrow().as_ref() {
                                    (render.as_ref())();
                                }
                            },
                        );
                    },
                )
            };
            let start_recording_selected_prompt: Rc<
                dyn Fn(RecordingRequest, RecordingSelection),
            > = {
                let begin_recording = begin_recording.clone();
                Rc::new(move |request, selection| {
                    (begin_recording.as_ref())(request, Some(selection), true);
                })
            };
            let start_recording: Rc<dyn Fn(RecordingRequest)> = {
                let begin_recording = begin_recording.clone();
                Rc::new(move |request| {
                    (begin_recording.as_ref())(request, None, false);
                })
            };
            let pause_recording_toggle: Rc<dyn Fn()> = {
                let machine = machine_for_activate.clone();
                let status_log = status_log_for_activate.clone();
                let recording_runtime = recording_runtime.clone();
                let render_handle = render_handle.clone();
                Rc::new(move || {
                    if !matches!(machine.borrow().state(), AppState::Recording) {
                        *status_log.borrow_mut() =
                            "pause recording requires recording state".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    }

                    let mut active_slot = recording_runtime.active.borrow_mut();
                    let Some(active) = active_slot.as_mut() else {
                        *status_log.borrow_mut() = "no active recording".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    };

                    let result = if active.is_paused() {
                        recording::resume_recording(&active.handle).map(|_| {
                            active.mark_resumed();
                            *status_log.borrow_mut() = "recording resumed".to_string();
                            jjaeng_core::notification::send("Recording resumed");
                        })
                    } else {
                        recording::pause_recording(&active.handle).map(|_| {
                            active.mark_paused();
                            *status_log.borrow_mut() = "recording paused".to_string();
                            jjaeng_core::notification::send("Recording paused");
                        })
                    };

                    match result {
                        Ok(()) => {
                            recording_runtime.elapsed_ms.set(active.elapsed_ms());
                        }
                        Err(err) => {
                            *status_log.borrow_mut() =
                                format!("recording pause toggle failed: {err}");
                            jjaeng_core::notification::send(format!(
                                "Recording control failed: {err}"
                            ));
                        }
                    }

                    if let Some(render) = render_handle.borrow().as_ref() {
                        (render.as_ref())();
                    }
                })
            };
            let stop_recording_slot = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));
            let prompt_recording: Rc<dyn Fn(RecordingRequest)> = {
                let app = app_for_preview.clone();
                let machine = machine_for_activate.clone();
                let status_log = status_log_for_activate.clone();
                let recording_runtime = recording_runtime.clone();
                let recording_prompt = recording_prompt.clone();
                let recording_result = recording_result.clone();
                let recording_flow_pending = recording_flow_pending.clone();
                let render_handle = render_handle.clone();
                let start_recording_selected_prompt = start_recording_selected_prompt.clone();
                let pause_recording_toggle = pause_recording_toggle.clone();
                let stop_recording_slot = stop_recording_slot.clone();
                Rc::new(move |request: RecordingRequest| {
                    tracing::debug!(target = ?request.target, "prompt recording requested");
                    if recording_runtime.is_active() {
                        *status_log.borrow_mut() = "recording already active".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    }
                    if !matches!(machine.borrow().state(), AppState::Idle) {
                        *status_log.borrow_mut() =
                            "recording can only start from idle state".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    }
                    if !recording::recording_backend_available() {
                        let message = recording::recording_backend_requirement_message();
                        *status_log.borrow_mut() = message.clone();
                        jjaeng_core::notification::send(message);
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    }

                    dismiss_recording_prompt(&recording_prompt);
                    dismiss_recording_result(&recording_result);
                    recording_flow_pending.set(true);
                    *status_log.borrow_mut() =
                        format!("select {:?} recording area", request.target);
                    if let Some(render) = render_handle.borrow().as_ref() {
                        (render.as_ref())();
                    }

                    let app = app.clone();
                    let status_log = status_log.clone();
                    let recording_prompt = recording_prompt.clone();
                    let recording_flow_pending = recording_flow_pending.clone();
                    let render_handle = render_handle.clone();
                    let start_recording_selected_prompt = start_recording_selected_prompt.clone();
                    let pause_recording_toggle = pause_recording_toggle.clone();
                    let stop_recording_slot = stop_recording_slot.clone();
                    spawn_worker_action(
                        move || recording::resolve_recording_selection(request.target),
                        move |result| {
                            match result {
                                Ok(selection) => {
                                    tracing::debug!(
                                        target = ?request.target,
                                        selection = ?selection,
                                        "recording selection resolved"
                                    );
                                    *status_log.borrow_mut() =
                                        "review recording settings".to_string();
                                    let microphone_sources =
                                        recording::list_microphone_sources().unwrap_or_default();
                                    let confirm: Rc<
                                        dyn Fn(RecordingRequest, RecordingSelection),
                                    > = {
                                        let start_recording_selected_prompt =
                                            start_recording_selected_prompt.clone();
                                        Rc::new(move |request, selection| {
                                            (start_recording_selected_prompt.as_ref())(
                                                request,
                                                selection,
                                            );
                                        })
                                    };
                                    let cancel: Rc<dyn Fn()> = {
                                        let status_log = status_log.clone();
                                        let recording_prompt = recording_prompt.clone();
                                        let recording_flow_pending =
                                            recording_flow_pending.clone();
                                        let render_handle = render_handle.clone();
                                        Rc::new(move || {
                                            dismiss_recording_prompt(&recording_prompt);
                                            recording_flow_pending.set(false);
                                            *status_log.borrow_mut() =
                                                "recording cancelled".to_string();
                                            if let Some(render) = render_handle.borrow().as_ref() {
                                                (render.as_ref())();
                                            }
                                        })
                                    };
                                    let stop: Rc<dyn Fn()> = {
                                        let stop_recording_slot = stop_recording_slot.clone();
                                        Rc::new(move || {
                                            if let Some(stop_recording) =
                                                stop_recording_slot.borrow().as_ref()
                                            {
                                                (stop_recording.as_ref())();
                                            }
                                        })
                                    };
                                    present_recording_prompt(
                                        &app,
                                        style_tokens,
                                        &recording_prompt,
                                        &request,
                                        &selection,
                                        &microphone_sources,
                                        &confirm,
                                        &cancel,
                                        &pause_recording_toggle,
                                        &stop,
                                    );
                                    recording_flow_pending.set(false);
                                }
                                Err(err) => {
                                    tracing::debug!(
                                        target = ?request.target,
                                        ?err,
                                        "recording selection failed"
                                    );
                                    recording_flow_pending.set(false);
                                    *status_log.borrow_mut() =
                                        format!("recording selection failed: {err}");
                                    jjaeng_core::notification::send(format!(
                                        "Recording selection failed: {err}"
                                    ));
                                }
                            }
                            if let Some(render) = render_handle.borrow().as_ref() {
                                (render.as_ref())();
                            }
                        },
                    );
                })
            };
            let stop_recording: Rc<dyn Fn()> = {
                let app = app_for_preview.clone();
                let machine = machine_for_activate.clone();
                let status_log = status_log_for_activate.clone();
                let recording_runtime = recording_runtime.clone();
                let recording_prompt = recording_prompt.clone();
                let recording_result = recording_result.clone();
                let history_service = history_service_for_activate.clone();
                let storage_service = storage_service_for_activate.clone();
                let render_handle = render_handle.clone();
                Rc::new(move || {
                    if !matches!(machine.borrow().state(), AppState::Recording) {
                        *status_log.borrow_mut() =
                            "stop recording requires recording state".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    }

                    let Some(mut active) = recording_runtime.active.borrow_mut().take() else {
                        *status_log.borrow_mut() = "no active recording".to_string();
                        if let Some(render) = render_handle.borrow().as_ref() {
                            (render.as_ref())();
                        }
                        return;
                    };

                    if active.is_paused() {
                        if let Err(err) = recording::resume_recording(&active.handle) {
                            *status_log.borrow_mut() =
                                format!("recording stop failed: {err}");
                            recording_runtime.active.borrow_mut().replace(active);
                            if let Some(render) = render_handle.borrow().as_ref() {
                                (render.as_ref())();
                            }
                            return;
                        }
                        active.mark_resumed();
                    }

                    match recording::stop_recording(&mut active.handle) {
                        Ok(artifact) => {
                            active.timer_source_id.remove();
                            recording_runtime.elapsed_ms.set(0);
                            dismiss_recording_prompt(&recording_prompt);

                            let mut history_entry = None;
                            let mut direct_saved_path = None;
                            if let Some(history_service) = history_service.as_ref().as_ref() {
                                match history_service.record_recording(&artifact) {
                                    Ok(entry) => {
                                        history_entry = Some(entry);
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            recording_id = %artifact.recording_id,
                                            ?err,
                                            "failed to record finished video in history"
                                        );
                                    }
                                }
                            }
                            if history_entry.is_none() {
                                if let Some(storage_service) = storage_service.as_ref().as_ref() {
                                    match storage_service.save_recording(&artifact) {
                                        Ok(saved_path) => {
                                            direct_saved_path = Some(saved_path);
                                        }
                                        Err(err) => {
                                            tracing::warn!(
                                                recording_id = %artifact.recording_id,
                                                ?err,
                                                "failed to persist finished recording"
                                            );
                                        }
                                    }
                                }
                            }

                            let mut result_output_path = artifact.output_path.clone();
                            let mut result_thumbnail_path = artifact.thumbnail_path.clone();
                            let mut result_saved_path = direct_saved_path.clone();
                            let cleanup_output_on_close = false;
                            let mut cleanup_thumbnail_on_close = false;
                            let save_source_path = if let Some(entry) = history_entry.as_ref() {
                                result_output_path = entry.media_path.clone();
                                result_thumbnail_path = entry.thumbnail_path.clone();
                                result_saved_path = entry.saved_path.clone();
                                if artifact.output_path != entry.media_path {
                                    let _ = std::fs::remove_file(&artifact.output_path);
                                }
                                if artifact.thumbnail_path != entry.thumbnail_path {
                                    let _ = std::fs::remove_file(&artifact.thumbnail_path);
                                }
                                entry.media_path.clone()
                            } else if let Some(saved_path) = direct_saved_path.as_ref() {
                                result_output_path = saved_path.clone();
                                if artifact.output_path != *saved_path {
                                    let _ = std::fs::remove_file(&artifact.output_path);
                                }
                                cleanup_thumbnail_on_close = artifact.thumbnail_path.exists();
                                artifact.output_path.clone()
                            } else {
                                artifact.output_path.clone()
                            };

                            let persisted = history_entry.is_some() || direct_saved_path.is_some();
                            if let Err(err) =
                                machine.borrow_mut().transition(AppEvent::StopRecording)
                            {
                                *status_log.borrow_mut() =
                                    format!("cannot leave recording state: {err}");
                            } else if persisted {
                                *status_log.borrow_mut() =
                                    format!("recording saved {}", artifact.recording_id);
                            } else {
                                *status_log.borrow_mut() = format!(
                                    "recording stopped but could not be persisted: {}",
                                    artifact.output_path.display()
                                );
                            }

                            if result_output_path.exists() && result_thumbnail_path.exists() {
                                let current_display_path = Rc::new(RefCell::new(
                                    result_saved_path
                                        .clone()
                                        .unwrap_or_else(|| result_output_path.clone()),
                                ));
                                let can_save_from_temp =
                                    save_source_path == artifact.output_path && artifact.output_path.exists();
                                let temp_thumbnail_kept =
                                    result_thumbnail_path == artifact.thumbnail_path
                                        && artifact.thumbnail_path.exists();
                                let recording_id = artifact.recording_id.clone();
                                let on_save: Rc<dyn Fn()> = {
                                    let status_log = status_log.clone();
                                    let storage_service = storage_service.clone();
                                    let history_service = history_service.clone();
                                    let recording_result = recording_result.clone();
                                    let render_handle = render_handle.clone();
                                    let current_display_path = current_display_path.clone();
                                    let save_source_path = save_source_path.clone();
                                    let recording_id = recording_id.clone();
                                    Rc::new(move || {
                                        let Some(storage_service) =
                                            storage_service.as_ref().as_ref()
                                        else {
                                            *status_log.borrow_mut() =
                                                "storage service unavailable".to_string();
                                            jjaeng_core::notification::send(
                                                "Save failed: storage unavailable",
                                            );
                                            if let Some(render) = render_handle.borrow().as_ref() {
                                                (render.as_ref())();
                                            }
                                            return;
                                        };

                                        let extension = save_source_path
                                            .extension()
                                            .and_then(|value| value.to_str())
                                            .unwrap_or("mp4");
                                        match storage_service.save_recording_path(
                                            &recording_id,
                                            &save_source_path,
                                            extension,
                                        ) {
                                            Ok(saved_path) => {
                                                if let Some(history_service) =
                                                    history_service.as_ref().as_ref()
                                                {
                                                    if let Err(err) = history_service
                                                        .mark_saved(&recording_id, &saved_path)
                                                    {
                                                        tracing::warn!(
                                                            recording_id = %recording_id,
                                                            ?err,
                                                            "failed to update history entry saved path"
                                                        );
                                                    }
                                                }
                                                *current_display_path.borrow_mut() =
                                                    saved_path.clone();
                                                set_recording_result_saved_path(
                                                    &recording_result,
                                                    &saved_path,
                                                    can_save_from_temp,
                                                    temp_thumbnail_kept,
                                                );
                                                *status_log.borrow_mut() =
                                                    format!("saved {recording_id}");
                                                jjaeng_core::notification::send(format!(
                                                    "Saved {recording_id}"
                                                ));
                                            }
                                            Err(err) => {
                                                *status_log.borrow_mut() = format!(
                                                    "save failed for {recording_id}: {err}"
                                                );
                                                jjaeng_core::notification::send(format!(
                                                    "Save failed: {err}"
                                                ));
                                            }
                                        }

                                        if let Some(render) = render_handle.borrow().as_ref() {
                                            (render.as_ref())();
                                        }
                                    })
                                };
                                let on_copy: Rc<dyn Fn()> = {
                                    let status_log = status_log.clone();
                                    let render_handle = render_handle.clone();
                                    let current_display_path = current_display_path.clone();
                                    let recording_id = recording_id.clone();
                                    Rc::new(move || {
                                        let path = current_display_path.borrow().clone();
                                        match WlCopyBackend.copy(&path) {
                                            Ok(()) => {
                                                *status_log.borrow_mut() =
                                                    format!("copied {recording_id}");
                                                jjaeng_core::notification::send(format!(
                                                    "Copied {recording_id}"
                                                ));
                                            }
                                            Err(err) => {
                                                *status_log.borrow_mut() = format!(
                                                    "copy failed for {recording_id}: {err}"
                                                );
                                                jjaeng_core::notification::send(format!(
                                                    "Copy failed: {err}"
                                                ));
                                            }
                                        }

                                        if let Some(render) = render_handle.borrow().as_ref() {
                                            (render.as_ref())();
                                        }
                                    })
                                };
                                let on_open: Rc<dyn Fn()> = {
                                    let status_log = status_log.clone();
                                    let render_handle = render_handle.clone();
                                    let current_display_path = current_display_path.clone();
                                    let recording_id = recording_id.clone();
                                    Rc::new(move || {
                                        let path = current_display_path.borrow().clone();
                                        match Command::new("xdg-open").arg(&path).spawn() {
                                            Ok(_) => {
                                                *status_log.borrow_mut() =
                                                    format!("opened {recording_id}");
                                            }
                                            Err(err) => {
                                                *status_log.borrow_mut() = format!(
                                                    "open failed for {recording_id}: {err}"
                                                );
                                                jjaeng_core::notification::send(format!(
                                                    "Open failed: {err}"
                                                ));
                                            }
                                        }

                                        if let Some(render) = render_handle.borrow().as_ref() {
                                            (render.as_ref())();
                                        }
                                    })
                                };
                                let on_close: Rc<dyn Fn()> = {
                                    let recording_result = recording_result.clone();
                                    let render_handle = render_handle.clone();
                                    Rc::new(move || {
                                        dismiss_recording_result(&recording_result);
                                        if let Some(render) = render_handle.borrow().as_ref() {
                                            (render.as_ref())();
                                        }
                                    })
                                };
                                let result_artifact = RecordingResultArtifact {
                                    recording_id: artifact.recording_id.clone(),
                                    output_path: result_output_path.clone(),
                                    thumbnail_path: result_thumbnail_path.clone(),
                                    width: artifact.width,
                                    height: artifact.height,
                                    duration_ms: artifact.duration_ms,
                                    saved_path: result_saved_path.clone(),
                                    cleanup_output_on_close,
                                    cleanup_thumbnail_on_close,
                                };
                                present_recording_result(
                                    &app,
                                    style_tokens,
                                    &recording_result,
                                    &result_artifact,
                                    &on_save,
                                    &on_copy,
                                    &on_open,
                                    &on_close,
                                );
                                if persisted {
                                    jjaeng_core::notification::send("Recording ready");
                                } else {
                                    jjaeng_core::notification::send(format!(
                                        "Recording stopped; file kept at {}",
                                        artifact.output_path.display()
                                    ));
                                }
                            } else if persisted {
                                jjaeng_core::notification::send("Recording stopped");
                            } else {
                                jjaeng_core::notification::send(format!(
                                    "Recording stopped; file kept at {}",
                                    artifact.output_path.display()
                                ));
                            }
                        }
                        Err(err) => {
                            if should_restore_active_recording_on_stop_error(&err) {
                                *status_log.borrow_mut() =
                                    format!("recording stop failed: {err}");
                                recording_runtime.active.borrow_mut().replace(active);
                                jjaeng_core::notification::send(format!(
                                    "Recording stop failed: {err}"
                                ));
                            } else {
                                let output_path = active.handle.output_path.clone();
                                let output_exists = output_path.exists();
                                active.timer_source_id.remove();
                                recording_runtime.elapsed_ms.set(0);
                                dismiss_recording_prompt(&recording_prompt);
                                if let Err(transition_err) =
                                    machine.borrow_mut().transition(AppEvent::StopRecording)
                                {
                                    *status_log.borrow_mut() = format!(
                                        "recording stopped but state reset failed: {transition_err}"
                                    );
                                } else if output_exists {
                                    *status_log.borrow_mut() = format!(
                                        "recording stopped but finalization failed; file kept at {}",
                                        output_path.display()
                                    );
                                } else {
                                    *status_log.borrow_mut() =
                                        format!("recording stopped but no output was produced: {err}");
                                }
                                jjaeng_core::notification::send(if output_exists {
                                    format!(
                                        "Recording stopped; file kept at {}",
                                        output_path.display()
                                    )
                                } else {
                                    format!("Recording stopped but finalization failed: {err}")
                                });
                            }
                        }
                    }

                    if let Some(render) = render_handle.borrow().as_ref() {
                        (render.as_ref())();
                    }
                })
            };
            stop_recording_slot
                .borrow_mut()
                .replace(stop_recording.clone());
            connect_launchpad_default_buttons(
                &launchpad,
                &launchpad_actions,
                &open_history_window,
                &start_recording,
                &stop_recording,
                &render,
            );

            if daemon_mode {
                jjaeng_core::service::spawn_command_server(remote_command_tx.clone());
                let remote_command_rx = remote_command_rx.clone();
                let launchpad_actions = launchpad_actions.clone();
                let open_history_window = open_history_window.clone();
                let toggle_history_window = toggle_history_window.clone();
                let start_recording_for_remote = start_recording.clone();
                let prompt_recording_for_remote = prompt_recording.clone();
                let stop_recording_for_remote = stop_recording.clone();
                let render = render.clone();
                gtk4::glib::timeout_add_local(Duration::from_millis(40), move || {
                    while let Ok(command) = remote_command_rx.borrow_mut().try_recv() {
                        dispatch_remote_command(
                            &launchpad_actions,
                            &open_history_window,
                            &toggle_history_window,
                            &start_recording_for_remote,
                            &prompt_recording_for_remote,
                            &stop_recording_for_remote,
                            command,
                            &render,
                        );
                    }
                    gtk4::glib::ControlFlow::Continue
                });
            }

            if local_startup_remote_command.is_none() {
                let render = render.clone();
                render();
            }

            run_startup_capture(&launchpad_actions, startup_capture, {
                let render = render.clone();
                let startup_capture_completed = startup_capture_completed.clone();
                move || {
                    startup_capture_completed.set(true);
                    (render.as_ref())();
                }
            });

            if let Some(command) = local_startup_remote_command.clone() {
                dispatch_remote_command(
                    &launchpad_actions,
                    &open_history_window,
                    &toggle_history_window,
                    &start_recording,
                    &prompt_recording,
                    &stop_recording,
                    command,
                    &render,
                );
                if headless_history_startup {
                    let history_startup_hold_guard = history_startup_hold_guard.clone();
                    gtk4::glib::idle_add_local_once(move || {
                        let _ = history_startup_hold_guard.borrow_mut().take();
                    });
                }
            }

            tracing::info!("presenting startup launcher window");
            if show_launchpad {
                window.present();
            } else {
                tracing::debug!("keeping hidden startup window available for async prompts");
            }
        });

        // Pass only argv[0] to GTK so app-specific flags (e.g. --launchpad) do not fail GTK parsing.
        let gtk_args = gtk_launch_args();
        application.run_with_args(&gtk_args);

        let remaining_capture_ids = runtime_session.borrow().ids_for_display();
        cleanup_remaining_session_artifacts(
            storage_service.as_ref().as_ref(),
            &remaining_capture_ids,
        );

        self.machine = std::mem::take(&mut *shared_machine.borrow_mut());
        Ok(())
    }

    pub fn state(&self) -> &StateMachine {
        &self.machine
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::editor_viewport::{
        zoom_percent_from_slider_value, zoom_slider_value_for_percent, ZOOM_SLIDER_STEPS,
    };
    #[test]
    fn zoom_slider_mapping_preserves_min_max_bounds() {
        assert_eq!(
            zoom_percent_from_slider_value(0.0),
            editor::EditorViewport::min_zoom_percent()
        );
        assert_eq!(
            zoom_percent_from_slider_value(ZOOM_SLIDER_STEPS),
            editor::EditorViewport::max_zoom_percent()
        );
        assert_eq!(
            zoom_slider_value_for_percent(editor::EditorViewport::min_zoom_percent()),
            0.0
        );
        assert_eq!(
            zoom_slider_value_for_percent(editor::EditorViewport::max_zoom_percent()),
            ZOOM_SLIDER_STEPS
        );
    }

    #[test]
    fn zoom_slider_mapping_round_trip_stays_near_input_levels() {
        for zoom_percent in [5_u16, 25, 50, 100, 200, 400, 800, 1600] {
            let slider_value = zoom_slider_value_for_percent(zoom_percent);
            let mapped = zoom_percent_from_slider_value(slider_value);
            assert!(
                (i32::from(mapped) - i32::from(zoom_percent)).abs() <= 1,
                "zoom_percent={zoom_percent}, mapped={mapped}, slider_value={slider_value}"
            );
        }
    }

    #[test]
    fn editor_window_geometry_helpers_match_style_tokens() {
        let tokens = crate::ui::LAYOUT_TOKENS;
        assert_eq!(
            editor_window_default_geometry(tokens),
            RuntimeWindowGeometry::new(tokens.editor_initial_width, tokens.editor_initial_height)
        );
        assert_eq!(
            editor_window_min_geometry(tokens),
            RuntimeWindowGeometry::new(tokens.editor_min_width, tokens.editor_min_height)
        );
    }

    #[test]
    fn reset_editor_session_state_clears_dirty_flags_and_modes() {
        let editor_runtime = EditorRuntimeState::new();
        *editor_runtime.has_unsaved_changes.borrow_mut() = true;
        *editor_runtime.close_dialog_open.borrow_mut() = true;
        editor_runtime.input_mode.borrow_mut().activate_crop();

        reset_editor_session_state(&editor_runtime);

        assert!(!*editor_runtime.has_unsaved_changes.borrow());
        assert!(!*editor_runtime.close_dialog_open.borrow());
        assert!(!editor_runtime.input_mode.borrow().crop_active());
        assert!(!editor_runtime.input_mode.borrow().text_input_active());
    }

    #[test]
    fn format_capture_ids_for_display_returns_none_for_empty_ids() {
        assert_eq!(format_capture_ids_for_display(&[]), "IDs: none");
    }

    #[test]
    fn format_capture_ids_for_display_numbers_each_capture_id() {
        let ids = vec!["capture-a".to_string(), "capture-b".to_string()];
        assert_eq!(
            format_capture_ids_for_display(&ids),
            "IDs:\n 1. capture-a\n 2. capture-b"
        );
    }

    #[test]
    fn shortcut_editor_tool_switch_maps_tool_shortcuts() {
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterSelect),
            Some((ToolKind::Select, "editor select tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterPan),
            Some((ToolKind::Pan, "editor pan tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterBlur),
            Some((ToolKind::Blur, "editor blur tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterPen),
            Some((ToolKind::Pen, "editor pen tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterArrow),
            Some((ToolKind::Arrow, "editor arrow tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterRectangle),
            Some((ToolKind::Rectangle, "editor rectangle tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterCrop),
            Some((ToolKind::Crop, "editor crop interaction armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterText),
            Some((ToolKind::Text, "editor text tool armed"))
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorEnterOcr),
            Some((ToolKind::Ocr, "editor OCR tool armed"))
        );
    }

    #[test]
    fn shortcut_editor_tool_switch_ignores_non_tool_actions() {
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::EditorSave),
            None
        );
        assert_eq!(
            shortcut_editor_tool_switch(ShortcutAction::CropCancel),
            None
        );
    }

    #[test]
    fn text_input_activation_auto_follows_text_tool() {
        assert!(text_input_should_be_active(
            ToolKind::Text,
            TextInputActivation::Auto
        ));
        assert!(!text_input_should_be_active(
            ToolKind::Select,
            TextInputActivation::Auto
        ));
    }

    #[test]
    fn stop_error_restores_active_recording_only_for_pre_stop_failures() {
        assert!(should_restore_active_recording_on_stop_error(
            &recording::RecordError::CommandFailed {
                command: "kill -INT".to_string(),
                message: "exit status 1".to_string(),
            }
        ));
        assert!(!should_restore_active_recording_on_stop_error(
            &recording::RecordError::CommandFailed {
                command: "ffmpeg".to_string(),
                message: "exit status 1".to_string(),
            }
        ));
    }

    #[test]
    fn text_input_activation_force_modes_override_tool_kind() {
        assert!(text_input_should_be_active(
            ToolKind::Select,
            TextInputActivation::ForceOn
        ));
        assert!(!text_input_should_be_active(
            ToolKind::Text,
            TextInputActivation::ForceOff
        ));
    }

    #[test]
    fn should_release_headless_startup_hold_only_when_idle_without_runtime_windows() {
        assert!(should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            0,
            false,
            false,
            false,
            false
        ));

        assert!(!should_release_headless_startup_hold(
            false,
            true,
            AppState::Idle,
            false,
            0,
            false,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            false,
            AppState::Idle,
            false,
            0,
            false,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Preview,
            false,
            0,
            false,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            true,
            0,
            false,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            1,
            false,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            0,
            true,
            false,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            0,
            false,
            true,
            false,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            0,
            false,
            false,
            true,
            false
        ));
        assert!(!should_release_headless_startup_hold(
            true,
            true,
            AppState::Idle,
            false,
            0,
            false,
            false,
            false,
            true
        ));
    }

    #[test]
    fn history_remote_command_detection_matches_open_and_toggle_only() {
        assert!(is_history_remote_command(&RemoteCommand::OpenHistory));
        assert!(is_history_remote_command(&RemoteCommand::ToggleHistory));
        assert!(!is_history_remote_command(&RemoteCommand::OpenPreview));
        assert!(!is_history_remote_command(&RemoteCommand::CaptureRegion));
    }
}
