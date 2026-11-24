use std::future::Future;
use std::sync::Arc;

use gpui_kit::*;
use tokio::sync::oneshot;
use xilo_engine::Engine;

/// Owns the tokio runtime that Bluetooth needs, next to GPUI's executors.
pub struct Runtime {
    pub tokio: tokio::runtime::Runtime,
    pub engine: Arc<Engine>,
    pub bluetooth_error: Option<String>,
    pub simulated: bool,
}

impl Global for Runtime {}

impl Runtime {
    pub fn get(cx: &App) -> &Runtime {
        cx.global::<Runtime>()
    }

    /// Runs a future on tokio and returns a receiver GPUI tasks can await.
    pub fn run<T, F>(cx: &App, future: F) -> oneshot::Receiver<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        cx.global::<Runtime>().tokio.spawn(async move {
            let _ = tx.send(future.await);
        });
        rx
    }

    /// Runs a future on tokio without waiting for its result.
    pub fn spawn<F>(cx: &App, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        cx.global::<Runtime>().tokio.spawn(future);
    }

    pub fn enter<R>(cx: &App, f: impl FnOnce(&Engine) -> R) -> R {
        let runtime = cx.global::<Runtime>();
        let _guard = runtime.tokio.enter();
        f(&runtime.engine)
    }
}
