use cditor_core::rich_text::RichBlockKind;
use cditor_core::schema::builtin_block_registry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlashBlockPresentation {
    pub(crate) kind: RichBlockKind,
    pub(crate) icon: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) keywords: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransformBlockPresentation {
    pub(crate) kind_tag: u16,
    pub(crate) kind: RichBlockKind,
    pub(crate) order: u16,
    pub(crate) icon: &'static str,
    pub(crate) label: &'static str,
}

pub(crate) fn slash_block_presentations() -> Vec<SlashBlockPresentation> {
    builtin_block_registry()
        .slash_descriptors()
        .into_iter()
        .map(|descriptor| {
            let metadata = descriptor
                .menu
                .slash
                .expect("slash descriptor must have presentation metadata");
            SlashBlockPresentation {
                kind: descriptor.default_kind.clone(),
                icon: metadata.icon,
                label: metadata.label,
                description: metadata.description,
                keywords: metadata.keywords,
            }
        })
        .collect()
}

pub(crate) fn transform_block_presentations() -> Vec<TransformBlockPresentation> {
    builtin_block_registry()
        .transform_descriptors()
        .into_iter()
        .map(transform_presentation)
        .collect()
}

pub(crate) fn transform_presentation_for_kind(
    kind: &RichBlockKind,
) -> Option<TransformBlockPresentation> {
    let descriptor = builtin_block_registry().descriptor_for_kind(kind);
    descriptor.menu.transform?;
    Some(transform_presentation(descriptor))
}

pub(crate) fn transform_presentation_by_tag(kind_tag: u16) -> Option<TransformBlockPresentation> {
    let descriptor = builtin_block_registry().descriptor_by_tag(kind_tag);
    descriptor.menu.transform?;
    Some(transform_presentation(descriptor))
}

fn transform_presentation(
    descriptor: &cditor_core::schema::BlockDescriptor,
) -> TransformBlockPresentation {
    let metadata = descriptor
        .menu
        .transform
        .expect("transform descriptor must have presentation metadata");
    TransformBlockPresentation {
        kind_tag: descriptor.kind_tag,
        kind: descriptor.default_kind.clone(),
        order: metadata.order,
        icon: metadata.icon,
        label: metadata.label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_adapter_preserves_order_and_complete_menu_copy() {
        let slash = slash_block_presentations();
        let transforms = transform_block_presentations();

        assert!(!slash.is_empty());
        assert!(!transforms.is_empty());
        assert!(slash.iter().all(|item| {
            !item.icon.is_empty()
                && !item.label.is_empty()
                && !item.description.is_empty()
                && !item.keywords.is_empty()
        }));
        assert!(
            transforms
                .iter()
                .all(|item| !item.icon.is_empty() && !item.label.is_empty())
        );
        assert_eq!(slash[0].kind, RichBlockKind::Paragraph);
        assert_eq!(transforms[0].kind, RichBlockKind::Paragraph);
    }

    #[test]
    fn transform_lookup_rejects_non_presented_and_unknown_kinds() {
        assert!(transform_presentation_for_kind(&RichBlockKind::Paragraph).is_some());
        assert!(transform_presentation_for_kind(&RichBlockKind::Image).is_none());
        assert!(transform_presentation_by_tag(u16::MAX).is_none());
    }
}
