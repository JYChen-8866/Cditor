use std::time::Duration;

use crate::storage_host::{CditorColdStartPlan, DocumentSchemaAccess, load_runtime_from_options};
use cditor_api::event::CditorEvent;
use cditor_api::{Cditor, CditorComponent, CditorError, CditorOptions, CditorViewFactory};
use cditor_editor::app::CditorV2View;
use cditor_storage::{StorageError, block_on_storage};
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, px, size,
};

struct CditorHostView {
    view: gpui::Entity<CditorV2View>,
}

impl CditorHostView {
    fn new(view: gpui::Entity<CditorV2View>) -> Self {
        Self { view }
    }
}

impl Render for CditorHostView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AppCditorViewFactory;

impl CditorViewFactory for AppCditorViewFactory {
    type View = CditorV2View;

    fn build_component(
        &self,
        builder: Cditor,
        cx: &mut App,
    ) -> Result<CditorComponent<Self::View>, CditorError> {
        if let CditorColdStartPlan::Invalid { reason } =
            CditorColdStartPlan::from_options(builder.options())
        {
            return Err(CditorError::InvalidInput(reason));
        }
        let view = cx.new(|cx| build_view(builder, cx));
        Ok(CditorComponent::from_view(view))
    }
}

pub fn build_component(
    builder: Cditor,
    cx: &mut App,
) -> Result<CditorComponent<CditorV2View>, CditorError> {
    builder.build_with(&AppCditorViewFactory, cx)
}

pub fn run_desktop(cditor: Cditor) {
    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cditor_editor::input::actions::bind_cditor_keys(cx);
        cx.activate(true);
        let component = build_component(cditor.clone(), cx).expect("build Cditor component");
        let window_options = default_window_options(cx);
        cx.open_window(window_options, move |_window, cx| {
            let view = component.view.clone();
            cx.new(|_cx| CditorHostView::new(view))
        })
        .expect("open Cditor window");
    });
}

fn build_view(builder: Cditor, cx: &mut Context<CditorV2View>) -> CditorV2View {
    let ai_provider = builder.ai_provider();
    let ai_enabled = builder.ai_enabled();
    let options = builder.into_options();
    let mut view = match CditorColdStartPlan::from_options(&options) {
        CditorColdStartPlan::Demo => CditorV2View::from_runtime_with_options(
            cditor_runtime::DocumentRuntime::demo(),
            options.debug_overlay,
            options.readonly,
            cx,
        ),
        CditorColdStartPlan::Memory => CditorV2View::from_runtime_with_options(
            cditor_runtime::DocumentRuntime::empty(),
            options.debug_overlay,
            options.readonly,
            cx,
        ),
        CditorColdStartPlan::LargeDemo => CditorV2View::from_runtime_with_options(
            cditor_runtime::DocumentRuntime::large_mixed_demo(),
            options.debug_overlay,
            options.readonly,
            cx,
        ),
        plan @ CditorColdStartPlan::Persistent { .. } => {
            let label = plan
                .persistent_label()
                .unwrap_or_else(|| "persistent document".to_owned());
            spawn_storage_cold_start(options.clone(), plan.timeout(), cx);
            CditorV2View::loading_with_options(
                format!("{label} is loading in background"),
                options.debug_overlay,
                options.readonly,
                options.autosave_interval,
                cx,
            )
        }
        CditorColdStartPlan::Cloud { endpoint } => CditorV2View::loading_with_options(
            format!("Cloud endpoint {endpoint} is loading in background"),
            options.debug_overlay,
            options.readonly,
            options.autosave_interval,
            cx,
        ),
        CditorColdStartPlan::Invalid { reason } => CditorV2View::load_failed_with_options(
            reason,
            options.debug_overlay,
            options.readonly,
            cx,
        ),
    };
    view.sdk_configure_ai(ai_provider, ai_enabled);
    view
}

fn spawn_storage_cold_start(
    options: CditorOptions,
    timeout: Duration,
    cx: &mut Context<CditorV2View>,
) {
    let load_task = cx.background_spawn(async move {
        block_on_storage(async move {
            tokio::time::timeout(timeout, load_runtime_from_options(&options))
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "document storage cold start",
                    timeout,
                })?
        })
        .and_then(|result| result.map_err(|error| error.to_string()))
    });

    cx.spawn(async move |view, cx| match load_task.await {
        Ok(Some(loaded)) => {
            let _ = view.update(cx, |view, cx| {
                let schema_access = loaded.schema_access;
                view.apply_loaded_runtime_with_storage(
                    loaded.runtime,
                    Some(loaded.storage_session),
                );
                if let DocumentSchemaAccess::ReadOnlyNewerMajor {
                    written_major,
                    supported_major,
                } = schema_access
                {
                    view.enforce_newer_schema_readonly(written_major, supported_major);
                }
                if let Some(document) = view.sdk_document_info() {
                    cx.emit(CditorEvent::Ready { document });
                }
                cx.notify();
            });
        }
        Ok(None) => {
            let _ = view.update(cx, |view, cx| {
                view.apply_load_failed("storage backend did not produce a runtime");
                cx.emit(CditorEvent::LoadFailed {
                    error: CditorError::Internal(
                        "storage backend did not produce a runtime".to_owned(),
                    ),
                });
                cx.notify();
            });
        }
        Err(message) => {
            let _ = view.update(cx, |view, cx| {
                view.apply_load_failed(message.clone());
                cx.emit(CditorEvent::LoadFailed {
                    error: CditorError::Persistence(message),
                });
                cx.notify();
            });
        }
    })
    .detach();
}

fn default_window_options(cx: &mut App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1200.0), px(800.0)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Cditor".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}
