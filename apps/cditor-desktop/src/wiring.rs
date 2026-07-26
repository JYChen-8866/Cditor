use std::time::Duration;
use std::{io::Read, sync::Arc};

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
    App, AppContext, Bounds, Context, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, px, size,
};

struct CditorHostView {
    view: gpui::Entity<CditorV2View>,
}

const REMOTE_IMAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;

struct DesktopRemoteImageDataSource {
    client: reqwest::blocking::Client,
}

impl DesktopRemoteImageDataSource {
    fn new() -> Result<Self, reqwest::Error> {
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .map(|client| Self { client })
    }
}

impl cditor_editor_gpui::RemoteImageDataSource for DesktopRemoteImageDataSource {
    fn load(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let content_length = response.content_length();
        read_remote_image_body(response, content_length)
    }
}

fn read_remote_image_body(
    reader: impl Read,
    content_length: Option<u64>,
) -> Result<Vec<u8>, String> {
    if content_length.is_some_and(|length| length > REMOTE_IMAGE_MAX_BYTES) {
        return Err("remote image exceeds the 32 MiB limit".to_owned());
    }
    let mut bytes = Vec::new();
    reader
        .take(REMOTE_IMAGE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > REMOTE_IMAGE_MAX_BYTES {
        return Err("remote image exceeds the 32 MiB limit".to_owned());
    }
    Ok(bytes)
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
        let remote_images = DesktopRemoteImageDataSource::new()
            .map_err(|error| CditorError::Internal(error.to_string()))?;
        cditor_editor_gpui::configure_remote_image_data_source(cx, Arc::new(remote_images));
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
            CditorV2View::loading_with_progress_options(
                format!("{label} is loading in background"),
                0,
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn remote_image_body_enforces_declared_and_streamed_limits() {
        assert_eq!(
            read_remote_image_body(Cursor::new(b"image"), Some(5)).unwrap(),
            b"image"
        );
        assert!(read_remote_image_body(Cursor::new([]), Some(REMOTE_IMAGE_MAX_BYTES + 1)).is_err());
        assert!(
            read_remote_image_body(std::io::repeat(0).take(REMOTE_IMAGE_MAX_BYTES + 1), None)
                .is_err()
        );
    }
}
