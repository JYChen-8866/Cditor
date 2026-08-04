use std::sync::Arc;

use cditor_core::rich_text::AssetRef;
use cditor_sdk::providers::{AssetError, AssetInput, AssetProvider, ImportedAsset, ResolvedAsset};
#[cfg(not(target_family = "wasm"))]
use cditor_session::SessionIoExecutor;

pub(crate) async fn import_asset(
    provider: Arc<dyn AssetProvider>,
    input: AssetInput,
) -> Result<ImportedAsset, AssetError> {
    run_asset_operation(async move { provider.import(input).await }).await
}

pub(crate) async fn resolve_asset(
    provider: Arc<dyn AssetProvider>,
    asset: AssetRef,
) -> Result<ResolvedAsset, AssetError> {
    run_asset_operation(async move { provider.resolve(&asset).await }).await
}

#[allow(dead_code)]
pub(crate) async fn delete_asset(
    provider: Arc<dyn AssetProvider>,
    asset: AssetRef,
) -> Result<(), AssetError> {
    run_asset_operation(async move { provider.delete(&asset).await }).await
}

async fn run_asset_operation<F, T>(future: F) -> Result<T, AssetError>
where
    F: Future<Output = Result<T, AssetError>> + Send + 'static,
    T: Send + 'static,
{
    #[cfg(not(target_family = "wasm"))]
    {
        SessionIoExecutor::shared()
            .run_async(future)
            .await
            .map_err(|message| AssetError { message })?
    }
    #[cfg(target_family = "wasm")]
    {
        future.await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use async_trait::async_trait;
    use cditor_core::edit::{AssetSnapshot, AssetState};
    use cditor_sdk::providers::ResolvedAsset;

    use super::*;

    struct TokioCheckingProvider;

    #[async_trait]
    impl AssetProvider for TokioCheckingProvider {
        async fn import(&self, input: AssetInput) -> Result<ImportedAsset, AssetError> {
            require_tokio_context().await?;
            let reference = AssetRef::local("assets/test.png");
            Ok(ImportedAsset {
                reference: reference.clone(),
                snapshot: AssetSnapshot {
                    asset_id: 1,
                    file_name: input.name,
                    media_type: "image/png".into(),
                    size_bytes: input.bytes.len() as u64,
                    source: reference.source,
                    checksum: None,
                    state: AssetState::Ready,
                },
            })
        }

        async fn resolve(&self, asset: &AssetRef) -> Result<ResolvedAsset, AssetError> {
            require_tokio_context().await?;
            Ok(ResolvedAsset {
                reference: asset.clone(),
                local_path: Some(PathBuf::from("test.png")),
                bytes: None,
            })
        }

        async fn delete(&self, _asset: &AssetRef) -> Result<(), AssetError> {
            require_tokio_context().await
        }
    }

    async fn require_tokio_context() -> Result<(), AssetError> {
        tokio::runtime::Handle::try_current().map_err(|error| AssetError {
            message: error.to_string(),
        })?;
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(())
    }

    #[test]
    fn all_asset_operations_run_with_tokio_context_from_plain_test_thread() {
        futures_lite::future::block_on(async {
            let provider: Arc<dyn AssetProvider> = Arc::new(TokioCheckingProvider);
            let imported = import_asset(
                provider.clone(),
                AssetInput {
                    name: "test.png".into(),
                    media_type: Some("image/png".into()),
                    bytes: vec![1, 2, 3],
                },
            )
            .await
            .unwrap();
            assert_eq!(imported.snapshot.file_name, "test.png");

            let resolved = resolve_asset(provider.clone(), imported.reference.clone())
                .await
                .unwrap();
            assert_eq!(resolved.reference, imported.reference);

            delete_asset(provider, resolved.reference).await.unwrap();
        });
    }

    #[test]
    fn provider_errors_are_preserved() {
        let error = futures_lite::future::block_on(run_asset_operation(async {
            Err::<(), _>(AssetError {
                message: "provider failed".into(),
            })
        }))
        .unwrap_err();

        assert_eq!(error.message, "provider failed");
    }
}
