use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

static POSTGRES_RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

fn postgres_runtime() -> Result<&'static Runtime, String> {
    POSTGRES_RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .enable_all()
                .thread_name("cditor-postgres")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub fn block_on_postgres<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    Ok(postgres_runtime()?.block_on(future))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_postgres_runtime_runs_future() {
        let value = block_on_postgres(async { 42 }).unwrap();
        assert_eq!(value, 42);
    }
}
