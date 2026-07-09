use std::time::Duration;

use gpui::{AppContext, Context, Entity};

use crate::gui::CditorV2View;
use crate::gui::persistence::PostgresPersistenceTarget;
use cditor_core::ids::DocumentId;
use cditor_runtime::DocumentRuntime;
use cditor_storage_postgres::block_on_postgres;

use super::cold_start::{CditorColdStartPlan, load_runtime_from_options};
use super::options::{CditorBackend, CditorOptions, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cditor {
    options: CditorOptions,
}

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

    pub fn with_postgres_url(mut self, url: impl Into<String>) -> Self {
        self.options.backend = CditorBackend::PostgresUrl { url: url.into() };
        self
    }

    pub fn with_postgres_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.options.backend = CditorBackend::PostgresPool { pool };
        self
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

    pub fn with_postgres_large_demo_seed(mut self, block_count: usize, force: bool) -> Self {
        self.options.seed_large_demo_to_postgres = true;
        self.options.seed_large_demo_block_count = block_count.max(1);
        self.options.force_reseed_large_demo = force;
        self
    }

    pub fn options(&self) -> &CditorOptions {
        &self.options
    }

    pub fn into_options(self) -> CditorOptions {
        self.options
    }

    pub fn build_view(self, cx: &mut Context<CditorV2View>) -> CditorV2View {
        match CditorColdStartPlan::from_options(&self.options) {
            CditorColdStartPlan::Demo | CditorColdStartPlan::Memory => {
                let runtime = match CditorColdStartPlan::from_options(&self.options) {
                    CditorColdStartPlan::Memory => DocumentRuntime::empty(),
                    _ => DocumentRuntime::demo(),
                };
                CditorV2View::from_runtime_with_options(
                    runtime,
                    self.options.debug_overlay,
                    self.options.readonly,
                    cx,
                )
            }
            CditorColdStartPlan::LargeDemo => CditorV2View::from_runtime_with_options(
                DocumentRuntime::large_mixed_demo(),
                self.options.debug_overlay,
                self.options.readonly,
                cx,
            ),
            CditorColdStartPlan::PostgresUrl { document_id, .. }
            | CditorColdStartPlan::PostgresPool { document_id } => {
                spawn_postgres_cold_start(self.options.clone(), cx);
                CditorV2View::loading_with_options(
                    format!("PostgreSQL document {document_id} is loading in background"),
                    self.options.debug_overlay,
                    self.options.readonly,
                    self.options.autosave_interval,
                    cx,
                )
            }
            CditorColdStartPlan::Cloud { endpoint } => CditorV2View::loading_with_options(
                format!("Cloud endpoint {endpoint} is loading in background"),
                self.options.debug_overlay,
                self.options.readonly,
                self.options.autosave_interval,
                cx,
            ),
            CditorColdStartPlan::Invalid { reason } => CditorV2View::load_failed_with_options(
                reason,
                self.options.debug_overlay,
                self.options.readonly,
                cx,
            ),
        }
    }

    pub fn build_entity(self, cx: &mut gpui::App) -> Entity<CditorV2View> {
        cx.new(|cx| self.build_view(cx))
    }
}

fn spawn_postgres_cold_start(options: CditorOptions, cx: &mut Context<CditorV2View>) {
    let load_task = cx.background_spawn(async move {
        block_on_postgres(load_runtime_from_options(&options))
            .and_then(|result| result.map_err(|error| error.to_string()))
    });

    cx.spawn(async move |view, cx| match load_task.await {
        Ok(Some(loaded)) => {
            let postgres_target = loaded.postgres_pool.map(|pool| {
                PostgresPersistenceTarget::from_runtime_document_id(
                    loaded.runtime.document_id,
                    pool,
                )
            });
            let _ = view.update(cx, |view, cx| {
                view.apply_loaded_runtime_with_postgres_target(loaded.runtime, postgres_target);
                cx.notify();
            });
        }
        Ok(None) => {
            let _ = view.update(cx, |view, cx| {
                view.apply_load_failed("PostgreSQL backend did not produce a runtime");
                cx.notify();
            });
        }
        Err(message) => {
            let _ = view.update(cx, |view, cx| {
                view.apply_load_failed(message);
                cx.notify();
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cditor_builder_defaults_to_demo_backend() {
        let cditor = Cditor::new();

        assert_eq!(cditor.options().backend, CditorBackend::Demo);
        assert_eq!(cditor.options().payload_window_size, 128);
        assert!(!cditor.options().debug_overlay);
    }

    #[test]
    fn cditor_builder_sets_document_backend_and_debug_options() {
        let cditor = Cditor::new()
            .with_workspace_id(7)
            .with_document_id(42)
            .with_postgres_url("postgres://localhost/cditor")
            .with_debug_overlay(true)
            .with_readonly(true)
            .with_payload_window_size(0);

        assert_eq!(cditor.options().workspace_id, Some(7));
        assert_eq!(cditor.options().document_id, Some(42));
        assert_eq!(
            cditor.options().backend,
            CditorBackend::PostgresUrl {
                url: "postgres://localhost/cditor".to_owned()
            }
        );
        assert!(cditor.options().debug_overlay);
        assert!(cditor.options().readonly);
        assert_eq!(cditor.options().payload_window_size, 1);
        assert!(!cditor.options().seed_large_demo_to_postgres);
    }

    #[test]
    fn cditor_builder_enables_postgres_large_demo_seed() {
        let cditor = Cditor::new().with_postgres_large_demo_seed(0, true);

        assert!(cditor.options().seed_large_demo_to_postgres);
        assert_eq!(cditor.options().seed_large_demo_block_count, 1);
        assert!(cditor.options().force_reseed_large_demo);
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
    fn memory_backend_builds_empty_editor_runtime() {
        let runtime = DocumentRuntime::empty();
        let projection = runtime.projection_for_window();

        assert_eq!(projection.blocks.len(), 1);
        assert_eq!(projection.blocks[0].block_id, 1);
        assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "");
    }
}
