use crate::rich_text::RichBlockKind;

pub const BODY_BLOCK_CONTENT_WIDTH_PX: f64 = 800.0;
pub const WIDE_BLOCK_CONTENT_WIDTH_PX: f64 = 960.0;
pub const FULL_BLOCK_CONTENT_WIDTH_PX: f64 = 1200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockWidthClass {
    Body,
    Wide,
    Full,
}

impl BlockWidthClass {
    pub const fn content_width_px(self) -> f64 {
        match self {
            Self::Body => BODY_BLOCK_CONTENT_WIDTH_PX,
            Self::Wide => WIDE_BLOCK_CONTENT_WIDTH_PX,
            Self::Full => FULL_BLOCK_CONTENT_WIDTH_PX,
        }
    }
}

pub const fn block_width_class_for_kind(kind: &RichBlockKind) -> BlockWidthClass {
    match kind {
        RichBlockKind::Table => BlockWidthClass::Full,
        _ => BlockWidthClass::Body,
    }
}

pub const fn layout_width_for_kind(kind: &RichBlockKind) -> f64 {
    block_width_class_for_kind(kind).content_width_px()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_classes_have_stable_content_tracks() {
        assert_eq!(BlockWidthClass::Body.content_width_px(), 800.0);
        assert_eq!(BlockWidthClass::Wide.content_width_px(), 960.0);
        assert_eq!(BlockWidthClass::Full.content_width_px(), 1200.0);
    }

    #[test]
    fn complex_block_kinds_select_their_layout_track() {
        assert_eq!(
            block_width_class_for_kind(&RichBlockKind::Paragraph),
            BlockWidthClass::Body
        );
        assert_eq!(
            block_width_class_for_kind(&RichBlockKind::Table),
            BlockWidthClass::Full
        );
        for kind in [
            RichBlockKind::Database,
            RichBlockKind::ColumnsGroup,
            RichBlockKind::MindMap,
            RichBlockKind::Whiteboard,
            RichBlockKind::Mermaid,
        ] {
            assert_eq!(block_width_class_for_kind(&kind), BlockWidthClass::Body);
        }
    }
}
