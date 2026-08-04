use std::future::Future;

#[cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;

#[cfg(not(target_family = "wasm"))]
use tokio::runtime::{Builder, Runtime};
#[cfg(not(target_family = "wasm"))]
use tokio::task::JoinHandle;

/// Async runtime bridge for synchronous Session and desktop host entrypoints.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionIoExecutor;

// The session runtime is shared by persistence, search, sync, and Copilot.
// Keep its fixed overhead bounded; CPU-heavy work must use the blocking pool.
#[cfg(not(target_family = "wasm"))]
const SESSION_IO_WORKER_THREADS: usize = 2;
#[cfg(not(target_family = "wasm"))]
// Blocking threads are created on demand and retire when idle. Keep enough
// headroom for the existing headless-edit path, which can nest a blocking
// document task with image/metadata preparation, without reserving memory at
// startup.
const SESSION_IO_MAX_BLOCKING_THREADS: usize = 8;
#[cfg(not(target_family = "wasm"))]
const SESSION_IO_THREAD_STACK_SIZE: usize = 1024 * 1024;

impl SessionIoExecutor {
    pub fn shared() -> Self {
        Self
    }

    /// Returns a handle to the process-wide session runtime.
    ///
    /// Hosts that integrate another Tokio-aware library (for example the
    /// embedded Comet UI) should initialize that library from this handle so
    /// all async work shares one runtime and one set of worker threads.
    #[cfg(not(target_family = "wasm"))]
    pub fn handle(&self) -> Result<tokio::runtime::Handle, String> {
        Ok(session_io_runtime()?.handle().clone())
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn run<F, T>(&self, future: F) -> Result<T, String>
    where
        F: Future<Output = T>,
    {
        let runtime = session_io_runtime()?;
        let Ok(current) = tokio::runtime::Handle::try_current() else {
            return Ok(runtime.block_on(future));
        };
        match current.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::MultiThread => {
                Ok(tokio::task::block_in_place(|| runtime.block_on(future)))
            }
            tokio::runtime::RuntimeFlavor::CurrentThread => Err(
                "cannot synchronously run session I/O from a current-thread Tokio runtime; use run_async"
                    .to_owned(),
            ),
            _ => Err("unsupported Tokio runtime flavor for synchronous session I/O".to_owned()),
        }
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

    /// Schedules an owned I/O future on the shared session runtime.
    ///
    /// This is useful for long-lived coordinators such as cloud sync. It avoids
    /// creating a dedicated OS thread merely to call `Runtime::block_on` while
    /// still keeping all Tokio work on the one runtime.
    #[cfg(not(target_family = "wasm"))]
    pub fn spawn<F, T>(&self, future: F) -> Result<JoinHandle<T>, String>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Ok(session_io_runtime()?.spawn(future))
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
                .worker_threads(SESSION_IO_WORKER_THREADS)
                .max_blocking_threads(SESSION_IO_MAX_BLOCKING_THREADS)
                .thread_stack_size(SESSION_IO_THREAD_STACK_SIZE)
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
    fn shared_executor_spawns_long_lived_tasks_without_a_bridge_thread() {
        let task = SessionIoExecutor::shared()
            .spawn(async {
                tokio::runtime::Handle::try_current().expect("task must run on a Tokio runtime");
                tokio::time::sleep(Duration::from_millis(1)).await;
                42
            })
            .unwrap();

        assert_eq!(SessionIoExecutor::shared().run(task).unwrap().unwrap(), 42);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn synchronous_bridge_is_safe_from_the_shared_blocking_pool() {
        let result = SessionIoExecutor::shared()
            .run(async {
                tokio::task::spawn_blocking(|| SessionIoExecutor::shared().run(async { 42 })).await
            })
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn synchronous_bridge_rejects_current_thread_runtime_without_panicking() {
        let caller_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result =
            caller_runtime.block_on(async { SessionIoExecutor::shared().run(async { 42 }) });

        assert!(result.unwrap_err().contains("current-thread Tokio runtime"));
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
