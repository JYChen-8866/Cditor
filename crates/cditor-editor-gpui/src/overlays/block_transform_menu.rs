use cditor_component::{
    PopupMenu, PopupMenuCheckSide, PopupMenuIcon, PopupMenuItem, PopupMenuStyle, SvgIcon,
};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{CalloutVariant, RichBlockKind};
use gpui::prelude::FluentBuilder;
use gpui::{App, Entity, IntoElement, ParentElement, Styled, Window, div, px};

use crate::editor_view::CditorV2View;
use crate::menu_metrics::{PRIMARY_MENU_HEIGHT_PX, PRIMARY_MENU_WIDTH_PX, SECONDARY_MENU_WIDTH_PX};
use crate::overlays::callout_menu::CALLOUT_MENU_ITEMS;
use crate::presentation::block_registry::{
    TransformBlockPresentation, block_presentation_registry,
};

pub const BLOCK_TRANSFORM_MENU_WIDTH_PX: f32 = SECONDARY_MENU_WIDTH_PX;
const BLOCK_TRANSFORM_MENU_GAP_PX: f32 = 6.0;
const PRIMARY_TOOLBAR_WIDTH_PX: f32 = PRIMARY_MENU_WIDTH_PX;
const PRIMARY_TOOLBAR_CONTENT_LEFT_PX: f32 = 8.0;
const BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX: f32 =
    PRIMARY_TOOLBAR_WIDTH_PX - PRIMARY_TOOLBAR_CONTENT_LEFT_PX + BLOCK_TRANSFORM_MENU_GAP_PX;
const BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX: f32 = -(BLOCK_TRANSFORM_MENU_WIDTH_PX
    + PRIMARY_TOOLBAR_CONTENT_LEFT_PX
    + BLOCK_TRANSFORM_MENU_GAP_PX);

const ICON_TEXT: &[u8] = include_bytes!("../../../../assets/icons/text.svg");
const ICON_HEADING_1: &[u8] = include_bytes!("../../../../assets/icons/heading-1.svg");
const ICON_HEADING_2: &[u8] = include_bytes!("../../../../assets/icons/heading-2.svg");
const ICON_HEADING_3: &[u8] = include_bytes!("../../../../assets/icons/heading-3.svg");
const ICON_BULLETED_LIST: &[u8] = include_bytes!("../../../../assets/icons/bulleted-list.svg");
const ICON_NUMBER_LIST: &[u8] = include_bytes!("../../../../assets/icons/number-list.svg");
const ICON_TODO: &[u8] = include_bytes!("../../../../assets/icons/todo.svg");
const ICON_QUOTE: &[u8] = include_bytes!("../../../../assets/icons/quote.svg");
const ICON_CALLOUT: &[u8] = include_bytes!("../../../../assets/icons/callout.svg");
const ICON_CODE: &[u8] = include_bytes!("../../../../assets/icons/code.svg");
const ICON_MATH: &[u8] = include_bytes!("../../../../assets/icons/math.svg");
const ICON_MERMAID: &[u8] = include_bytes!("../../../../assets/icons/mermaid.svg");

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
    #[cfg(test)]
    pub const TEXT: Self = Self(1);
    #[cfg(test)]
    pub const HEADING_1: Self = Self(2);
    #[cfg(test)]
    pub const CODE_BLOCK: Self = Self(9);

    pub fn all() -> Vec<Self> {
        block_presentation_registry()
            .transform_presentations()
            .into_iter()
            .filter(|presentation| !matches!(presentation.kind, RichBlockKind::Toggle))
            .map(|presentation| Self(presentation.kind_tag))
            .collect()
    }

    pub fn from_kind(kind: &RichBlockKind) -> Option<Self> {
        block_presentation_registry()
            .transform_for_kind(kind)
            .map(|presentation| Self(presentation.kind_tag))
    }

    pub fn kind(self) -> RichBlockKind {
        self.metadata().kind
    }

    fn metadata(self) -> TransformBlockPresentation {
        block_presentation_registry()
            .transform_by_tag(self.0)
            .expect("transform action must reference registered metadata")
    }

    fn label(self) -> &'static str {
        self.metadata().label
    }

    fn description(self) -> &'static str {
        match self.kind() {
            RichBlockKind::Paragraph => "转换为普通文本区块",
            RichBlockKind::Heading { level: 1 } => "转换为一级标题",
            RichBlockKind::Heading { level: 2 } => "转换为二级标题",
            RichBlockKind::Heading { .. } => "转换为三级标题",
            RichBlockKind::BulletedList => "转换为项目符号列表",
            RichBlockKind::NumberedList => "转换为编号列表",
            RichBlockKind::Todo { .. } => "转换为待办事项",
            RichBlockKind::Quote => "转换为引用区块",
            RichBlockKind::Callout { .. } => "选择提示区块类型",
            RichBlockKind::Code { .. } => "转换为代码区块",
            RichBlockKind::Math => "转换为数学公式",
            RichBlockKind::Mermaid => "转换为 Mermaid 图表",
            _ => "转换当前区块类型",
        }
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
    let max_top = (viewport_height - PRIMARY_MENU_HEIGHT_PX - 10.0).max(10.0);
    let clamped_top = toolbar_y.clamp(10.0, max_top);
    clamped_top - toolbar_y - 8.0
}

pub fn build_block_transform_popup_menu(
    window: &mut Window,
    cx: &mut App,
    style: PopupMenuStyle,
    view: Entity<CditorV2View>,
    block_id: BlockId,
    current: Option<BlockTransformAction>,
    current_callout: Option<CalloutVariant>,
    availability: BlockTransformAvailability,
) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, move |menu, window, cx| {
        let mut menu = menu
            .style(style)
            .rich_rows(true)
            .check_side(PopupMenuCheckSide::Right)
            .min_w(px(BLOCK_TRANSFORM_MENU_WIDTH_PX))
            .max_w(px(BLOCK_TRANSFORM_MENU_WIDTH_PX))
            .max_h(px(PRIMARY_MENU_HEIGHT_PX))
            .scrollable(true);

        for action in BlockTransformAction::all() {
            let enabled = availability.contains(action);
            let icon = popup_icon_for_action(action, style);
            if matches!(action.kind(), RichBlockKind::Callout { .. }) {
                let submenu_view = view.clone();
                menu = menu
                    .submenu_with_icon_and_disabled(
                        Some(icon),
                        action.label(),
                        !enabled,
                        window,
                        cx,
                        move |submenu, _window, _cx| {
                            CALLOUT_MENU_ITEMS.iter().fold(
                                submenu
                                    .style(style)
                                    .rich_rows(true)
                                    .check_side(PopupMenuCheckSide::Right)
                                    .min_w(px(BLOCK_TRANSFORM_MENU_WIDTH_PX))
                                    .max_w(px(BLOCK_TRANSFORM_MENU_WIDTH_PX)),
                                |submenu, item| {
                                    let item_view = submenu_view.clone();
                                    let variant = item.variant;
                                    submenu.item(
                                        PopupMenuItem::new(item.label)
                                            .description(item.description)
                                            .icon(popup_icon(item.icon_key, item.icon, style))
                                            .checked(current_callout == Some(variant))
                                            .disabled(!enabled)
                                            .on_click(move |_, _, cx| {
                                                item_view.update(cx, |view, cx| {
                                                    view.transform_block_kind_from_toolbar(
                                                        block_id,
                                                        RichBlockKind::Callout { variant },
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                },
                            )
                        },
                    )
                    .map_last_item(|item| item.description(action.description()));
                continue;
            }

            let row_view = view.clone();
            menu = menu.item(
                PopupMenuItem::new(action.label())
                    .description(action.description())
                    .icon(icon)
                    .checked(current == Some(action))
                    .disabled(!enabled)
                    .on_click(move |_, _, cx| {
                        row_view.update(cx, |view, cx| {
                            view.transform_block_from_toolbar(block_id, action, cx);
                        });
                    }),
            );
        }
        menu
    })
}

pub fn render_block_transform_menu(
    menu: Entity<PopupMenu>,
    opens_left: bool,
    top_offset: f32,
) -> gpui::AnyElement {
    div()
        .absolute()
        .top(px(top_offset))
        .when(opens_left, |container| {
            container.left(px(BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX))
        })
        .when(!opens_left, |container| {
            container.left(px(BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX))
        })
        .child(menu)
        .into_any_element()
}

fn popup_icon_for_action(action: BlockTransformAction, style: PopupMenuStyle) -> PopupMenuIcon {
    let (key, bytes) = transform_icon_source(&action.kind());
    popup_icon(key, bytes, style)
}

fn popup_icon(key: &'static str, bytes: &'static [u8], style: PopupMenuStyle) -> PopupMenuIcon {
    PopupMenuIcon::new(move |_, _| {
        SvgIcon::new(key, bytes)
            .color(style.foreground)
            .size(px(16.0))
    })
}

fn transform_icon_source(kind: &RichBlockKind) -> (&'static str, &'static [u8]) {
    match kind {
        RichBlockKind::Paragraph => ("block-transform-text", ICON_TEXT),
        RichBlockKind::Heading { level: 1 } => ("block-transform-heading-1", ICON_HEADING_1),
        RichBlockKind::Heading { level: 2 } => ("block-transform-heading-2", ICON_HEADING_2),
        RichBlockKind::Heading { .. } => ("block-transform-heading-3", ICON_HEADING_3),
        RichBlockKind::BulletedList => ("block-transform-bulleted-list", ICON_BULLETED_LIST),
        RichBlockKind::NumberedList => ("block-transform-number-list", ICON_NUMBER_LIST),
        RichBlockKind::Todo { .. } => ("block-transform-todo", ICON_TODO),
        RichBlockKind::Quote => ("block-transform-quote", ICON_QUOTE),
        RichBlockKind::Callout { .. } => ("block-transform-callout", ICON_CALLOUT),
        RichBlockKind::Code { .. } => ("block-transform-code", ICON_CODE),
        RichBlockKind::Math => ("block-transform-math", ICON_MATH),
        RichBlockKind::Mermaid => ("block-transform-mermaid", ICON_MERMAID),
        _ => ("block-transform-text", ICON_TEXT),
    }
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
        assert_eq!(actions.len(), 12);
        assert_eq!(actions[0], BlockTransformAction::TEXT);
        assert_eq!(actions[11].kind(), RichBlockKind::Mermaid);
        assert!(
            actions
                .iter()
                .all(|action| !matches!(action.kind(), RichBlockKind::Toggle))
        );
        for action in actions {
            assert!(!action.description().is_empty());
            assert_eq!(
                BlockTransformAction::from_kind(&action.kind()),
                Some(action)
            );
        }
    }

    #[test]
    fn every_transform_action_uses_an_embedded_svg() {
        for action in BlockTransformAction::all() {
            let (key, bytes) = transform_icon_source(&action.kind());
            assert!(key.starts_with("block-transform-"));
            assert!(bytes.starts_with(b"<svg"));
        }
    }

    #[test]
    fn callout_submenu_contains_the_five_standard_markers() {
        assert_eq!(
            CALLOUT_MENU_ITEMS
                .iter()
                .map(|item| item.label)
                .collect::<Vec<_>>(),
            vec!["!NOTE", "!TIP", "!IMPORTANT", "!WARNING", "!CAUTION"]
        );
        assert!(
            CALLOUT_MENU_ITEMS
                .iter()
                .all(|item| !item.description.is_empty())
        );
        assert_eq!(
            CALLOUT_MENU_ITEMS
                .iter()
                .map(|item| item.variant)
                .collect::<Vec<_>>(),
            vec![
                CalloutVariant::Note,
                CalloutVariant::Tip,
                CalloutVariant::Important,
                CalloutVariant::Warning,
                CalloutVariant::Caution,
            ]
        );
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
        assert_eq!(block_transform_menu_top_offset(320.0, 600.0), -164.0);
        assert_eq!(PRIMARY_MENU_HEIGHT_PX, 426.0);
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
