use cditor_component::{
    InteractiveScrollbar, InteractiveScrollbarStyle, PopupMenu, PopupMenuIcon, PopupMenuItem,
    PopupMenuStyle, ScrollbarAxis, SvgIcon,
};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::RichBlockKind;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    deferred, div, px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::menu_metrics::{EditorViewport, PRIMARY_MENU_WIDTH_PX, SECONDARY_MENU_WIDTH_PX};
use crate::overlays::callout_menu::CALLOUT_MENU_ITEMS;
use crate::presentation::block_registry::block_presentation_registry;
use crate::theme::GuiTheme;

pub const SLASH_MENU_VISIBLE_ITEMS: usize = 8;
const SLASH_MENU_ROW_HEIGHT_PX: f32 = 48.0;
const SLASH_MENU_WIDTH_PX: f32 = PRIMARY_MENU_WIDTH_PX;
const SLASH_MENU_GROUP_HEIGHT_PX: f32 = 28.0;
const SLASH_MENU_PANEL_PADDING_PX: f32 = 4.0;
const SLASH_MENU_ICON_SIZE_PX: f32 = 36.0;
const SLASH_MENU_ICON_GLYPH_SIZE_PX: f32 = 20.0;
const SLASH_MENU_LABEL_SIZE_PX: f32 = 14.0;
const SLASH_MENU_DESCRIPTION_SIZE_PX: f32 = 12.0;
const SLASH_MENU_GROUP_SIZE_PX: f32 = 12.0;
const SLASH_MENU_VIEWPORT_MARGIN_PX: f32 = 8.0;
const SLASH_MENU_ANCHOR_GAP_PX: f32 = 4.0;
const SLASH_MENU_SVG_ICON_SIZE_PX: f32 = 20.0;

const ICON_AI: &[u8] = include_bytes!("../../../../assets/icons/inline-ai.svg");
const ICON_TEXT: &[u8] = include_bytes!("../../../../assets/icons/text.svg");
const ICON_HEADING_1: &[u8] = include_bytes!("../../../../assets/icons/heading-1.svg");
const ICON_HEADING_2: &[u8] = include_bytes!("../../../../assets/icons/heading-2.svg");
const ICON_HEADING_3: &[u8] = include_bytes!("../../../../assets/icons/heading-3.svg");
const ICON_TODO: &[u8] = include_bytes!("../../../../assets/icons/todo.svg");
const ICON_BULLETED_LIST: &[u8] = include_bytes!("../../../../assets/icons/bulleted-list.svg");
const ICON_NUMBER_LIST: &[u8] = include_bytes!("../../../../assets/icons/number-list.svg");
const ICON_QUOTE: &[u8] = include_bytes!("../../../../assets/icons/quote.svg");
const ICON_CALLOUT: &[u8] = include_bytes!("../../../../assets/icons/callout.svg");
const ICON_CODE: &[u8] = include_bytes!("../../../../assets/icons/code.svg");
const ICON_MATH: &[u8] = include_bytes!("../../../../assets/icons/math.svg");
const ICON_MERMAID: &[u8] = include_bytes!("../../../../assets/icons/mermaid.svg");
const ICON_TABLE: &[u8] = include_bytes!("../../../../assets/icons/table.svg");
const ICON_WHITEBOARD: &[u8] = include_bytes!("../../../../assets/icons/whiteboard.svg");
const ICON_DIVIDER: &[u8] = include_bytes!("../../../../assets/icons/divider.svg");
const ICON_SUBMENU_ARROW: &[u8] = include_bytes!("../../../../assets/icons/jiantou.svg");

#[derive(Debug, Clone, PartialEq)]
pub struct SlashMenuState {
    pub block_id: BlockId,
    pub trigger_start: usize,
    pub query: String,
    pub selected_index: usize,
    pub scroll_start: usize,
    pub x: f32,
    pub y: f32,
    pub callout_submenu_open: bool,
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
            callout_submenu_open: false,
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
        self.callout_submenu_open = false;
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
        self.set_scroll_start(next)
    }

    pub fn set_scroll_start(&mut self, scroll_start: usize) -> bool {
        let len = self.visible_items().len();
        let next = scroll_start.min(len.saturating_sub(SLASH_MENU_VISIBLE_ITEMS));
        if next == self.scroll_start {
            return false;
        }
        self.scroll_start = next;
        if self.selected_index < self.scroll_start {
            self.selected_index = self.scroll_start;
        } else if self.selected_index >= self.scroll_start + SLASH_MENU_VISIBLE_ITEMS {
            self.selected_index = self.scroll_start + SLASH_MENU_VISIBLE_ITEMS - 1;
        }
        self.callout_submenu_open = false;
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
    let mut items = vec![SlashMenuItem {
        icon: "AI",
        label: "Ask AI",
        description: "Write, improve, translate, or transform text.",
        keywords: &["ai", "write", "rewrite", "translate"],
        kind: RichBlockKind::Paragraph,
        command: Some(SlashMenuCommand::AskAi),
    }];
    items.extend(
        block_presentation_registry()
            .slash_presentations()
            .into_iter()
            .filter(|presentation| slash_menu_supports_kind(&presentation.kind))
            .map(|presentation| SlashMenuItem {
                icon: presentation.icon,
                label: presentation.label,
                description: presentation.description,
                keywords: presentation.keywords,
                kind: presentation.kind,
                command: None,
            }),
    );
    items
}

fn slash_menu_supports_kind(kind: &RichBlockKind) -> bool {
    !matches!(
        kind,
        RichBlockKind::Toggle
            | RichBlockKind::Html
            | RichBlockKind::Separator
            | RichBlockKind::FootnoteDefinition
            | RichBlockKind::Comment
            | RichBlockKind::RawMarkdown
    )
}

fn slash_item_matches(item: &SlashMenuItem, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || item.label.to_lowercase().contains(&query)
        || item.keywords.iter().any(|keyword| keyword.contains(&query))
        || (item.is_callout()
            && CALLOUT_MENU_ITEMS
                .iter()
                .any(|variant| variant.label[1..].to_lowercase().contains(&query)))
}

impl SlashMenuItem {
    pub fn is_callout(&self) -> bool {
        self.command.is_none() && matches!(self.kind, RichBlockKind::Callout { .. })
    }
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

pub fn build_slash_callout_popup_menu(
    window: &mut gpui::Window,
    cx: &mut gpui::App,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    action_context: gpui::FocusHandle,
) -> Entity<PopupMenu> {
    let style = slash_popup_menu_style(theme);
    PopupMenu::build(window, cx, move |menu, _window, _cx| {
        CALLOUT_MENU_ITEMS.iter().fold(
            menu.style(style)
                .rich_rows(true)
                .action_context(action_context)
                .min_w(px(SECONDARY_MENU_WIDTH_PX))
                .max_w(px(SECONDARY_MENU_WIDTH_PX)),
            |menu, item| {
                let item_view = view.clone();
                let variant = item.variant;
                menu.item(
                    PopupMenuItem::new(item.label)
                        .description(item.description)
                        .icon(PopupMenuIcon::new(move |_, _| {
                            SvgIcon::new(item.icon_key, item.icon)
                                .color(style.foreground)
                                .size(px(16.0))
                        }))
                        .on_click(move |_, _, cx| {
                            item_view.update(cx, |view, cx| {
                                view.apply_slash_callout_variant_from_gui(variant, cx);
                            });
                        }),
                )
            },
        )
    })
}

pub fn slash_popup_menu_style(theme: GuiTheme) -> PopupMenuStyle {
    PopupMenuStyle {
        background: rgb(theme.panel).into(),
        foreground: rgb(theme.text).into(),
        muted_foreground: rgb(theme.muted).into(),
        border: rgb(theme.border).into(),
        selected_background: rgb(theme.hover_surface).into(),
        selected_foreground: rgb(theme.text).into(),
        radius: px(4.0),
    }
}

pub fn build_slash_popup_menu(
    window: &mut gpui::Window,
    cx: &mut gpui::App,
    theme: GuiTheme,
    action_context: gpui::FocusHandle,
) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, move |menu, _window, _cx| {
        menu.style(slash_popup_menu_style(theme))
            .action_context(action_context)
            .min_w(px(SLASH_MENU_WIDTH_PX))
            .max_w(px(SLASH_MENU_WIDTH_PX))
    })
}

pub fn update_slash_popup_menu(
    menu: &mut PopupMenu,
    state: SlashMenuState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    viewport: EditorViewport,
    callout_popup_menu: Option<Entity<PopupMenu>>,
) {
    menu.set_style(slash_popup_menu_style(theme));
    menu.replace_items([PopupMenuItem::element(move |_window, _cx| {
        render_slash_popup_content(
            &state,
            theme,
            view.clone(),
            viewport,
            callout_popup_menu.clone(),
        )
    })]);
}

pub(crate) fn render_slash_menu(
    state: &SlashMenuState,
    menu: Entity<PopupMenu>,
    viewport: EditorViewport,
) -> AnyElement {
    let height = slash_menu_panel_height(state.visible_items().len());
    let (x, y) =
        slash_menu_panel_position(state.x, state.y, height, viewport.width, viewport.height);
    deferred(
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(SLASH_MENU_WIDTH_PX))
            .child(menu),
    )
    .with_priority(120)
    .into_any_element()
}

fn render_slash_popup_content(
    state: &SlashMenuState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    viewport: EditorViewport,
    callout_popup_menu: Option<Entity<PopupMenu>>,
) -> AnyElement {
    let items = state.visible_items();
    let total_items = items.len();
    let height = slash_menu_panel_height(total_items);
    let (x, _y) =
        slash_menu_panel_position(state.x, state.y, height, viewport.width, viewport.height);
    let mut panel = div()
        .relative()
        .w_full()
        .h(px(height - SLASH_MENU_PANEL_PADDING_PX * 2.0))
        .flex()
        .flex_col()
        .p(px(SLASH_MENU_PANEL_PADDING_PX))
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation()
        })
        .on_scroll_wheel({
            let view = view.clone();
            move |event, _window, cx| {
                let delta_y = f32::from(event.delta.pixel_delta(px(SLASH_MENU_ROW_HEIGHT_PX)).y);
                let rows = slash_scroll_delta_rows(delta_y);
                if rows != 0 {
                    view.update(cx, |view, cx| {
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
                view,
            ));
        }
    }
    if let Some(callout_menu) = callout_popup_menu {
        let row_offset = state.selected_index.saturating_sub(state.scroll_start) as f32
            * SLASH_MENU_ROW_HEIGHT_PX;
        let opens_left = x + SLASH_MENU_WIDTH_PX + 4.0 + SECONDARY_MENU_WIDTH_PX
            > viewport.width - SLASH_MENU_VIEWPORT_MARGIN_PX;
        panel = panel.child(
            div()
                .absolute()
                .top(px(SLASH_MENU_PANEL_PADDING_PX
                    + SLASH_MENU_GROUP_HEIGHT_PX
                    + row_offset))
                .left(px(if opens_left {
                    -SECONDARY_MENU_WIDTH_PX - 4.0
                } else {
                    SLASH_MENU_WIDTH_PX + 4.0
                }))
                .child(callout_menu),
        );
    }
    panel.into_any_element()
}

fn slash_menu_panel_height(total_items: usize) -> f32 {
    SLASH_MENU_PANEL_PADDING_PX * 2.0
        + SLASH_MENU_GROUP_HEIGHT_PX
        + total_items.clamp(1, SLASH_MENU_VISIBLE_ITEMS) as f32 * SLASH_MENU_ROW_HEIGHT_PX
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

fn render_slash_scrollbar(
    theme: GuiTheme,
    total_items: usize,
    scroll_start: usize,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let visible = SLASH_MENU_VISIBLE_ITEMS.min(total_items);
    let track_height = visible as f32 * SLASH_MENU_ROW_HEIGHT_PX - 8.0;
    let max_start = total_items.saturating_sub(visible);
    let row_height = SLASH_MENU_ROW_HEIGHT_PX;

    div()
        .absolute()
        .right_0()
        .top(px(SLASH_MENU_PANEL_PADDING_PX
            + SLASH_MENU_GROUP_HEIGHT_PX
            + 4.0))
        .w(px(10.0))
        .h(px(track_height))
        .child(InteractiveScrollbar::for_callback(
            ScrollbarAxis::Vertical,
            scroll_start.min(max_start) as f32 * row_height,
            max_start as f32 * row_height,
            visible as f32 / total_items as f32,
            InteractiveScrollbarStyle::notion(theme.scrollbar, theme.scrollbar_hover),
            move |offset_px, _window, cx| {
                let target_start = (offset_px / row_height).round() as usize;
                view.update(cx, |view, cx| {
                    view.set_slash_menu_scroll_start_from_gui(target_start, cx);
                });
            },
        ))
        .into_any_element()
}

fn render_slash_menu_group_header(theme: GuiTheme) -> AnyElement {
    div()
        .flex_none()
        .h(px(SLASH_MENU_GROUP_HEIGHT_PX))
        .flex()
        .items_center()
        .px(px(8.0))
        .text_size(px(SLASH_MENU_GROUP_SIZE_PX))
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
    let icon = render_slash_menu_item_icon(&item, theme);
    let has_submenu = item.is_callout();
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
                view.update(cx, |view, cx| {
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
                .child(icon),
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
                        .text_size(px(SLASH_MENU_LABEL_SIZE_PX))
                        .text_color(rgb(theme.text))
                        .child(item.label),
                )
                .child(
                    div()
                        .text_size(px(SLASH_MENU_DESCRIPTION_SIZE_PX))
                        .text_color(rgb(theme.muted))
                        .child(item.description),
                ),
        )
        .when(has_submenu, |row| {
            row.child(
                SvgIcon::new("slash-callout-submenu-arrow", ICON_SUBMENU_ARROW)
                    .color(rgb(theme.muted))
                    .size(px(16.0)),
            )
        })
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            view.update(cx, |view, cx| {
                view.apply_slash_menu_index_from_gui(index, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_slash_menu_item_icon(item: &SlashMenuItem, theme: GuiTheme) -> AnyElement {
    if let Some((key, source)) = slash_menu_svg_icon(item) {
        SvgIcon::new(key, source)
            .color(rgb(theme.text))
            .size(px(SLASH_MENU_SVG_ICON_SIZE_PX))
            .into_any_element()
    } else {
        div()
            .text_size(px(SLASH_MENU_ICON_GLYPH_SIZE_PX))
            .child(item.icon)
            .into_any_element()
    }
}

fn slash_menu_svg_icon(item: &SlashMenuItem) -> Option<(&'static str, &'static [u8])> {
    if item.command == Some(SlashMenuCommand::AskAi) {
        return Some(("slash-menu-ai", ICON_AI));
    }
    match &item.kind {
        RichBlockKind::Paragraph => Some(("slash-menu-text", ICON_TEXT)),
        RichBlockKind::Heading { level: 1 } => Some(("slash-menu-heading-1", ICON_HEADING_1)),
        RichBlockKind::Heading { level: 2 } => Some(("slash-menu-heading-2", ICON_HEADING_2)),
        RichBlockKind::Heading { level: 3 } => Some(("slash-menu-heading-3", ICON_HEADING_3)),
        RichBlockKind::Todo { .. } => Some(("slash-menu-todo", ICON_TODO)),
        RichBlockKind::BulletedList => Some(("slash-menu-bulleted-list", ICON_BULLETED_LIST)),
        RichBlockKind::NumberedList => Some(("slash-menu-number-list", ICON_NUMBER_LIST)),
        RichBlockKind::Quote => Some(("slash-menu-quote", ICON_QUOTE)),
        RichBlockKind::Callout { .. } => Some(("slash-menu-callout", ICON_CALLOUT)),
        RichBlockKind::Code { .. } => Some(("slash-menu-code", ICON_CODE)),
        RichBlockKind::Math => Some(("slash-menu-math", ICON_MATH)),
        RichBlockKind::Mermaid => Some(("slash-menu-mermaid", ICON_MERMAID)),
        RichBlockKind::Table => Some(("slash-menu-table", ICON_TABLE)),
        RichBlockKind::Whiteboard => Some(("slash-menu-whiteboard", ICON_WHITEBOARD)),
        RichBlockKind::Divider => Some(("slash-menu-divider", ICON_DIVIDER)),
        _ => None,
    }
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
    fn provided_svg_icons_cover_the_matching_slash_menu_items() {
        let items = slash_menu_items();
        for label in [
            "Ask AI",
            "Text",
            "Heading 1",
            "Heading 2",
            "Heading 3",
            "Todo",
            "Bulleted list",
            "Numbered list",
            "Quote",
            "Callout",
            "Code",
            "Math",
            "Mermaid",
            "Table",
            "Whiteboard",
            "Divider",
        ] {
            let item = items.iter().find(|item| item.label == label).unwrap();
            let (_, source) = slash_menu_svg_icon(item).expect("provided SVG is mapped");
            assert!(std::str::from_utf8(source).unwrap().starts_with("<svg"));
        }
        assert!(items.iter().all(|item| slash_menu_svg_icon(item).is_some()));
        assert!(
            std::str::from_utf8(ICON_SUBMENU_ARROW)
                .unwrap()
                .starts_with("<svg")
        );
    }

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
        assert_eq!(items.len(), 16, "Ask AI plus 15 supported block entries");
        assert_eq!(items[0].command, Some(SlashMenuCommand::AskAi));
        assert_eq!(items[1].kind, RichBlockKind::Paragraph);
        assert_eq!(items[15].kind, RichBlockKind::Divider);
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
    fn slash_menu_hides_incomplete_block_syntaxes() {
        let items = slash_menu_items();
        for unsupported in [
            RichBlockKind::Toggle,
            RichBlockKind::Html,
            RichBlockKind::Separator,
            RichBlockKind::FootnoteDefinition,
            RichBlockKind::Comment,
            RichBlockKind::RawMarkdown,
        ] {
            assert!(!slash_menu_supports_kind(&unsupported));
            assert!(items.iter().all(|item| item.kind != unsupported));
        }
        for query in [
            "toggle",
            "html",
            "separator",
            "footnote",
            "comment",
            "markdown",
        ] {
            assert!(
                items.iter().all(|item| !slash_item_matches(item, query)),
                "unsupported query {query} must not resolve to a slash item"
            );
        }
    }

    #[test]
    fn callout_variant_queries_resolve_to_the_callout_parent_item() {
        let callout = slash_menu_items()
            .into_iter()
            .find(SlashMenuItem::is_callout)
            .expect("callout slash item");
        for query in ["note", "tip", "important", "warning", "caution"] {
            assert!(slash_item_matches(&callout, query), "missing query {query}");
        }
    }

    #[test]
    fn keyboard_selection_does_not_open_callout_until_it_is_confirmed() {
        let mut state = SlashMenuState::new(1, 0, String::new(), 0.0, 0.0);
        let callout_index = state
            .visible_items()
            .iter()
            .position(SlashMenuItem::is_callout)
            .expect("callout slash item");
        state.selected_index = callout_index - 1;

        assert!(state.move_selection(1));
        assert_eq!(state.selected_index, callout_index);
        assert!(!state.callout_submenu_open);

        assert!(state.move_selection(1));
        assert!(!state.callout_submenu_open);
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
    fn slash_menu_clamps_direct_scrollbar_positions_and_keeps_selection_visible() {
        let mut state = SlashMenuState::new(1, 0, String::new(), 0.0, 0.0);
        let max_start = state
            .visible_items()
            .len()
            .saturating_sub(SLASH_MENU_VISIBLE_ITEMS);

        assert!(state.set_scroll_start(usize::MAX));
        assert_eq!(state.scroll_start, max_start);
        assert!(state.selected_index >= state.scroll_start);
        assert!(state.selected_index < state.scroll_start + SLASH_MENU_VISIBLE_ITEMS);
        assert!(!state.set_scroll_start(max_start));
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
    fn slash_menu_typography_and_icon_geometry_match_notion_density() {
        assert_eq!(SLASH_MENU_LABEL_SIZE_PX, 14.0);
        assert_eq!(SLASH_MENU_DESCRIPTION_SIZE_PX, 12.0);
        assert_eq!(SLASH_MENU_GROUP_SIZE_PX, 12.0);
        assert_eq!(SLASH_MENU_ICON_GLYPH_SIZE_PX, 20.0);
        assert_eq!(SLASH_MENU_ICON_SIZE_PX, 36.0);
    }

    #[test]
    fn slash_menu_position_clamps_wider_notion_panel_in_narrow_viewport() {
        let (x, y) = slash_menu_panel_position(300.0, 300.0, 420.0, 300.0, 600.0);

        assert_eq!(x, SLASH_MENU_VIEWPORT_MARGIN_PX);
        assert_eq!(y, SLASH_MENU_VIEWPORT_MARGIN_PX);
    }
}
