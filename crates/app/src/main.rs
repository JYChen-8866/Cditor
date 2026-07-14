#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::env;

use cditor_app::{CditorBuilder, CditorComponent};
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, px, size,
};

struct CditorHostView {
    editor: CditorComponent,
}

impl CditorHostView {
    fn new(builder: CditorBuilder, cx: &mut Context<Self>) -> Self {
        let editor = builder.build(cx).expect("build Cditor component");
        Self { editor }
    }
}

impl Render for CditorHostView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.view.clone())
    }
}

fn main() {
    let app = gpui_platform::application();
    app.run(|cx: &mut App| {
        cditor_app::gui::input::bind_cditor_keys(cx);
        cx.activate(true);
        cx.open_window(
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
            },
            |_window, cx| cx.new(|cx| CditorHostView::new(cditor_from_env(), cx)),
        )
        .expect("open Cditor window");
    });
}

fn cditor_from_env() -> CditorBuilder {
    let mut cditor = if env_flag("CDITOR_LARGE_DEMO", false) {
        CditorBuilder::new().large_demo()
    } else if env_flag("CDITOR_SMALL_DEMO", false) {
        CditorBuilder::new().demo()
    } else {
        CditorBuilder::new().memory()
    }
    .with_debug_overlay(env_flag("CDITOR_DEBUG_OVERLAY", false))
    .with_readonly(env_flag("CDITOR_READONLY", false));

    if let Some(size) = env_usize("CDITOR_PAYLOAD_WINDOW_SIZE") {
        cditor = cditor.with_payload_window_size(size);
    }

    if let Some(workspace_id) = env_u64("CDITOR_WORKSPACE_ID") {
        cditor = cditor.with_workspace_id(workspace_id);
    }

    let seed_large_demo = env_flag("CDITOR_SEED_LARGE_DEMO", false);
    let force_reseed = env_flag("CDITOR_FORCE_RESEED_LARGE_DEMO", false);
    let seed_block_count = env_usize("CDITOR_SEED_LARGE_DEMO_BLOCKS")
        .unwrap_or(cditor_app::runtime::LARGE_MIXED_DEMO_BLOCKS);

    match (
        env::var("CDITOR_DATABASE_URL").ok(),
        env_u64("CDITOR_DOCUMENT_ID"),
    ) {
        (Some(database_url), Some(document_id)) => {
            let cditor = cditor
                .with_document_id(document_id)
                .with_postgres_url(database_url);
            if seed_large_demo {
                cditor.with_postgres_large_demo_seed(seed_block_count, force_reseed)
            } else {
                cditor
            }
        }
        (Some(database_url), None) => {
            let cditor = cditor.with_document_id(1).with_postgres_url(database_url);
            if seed_large_demo {
                cditor.with_postgres_large_demo_seed(seed_block_count, force_reseed)
            } else {
                cditor
            }
        }
        _ => cditor,
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse().ok()
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn desktop_host_owns_sdk_component_and_handle(cx: &mut TestAppContext) {
        let (_host, handle) = cx.update(|cx| {
            let host = cx.new(|cx| CditorHostView::new(CditorBuilder::new().memory(), cx));
            let handle = host.read(cx).editor.handle.clone();
            (host, handle)
        });

        assert!(cx.read(|cx| handle.is_ready(cx)));
        assert_eq!(
            cx.read(|cx| handle.document_info(cx).unwrap().block_count),
            1
        );
    }
}
