pub const APP_NAME: &str = "Agent Screen";
pub const APP_SLUG: &str = "agent-screen";
pub const APP_ID: &str = "com.github.chllming.agent_screen";
pub const APP_CSS_ROOT: &str = "agent-screen-root";
pub const APP_PREVIEW_TITLE: &str = "agent-screen-preview";
pub const APP_EDITOR_TITLE: &str = "Agent Screen Editor";
pub const APP_LAUNCHPAD_TITLE: &str = "Agent Screen Launchpad";
pub const APP_RUNTIME_SOCKET: &str = "agent-screen.sock";
pub const APP_STATUS_SNAPSHOT: &str = "agent-screen-status.json";
pub const APP_RESOURCE_BUNDLE: &str = "agent-screen.gresource";
pub const APP_ICON_RESOURCE_PATH: &str = "/com/github/chllming/agent_screen/icons/hicolor";
pub const DEFAULT_RUNTIME_DIR: &str = "/tmp/agent-screen";
pub const DEFAULT_SYSTEM_MODEL_DIR: &str = "/usr/share/agent-screen/models";
pub const LEGACY_APP_SLUG: &str = "jjaeng";
pub const UPSTREAM_NAME: &str = "ChalKak";
pub const UPSTREAM_SLUG: &str = "chalkak";
pub const UPSTREAM_REPOSITORY: &str = "https://github.com/BitYoungjae/ChalKak";
pub const REPOSITORY: &str = "https://github.com/chllming/Jjaeng";
pub const LEGACY_RUNTIME_DIR: &str = "/tmp/jjaeng";
pub const UPSTREAM_RUNTIME_DIR: &str = "/tmp/chalkak";
pub const LEGACY_SYSTEM_MODEL_DIR: &str = "/usr/share/jjaeng/models";
pub const UPSTREAM_SYSTEM_MODEL_DIR: &str = "/usr/share/chalkak/models";

pub const CONFIG_DIR_CANDIDATES: [&str; 3] = [APP_SLUG, LEGACY_APP_SLUG, UPSTREAM_SLUG];

pub fn config_dir_candidates() -> &'static [&'static str] {
    &CONFIG_DIR_CANDIDATES
}
