pub const APP_NAME: &str = "Jjaeng";
pub const APP_SLUG: &str = "jjaeng";
pub const APP_ID: &str = "com.github.chllming.jjaeng";
pub const APP_CSS_ROOT: &str = "jjaeng-root";
pub const APP_PREVIEW_TITLE: &str = "jjaeng-preview";
pub const APP_EDITOR_TITLE: &str = "Jjaeng Editor";
pub const APP_LAUNCHPAD_TITLE: &str = "Jjaeng Launchpad";
pub const APP_RUNTIME_SOCKET: &str = "jjaeng.sock";
pub const APP_STATUS_SNAPSHOT: &str = "jjaeng-status.json";
pub const APP_RESOURCE_BUNDLE: &str = "jjaeng.gresource";
pub const APP_ICON_RESOURCE_PATH: &str = "/com/github/chllming/jjaeng/icons/hicolor";
pub const DEFAULT_RUNTIME_DIR: &str = "/tmp/jjaeng";
pub const DEFAULT_SYSTEM_MODEL_DIR: &str = "/usr/share/jjaeng/models";
pub const UPSTREAM_NAME: &str = "ChalKak";
pub const UPSTREAM_SLUG: &str = "chalkak";
pub const UPSTREAM_REPOSITORY: &str = "https://github.com/BitYoungjae/ChalKak";
pub const REPOSITORY: &str = "https://github.com/chllming/Jjaeng";
pub const LEGACY_RUNTIME_DIR: &str = "/tmp/chalkak";
pub const LEGACY_SYSTEM_MODEL_DIR: &str = "/usr/share/chalkak/models";

pub const CONFIG_DIR_CANDIDATES: [&str; 2] = [APP_SLUG, UPSTREAM_SLUG];

pub fn config_dir_candidates() -> &'static [&'static str] {
    &CONFIG_DIR_CANDIDATES
}
