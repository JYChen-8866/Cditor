use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, IntoElement, ParentElement, SharedString, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::rich_text::{
    NOTION_INLINE_CODE_PADDING_X_PX, NOTION_INLINE_CODE_PADDING_Y_PX, NOTION_INLINE_CODE_RADIUS_PX,
    NOTION_INLINE_CODE_TEXT_SIZE_PX, NOTION_MONO_FONT_FAMILY,
};

use super::VisualRun;

pub(super) fn render_visual_run_segments(
    text: &str,
    run: &VisualRun,
    theme: GuiTheme,
    marked_range: Option<&Range<usize>>,
) -> Vec<AnyElement> {
    let Some(marked_range) = marked_range else {
        return vec![render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.clone(),
            false,
        )];
    };
    let marked_start = run.logical_range.start.max(marked_range.start);
    let marked_end = run.logical_range.end.min(marked_range.end);
    if marked_start >= marked_end {
        return vec![render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.clone(),
            false,
        )];
    }

    let mut segments = Vec::with_capacity(3);
    if run.logical_range.start < marked_start {
        segments.push(render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.start..marked_start,
            false,
        ));
    }
    segments.push(render_visual_run_segment(
        text,
        run,
        theme,
        marked_start..marked_end,
        true,
    ));
    if marked_end < run.logical_range.end {
        segments.push(render_visual_run_segment(
            text,
            run,
            theme,
            marked_end..run.logical_range.end,
            false,
        ));
    }
    segments
}

fn render_visual_run_segment(
    text: &str,
    run: &VisualRun,
    theme: GuiTheme,
    range: Range<usize>,
    marked: bool,
) -> AnyElement {
    let label = text.get(range).unwrap_or_default().to_owned();
    div()
        .when(run.mark_style.code, |this| {
            this.px(px(NOTION_INLINE_CODE_PADDING_X_PX))
                .py(px(NOTION_INLINE_CODE_PADDING_Y_PX))
                .rounded(px(NOTION_INLINE_CODE_RADIUS_PX))
                .bg(rgb(theme.inline_code_background))
                .font_family(NOTION_MONO_FONT_FAMILY)
                .text_size(px(NOTION_INLINE_CODE_TEXT_SIZE_PX))
        })
        .when(run.mark_style.bold, |this| {
            this.font_weight(FontWeight::BOLD)
        })
        .when(run.mark_style.italic, |this| this.italic())
        .when(
            marked || run.mark_style.underline || run.mark_style.link,
            |this| this.text_decoration_1(),
        )
        .when(run.mark_style.strike, |this| this.line_through())
        .text_color(rgb(if run.mark_style.link {
            theme.focused
        } else if run.mark_style.code {
            theme.inline_code_text
        } else {
            theme.text
        }))
        .child(SharedString::from(label))
        .into_any_element()
}
