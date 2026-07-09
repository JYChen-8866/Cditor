use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::skeleton::{SkeletonItem, SkeletonRows, SkeletonVariant};
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

pub fn render_block_skeleton(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    match &block.kind {
        RichBlockKind::Heading { level } => render_heading_skeleton(*level, theme),
        RichBlockKind::Code { .. } => render_code_skeleton(theme),
        RichBlockKind::Quote => render_quote_skeleton(theme),
        RichBlockKind::Callout { .. } => render_callout_skeleton(theme),
        RichBlockKind::Image => render_image_skeleton(theme),
        RichBlockKind::Table => render_table_skeleton(theme),
        RichBlockKind::Divider | RichBlockKind::Separator => render_divider_skeleton(theme),
        RichBlockKind::Todo { .. } | RichBlockKind::BulletedList | RichBlockKind::NumberedList => {
            render_list_skeleton(theme)
        }
        _ => render_paragraph_skeleton(theme),
    }
}

fn render_heading_skeleton(level: u8, theme: GuiTheme) -> AnyElement {
    let width = match level {
        1 => gpui::relative(0.58),
        2 => gpui::relative(0.64),
        _ => gpui::relative(0.7),
    };
    let height = match level {
        1 => 28.0,
        2 => 24.0,
        _ => 20.0,
    };
    SkeletonItem::new(SkeletonVariant::Heading)
        .width(width)
        .height_px(height)
        .render(theme)
}

fn render_paragraph_skeleton(theme: GuiTheme) -> AnyElement {
    SkeletonRows::new(2)
        .last_width(gpui::relative(0.68))
        .render(theme)
}

fn render_list_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap_3()
        .w_full()
        .child(
            SkeletonItem::new(SkeletonVariant::Circle)
                .width(px(14.0))
                .height_px(14.0)
                .render(theme),
        )
        .child(
            SkeletonRows::new(2)
                .last_width(gpui::relative(0.52))
                .render(theme),
        )
        .into_any_element()
}

fn render_quote_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .flex()
        .gap_3()
        .w_full()
        .child(
            div()
                .w(px(3.0))
                .h(px(44.0))
                .rounded(px(2.0))
                .bg(rgb(theme.border)),
        )
        .child(
            SkeletonRows::new(2)
                .last_width(gpui::relative(0.56))
                .render(theme),
        )
        .into_any_element()
}

fn render_callout_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .p_3()
        .flex()
        .gap_3()
        .child(
            SkeletonItem::new(SkeletonVariant::Circle)
                .width(px(18.0))
                .height_px(18.0)
                .render(theme),
        )
        .child(
            SkeletonRows::new(2)
                .last_width(gpui::relative(0.48))
                .render(theme),
        )
        .into_any_element()
}

fn render_code_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .p_3()
        .child(
            SkeletonRows::new(4)
                .row_height_px(12.0)
                .width(gpui::relative(0.72))
                .last_width(gpui::relative(0.42))
                .gap_px(7.0)
                .render(theme),
        )
        .into_any_element()
}

fn render_image_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(
            SkeletonItem::new(SkeletonVariant::Image)
                .width(gpui::relative(0.72))
                .height_px(180.0)
                .render(theme),
        )
        .into_any_element()
}

fn render_table_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .overflow_hidden()
        .children((0..3).map(|row| {
            div()
                .flex()
                .gap_2()
                .p_2()
                .when(row == 0, |this| this.bg(rgb(theme.surface)))
                .children((0..3).map(|col| {
                    let width = if col == 2 {
                        gpui::relative(0.22)
                    } else {
                        gpui::relative(0.32)
                    };
                    SkeletonItem::new(SkeletonVariant::Text)
                        .width(width)
                        .height_px(12.0)
                        .render(theme)
                }))
        }))
        .into_any_element()
}

fn render_divider_skeleton(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .py_2()
        .child(div().h(px(1.0)).bg(rgb(theme.border)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use cditor_core::block::BlockChromeSnapshot;
    use cditor_core::layout::BlockLayoutMeta;
    use cditor_core::rich_text::{BlockAttrs, BlockPayloadView};

    use super::*;

    fn block(kind: RichBlockKind) -> ViewBlockSnapshot {
        ViewBlockSnapshot {
            block_id: 1,
            visible_index: 0,
            depth: 0,
            chrome: BlockChromeSnapshot::plain(),
            kind,
            attrs: BlockAttrs::default(),
            payload: BlockPayloadView::Placeholder {
                estimated_height: 32.0,
            },
            layout: BlockLayoutMeta::new(1, 32.0),
            selected: false,
            selection_range: None,
            focused: false,
            caret_offset: None,
            marked_range: None,
            table_view: None,
            focused_table_cell: None,
            focused_table_cell_offset: None,
            pinned: false,
            placeholder: false,
        }
    }

    #[test]
    fn block_skeleton_supports_major_kinds() {
        for kind in [
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Code { language: None },
            RichBlockKind::Quote,
            RichBlockKind::Image,
            RichBlockKind::Table,
            RichBlockKind::Todo { checked: false },
        ] {
            let _ = render_block_skeleton(&block(kind), GuiTheme::light());
        }
    }
}
