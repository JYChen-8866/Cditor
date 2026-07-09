use std::ops::Range;
use std::{cell::RefCell, rc::Rc};

use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, Entity, FocusHandle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Size,
    Style, TextAlign, TextRun, UnderlineStyle, Window, WrappedLine, fill, point, px, rgb, rgba,
};

use crate::gui::GuiTheme;
use crate::gui::app::{CditorV2View, GuiPlatformInputTarget};
use crate::gui::input::platform_adapter::handle_registered_platform_input;
use crate::gui::text::{
    RichTextPlatformLayout, platform_cursor_bounds_for_offset, platform_range_segment_bounds,
};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::TableCellAlign;
use cditor_runtime::TableCellPosition;

use super::style::{table_active_border_color, table_cell_line_height, table_cell_text_size};
use super::trace_table;

pub(super) struct TableCellTextElement {
    block_id: BlockId,
    content_version: u64,
    position: TableCellPosition,
    text: String,
    active: bool,
    caret_offset: Option<usize>,
    marked_range: Option<Range<usize>>,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    align: TableCellAlign,
}

pub struct TableCellTextPrepaintState {
    lines: Vec<WrappedLine>,
    cursor: Option<gpui::PaintQuad>,
    marked_backgrounds: Vec<gpui::PaintQuad>,
    line_height: Pixels,
}

impl TableCellTextElement {
    pub(super) fn new(
        block_id: BlockId,
        content_version: u64,
        position: TableCellPosition,
        text: String,
        active: bool,
        caret_offset: Option<usize>,
        marked_range: Option<Range<usize>>,
        theme: GuiTheme,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        align: TableCellAlign,
    ) -> Self {
        Self {
            block_id,
            content_version,
            position,
            text,
            active,
            caret_offset,
            marked_range,
            theme,
            view,
            focus,
            align,
        }
    }
}

impl IntoElement for TableCellTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TableCellTextElement {
    type RequestLayoutState = Rc<RefCell<Option<Vec<WrappedLine>>>>;
    type PrepaintState = TableCellTextPrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(
            (
                "cditor-table-cell-text",
                self.block_id ^ ((self.position.row as u64) << 32) ^ self.position.col as u64,
            )
                .into(),
        )
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let shared_lines = Rc::new(RefCell::new(None));
        let shared_lines_clone = shared_lines.clone();
        let display_text = self.display_text();
        let text = SharedString::from(display_text.clone());
        let text_size = table_cell_text_size();
        let runs = table_cell_text_runs(
            &display_text,
            self.marked_range.as_ref(),
            self.theme,
            self.placeholder_visible(),
            window,
        );
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.min_size.width = px(0.0).into();
        style.max_size.width = gpui::relative(1.0).into();

        let layout_id =
            window.request_measured_layout(style, move |known, available, window, _cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    AvailableSpace::MinContent => Some(px(1.0)),
                    AvailableSpace::MaxContent => Some(window.viewport_size().width.max(px(1.0))),
                });
                match window.text_system().shape_text(
                    text.clone(),
                    text_size,
                    &runs,
                    wrap_width,
                    None,
                ) {
                    Ok(lines) => {
                        let lines = lines.into_vec();
                        let mut total_size: Size<Pixels> = Size::default();
                        let line_height = table_cell_line_height();
                        for line in &lines {
                            let size = line.size(line_height);
                            total_size.height += size.height;
                            total_size.width = total_size.width.max(size.width);
                        }
                        total_size.height = total_size.height.max(line_height);
                        *shared_lines_clone.borrow_mut() = Some(lines);
                        total_size
                    }
                    Err(_) => Size::default(),
                }
            });

        (layout_id, shared_lines)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let lines = request_layout.borrow_mut().take().unwrap_or_default();
        let line_height = table_cell_line_height();
        trace_table(
            "cell.prepaint",
            format_args!(
                "block={} row={} col={} bounds=({}, {}, {}, {}) text_len={} lines={} caret={:?} marked={:?}",
                self.block_id,
                self.position.row,
                self.position.col,
                f32::from(bounds.left()),
                f32::from(bounds.top()),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                self.text.len(),
                lines.len(),
                self.caret_offset,
                self.marked_range
            ),
        );
        let cursor = if self.active && self.marked_range.is_none() {
            platform_cursor_bounds_for_offset(
                &lines,
                bounds,
                line_height,
                &self.text,
                self.caret_offset.unwrap_or(self.text.len()),
                px(1.5),
            )
            .map(|bounds| fill(bounds, rgb(table_active_border_color(self.theme))))
        } else {
            None
        };
        let marked_backgrounds = self
            .marked_range
            .clone()
            .map(|range| {
                platform_range_segment_bounds(&lines, bounds, line_height, &self.text, range)
                    .into_iter()
                    .map(|segment| fill(segment, rgba(0x0969da1f)))
                    .collect()
            })
            .unwrap_or_default();

        TableCellTextPrepaintState {
            lines,
            cursor,
            marked_backgrounds,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.active {
            handle_registered_platform_input(
                &self.view,
                &self.focus,
                GuiPlatformInputTarget::TableCell {
                    block_id: self.block_id,
                    row: self.position.row,
                    col: self.position.col,
                },
                bounds,
                window,
                cx,
            );
        }
        trace_table(
            "cell.paint",
            format_args!(
                "block={} row={} col={} bounds=({}, {}, {}, {}) text_len={} lines={}",
                self.block_id,
                self.position.row,
                self.position.col,
                f32::from(bounds.left()),
                f32::from(bounds.top()),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                self.text.len(),
                prepaint.lines.len()
            ),
        );

        for background in prepaint.marked_backgrounds.drain(..) {
            window.paint_quad(background);
        }

        let mut y_offset = Pixels::default();
        let text_align = gpui_text_align(self.align);
        for line in &prepaint.lines {
            line.paint(
                point(bounds.left(), bounds.top() + y_offset),
                prepaint.line_height,
                text_align,
                None,
                window,
                cx,
            )
            .ok();
            y_offset += line.size(prepaint.line_height).height;
        }

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        let cache = RichTextPlatformLayout {
            block_id: self.block_id,
            content_version: self.content_version,
            text: self.text.clone(),
            lines: std::mem::take(&mut prepaint.lines),
            bounds,
            line_height: prepaint.line_height,
            measured_height: f64::from(bounds.size.height),
            table_cell_position: Some(self.position),
        };
        self.view.update(cx, |view, _cx| {
            view.update_text_layout_cache(cache);
        });
    }
}

fn gpui_text_align(align: TableCellAlign) -> TextAlign {
    match align {
        TableCellAlign::Left => TextAlign::Left,
        TableCellAlign::Center => TextAlign::Center,
        TableCellAlign::Right => TextAlign::Right,
    }
}

impl TableCellTextElement {
    fn placeholder_visible(&self) -> bool {
        table_cell_placeholder_visible(self.active, &self.text)
    }

    fn display_text(&self) -> String {
        if self.placeholder_visible() {
            "请输入...".to_owned()
        } else {
            self.text.clone()
        }
    }
}

fn table_cell_text_runs(
    text: &str,
    marked_range: Option<&Range<usize>>,
    theme: GuiTheme,
    placeholder: bool,
    window: &Window,
) -> Vec<TextRun> {
    let mut font = window.text_style().font();
    font.weight = FontWeight::NORMAL;
    if text.is_empty() {
        return vec![TextRun {
            len: 0,
            font,
            color: Hsla::from(rgb(if placeholder { theme.muted } else { theme.text })),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let mut boundaries = vec![0, text.len()];
    if let Some(range) = marked_range {
        boundaries.push(range.start.min(text.len()));
        boundaries.push(range.end.min(text.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    boundaries
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            (start < end).then(|| {
                let marked = table_cell_segment_is_marked(start..end, marked_range);
                TextRun {
                    len: end - start,
                    font: font.clone(),
                    color: Hsla::from(rgb(if placeholder { theme.muted } else { theme.text })),
                    background_color: None,
                    underline: marked.then_some(UnderlineStyle {
                        color: Some(Hsla::from(rgb(theme.focused))),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    strikethrough: None,
                }
            })
        })
        .collect()
}

fn table_cell_segment_is_marked(
    segment: Range<usize>,
    marked_range: Option<&Range<usize>>,
) -> bool {
    marked_range
        .map(|range| segment.start < range.end && range.start < segment.end)
        .unwrap_or(false)
}

fn table_cell_placeholder_visible(active: bool, text: &str) -> bool {
    !active && text.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cell_marked_segment_detection_only_matches_overlap() {
        let marked = 2..5;

        assert!(!table_cell_segment_is_marked(0..2, Some(&marked)));
        assert!(table_cell_segment_is_marked(2..5, Some(&marked)));
        assert!(table_cell_segment_is_marked(4..8, Some(&marked)));
        assert!(!table_cell_segment_is_marked(5..8, Some(&marked)));
        assert!(!table_cell_segment_is_marked(2..5, None));
    }

    #[test]
    fn table_cell_placeholder_is_hidden_while_editing_empty_cell() {
        assert!(table_cell_placeholder_visible(false, ""));
        assert!(!table_cell_placeholder_visible(true, ""));
        assert!(!table_cell_placeholder_visible(false, "cell"));
    }
}
