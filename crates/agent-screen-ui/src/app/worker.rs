use std::sync::mpsc;
use std::time::Duration;

pub(super) const ACTION_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(24);

pub(super) fn spawn_worker_action<T, W, H>(work: W, mut on_result: H)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    H: FnMut(T) + 'static,
{
    let (tx, rx) = mpsc::channel::<T>();
    std::thread::spawn(move || {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
            Ok(result) => {
                let _ = tx.send(result);
            }
            Err(panic_info) => {
                let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                tracing::error!(message = %message, "worker thread panicked");
                // tx drops here, triggering Disconnected in the poller
            }
        }
    });

    gtk4::glib::timeout_add_local(ACTION_RESULT_POLL_INTERVAL, move || match rx.try_recv() {
        Ok(result) => {
            on_result(result);
            gtk4::glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            tracing::warn!("worker thread terminated without producing a result");
            gtk4::glib::ControlFlow::Break
        }
    });
}
