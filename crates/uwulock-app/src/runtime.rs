//! The async runtime the app's work runs on: the one the caller is in, or
//! else UwULock's own, made on first use. The Tauri app hands UwULock's own
//! to Tauri (`tauri::async_runtime::set`), so commands and background work
//! share one.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Handle, Runtime};
use tokio::task::JoinHandle;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// UwULock's own runtime, for threads that aren't in one.
pub fn shared() -> Handle {
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("the async runtime starts")
        })
        .handle()
        .clone()
}

fn current() -> Handle {
    Handle::try_current().unwrap_or_else(|_| shared())
}

/// Runs `future` in the background.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    current().spawn(future)
}

/// Runs `work` on a thread that may block.
pub fn spawn_blocking<F, R>(work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    current().spawn_blocking(work)
}

/// Waits for `future` on a thread outside the runtime (a device's or the
/// system's own threads).
pub fn block_on<F: Future>(future: F) -> F::Output {
    shared().block_on(future)
}
