use cditor_core::ids::BlockId;
use cditor_core::rich_text::{CalloutVariant, RichBlockKind};
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    deferred, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;

pub const SLASH_MENU_VISIBLE_ITEMS: usize = 8;
const SLASH_MENU_ROW_HEIGHT_PX: f32 = 48.0;
const SLASH_MENU_WIDTH_PX: f32 = 320.0;
const SLASH_MENU_GROUP_HEIGHT_PX: f32 = 28.0;
const SLASH_MENU_PANEL_PADDING_PX: f32 = 4.0;
const SLASH_MENU_ICON_SIZE_PX: f32 = 36.0;
const SLASH_MENU_VIEWPORT_MARGIN_PX: f32 = 8.0;
const SLASH_MENU_ANCHOR_GAP_PX: f32 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SlashMenuState {
    pub block_id: BlockId,
    pub trigger_start: usize,
    pub query: String,
    pub selected_index: usize,
    pub scroll_start: usize,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashMenuItem {
    pub icon: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub kind: RichBlockKind,
    pub command: Option<SlashMenuCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashMenuCommand {
    AskAi,
}

impl SlashMenuState {
    pub fn new(block_id: BlockId, trigger_start: usize, query: String, x: f32, y: f32) -> Self {
        Self {
            block_id,
            trigger_start,
            query,
            selected_index: 0,
            scroll_start: 0,
            x,
            y,
        }
    }

    pub fn visible_items(&self) -> Vec<SlashMenuItem> {
        slash_menu_items()
            .into_iter()
            .filter(|item| slash_item_matches(item, &self.query))
            .collect()
    }

    pub fn selected_item(&self) -> Option<SlashMenuItem> {
        self.visible_items().get(self.selected_index).cloned()
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        let len = self.visible_items().len();
        if len == 0 {
            self.selected_index = 0;
            self.scroll_start = 0;
            return false;
        }
        let current = self.selected_index.min(len - 1) as isize;
        self.selected_index = (current + delta).rem_euclid(len as isize) as usize;
        self.keep_selected_visible(len);
        true
    }

    pub fn scroll(&mut self, delta_rows: isize) -> bool {
        let len = self.visible_items().len();
        if len <= SLASH_MENU_VISIBLE_ITEMS || delta_rows == 0 {
            return false;
        }
        let max_start = len.saturating_sub(SLASH_MENU_VISIBLE_ITEMS);
        let next = (self.scroll_start as isize + delta_rows).clamp(0, max_start as isize) as usize;
        if next == self.scroll_start {
            return false;
        }
        self.scroll_start = next;
        if self.selected_index < self.scroll_start {
            self.selected_index = self.scroll_start;
        } else if self.selected_index >= self.scroll_start + SLASH_MENU_VISIBLE_ITEMS {
            self.selected_index = self.scroll_start + SLASH_MENU_VISIBLE_ITEMS - 1;
        }
        true
    }

    fn keep_selected_visible(&mut self, len: usize) {
        if len <= SLASH_MENU_VISIBLE_ITEMS {
            self.scroll_start = 0;
        } else if self.selected_index < self.scroll_start {
            self.scroll_start = self.selected_index;
        } else if self.selected_index >= self.scroll_start + SLASH_MENU_VISIBLE_ITEMS {
            self.scroll_start = self.selected_index + 1 - SLASH_MENU_VISIBLE_ITEMS;
        }
    }
}

pub fn slash_menu_items() -> Vec<SlashMenuItem> {
    vec![
        SlashMenuItem {
            icon: "AI",
            label: "Ask AI",
            description: "Write, improve, translate, or transform text.",
            keywords: &["ai", "write", "rewrite", "translate"],
            kind: RichBlockKind::Paragraph,
            command: Some(SlashMenuCommand::AskAi),
        },
        item(
            "T",
            "Text",
            "Just start writing with plain text.",
            &["paragraph", "text"],
            RichBlockKind::Paragraph,
        ),
        item(
            "H1",
            "Heading 1",
            "Big section heading.",
            &["h1", "heading"],
            RichBlockKind::Heading { level: 1 },
        ),
        item(
            "H2",
            "Heading 2",
            "Medium section heading.",
            &["h2", "heading"],
            RichBlockKind::Heading { level: 2 },
        ),
        item(
            "H3",
            "Heading 3",
            "Small section heading.",
            &["h3", "heading"],
            RichBlockKind::Heading { level: 3 },
        ),
        item(
            "[]",
            "Todo",
            "Track a task with a checkbox.",
            &["task", "checkbox"],
            RichBlockKind::Todo { checked: false },
        ),
        item(
            "*",
            "Bulleted list",
            "Create a simple bulleted list.",
            &["bullet", "ul", "list"],
            RichBlockKind::BulletedList,
        ),
        item(
            "1.",
            "Numbered list",
            "Create a list with numbering.",
            &["number", "ol", "list"],
            RichBlockKind::NumberedList,
        ),
        item(
            ">",
            "Toggle",
            "Hide content inside a toggle.",
            &["details"],
            RichBlockKind::Toggle,
        ),
        item(
            "\"",
            "Quote",
            "Capture a quote.",
            &["blockquote"],
            RichBlockKind::Quote,
        ),
        item(
            "!",
            "Callout",
            "Make writing stand out.",
            &["note"],
            RichBlockKind::Callout {
                variant: CalloutVariant::Note,
            },
        ),
        item(
            "</>",
            "Code",
            "Capture a code snippet.",
            &["code block"],
            RichBlockKind::Code { language: None },
        ),
        item(
            "fx",
            "Math",
            "Write a block equation.",
            &["equation"],
            RichBlockKind::Math,
        ),
        item(
            "M",
            "Mermaid",
            "Create a Mermaid diagram.",
            &["diagram"],
            RichBlockKind::Mermaid,
        ),
        item(
            "<>",
            "HTML",
            "Embed an HTML snippet.",
            &["html"],
            RichBlockKind::Html,
        ),
        item(
            "#",
            "Table",
            "Add a simple table.",
            &["grid"],
            RichBlockKind::Table,
        ),
        item(
            "WB",
            "Whiteboard",
            "Sketch and arrange ideas on a canvas.",
            &["board", "canvas", "draw", "diagram", "白板"],
            RichBlockKind::Whiteboard,
        ),
        item(
            "---",
            "Divider",
            "Visually divide blocks.",
            &["hr", "line"],
            RichBlockKind::Divider,
        ),
        item(
            "|",
            "Separator",
            "Add a section separator.",
            &["separator"],
            RichBlockKind::Separator,
        ),
        item(
            "fn",
            "Footnote",
            "Add a footnote definition.",
            &["footnote"],
            RichBlockKind::FootnoteDefinition,
        ),
        item(
            "//",
            "Comment",
            "Add a comment block.",
            &["comment"],
            RichBlockKind::Comment,
        ),
        item(
            "MD",
            "Raw Markdown",
            "Keep text as raw Markdown.",
            &["markdown", "md"],
            RichBlockKind::RawMarkdown,
        ),
    ]
}

fn item(
    icon: &'static str,
    label: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
    kind: RichBlockKind,
) -> SlashMenuItem {
    SlashMenuItem {
        icon,
        label,
        description,
        keywords,
        kind,
        command: None,
    }
}

fn slash_item_matches(item: &SlashMenuItem, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || item.label.to_lowercase().contains(&query)
        || item.keywords.iter().any(|keyword| keyword.contains(&query))
}

pub fn slash_query_before_caret(text: &str, caret: usize) -> Option<(usize, String)> {
    let caret = floor_char_boundary(text, caret);
    let before = &text[..caret];
    let start = before.rfind('/')?;
    if start > 0
        && !before[..start]
            .chars()
            .last()
            .is_some_and(char::is_whitespace)
    {
        return None;
    }
    let query = &before[start + 1..];
    (!query.chars().any(char::is_whitespace)).then(|| (start, query.to_owned()))
}

fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn render_slash_menu(
    state: &SlashMenuState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    viewport_width: f32,
    viewport_height: f32,
) -> AnyElement {
    let items = state.visible_items();
    let total_items = items.len();
    let height = slash_menu_panel_height(total_items);
    let (x, y) =
        slash_menu_panel_position(state.x, state.y, height, viewport_width, viewport_height);
    let mut panel = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(SLASH_MENU_WIDTH_PX))
        .h(px(height))
        .flex()
        .flex_col()
        .p(px(SLASH_MENU_PANEL_PADDING_PX))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation()
        })
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.cancel_slash_menu(cx);
                });
            }
        })
        .on_scroll_wheel({
            let view = view.clone();
            move |event, _window, cx| {
                let delta_y = f32::from(event.delta.pixel_delta(px(SLASH_MENU_ROW_HEIGHT_PX)).y);
                let rows = slash_scroll_delta_rows(delta_y);
                if rows != 0 {
                    let _ = view.update(cx, |view, cx| {
                        view.scroll_slash_menu_from_gui(rows, cx);
                    });
                }
                cx.stop_propagation();
            }
        })
        .child(render_slash_menu_group_header(theme));

    if items.is_empty() {
        panel = panel.child(
            div()
                .h(px(SLASH_MENU_ROW_HEIGHT_PX))
                .flex()
                .items_center()
                .px(px(12.0))
                .text_size(px(12.0))
                .text_color(rgb(theme.muted))
                .child("No matching blocks"),
        );
    } else {
        panel = panel.children(
            items
                .into_iter()
                .enumerate()
                .skip(state.scroll_start)
                .take(SLASH_MENU_VISIBLE_ITEMS)
                .map(|(index, item)| {
                    render_slash_menu_row(
                        index,
                        item,
                        index == state.selected_index,
                        theme,
                        view.clone(),
                    )
                }),
        );
        if total_items > SLASH_MENU_VISIBLE_ITEMS {
            panel = panel.child(render_slash_scrollbar(
                theme,
                total_items,
                state.scroll_start,
            ));
        }
    }
    deferred(panel).with_priority(120).into_any_element()
}

fn slash_menu_panel_height(total_items: usize) -> f32 {
    SLASH_MENU_PANEL_PADDING_PX * 2.0
        + SLASH_MENU_GROUP_HEIGHT_PX
        + total_items.min(SLASH_MENU_VISIBLE_ITEMS).max(1) as f32 * SLASH_MENU_ROW_HEIGHT_PX
}

fn slash_menu_panel_position(
    anchor_x: f32,
    anchor_y: f32,
    panel_height: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    let usable_width = viewport_width.max(SLASH_MENU_VIEWPORT_MARGIN_PX * 2.0);
    let usable_height = viewport_height.max(SLASH_MENU_VIEWPORT_MARGIN_PX * 2.0);
    let max_x = (usable_width - SLASH_MENU_VIEWPORT_MARGIN_PX - SLASH_MENU_WIDTH_PX)
        .max(SLASH_MENU_VIEWPORT_MARGIN_PX);
    let x = anchor_x.clamp(SLASH_MENU_VIEWPORT_MARGIN_PX, max_x);
    let below_y = anchor_y + SLASH_MENU_ANCHOR_GAP_PX;
    let below_bottom = below_y + panel_height;
    let y = if below_bottom <= usable_height - SLASH_MENU_VIEWPORT_MARGIN_PX {
        below_y
    } else {
        (anchor_y - SLASH_MENU_ANCHOR_GAP_PX - panel_height)
            .max(SLASH_MENU_VIEWPORT_MARGIN_PX)
            .min(
                (usable_height - SLASH_MENU_VIEWPORT_MARGIN_PX - panel_height)
                    .max(SLASH_MENU_VIEWPORT_MARGIN_PX),
            )
    };
    (x, y)
}

fn render_slash_scrollbar(theme: GuiTheme, total_items: usize, scroll_start: usize) -> AnyElement {
    let visible = SLASH_MENU_VISIBLE_ITEMS.min(total_items);
    let track_height = visible as f32 * SLASH_MENU_ROW_HEIGHT_PX - 8.0;
    let thumb_height = (track_height * visible as f32 / total_items as f32).max(24.0);
    let max_start = total_items.saturating_sub(visible).max(1);
    let max_top = (track_height - thumb_height).max(0.0);
    let thumb_top = max_top * scroll_start.min(max_start) as f32 / max_start as f32;

    div()
        .absolute()
        .right(px(3.0))
        .top(px(SLASH_MENU_PANEL_PADDING_PX
            + SLASH_MENU_GROUP_HEIGHT_PX
            + 4.0))
        .w(px(3.0))
        .h(px(track_height))
        .rounded(px(2.0))
        .bg(rgb(theme.border))
        .child(
            div()
                .absolute()
                .top(px(thumb_top))
                .w(px(3.0))
                .h(px(thumb_height))
                .rounded(px(2.0))
                .bg(rgb(theme.scrollbar)),
        )
        .into_any_element()
}

fn render_slash_menu_group_header(theme: GuiTheme) -> AnyElement {
    div()
        .flex_none()
        .h(px(SLASH_MENU_GROUP_HEIGHT_PX))
        .flex()
        .items_center()
        .px(px(8.0))
        .text_size(px(11.0))
        .text_color(rgb(theme.muted))
        .child("Basic blocks")
        .into_any_element()
}

fn render_slash_menu_row(
    index: usize,
    item: SlashMenuItem,
    selected: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let background = if selected {
        theme.hover_surface
    } else {
        theme.panel
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .w_full()
        .h(px(SLASH_MENU_ROW_HEIGHT_PX))
        .gap(px(10.0))
        .px(px(8.0))
        .rounded(px(4.0))
        .bg(rgb(background))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.hover_surface)).cursor_pointer())
        .on_mouse_move({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.select_slash_menu_index_from_gui(index, cx);
                });
            }
        })
        .child(
            div()
                .flex_none()
                .w(px(SLASH_MENU_ICON_SIZE_PX))
                .h(px(SLASH_MENU_ICON_SIZE_PX))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.panel))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(rgb(theme.text))
                .child(item.icon),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(theme.text))
                        .child(item.label),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.muted))
                        .child(item.description),
                ),
        )
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.apply_slash_menu_index_from_gui(index, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

pub fn slash_scroll_delta_rows(delta_y: f32) -> isize {
    if delta_y.abs() < 1.0 {
        return 0;
    }
    let rows = (delta_y.abs() / SLASH_MENU_ROW_HEIGHT_PX).ceil().max(1.0) as isize;
    if delta_y > 0.0 { -rows } else { rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_query_detects_active_token_before_caret() {
        assert_eq!(
            slash_query_before_caret("/he", 3),
            Some((0, "he".to_owned()))
        );
        assert_eq!(
            slash_query_before_caret("x /to", 5),
            Some((2, "to".to_owned()))
        );
        assert_eq!(slash_query_before_caret("x/to", 4), None);
        assert_eq!(slash_query_before_caret("/two words", 10), None);
    }

    #[test]
    fn slash_query_handles_caret_inside_multibyte_ime_text() {
        let text = "            埃塞    ";
        assert_eq!(slash_query_before_caret(text, 3), None);

        let text = "/埃塞";
        assert_eq!(slash_query_before_caret(text, 3), Some((0, String::new())));
        assert_eq!(
            slash_query_before_caret(text, text.len()),
            Some((0, "埃塞".to_owned()))
        );
    }

    #[test]
    fn slash_menu_contains_supported_block_kinds() {
        let items = slash_menu_items();
        assert!(
            items
                .iter()
                .any(|item| item.kind == RichBlockKind::Paragraph)
        );
        assert!(
            items
                .iter()
                .any(|item| item.kind == RichBlockKind::Code { language: None })
        );
        assert!(items.iter().any(|item| item.kind == RichBlockKind::Table));
        assert!(
            items
                .iter()
                .any(|item| item.kind == RichBlockKind::Whiteboard)
        );
        assert!(
            items
                .iter()
                .all(|item| !item.icon.is_empty() && !item.description.is_empty())
        );
    }

    #[test]
    fn slash_menu_finds_whiteboard_by_canvas_keyword() {
        let items = slash_menu_items();
        let whiteboard = items
            .iter()
            .find(|item| item.kind == RichBlockKind::Whiteboard)
            .expect("whiteboard slash item");

        assert!(slash_item_matches(whiteboard, "whiteboard"));
        assert!(slash_item_matches(whiteboard, "canvas"));
        assert!(slash_item_matches(whiteboard, "draw"));
        assert!(slash_item_matches(whiteboard, "白板"));
    }

    #[test]
    fn slash_scroll_delta_maps_to_rows() {
        assert_eq!(slash_scroll_delta_rows(0.5), 0);
        assert_eq!(slash_scroll_delta_rows(1.0), -1);
        assert_eq!(slash_scroll_delta_rows(49.0), -2);
        assert_eq!(slash_scroll_delta_rows(-49.0), 2);
    }

    #[test]
    fn slash_menu_position_clamps_to_viewport_edges() {
        let (x, y) = slash_menu_panel_position(780.0, 40.0, 120.0, 800.0, 600.0);
        assert_eq!(x, 472.0);
        assert_eq!(y, 44.0);
    }

    #[test]
    fn slash_menu_position_flips_above_when_bottom_would_overflow() {
        let (x, y) = slash_menu_panel_position(120.0, 590.0, 272.0, 800.0, 600.0);
        assert_eq!(x, 120.0);
        assert_eq!(y, 314.0);
    }

    #[test]
    fn slash_menu_panel_height_uses_visible_row_limit() {
        assert_eq!(slash_menu_panel_height(0), 84.0);
        assert_eq!(slash_menu_panel_height(3), 180.0);
        assert_eq!(slash_menu_panel_height(20), 420.0);
    }

    #[test]
    fn slash_menu_position_clamps_wider_notion_panel_in_narrow_viewport() {
        let (x, y) = slash_menu_panel_position(300.0, 300.0, 420.0, 300.0, 600.0);

        assert_eq!(x, SLASH_MENU_VIEWPORT_MARGIN_PX);
        assert_eq!(y, SLASH_MENU_VIEWPORT_MARGIN_PX);
    }
}
