use crate::rich_text::RichBlockKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockListInfo {
    pub depth: usize,
    pub numbered_ordinal: Option<usize>,
}

impl BlockListInfo {
    pub const fn root() -> Self {
        Self {
            depth: 0,
            numbered_ordinal: None,
        }
    }

    pub const fn with_depth(depth: usize) -> Self {
        Self {
            depth,
            numbered_ordinal: None,
        }
    }

    pub const fn with_numbered_ordinal(mut self, ordinal: usize) -> Self {
        self.numbered_ordinal = Some(ordinal);
        self
    }
}

pub fn is_list_item_kind(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::BulletedList | RichBlockKind::NumberedList | RichBlockKind::Todo { .. }
    )
}

pub fn is_numbered_list_item_kind(kind: &RichBlockKind) -> bool {
    matches!(kind, RichBlockKind::NumberedList)
}

pub fn supports_list_children(kind: &RichBlockKind) -> bool {
    kind.supports_children()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_list_info_defaults_to_root_without_ordinal() {
        assert_eq!(BlockListInfo::default(), BlockListInfo::root());
        assert_eq!(BlockListInfo::root().depth, 0);
        assert_eq!(BlockListInfo::root().numbered_ordinal, None);
    }

    #[test]
    fn block_list_info_can_carry_depth_and_numbered_ordinal() {
        let info = BlockListInfo::with_depth(2).with_numbered_ordinal(3);

        assert_eq!(info.depth, 2);
        assert_eq!(info.numbered_ordinal, Some(3));
    }

    #[test]
    fn list_kind_classification_matches_rich_block_kinds() {
        assert!(is_list_item_kind(&RichBlockKind::BulletedList));
        assert!(is_list_item_kind(&RichBlockKind::NumberedList));
        assert!(is_list_item_kind(&RichBlockKind::Todo { checked: false }));
        assert!(!is_list_item_kind(&RichBlockKind::Paragraph));
        assert!(is_numbered_list_item_kind(&RichBlockKind::NumberedList));
        assert!(!is_numbered_list_item_kind(&RichBlockKind::BulletedList));
    }
}
