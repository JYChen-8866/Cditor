use dc_board::BoardView;
use gpui::*;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.activate(true);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("DC Board".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| BoardView::new(cx)),
        )
        .expect("open DC Board window");
    });
}
