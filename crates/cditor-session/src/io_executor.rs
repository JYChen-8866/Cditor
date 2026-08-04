use std::future::Future;

#[cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;

#[cfg(not(target_family = "wasm"))]
use tokio::runtime::{Builder, Runtime};

/// Async runtime bridge for synchronous Session and desktop host entrypoints.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionIoExecutor;

impl SessionIoExecutor {
    pub fn shared() -> Self {
        Self
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn run<F, T>(&self, future: F) -> Result<T, String>
    where
        F: Future<Output = T>,
    {
        Ok(session_io_runtime()?.block_on(future))
    }

    /// Runs an owned I/O future on the session Tokio runtime without requiring
    /// the caller's executor to provide a Tokio context.
    #[cfg(not(target_family = "wasm"))]
    pub async fn run_async<F, T>(&self, future: F) -> Result<T, String>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        session_io_runtime()?
            .spawn(future)
            .await
            .map_err(|error| format!("session I/O task failed: {error}"))
    }

    #[cfg(target_family = "wasm")]
    pub fn run<F, T>(&self, _future: F) -> Result<T, String>
    where
        F: Future<Output = T>,
    {
        Err("SessionIoExecutor is not supported on WASM".to_owned())
    }

    #[cfg(target_family = "wasm")]
    pub async fn run_async<F, T>(&self, _future: F) -> Result<T, String>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Err("SessionIoExecutor is not supported on WASM".to_owned())
    }
}

#[cfg(not(target_family = "wasm"))]
fn session_io_runtime() -> Result<&'static Runtime, String> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .enable_all()
                .thread_name("cditor-session-io")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn shared_executor_runs_session_future() {
        assert_eq!(SessionIoExecutor::shared().run(async { 42 }).unwrap(), 42);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn shared_executor_supports_timeout_policy() {
        let result = SessionIoExecutor::shared()
            .run(async {
                tokio::time::timeout(Duration::from_millis(1), async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                })
                .await
            })
            .unwrap();
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn async_bridge_supplies_tokio_context_to_non_tokio_callers() {
        let result = futures_lite::future::block_on(SessionIoExecutor::shared().run_async(async {
            tokio::runtime::Handle::try_current().expect("future must run on a Tokio runtime");
            tokio::time::sleep(Duration::from_millis(1)).await;
            42
        }));

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn async_bridge_is_safe_inside_an_existing_tokio_runtime() {
        let caller_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = caller_runtime.block_on(SessionIoExecutor::shared().run_async(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            42
        }));

        assert_eq!(result.unwrap(), 42);
    }
}
