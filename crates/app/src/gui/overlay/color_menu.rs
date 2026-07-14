use cditor_core::rich_text::InlineColorTarget;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    ScrollHandle, StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::diagnostics::block_color::trace as trace_block_color;

pub const COLOR_MENU_WIDTH_PX: f32 = 220.0;
pub const COLOR_MENU_DESIRED_HEIGHT_PX: f32 = 520.0;
const COLOR_MENU_MIN_HEIGHT_PX: f32 = 180.0;
const COLOR_MENU_GAP_PX: f32 = 6.0;
const PRIMARY_TOOLBAR_WIDTH_PX: f32 = 194.0;
const PRIMARY_TOOLBAR_CONTENT_LEFT_PX: f32 = 8.0;
const COLOR_MENU_RIGHT_OFFSET_PX: f32 =
    PRIMARY_TOOLBAR_WIDTH_PX - PRIMARY_TOOLBAR_CONTENT_LEFT_PX + COLOR_MENU_GAP_PX;
const COLOR_MENU_LEFT_OFFSET_PX: f32 =
    -(COLOR_MENU_WIDTH_PX + PRIMARY_TOOLBAR_CONTENT_LEFT_PX + COLOR_MENU_GAP_PX);
const COLOR_TRIGGER_TOP_IN_TOOLBAR_PX: f32 = 40.0;
const COLOR_MENU_ESTIMATED_CONTENT_HEIGHT_PX: f32 = 690.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteColor {
    Gray,
    Brown,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Red,
}

impl PaletteColor {
    pub const ALL: [Self; 9] = [
        Self::Gray,
        Self::Brown,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Red,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Gray => "灰色",
            Self::Brown => "棕色",
            Self::Orange => "橙色",
            Self::Yellow => "黄色",
            Self::Green => "绿色",
            Self::Blue => "蓝色",
            Self::Purple => "紫色",
            Self::Pink => "粉色",
            Self::Red => "红色",
        }
    }

    pub const fn text_value(self) -> &'static str {
        match self {
            Self::Gray => "#787774",
            Self::Brown => "#9f6b53",
            Self::Orange => "#d9730d",
            Self::Yellow => "#cb912f",
            Self::Green => "#448361",
            Self::Blue => "#337ea9",
            Self::Purple => "#9065b0",
            Self::Pink => "#c14c8a",
            Self::Red => "#d44c47",
        }
    }

    pub const fn background_value(self) -> &'static str {
        match self {
            Self::Gray => "#f1f1ef",
            Self::Brown => "#f4eeee",
            Self::Orange => "#fbecdd",
            Self::Yellow => "#fbf3db",
            Self::Green => "#edf3ec",
            Self::Blue => "#e7f3f8",
            Self::Purple => "#f4f0f7",
            Self::Pink => "#f9eef3",
            Self::Red => "#fdebec",
        }
    }

    const fn text_swatch(self) -> u32 {
        match self {
            Self::Gray => 0x787774,
            Self::Brown => 0x9f6b53,
            Self::Orange => 0xd9730d,
            Self::Yellow => 0xcb912f,
            Self::Green => 0x448361,
            Self::Blue => 0x337ea9,
            Self::Purple => 0x9065b0,
            Self::Pink => 0xc14c8a,
            Self::Red => 0xd44c47,
        }
    }

    const fn background_swatch(self) -> u32 {
        match self {
            Self::Gray => 0xf1f1ef,
            Self::Brown => 0xf4eeee,
            Self::Orange => 0xfbecdd,
            Self::Yellow => 0xfbf3db,
            Self::Green => 0xedf3ec,
            Self::Blue => 0xe7f3f8,
            Self::Purple => 0xf4f0f7,
            Self::Pink => 0xf9eef3,
            Self::Red => 0xfdebec,
        }
    }

    pub fn from_value(target: InlineColorTarget, value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| {
            let candidate = match target {
                InlineColorTarget::Text => color.text_value(),
                InlineColorTarget::Background => color.background_value(),
            };
            candidate.eq_ignore_ascii_case(value)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveColor {
    Default,
    Palette(PaletteColor),
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorMenuAction {
    pub target: InlineColorTarget,
    pub color: Option<PaletteColor>,
}

impl ColorMenuAction {
    pub const fn text(color: Option<PaletteColor>) -> Self {
        Self {
            target: InlineColorTarget::Text,
            color,
        }
    }

    pub const fn background(color: Option<PaletteColor>) -> Self {
        Self {
            target: InlineColorTarget::Background,
            color,
        }
    }

    pub const fn value(self) -> Option<&'static str> {
        match (self.target, self.color) {
            (_, None) => None,
            (InlineColorTarget::Text, Some(color)) => Some(color.text_value()),
            (InlineColorTarget::Background, Some(color)) => Some(color.background_value()),
        }
    }

    fn label(self) -> String {
        match (self.target, self.color) {
            (InlineColorTarget::Text, None) => "默认文本".to_owned(),
            (InlineColorTarget::Background, None) => "默认背景".to_owned(),
            (InlineColorTarget::Text, Some(color)) => format!("{}文本", color.name()),
            (InlineColorTarget::Background, Some(color)) => format!("{}背景", color.name()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorMenuGeometry {
    pub opens_left: bool,
    pub top_offset: f32,
    pub height: f32,
}

pub fn color_menu_geometry(
    toolbar_x: f32,
    toolbar_y: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> ColorMenuGeometry {
    let opens_left = toolbar_x + PRIMARY_TOOLBAR_WIDTH_PX + COLOR_MENU_GAP_PX + COLOR_MENU_WIDTH_PX
        > viewport_width - 10.0;
    let available_height = (viewport_height - 20.0).max(1.0);
    let height = COLOR_MENU_DESIRED_HEIGHT_PX
        .min(available_height)
        .max(COLOR_MENU_MIN_HEIGHT_PX.min(available_height));
    let max_top = (viewport_height - height - 10.0).max(10.0);
    let clamped_top = toolbar_y.clamp(10.0, max_top);
    ColorMenuGeometry {
        opens_left,
        top_offset: clamped_top - toolbar_y - COLOR_TRIGGER_TOP_IN_TOOLBAR_PX,
        height,
    }
}

pub fn render_color_menu(
    state: super::FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    scroll_handle: &ScrollHandle,
) -> AnyElement {
    let content_view = view.clone();
    let content = div()
        .id("floating-toolbar-color-scroll")
        .h_full()
        .w_full()
        .pr(px(8.0))
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .on_scroll_wheel(move |_event, _window, cx| {
            let _ = content_view.update(cx, |_view, cx| cx.notify());
        })
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .when_some(state.last_color_action, |content, last| {
                    content
                        .child(section_label("上次使用", theme))
                        .child(render_color_action_row(
                            last,
                            action_is_active(last, state),
                            theme,
                            view.clone(),
                            state.has_text_selection,
                            state.block_id,
                            true,
                        ))
                        .child(section_divider(theme))
                })
                .child(section_label("文本颜色", theme))
                .child(render_color_action_row(
                    ColorMenuAction::text(None),
                    state.text_color == ActiveColor::Default,
                    theme,
                    view.clone(),
                    state.has_text_selection,
                    state.block_id,
                    false,
                ))
                .children(PaletteColor::ALL.into_iter().map(|color| {
                    let action = ColorMenuAction::text(Some(color));
                    render_color_action_row(
                        action,
                        action_is_active(action, state),
                        theme,
                        view.clone(),
                        state.has_text_selection,
                        state.block_id,
                        false,
                    )
                }))
                .child(section_divider(theme))
                .child(section_label("背景颜色", theme))
                .child(render_color_action_row(
                    ColorMenuAction::background(None),
                    state.background_color == ActiveColor::Default,
                    theme,
                    view.clone(),
                    state.has_text_selection,
                    state.block_id,
                    false,
                ))
                .children(PaletteColor::ALL.into_iter().map(|color| {
                    let action = ColorMenuAction::background(Some(color));
                    render_color_action_row(
                        action,
                        action_is_active(action, state),
                        theme,
                        view.clone(),
                        state.has_text_selection,
                        state.block_id,
                        false,
                    )
                })),
        );

    div()
        .id("floating-toolbar-color-menu")
        .absolute()
        .top(px(state.color_menu_top_offset))
        .when(state.color_menu_opens_left, |menu| {
            menu.left(px(COLOR_MENU_LEFT_OFFSET_PX))
        })
        .when(!state.color_menu_opens_left, |menu| {
            menu.left(px(COLOR_MENU_RIGHT_OFFSET_PX))
        })
        .w(px(COLOR_MENU_WIDTH_PX))
        .h(px(state.color_menu_height))
        .p(px(6.0))
        .rounded(px(9.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .on_hover({
            let view = view.clone();
            move |hovered, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.set_color_menu_hovered(*hovered, cx);
                });
            }
        })
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(content)
        .child(render_scrollbar(
            scroll_handle,
            state.color_menu_height - 12.0,
            theme,
        ))
        .into_any_element()
}

fn render_color_action_row(
    action: ColorMenuAction,
    active: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    has_text_selection: bool,
    target_block_id: Option<cditor_core::ids::BlockId>,
    last_used: bool,
) -> AnyElement {
    let id = color_action_index(action) + usize::from(last_used) * 32;
    div()
        .id(("color-menu-action", id))
        .h(px(29.0))
        .w_full()
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .rounded(px(4.0))
        .bg(rgb(if active {
            theme.action_background
        } else {
            theme.panel
        }))
        .text_color(rgb(theme.text))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.hover_surface)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            trace_block_color(
                "menu.click",
                format_args!(
                    "target={:?} value={:?} has_text_selection={has_text_selection} captured_block={target_block_id:?}",
                    action.target,
                    action.value(),
                ),
            );
            let _ = view.update(cx, |view, cx| {
                view.apply_color_from_toolbar(
                    action,
                    has_text_selection,
                    target_block_id,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(render_swatch(action, theme))
        .child(div().flex_1().text_size(px(13.0)).child(action.label()))
        .when(active, |row| {
            row.child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child("✓"),
            )
        })
        .into_any_element()
}

fn render_swatch(action: ColorMenuAction, theme: GuiTheme) -> AnyElement {
    match action.target {
        InlineColorTarget::Text => div()
            .size(px(24.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(rgb(theme.border))
            .text_size(px(15.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(action
                .color
                .map(PaletteColor::text_swatch)
                .unwrap_or(theme.text)))
            .child("A")
            .into_any_element(),
        InlineColorTarget::Background => div()
            .size(px(24.0))
            .flex_none()
            .rounded(px(4.0))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(action
                .color
                .map(PaletteColor::background_swatch)
                .unwrap_or(theme.panel)))
            .into_any_element(),
    }
}

fn section_label(label: &'static str, theme: GuiTheme) -> AnyElement {
    div()
        .h(px(25.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .text_size(px(12.0))
        .text_color(rgb(theme.muted))
        .child(label)
        .into_any_element()
}

fn section_divider(theme: GuiTheme) -> AnyElement {
    div()
        .h(px(9.0))
        .mb(px(4.0))
        .border_b_1()
        .border_color(rgb(theme.border))
        .into_any_element()
}

fn action_is_active(action: ColorMenuAction, state: super::FloatingToolbarState) -> bool {
    let current = match action.target {
        InlineColorTarget::Text => state.text_color,
        InlineColorTarget::Background => state.background_color,
    };
    match action.color {
        None => current == ActiveColor::Default,
        Some(color) => current == ActiveColor::Palette(color),
    }
}

fn render_scrollbar(
    scroll_handle: &ScrollHandle,
    track_height: f32,
    theme: GuiTheme,
) -> AnyElement {
    let max_offset = f32::from(scroll_handle.max_offset().y)
        .max((COLOR_MENU_ESTIMATED_CONTENT_HEIGHT_PX - track_height).max(0.0));
    let thumb_height =
        (track_height * track_height / (track_height + max_offset)).clamp(28.0, track_height);
    let progress = (-f32::from(scroll_handle.offset().y) / max_offset.max(1.0)).clamp(0.0, 1.0);
    let thumb_top = (track_height - thumb_height) * progress;
    div()
        .absolute()
        .top(px(6.0))
        .right(px(3.0))
        .w(px(5.0))
        .h(px(track_height))
        .child(
            div()
                .absolute()
                .top(px(thumb_top))
                .w_full()
                .h(px(thumb_height))
                .rounded(px(3.0))
                .bg(rgb(theme.scrollbar))
                .hover(|style| style.bg(rgb(theme.scrollbar_hover))),
        )
        .into_any_element()
}

const fn color_action_index(action: ColorMenuAction) -> usize {
    let family_offset = match action.target {
        InlineColorTarget::Text => 0,
        InlineColorTarget::Background => 16,
    };
    family_offset
        + match action.color {
            None => 0,
            Some(PaletteColor::Gray) => 1,
            Some(PaletteColor::Brown) => 2,
            Some(PaletteColor::Orange) => 3,
            Some(PaletteColor::Yellow) => 4,
            Some(PaletteColor::Green) => 5,
            Some(PaletteColor::Blue) => 6,
            Some(PaletteColor::Purple) => 7,
            Some(PaletteColor::Pink) => 8,
            Some(PaletteColor::Red) => 9,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_palette_has_unique_text_and_background_values() {
        let text = PaletteColor::ALL
            .into_iter()
            .map(PaletteColor::text_value)
            .collect::<std::collections::HashSet<_>>();
        let background = PaletteColor::ALL
            .into_iter()
            .map(PaletteColor::background_value)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(text.len(), PaletteColor::ALL.len());
        assert_eq!(background.len(), PaletteColor::ALL.len());
    }

    #[test]
    fn palette_values_roundtrip_to_their_named_color() {
        for color in PaletteColor::ALL {
            assert_eq!(
                PaletteColor::from_value(InlineColorTarget::Text, color.text_value()),
                Some(color)
            );
            assert_eq!(
                PaletteColor::from_value(InlineColorTarget::Background, color.background_value()),
                Some(color)
            );
        }
    }

    #[test]
    fn submenu_flips_and_clamps_to_the_viewport() {
        let right = color_menu_geometry(500.0, 300.0, 800.0, 600.0);
        assert!(right.opens_left);
        assert_eq!(right.height, 520.0);
        assert_eq!(right.top_offset, -270.0);

        let small = color_menu_geometry(100.0, 10.0, 900.0, 300.0);
        assert!(!small.opens_left);
        assert_eq!(small.height, 280.0);
        assert_eq!(small.top_offset, -40.0);
    }

    #[test]
    fn submenu_has_an_exact_visual_gap_from_the_primary_panel() {
        assert_eq!(
            PRIMARY_TOOLBAR_CONTENT_LEFT_PX + COLOR_MENU_RIGHT_OFFSET_PX - PRIMARY_TOOLBAR_WIDTH_PX,
            COLOR_MENU_GAP_PX
        );
        assert_eq!(
            -(PRIMARY_TOOLBAR_CONTENT_LEFT_PX + COLOR_MENU_LEFT_OFFSET_PX + COLOR_MENU_WIDTH_PX),
            COLOR_MENU_GAP_PX
        );
    }
}
