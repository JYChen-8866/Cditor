use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{CalloutVariant, RichBlockKind};

pub const BLOCK_TRANSFORM_MENU_WIDTH_PX: f32 = 220.0;
const BLOCK_TRANSFORM_MENU_HEIGHT_PX: f32 = 372.0;
const BLOCK_TRANSFORM_MENU_GAP_PX: f32 = 6.0;
const PRIMARY_TOOLBAR_WIDTH_PX: f32 = 194.0;
const PRIMARY_TOOLBAR_CONTENT_LEFT_PX: f32 = 8.0;
const BLOCK_TRANSFORM_MENU_RIGHT_OFFSET_PX: f32 =
    PRIMARY_TOOLBAR_WIDTH_PX - PRIMARY_TOOLBAR_CONTENT_LEFT_PX + BLOCK_TRANSFORM_MENU_GAP_PX;
const BLOCK_TRANSFORM_MENU_LEFT_OFFSET_PX: f32 = -(BLOCK_TRANSFORM_MENU_WIDTH_PX
    + PRIMARY_TOOLBAR_CONTENT_LEFT_PX
    + BLOCK_TRANSFORM_MENU_GAP_PX);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTransformAction {
    Text,
    Heading1,
    Heading2,
    Heading3,
    BulletedList,
    NumberedList,
    Todo,
    Toggle,
    Quote,
    Callout,
    CodeBlock,
    MathBlock,
    MermaidBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockTransformAvailability(u16);

impl BlockTransformAvailability {
    pub fn from_enabled(actions: impl IntoIterator<Item = BlockTransformAction>) -> Self {
        let mut availability = Self::default();
        for action in actions {
            availability.0 |= 1 << transform_action_index(action);
        }
        availability
    }

    pub const fn contains(self, action: BlockTransformAction) -> bool {
        self.0 & (1 << transform_action_index(action)) != 0
    }
}

impl BlockTransformAction {
    pub const ALL: [Self; 13] = [
        Self::Text,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::BulletedList,
        Self::NumberedList,
        Self::Todo,
        Self::Toggle,
        Self::Quote,
        Self::Callout,
        Self::CodeBlock,
        Self::MathBlock,
        Self::MermaidBlock,
    ];

    pub fn from_kind(kind: &RichBlockKind) -> Option<Self> {
        match kind {
            RichBlockKind::Paragraph => Some(Self::Text),
            RichBlockKind::Heading { level: 1 } => Some(Self::Heading1),
            RichBlockKind::Heading { level: 2 } => Some(Self::Heading2),
            RichBlockKind::Heading { .. } => Some(Self::Heading3),
            RichBlockKind::BulletedList => Some(Self::BulletedList),
            RichBlockKind::NumberedList => Some(Self::NumberedList),
            RichBlockKind::Todo { .. } => Some(Self::Todo),
            RichBlockKind::Toggle => Some(Self::Toggle),
            RichBlockKind::Quote => Some(Self::Quote),
            RichBlockKind::Callout { .. } => Some(Self::Callout),
            RichBlockKind::Code { .. } => Some(Self::CodeBlock),
            RichBlockKind::Math => Some(Self::MathBlock),
            RichBlockKind::Mermaid => Some(Self::MermaidBlock),
            _ => None,
        }
    }

    pub fn kind(self) -> RichBlockKind {
        match self {
            Self::Text => RichBlockKind::Paragraph,
            Self::Heading1 => RichBlockKind::Heading { level: 1 },
            Self::Heading2 => RichBlockKind::Heading { level: 2 },
            Self::Heading3 => RichBlockKind::Heading { level: 3 },
            Self::BulletedList => RichBlockKind::BulletedList,
            Self::NumberedList => RichBlockKind::NumberedList,
            Self::Todo => RichBlockKind::Todo { checked: false },
            Self::Toggle => RichBlockKind::Toggle,
            Self::Quote => RichBlockKind::Quote,
            Self::Callout => RichBlockKind::Callout {
                variant: CalloutVariant::Note,
            },
            Self::CodeBlock => RichBlockKind::Code { language: None },
            Self::MathBlock => RichBlockKind::Math,
            Self::MermaidBlock => RichBlockKind::Mermaid,
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Text => "T",
            Self::Heading1 => "H1",
            Self::Heading2 => "H2",
            Self::Heading3 => "H3",
            Self::BulletedList => "•",
            Self::NumberedList => "1.",
            Self::Todo => "☑",
            Self::Toggle => "▸",
            Self::Quote => "❝",
            Self::Callout => "!",
            Self::CodeBlock => "</>",
            Self::MathBlock => "Σ",
            Self::MermaidBlock => "◇",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Text => "正文",
            Self::Heading1 => "标题 1",
            Self::Heading2 => "标题 2",
            Self::Heading3 => "标题 3",
            Self::BulletedList => "项目符号列表",
            Self::NumberedList => "有序列表",
            Self::Todo => "待办事项",
            Self::Toggle => "折叠列表",
            Self::Quote => "引用",
            Self::Callout => "标注",
            Self::CodeBlock => "代码块",
            Self::MathBlock => "公式区块",
            Self::MermaidBlock => "Mermaid 图表",
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
        .children(BlockTransformAction::ALL.into_iter().map(|action| {
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
                            let _ = row_view.update(cx, |view, cx| {
                                view.transform_block_from_toolbar(block_id, action, cx);
                            });
                            cx.stop_propagation();
                        })
                })
                .child(
                    div()
                        .w(px(26.0))
                        .text_size(px(if matches!(action, BlockTransformAction::CodeBlock) {
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

const fn transform_action_index(action: BlockTransformAction) -> usize {
    match action {
        BlockTransformAction::Text => 0,
        BlockTransformAction::Heading1 => 1,
        BlockTransformAction::Heading2 => 2,
        BlockTransformAction::Heading3 => 3,
        BlockTransformAction::BulletedList => 4,
        BlockTransformAction::NumberedList => 5,
        BlockTransformAction::Todo => 6,
        BlockTransformAction::Toggle => 7,
        BlockTransformAction::Quote => 8,
        BlockTransformAction::Callout => 9,
        BlockTransformAction::CodeBlock => 10,
        BlockTransformAction::MathBlock => 11,
        BlockTransformAction::MermaidBlock => 12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_actions_roundtrip_supported_block_kinds() {
        for action in BlockTransformAction::ALL {
            assert_eq!(
                BlockTransformAction::from_kind(&action.kind()),
                Some(action)
            );
        }
    }

    #[test]
    fn transform_availability_tracks_each_action_independently() {
        let availability = BlockTransformAvailability::from_enabled([
            BlockTransformAction::Text,
            BlockTransformAction::CodeBlock,
        ]);

        assert!(availability.contains(BlockTransformAction::Text));
        assert!(availability.contains(BlockTransformAction::CodeBlock));
        assert!(!availability.contains(BlockTransformAction::Heading1));
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
