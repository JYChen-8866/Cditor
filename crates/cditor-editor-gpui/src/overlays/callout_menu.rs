use cditor_core::rich_text::CalloutVariant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalloutMenuItem {
    pub variant: CalloutVariant,
    pub label: &'static str,
    pub icon_key: &'static str,
    pub icon: &'static [u8],
    pub description: &'static str,
}

const ICON_NOTE: &[u8] = include_bytes!("../../../../assets/icons/note.svg");
const ICON_TIP: &[u8] = include_bytes!("../../../../assets/icons/bulb.svg");
const ICON_IMPORTANT: &[u8] = include_bytes!("../../../../assets/icons/important.svg");
const ICON_WARNING: &[u8] = include_bytes!("../../../../assets/icons/warning.svg");
const ICON_CAUTION: &[u8] = include_bytes!("../../../../assets/icons/cuttion.svg");

pub const CALLOUT_MENU_ITEMS: &[CalloutMenuItem] = &[
    CalloutMenuItem {
        variant: CalloutVariant::Note,
        label: "!NOTE",
        icon_key: "callout-note",
        icon: ICON_NOTE,
        description: "普通提示信息",
    },
    CalloutMenuItem {
        variant: CalloutVariant::Tip,
        label: "!TIP",
        icon_key: "callout-tip",
        icon: ICON_TIP,
        description: "有用的建议或技巧",
    },
    CalloutMenuItem {
        variant: CalloutVariant::Important,
        label: "!IMPORTANT",
        icon_key: "callout-important",
        icon: ICON_IMPORTANT,
        description: "需要特别关注的信息",
    },
    CalloutMenuItem {
        variant: CalloutVariant::Warning,
        label: "!WARNING",
        icon_key: "callout-warning",
        icon: ICON_WARNING,
        description: "需要谨慎处理的警告",
    },
    CalloutMenuItem {
        variant: CalloutVariant::Caution,
        label: "!CAUTION",
        icon_key: "callout-caution",
        icon: ICON_CAUTION,
        description: "可能导致风险的注意事项",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_callout_variants_have_the_provided_svg_assets() {
        assert_eq!(
            CALLOUT_MENU_ITEMS
                .iter()
                .map(|item| (item.label, item.variant))
                .collect::<Vec<_>>(),
            vec![
                ("!NOTE", CalloutVariant::Note),
                ("!TIP", CalloutVariant::Tip),
                ("!IMPORTANT", CalloutVariant::Important),
                ("!WARNING", CalloutVariant::Warning),
                ("!CAUTION", CalloutVariant::Caution),
            ]
        );
        assert!(
            CALLOUT_MENU_ITEMS
                .iter()
                .all(|item| item.icon.starts_with(b"<svg") && !item.description.is_empty())
        );
    }
}
