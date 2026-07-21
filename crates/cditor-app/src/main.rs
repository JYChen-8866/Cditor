#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cditor_api::Cditor;
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, px, size,
};

struct CditorHostView {
    view: gpui::Entity<cditor_editor::app::CditorV2View>,
}

impl CditorHostView {
    fn from_cditor(cditor: Cditor, cx: &mut Context<Self>) -> Self {
        let options = cditor.into_options();
        let runtime = cditor_runtime::DocumentRuntime::demo();
        let view = cx.new(|cx| {
            cditor_editor::app::CditorV2View::from_runtime_with_options(
                runtime,
                options.debug_overlay,
                options.readonly,
                cx,
            )
        });
        Self { view }
    }
}

impl Render for CditorHostView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

fn cditor_from_env() -> Cditor {
    Cditor::new().demo()
}

fn main() {
    let cditor = cditor_from_env();

    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cditor_editor::input::actions::bind_cditor_keys(cx);
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
            |_window, cx| cx.new(|cx| CditorHostView::from_cditor(cditor.clone(), cx)),
        )
        .expect("open Cditor window");
    });
}
