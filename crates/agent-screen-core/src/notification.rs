use crate::identity::APP_NAME;

pub fn send(body: impl Into<String>) {
    let body = body.into();
    if let Err(err) = notify_rust::Notification::new()
        .appname(APP_NAME)
        .summary(APP_NAME)
        .body(&body)
        .show()
    {
        tracing::warn!("system notification failed: {err}");
    }
}
