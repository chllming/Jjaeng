pub mod app;
pub mod ui;

pub use app::{StartupCaptureMode, StartupConfig};
pub use jjaeng_core::{AppError, AppResult};

/// Entrypoint used by CLI and higher-level integrations.
pub fn run() -> AppResult<()> {
    run_with_config(None)
}

/// Entrypoint that allows the daemon binary to force startup behavior.
pub fn run_with_config(startup_override: Option<StartupConfig>) -> AppResult<()> {
    jjaeng_core::logging::init();
    tracing::info!("starting Jjaeng");

    let mut app = app::App::new();
    app.start(startup_override)?;

    tracing::info!("startup complete with state={:?}", app.state().state());
    Ok(())
}
