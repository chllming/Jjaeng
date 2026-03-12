use jjaeng_core::config::load_app_config;
use jjaeng_core::identity::APP_SLUG;
use jjaeng_core::storage::StorageService;

pub(super) fn initialize_storage_service() -> Option<StorageService> {
    let app_config = load_app_config();
    match StorageService::with_runtime_paths(app_config.screenshot_dir) {
        Ok(service) => {
            tracing::info!(
                temp_dir = %service.temp_dir().display(),
                pictures_dir = %service.pictures_dir().display(),
                "initialized storage service"
            );
            Some(service)
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to initialize storage service; disabling save/cleanup"
            );
            None
        }
    }
}

pub(super) fn gtk_launch_args() -> Vec<String> {
    std::env::args()
        .next()
        .map(|arg0| vec![arg0])
        .unwrap_or_else(|| vec![APP_SLUG.to_string()])
}

pub(super) fn cleanup_remaining_session_artifacts(
    storage_service: Option<&StorageService>,
    capture_ids: &[String],
) {
    let Some(service) = storage_service else {
        if !capture_ids.is_empty() {
            tracing::warn!(
                capture_count = capture_ids.len(),
                "skipping shutdown capture cleanup because storage service is unavailable"
            );
        }
        return;
    };

    let mut removed = 0usize;
    let mut failed = 0usize;
    for capture_id in capture_ids {
        match service.discard_session_artifacts(capture_id) {
            Ok(()) => {
                removed += 1;
            }
            Err(err) => {
                failed += 1;
                tracing::warn!(
                    capture_id = %capture_id,
                    ?err,
                    "failed to discard temporary capture artifact during shutdown"
                );
            }
        }
    }

    if !capture_ids.is_empty() {
        tracing::info!(
            capture_count = capture_ids.len(),
            removed,
            failed,
            "finished shutdown capture artifact cleanup"
        );
    }
}
