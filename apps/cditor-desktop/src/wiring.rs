use std::time::Duration;

use crate::storage_host::{
    CditorColdStartPlan, CditorColdStartProgress, CditorSessionLoadResult, DocumentSchemaAccess,
    load_session_from_options_with_progress,
};
use cditor_editor_gpui::{CditorComponent, CditorV2View, CditorViewFactory};
use cditor_sdk::event::CditorEvent;
use cditor_sdk::{Cditor, CditorError, CditorOptions};
use cditor_session::SessionIoExecutor;
use cditor_storage::StorageError;
use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, Render, StyleRefinement, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, px, size,
};

struct CditorHostView {
    _blink_subscription: Subscription,
    view: gpui::Entity<CditorV2View>,
}

impl CditorHostView {
    fn new(view: gpui::Entity<CditorV2View>, sub: Subscription) -> Self {
        Self {
            view,
            _blink_subscription: sub,
        }
    }
}

impl Render for CditorHostView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::AnyView::from(self.view.clone()).cached(StyleRefinement::default().flex().size_full())
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
        // The editor image loader owns one process-wide, lazily initialized
        // HTTP client. Constructing and replacing a client for every document
        // duplicated connection pools and retained allocator high-water pages.
        let view = cx.new(|cx| build_view(builder, cx));
        Ok(CditorComponent::from_view(view))
    }
}

pub fn build_component(
    builder: Cditor,
    cx: &mut App,
) -> Result<CditorComponent<CditorV2View>, CditorError> {
    AppCditorViewFactory.build_component(builder, cx)
}

pub fn run_desktop(cditor: Cditor) {
    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cditor_editor_gpui::bind_cditor_keys(cx);
        cx.activate(true);
        let component = build_component(cditor.clone(), cx).expect("build Cditor component");
        let window_options = default_window_options(cx);
        cx.open_window(window_options, move |window, cx| {
            let view = component.view.clone();
            let blink_entity = view.read(cx).caret_blink_entity().clone();
            let blink_sub = window.observe(
                &blink_entity,
                cx,
                |_: Entity<cditor_editor_gpui::CaretBlink>, window: &mut Window, _: &mut App| {
                    window.refresh()
                },
            );
            cx.new(move |_cx| CditorHostView::new(view, blink_sub))
        })
        .expect("open Cditor window");
    });
}

fn build_view(builder: Cditor, cx: &mut Context<CditorV2View>) -> CditorV2View {
    let ai_provider = builder.ai_provider();
    let ai_enabled = builder.ai_enabled();
    let asset_provider = builder.asset_provider();
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
            CditorV2View::loading_with_progress_options(
                format!("{label} is loading in background"),
                0,
                options.debug_overlay,
                options.readonly,
                options.autosave_interval,
                cx,
            )
        }
        CditorColdStartPlan::Invalid { reason } => CditorV2View::load_failed_with_options(
            reason,
            options.debug_overlay,
            options.readonly,
            cx,
        ),
    };
    view.sdk_configure_ai(ai_provider, ai_enabled);
    view.sdk_configure_asset_provider(asset_provider);
    view
}

fn spawn_storage_cold_start(
    options: CditorOptions,
    timeout: Duration,
    cx: &mut Context<CditorV2View>,
) {
    enum ColdStartEvent {
        Progress(CditorColdStartProgress),
        Finished(Result<Option<CditorSessionLoadResult>, String>),
    }

    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
    cx.background_spawn(async move {
        let progress_tx = events_tx.clone();
        let result = SessionIoExecutor::shared()
            .run(async move {
                tokio::time::timeout(
                    timeout,
                    load_session_from_options_with_progress(&options, move |progress| {
                        let _ = progress_tx.send(ColdStartEvent::Progress(progress));
                    }),
                )
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "document storage cold start",
                    timeout,
                })?
            })
            .and_then(|result| result.map_err(|error| error.to_string()));
        let _ = events_tx.send(ColdStartEvent::Finished(result));
    })
    .detach();

    cx.spawn(async move |view, cx| {
        while let Some(event) = events_rx.recv().await {
            match event {
                ColdStartEvent::Progress(progress) => {
                    let percentage = progress.percentage();
                    let _ = view.update(cx, |view, cx| {
                        if view.apply_load_progress(progress.message, percentage) {
                            cx.notify();
                        }
                    });
                }
                ColdStartEvent::Finished(result) => {
                    match result {
                        Ok(Some(loaded)) => {
                            let _ = view.update(cx, |view, cx| {
                                let schema_access = loaded.schema_access;
                                let opened = loaded.prepared.into_opened();
                                let recovered_transactions = opened
                                    .emergency_recovery
                                    .as_ref()
                                    .map_or(0, |report| report.replayed_transactions);
                                view.apply_recovered_session(
                                    opened.session,
                                    recovered_transactions,
                                    cx,
                                );
                                if let DocumentSchemaAccess::ReadOnlyNewerMajor {
                                    written_major,
                                    supported_major,
                                } = schema_access
                                {
                                    view.enforce_newer_schema_readonly(
                                        written_major,
                                        supported_major,
                                    );
                                }
                                if let DocumentSchemaAccess::ReadOnlyNewerOperationMajor {
                                    written_major,
                                    supported_major,
                                } = schema_access
                                {
                                    view.enforce_newer_operation_schema_readonly(
                                        written_major,
                                        supported_major,
                                    );
                                }
                                if let Some(document) = view.sdk_document_info() {
                                    cx.emit(CditorEvent::Ready { document });
                                }
                                cx.notify();
                            });
                        }
                        Ok(None) => {
                            let _ = view.update(cx, |view, cx| {
                                view.apply_load_failed(
                                    "storage backend did not produce a runtime",
                                    cx,
                                );
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
                                view.apply_load_failed(message.clone(), cx);
                                cx.emit(CditorEvent::LoadFailed {
                                    error: CditorError::Persistence(message),
                                });
                                cx.notify();
                            });
                        }
                    }
                    break;
                }
            }
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
