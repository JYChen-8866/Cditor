use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, div, px, rgb,
};

use crate::app::CditorV2View;
use crate::menu_metrics::SECONDARY_MENU_WIDTH_PX;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_core::schema::{TransformMenuMetadata, builtin_block_registry};

pub const BLOCK_TRANSFORM_MENU_WIDTH_PX: f32 = SECONDARY_MENU_WIDTH_PX;
const BLOCK_TRANSFORM_MENU_HEIGHT_PX: f32 = 372.0;
const BLOCK_TRANSFORM_MENU_GAP_PX: f32 = 6.0;
const PRIMARY_TOOLBAR_WIDTH_PX: f32 = 194.0;
const PRIMARY_TOOLBAR_CONTENT_LEFT_PX: f32 = 8.0;
const BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX: f32 =
    PRIMARY_TOOLBAR_WIDTH_PX - PRIMARY_TOOLBAR_CONTENT_LEFT_PX + BLOCK_TRANSFORM_MENU_GAP_PX;
const BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX: f32 = -(BLOCK_TRANSFORM_MENU_WIDTH_PX
    + PRIMARY_TOOLBAR_CONTENT_LEFT_PX
    + BLOCK_TRANSFORM_MENU_GAP_PX);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockTransformAction(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockTransformAvailability(u64);

impl BlockTransformAvailability {
    pub fn from_enabled(actions: impl IntoIterator<Item = BlockTransformAction>) -> Self {
        let mut availability = Self::default();
        for action in actions {
            availability.0 |= 1 << transform_action_index(action);
        }
        availability
    }

    pub fn contains(self, action: BlockTransformAction) -> bool {
        self.0 & (1 << transform_action_index(action)) != 0
    }
}

impl BlockTransformAction {
    pub const TEXT: Self = Self(1);
    pub const HEADING_1: Self = Self(2);
    pub const HEADING_2: Self = Self(26);
    pub const HEADING_3: Self = Self(27);
    pub const CODE_BLOCK: Self = Self(9);

    pub fn all() -> Vec<Self> {
        builtin_block_registry()
            .transform_descriptors()
            .into_iter()
            .map(|descriptor| Self(descriptor.kind_tag))
            .collect()
    }

    pub fn from_kind(kind: &RichBlockKind) -> Option<Self> {
        builtin_block_registry()
            .descriptor_for_kind(kind)
            .menu
            .transform
            .map(|_| Self(kind_tag_for_rich_block_kind(kind)))
    }

    pub fn kind(self) -> RichBlockKind {
        builtin_block_registry()
            .descriptor_by_tag(self.0)
            .default_kind
            .clone()
    }

    fn metadata(self) -> TransformMenuMetadata {
        builtin_block_registry()
            .descriptor_by_tag(self.0)
            .menu
            .transform
            .expect("transform action must reference registered metadata")
    }

    fn icon(self) -> &'static str {
        self.metadata().icon
    }

    fn label(self) -> &'static str {
        self.metadata().label
    }
}

pub fn block_transform_menu_opens_left(toolbar_x: f32, viewport_width: f32) -> bool {
    toolbar_x
        + PRIMARY_TOOLBAR_WIDTH_PX
        + BLOCK_TRANSFORM_MENU_GAP_PX
        + BLOCK_TRANSFORM_MENU_WIDTH_PX
        > viewport_width - 10.0
}

pub fn block_transform_menu_top_offset(toolbar_y: f32, viewport_height: f32) -> f32 {
    let max_top = (viewport_height - BLOCK_TRANSFORM_MENU_HEIGHT_PX - 10.0).max(10.0);
    let clamped_top = toolbar_y.clamp(10.0, max_top);
    clamped_top - toolbar_y - 8.0
}

pub fn render_block_transform_menu(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: BlockId,
    current: Option<BlockTransformAction>,
    availability: BlockTransformAvailability,
    opens_left: bool,
    top_offset: f32,
) -> AnyElement {
    let menu = div()
        .id(("block-transform-menu", block_id))
        .absolute()
        .top(px(top_offset))
        .when(opens_left, |menu| {
            menu.left(px(BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX))
        })
        .when(!opens_left, |menu| {
            menu.left(px(BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX))
        })
        .w(px(BLOCK_TRANSFORM_MENU_WIDTH_PX))
        .h(px(BLOCK_TRANSFORM_MENU_HEIGHT_PX))
        .p(px(6.0))
        .flex()
        .flex_col()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .children(BlockTransformAction::all().into_iter().map(|action| {
            let active = current == Some(action);
            let enabled = availability.contains(action);
            let row_view = view.clone();
            div()
                .id(("block-transform-action", transform_action_index(action)))
                .h(px(27.0))
                .w_full()
                .px(px(7.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(4.0))
                .bg(rgb(if active {
                    theme.action_background
                } else {
                    theme.panel
                }))
                .text_color(rgb(if enabled { theme.text } else { theme.muted }))
                .when(!enabled, |row| row.opacity(0.45))
                .when(enabled, |row| {
                    row.cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.hover_surface)))
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            row_view.update(cx, |view, cx| {
                                view.transform_block_from_toolbar(block_id, action, cx);
                            });
                            cx.stop_propagation();
                        })
                })
                .child(
                    div()
                        .w(px(26.0))
                        .text_size(px(if action == BlockTransformAction::CODE_BLOCK {
                            10.0
                        } else {
                            12.0
                        }))
                        .font_weight(FontWeight::MEDIUM)
                        .child(action.icon()),
                )
                .child(div().flex_1().text_size(px(13.0)).child(action.label()))
                .when(active, |row| {
                    row.child(div().text_size(px(13.0)).child("✓"))
                })
                .into_any_element()
        }));
    menu.into_any_element()
}

fn transform_action_index(action: BlockTransformAction) -> usize {
    usize::from(action.metadata().order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_actions_roundtrip_supported_block_kinds() {
        let actions = BlockTransformAction::all();
        assert_eq!(actions.len(), 13);
        assert_eq!(actions[0], BlockTransformAction::TEXT);
        assert_eq!(actions[12].kind(), RichBlockKind::Mermaid);
        for action in actions {
            assert_eq!(
                BlockTransformAction::from_kind(&action.kind()),
                Some(action)
            );
        }
    }

    #[test]
    fn transform_availability_tracks_each_action_independently() {
        let availability = BlockTransformAvailability::from_enabled([
            BlockTransformAction::TEXT,
            BlockTransformAction::CODE_BLOCK,
        ]);

        assert!(availability.contains(BlockTransformAction::TEXT));
        assert!(availability.contains(BlockTransformAction::CODE_BLOCK));
        assert!(!availability.contains(BlockTransformAction::HEADING_1));
        assert_eq!(
            BlockTransformAvailability::default(),
            BlockTransformAvailability(0)
        );
    }

    #[test]
    fn transform_submenu_flips_left_before_it_overflows_viewport() {
        assert!(!block_transform_menu_opens_left(100.0, 900.0));
        assert!(block_transform_menu_opens_left(500.0, 800.0));
    }

    #[test]
    fn transform_submenu_clamps_inside_the_vertical_viewport() {
        assert_eq!(block_transform_menu_top_offset(10.0, 600.0), -8.0);
        assert_eq!(block_transform_menu_top_offset(320.0, 600.0), -110.0);
    }

    #[test]
    fn transform_submenu_has_an_exact_visual_gap_from_the_primary_panel() {
        assert_eq!(
            PRIMARY_TOOLBAR_CONTENT_LEFT_PX + BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX
                - PRIMARY_TOOLBAR_WIDTH_PX,
            BLOCK_TRANSFORM_MENU_GAP_PX
        );
        assert_eq!(
            -(PRIMARY_TOOLBAR_CONTENT_LEFT_PX
                + BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX
                + BLOCK_TRANSFORM_MENU_WIDTH_PX),
            BLOCK_TRANSFORM_MENU_GAP_PX
        );
    }
}
