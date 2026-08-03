use std::{fmt, sync::Arc, time::Duration};

use cditor_core::ids::DocumentId;

use super::options::{CditorDocumentSource, CditorOptions};

#[derive(Clone)]
pub struct Cditor {
    options: CditorOptions,
    ai_provider: Option<Arc<dyn cditor_ai::AiProvider>>,
    asset_provider: Option<Arc<dyn crate::providers::AssetProvider>>,
    ai_enabled: bool,
}

impl Default for Cditor {
    fn default() -> Self {
        Self {
            options: CditorOptions::default(),
            ai_provider: None,
            asset_provider: None,
            ai_enabled: true,
        }
    }
}

impl fmt::Debug for Cditor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Cditor")
            .field("options", &self.options)
            .field(
                "ai_provider",
                &self.ai_provider.as_ref().map(|provider| provider.id()),
            )
            .field("asset_provider", &self.asset_provider.is_some())
            .field("ai_enabled", &self.ai_enabled)
            .finish()
    }
}

impl PartialEq for Cditor {
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options
            && self.ai_enabled == other.ai_enabled
            && match (&self.asset_provider, &other.asset_provider) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.ai_provider.as_ref().map(|provider| provider.id())
                == other.ai_provider.as_ref().map(|provider| provider.id())
    }
}

impl Eq for Cditor {}

impl Cditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn demo(mut self) -> Self {
        self.options.source = CditorDocumentSource::Demo;
        self
    }

    pub fn large_demo(mut self) -> Self {
        self.options.source = CditorDocumentSource::LargeDemo;
        self
    }

    pub fn memory(mut self) -> Self {
        self.options.source = CditorDocumentSource::Memory;
        self
    }

    pub fn with_document_id(mut self, document_id: DocumentId) -> Self {
        self.options.document_id = Some(document_id);
        self
    }

    pub fn with_storage(
        mut self,
        storage: std::sync::Arc<dyn cditor_storage::DocumentStorage>,
    ) -> Self {
        self.options.source = CditorDocumentSource::Storage(storage);
        self
    }

    pub fn with_storage_load_timeout(mut self, timeout: Duration) -> Self {
        self.options.storage_load_timeout = timeout.max(Duration::from_secs(1));
        self
    }

    pub fn with_readonly(mut self, readonly: bool) -> Self {
        self.options.readonly = readonly;
        self
    }

    pub fn with_debug_overlay(mut self, enabled: bool) -> Self {
        self.options.debug_overlay = enabled;
        self
    }

    pub fn with_payload_window_size(mut self, size: usize) -> Self {
        self.options.payload_window_size = size.max(1);
        self
    }

    pub fn with_autosave(mut self, seconds: u64) -> Self {
        self.options.autosave_interval = Some(Duration::from_secs(seconds.max(1)));
        self
    }

    pub fn with_autosave_interval(mut self, interval: Duration) -> Self {
        self.options.autosave_interval = Some(interval.max(Duration::from_secs(1)));
        self
    }

    pub fn without_autosave(mut self) -> Self {
        self.options.autosave_interval = None;
        self
    }

    pub fn with_ai_provider(mut self, provider: Arc<dyn cditor_ai::AiProvider>) -> Self {
        self.ai_provider = Some(provider);
        self.ai_enabled = true;
        self
    }

    pub fn without_ai(mut self) -> Self {
        self.ai_provider = None;
        self.ai_enabled = false;
        self
    }

    pub fn with_asset_provider(
        mut self,
        provider: Arc<dyn crate::providers::AssetProvider>,
    ) -> Self {
        self.asset_provider = Some(provider);
        self
    }

    pub fn asset_provider(&self) -> Option<Arc<dyn crate::providers::AssetProvider>> {
        self.asset_provider.clone()
    }

    pub fn options(&self) -> &CditorOptions {
        &self.options
    }

    pub fn ai_provider(&self) -> Option<Arc<dyn cditor_ai::AiProvider>> {
        self.ai_provider.clone()
    }

    pub const fn ai_enabled(&self) -> bool {
        self.ai_enabled
    }

    pub fn into_options(self) -> CditorOptions {
        self.options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cditor_builder_defaults_to_memory_source() {
        let cditor = Cditor::new();

        assert_eq!(cditor.options().source, CditorDocumentSource::Memory);
        assert_eq!(cditor.options().payload_window_size, 128);
        assert_eq!(
            cditor.options().autosave_interval,
            Some(Duration::from_millis(250))
        );
        assert!(!cditor.options().debug_overlay);
    }

    #[test]
    fn cditor_builder_sets_document_backend_and_debug_options() {
        let storage: Arc<dyn cditor_storage::DocumentStorage> = Arc::new(TestStorage::default());
        let cditor = Cditor::new()
            .with_document_id(42)
            .with_storage(storage.clone())
            .with_debug_overlay(true)
            .with_readonly(true)
            .with_payload_window_size(0);

        assert_eq!(cditor.options().document_id, Some(42));
        assert!(matches!(
            &cditor.options().source,
            CditorDocumentSource::Storage(configured) if Arc::ptr_eq(configured, &storage)
        ));
        assert!(cditor.options().debug_overlay);
        assert!(cditor.options().readonly);
        assert_eq!(cditor.options().payload_window_size, 1);
    }

    #[derive(Default)]
    struct TestStorage;

    #[async_trait::async_trait]
    impl cditor_storage::DocumentStorage for TestStorage {
        fn backend_kind(&self) -> cditor_storage::StorageBackendKind {
            cditor_storage::StorageBackendKind::Custom
        }

        fn capabilities(&self) -> cditor_storage::StorageCapabilities {
            cditor_storage::StorageCapabilities {
                payload_window: false,
                emergency_log: false,
            }
        }

        async fn load_document(
            &self,
            _request: cditor_storage::LoadDocumentRequest,
        ) -> cditor_storage::StorageResult<cditor_storage::LoadedDocument> {
            unreachable!("builder tests never open storage")
        }

        async fn load_payloads(
            &self,
            _document_id: DocumentId,
            _block_ids: &[cditor_core::ids::BlockId],
        ) -> cditor_storage::StorageResult<cditor_storage::LoadedPayloadBatch> {
            unreachable!("builder tests never load payloads")
        }

        async fn commit(
            &self,
            _batch: cditor_storage::StorageSaveBatch,
        ) -> cditor_storage::StorageResult<cditor_storage::StorageSaveOutcome> {
            unreachable!("builder tests never commit")
        }
    }

    #[test]
    fn cditor_builder_sets_autosave_interval() {
        let cditor = Cditor::new().with_autosave(10);

        assert_eq!(
            cditor.options().autosave_interval,
            Some(Duration::from_secs(10))
        );
    }

    #[test]
    fn cditor_builder_clamps_autosave_to_one_second() {
        let by_seconds = Cditor::new().with_autosave(0);
        let by_duration = Cditor::new().with_autosave_interval(Duration::from_millis(250));

        assert_eq!(
            by_seconds.options().autosave_interval,
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            by_duration.options().autosave_interval,
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn cditor_builder_clears_custom_autosave_interval() {
        let cditor = Cditor::new().with_autosave(10).without_autosave();

        assert_eq!(cditor.options().autosave_interval, None);
    }

    #[test]
    fn cditor_builder_configures_and_disables_ai() {
        let provider = Arc::new(cditor_ai::MockAiProvider::default());
        let configured = Cditor::new().with_ai_provider(provider);
        assert!(configured.ai_enabled);
        assert_eq!(
            configured
                .ai_provider
                .as_ref()
                .map(|provider| provider.id()),
            Some("mock")
        );

        let disabled = configured.without_ai();
        assert!(!disabled.ai_enabled);
        assert!(disabled.ai_provider.is_none());
    }
}
