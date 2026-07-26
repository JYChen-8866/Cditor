use cditor_whiteboard_drafft::{
    CANVAS_FONT_FAMILY, DrafftBoardView, UI_FONT_FAMILY, bind_drafft_keys, bundled_fonts,
};
use gpui::{
    AppContext, Bounds, Point, TitlebarOptions, WindowBounds, WindowOptions, font, px, size,
};

fn main() {
    gpui_platform::application().run(|cx| {
        bind_drafft_keys(cx);
        cx.text_system()
            .add_fonts(bundled_fonts())
            .expect("register Drafft UI fonts");
        let font_names = cx.text_system().all_font_names();
        eprintln!(
            "[drafft-ui] fonts_registered={} matching_names={:?}",
            [UI_FONT_FAMILY, CANVAS_FONT_FAMILY]
                .into_iter()
                .all(|family| font_names.iter().any(|name| name == family)),
            font_names
                .iter()
                .filter(|name| matches!(name.as_str(), "Assistant" | "Virgil"))
                .collect::<Vec<_>>()
        );
        for family in [UI_FONT_FAMILY, CANVAS_FONT_FAMILY, "HanziPen SC"] {
            let font_id = cx.text_system().resolve_font(&font(family));
            eprintln!(
                "[drafft-ui] requested_font={family:?} resolved_font={:?}",
                cx.text_system().get_font_for_id(font_id),
            );
        }
        cx.activate(true);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1100.0), px(760.0)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Drafft Ink - GPUI".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                ..WindowOptions::default()
            },
            |_window, cx| cx.new(DrafftBoardView::new),
        )
        .expect("open Drafft GPUI window");
    });
}
