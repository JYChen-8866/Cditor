use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

/// Async runtime bridge for synchronous Session and desktop host entrypoints.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionIoExecutor;

impl SessionIoExecutor {
    pub fn shared() -> Self {
        Self
    }

    pub fn run<F, T>(&self, future: F) -> Result<T, String>
    where
        F: Future<Output = T>,
    {
        Ok(session_io_runtime()?.block_on(future))
    }
}

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
    fn shared_executor_runs_session_future() {
        assert_eq!(SessionIoExecutor::shared().run(async { 42 }).unwrap(), 42);
    }

    #[test]
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
}
