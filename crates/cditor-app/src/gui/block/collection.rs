use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::{CollectionPayload, TextAlign};
use gpui::{
    AnyElement, App, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::text::{RichTextElement, RichTextLayoutInput, RichTextTypography};

pub(crate) fn render_collection_block(
    block_id: BlockId,
    layout_version: u64,
    collection: &CollectionPayload,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    cx: &mut App,
) -> AnyElement {
    let surface_id = SurfaceId::CollectionTitle { block_id };
    let Some(state) = view.read(cx).text_surface_render_state(surface_id) else {
        return div().into_any_element();
    };
    let input = RichTextLayoutInput::from_text_surface_snapshot(
        state.snapshot,
        layout_version,
        TextAlign::Start,
        704.0,
        1,
        1,
    );
    let focus_view = view.clone();
    let title = div()
        .w_full()
        .min_h(px(32.0))
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            focus_view.update(cx, |view, cx| {
                view.focus_text_surface_from_gui_at_position(
                    surface_id,
                    event.position,
                    event.click_count,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(
            RichTextElement::new(input, theme)
                .with_caret(state.caret_offset)
                .with_caret_affinity(state.caret_affinity)
                .with_selection_range(state.selection_range)
                .with_marked_range(state.marked_range)
                .with_typography(RichTextTypography {
                    font_size_px: Some(20.0),
                    line_height_px: Some(28.0),
                    font_weight: Some(gpui::FontWeight::SEMIBOLD),
                })
                .with_placeholder(Some("无标题集合"))
                .with_input_handler(view, focus, state.focused)
                .render(),
        );

    let headers = collection.properties.iter().map(|property| {
        div()
            .min_w(px(140.0))
            .flex_1()
            .px(px(10.0))
            .py(px(7.0))
            .border_r_1()
            .border_color(rgb(theme.border))
            .text_size(px(12.0))
            .text_color(rgb(theme.muted))
            .child(property.name.clone())
    });
    div()
        .w_full()
        .min_h(px(120.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(title)
        .child(
            div()
                .w_full()
                .border_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .children(headers),
                )
                .child(
                    div()
                        .w_full()
                        .min_h(px(48.0))
                        .flex()
                        .items_center()
                        .px(px(10.0))
                        .text_size(px(13.0))
                        .text_color(rgb(theme.muted))
                        .child("空集合"),
                ),
        )
        .into_any_element()
}
