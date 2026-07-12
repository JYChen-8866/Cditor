use std::rc::Rc;

use ding_board::{Scene, WhiteboardStyle, WhiteboardView};
use gpui::*;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.activate(true);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1100.0), px(760.0)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Ding Board · 白板".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                ..WindowOptions::default()
            },
            |_window, cx| {
                cx.new(|cx| {
                    let mut whiteboard =
                        WhiteboardView::new(Scene::default(), Rc::new(light_style), cx);
                    whiteboard.add_mindmap_seed(360.0, 240.0, cx);
                    whiteboard.set_on_change(Rc::new(|json, _window, _cx| {
                        println!("scene changed: {json}");
                    }));
                    whiteboard
                })
            },
        )
        .expect("failed to open board window");
    });
}

fn light_style() -> WhiteboardStyle {
    WhiteboardStyle {
        bg: rgb(0xf8fafc).into(),
        grid: rgba(0x94a3b866).into(),
        text: rgb(0x64748b).into(),
        ink: rgb(0x0f172a).into(),
        panel: rgba(0xffffffee).into(),
        panel_strong: rgb(0xffffff).into(),
        accent: rgb(0xdbeafe).into(),
        selection: rgb(0x2563eb).into(),
        swatches: [0x0f172a, 0x2563eb, 0x16a34a, 0xdc2626, 0x9333ea, 0xf59e0b]
            .into_iter()
            .map(|color| rgb(color).into())
            .collect(),
    }
}
