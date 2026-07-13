use crate::block::BlockListInfo;
use crate::rich_text::{CalloutVariant, RichBlockKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockChromeSnapshot {
    pub list_info: BlockListInfo,
    pub prefix: BlockPrefixSnapshot,
    pub has_children: bool,
    pub collapsed: bool,
}

impl BlockChromeSnapshot {
    pub const fn plain() -> Self {
        Self {
            list_info: BlockListInfo::root(),
            prefix: BlockPrefixSnapshot::None,
            has_children: false,
            collapsed: false,
        }
    }

    pub fn from_kind(
        kind: &RichBlockKind,
        list_info: BlockListInfo,
        has_children: bool,
        collapsed: bool,
    ) -> Self {
        Self {
            list_info,
            prefix: BlockPrefixSnapshot::from_kind(kind, list_info, has_children, collapsed),
            has_children,
            collapsed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockPrefixSnapshot {
    None,
    Bullet { depth: usize },
    Number { ordinal: usize },
    Todo { checked: bool },
    Callout { variant: CalloutVariant },
    Heading { collapsed: bool },
    Toggle { collapsed: bool },
}

impl BlockPrefixSnapshot {
    pub fn from_kind(
        kind: &RichBlockKind,
        list_info: BlockListInfo,
        _has_children: bool,
        collapsed: bool,
    ) -> Self {
        match kind {
            RichBlockKind::BulletedList => Self::Bullet {
                depth: list_info.depth,
            },
            RichBlockKind::NumberedList => Self::Number {
                ordinal: list_info.numbered_ordinal.unwrap_or(1),
            },
            RichBlockKind::Todo { checked } => Self::Todo { checked: *checked },
            RichBlockKind::Callout { variant } => Self::Callout { variant: *variant },
            RichBlockKind::Heading { .. } => Self::Heading { collapsed },
            RichBlockKind::Toggle => Self::Toggle { collapsed },
            _ => Self::None,
        }
    }
}

pub fn bullet_marker_for_depth(depth: usize) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_chrome_has_no_prefix_or_children() {
        let chrome = BlockChromeSnapshot::plain();

        assert_eq!(chrome.list_info, BlockListInfo::root());
        assert_eq!(chrome.prefix, BlockPrefixSnapshot::None);
        assert!(!chrome.has_children);
        assert!(!chrome.collapsed);
    }

    #[test]
    fn prefix_snapshot_follows_block_kind_and_list_info() {
        assert_eq!(
            BlockPrefixSnapshot::from_kind(
                &RichBlockKind::BulletedList,
                BlockListInfo::with_depth(4),
                false,
                false,
            ),
            BlockPrefixSnapshot::Bullet { depth: 4 }
        );
        assert_eq!(
            BlockPrefixSnapshot::from_kind(
                &RichBlockKind::NumberedList,
                BlockListInfo::with_depth(1).with_numbered_ordinal(7),
                false,
                false,
            ),
            BlockPrefixSnapshot::Number { ordinal: 7 }
        );
        assert_eq!(
            BlockPrefixSnapshot::from_kind(
                &RichBlockKind::Todo { checked: true },
                BlockListInfo::root(),
                false,
                false,
            ),
            BlockPrefixSnapshot::Todo { checked: true }
        );
        assert_eq!(
            BlockPrefixSnapshot::from_kind(
                &RichBlockKind::Heading { level: 6 },
                BlockListInfo::root(),
                false,
                false,
            ),
            BlockPrefixSnapshot::Heading { collapsed: false }
        );
    }

    #[test]
    fn bullet_marker_cycles_by_depth() {
        assert_eq!(bullet_marker_for_depth(0), "•");
        assert_eq!(bullet_marker_for_depth(1), "◦");
        assert_eq!(bullet_marker_for_depth(2), "▪");
        assert_eq!(bullet_marker_for_depth(3), "•");
    }
}
