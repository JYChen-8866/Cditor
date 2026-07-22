use std::{fmt, sync::Arc, time::Duration};

use gpui::App;

use cditor_core::ids::DocumentId;

use super::component::CditorComponent;
use super::error::CditorError;
use super::options::{CditorBackend, CditorOptions, WorkspaceId};

#[derive(Clone)]
pub struct Cditor {
    options: CditorOptions,
    ai_provider: Option<Arc<dyn cditor_ai::AiProvider>>,
    ai_enabled: bool,
}

impl Default for Cditor {
    fn default() -> Self {
        Self {
            options: CditorOptions::default(),
            ai_provider: None,
            ai_enabled: true,
        }
    }
}

impl fmt::Debug for Cditor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CditorBuilder")
            .field("options", &self.options)
            .field(
                "ai_provider",
                &self.ai_provider.as_ref().map(|provider| provider.id()),
            )
            .field("ai_enabled", &self.ai_enabled)
            .finish()
    }
}

impl PartialEq for Cditor {
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options
            && self.ai_enabled == other.ai_enabled
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
        self.options.backend = CditorBackend::Demo;
        self
    }

    pub fn large_demo(mut self) -> Self {
        self.options.backend = CditorBackend::LargeDemo;
        self
    }

    pub fn memory(mut self) -> Self {
        self.options.backend = CditorBackend::Memory;
        self
    }

    pub fn with_workspace_id(mut self, workspace_id: WorkspaceId) -> Self {
        self.options.workspace_id = Some(workspace_id);
        self
    }

    pub fn with_document_id(mut self, document_id: DocumentId) -> Self {
        self.options.document_id = Some(document_id);
        self
    }

    pub fn with_storage_provider(
        mut self,
        provider: std::sync::Arc<dyn cditor_storage::StorageProvider>,
    ) -> Self {
        self.options.backend = CditorBackend::Persistent { provider };
        self
    }

    pub fn with_storage(
        self,
        storage: std::sync::Arc<dyn cditor_storage::DocumentStorage>,
        label: impl Into<String>,
    ) -> Self {
        self.with_storage_provider(std::sync::Arc::new(
            cditor_storage::StaticStorageProvider::new(label, storage),
        ))
    }

    pub fn with_cloud_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.options.backend = CditorBackend::Cloud {
            endpoint: endpoint.into(),
        };
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

    /// Builds a typed component through the host application's composition factory.
    pub fn build_with<F: crate::CditorViewFactory>(
        self,
        factory: &F,
        cx: &mut App,
    ) -> Result<CditorComponent<F::View>, CditorError> {
        factory.build_component(self, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cditor_builder_defaults_to_demo_backend() {
        let cditor = Cditor::new();

        assert_eq!(cditor.options().backend, CditorBackend::Demo);
        assert_eq!(cditor.options().payload_window_size, 128);
        assert_eq!(
            cditor.options().autosave_interval,
            Some(Duration::from_millis(250))
        );
        assert!(!cditor.options().debug_overlay);
    }

    #[test]
    fn cditor_builder_sets_document_backend_and_debug_options() {
        struct TestProvider;
        #[async_trait::async_trait]
        impl cditor_storage::StorageProvider for TestProvider {
            fn label(&self) -> &str {
                "test"
            }

            async fn open(
                &self,
            ) -> cditor_storage::StorageResult<Arc<dyn cditor_storage::DocumentStorage>>
            {
                Err(cditor_storage::StorageError::InvalidConfiguration(
                    "not opened by builder test".to_owned(),
                ))
            }
        }
        let provider: Arc<dyn cditor_storage::StorageProvider> = Arc::new(TestProvider);
        let cditor = Cditor::new()
            .with_workspace_id(7)
            .with_document_id(42)
            .with_storage_provider(provider.clone())
            .with_debug_overlay(true)
            .with_readonly(true)
            .with_payload_window_size(0);

        assert_eq!(cditor.options().workspace_id, Some(7));
        assert_eq!(cditor.options().document_id, Some(42));
        assert!(matches!(
            &cditor.options().backend,
            CditorBackend::Persistent { provider: configured } if Arc::ptr_eq(configured, &provider)
        ));
        assert!(cditor.options().debug_overlay);
        assert!(cditor.options().readonly);
        assert_eq!(cditor.options().payload_window_size, 1);
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
