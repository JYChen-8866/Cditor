use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

static STORAGE_RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

fn storage_runtime() -> Result<&'static Runtime, String> {
    STORAGE_RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .enable_all()
                .thread_name("cditor-storage")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub fn block_on_storage<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    Ok(storage_runtime()?.block_on(future))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_storage_runtime_runs_future() {
        assert_eq!(block_on_storage(async { 42 }).unwrap(), 42);
    }
}
