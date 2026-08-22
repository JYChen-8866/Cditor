use std::collections::HashSet;
use std::sync::OnceLock;

use cditor_core::rich_text::{CalloutVariant, RichBlockKind, kind_tag_for_rich_block_kind};
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

#[derive(Debug, Clone, Copy)]
struct SlashMetadata {
    icon: &'static str,
    label: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
struct TransformMetadata {
    order: u16,
    icon: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone)]
struct BlockPresentationDescriptor {
    kind: RichBlockKind,
    slash: SlashMetadata,
    transform: Option<TransformMetadata>,
}

pub(crate) struct BlockPresentationRegistry {
    descriptors: &'static [BlockPresentationDescriptor],
}

impl BlockPresentationRegistry {
    fn builtin() -> Self {
        let registry = Self {
            descriptors: BUILTIN_PRESENTATIONS,
        };
        registry
            .validate()
            .expect("builtin block presentations must be valid");
        registry
    }

    pub(crate) fn slash_presentations(&self) -> Vec<SlashBlockPresentation> {
        self.descriptors
            .iter()
            .map(|descriptor| SlashBlockPresentation {
                kind: descriptor.kind.clone(),
                icon: descriptor.slash.icon,
                label: descriptor.slash.label,
                description: descriptor.slash.description,
                keywords: descriptor.slash.keywords,
            })
            .collect()
    }

    pub(crate) fn transform_presentations(&self) -> Vec<TransformBlockPresentation> {
        let mut presentations = self
            .descriptors
            .iter()
            .filter_map(Self::transform_presentation)
            .collect::<Vec<_>>();
        presentations.sort_by_key(|presentation| presentation.order);
        presentations
    }

    pub(crate) fn transform_for_kind(
        &self,
        kind: &RichBlockKind,
    ) -> Option<TransformBlockPresentation> {
        self.transform_by_tag(kind_tag_for_rich_block_kind(kind))
    }

    pub(crate) fn transform_by_tag(&self, kind_tag: u16) -> Option<TransformBlockPresentation> {
        self.descriptors
            .iter()
            .find(|descriptor| kind_tag_for_rich_block_kind(&descriptor.kind) == kind_tag)
            .and_then(Self::transform_presentation)
    }

    fn transform_presentation(
        descriptor: &BlockPresentationDescriptor,
    ) -> Option<TransformBlockPresentation> {
        let metadata = descriptor.transform?;
        Some(TransformBlockPresentation {
            kind_tag: kind_tag_for_rich_block_kind(&descriptor.kind),
            kind: descriptor.kind.clone(),
            order: metadata.order,
            icon: metadata.icon,
            label: metadata.label,
        })
    }

    fn validate(&self) -> Result<(), String> {
        let core = builtin_block_registry();
        self.validate_with_known_kind(|tag| core.is_known(tag))
    }

    fn validate_with_known_kind(
        &self,
        mut is_known: impl FnMut(u16) -> bool,
    ) -> Result<(), String> {
        let mut slash_tags = HashSet::new();
        let mut transform_orders = HashSet::new();

        for descriptor in self.descriptors {
            let tag = kind_tag_for_rich_block_kind(&descriptor.kind);
            if !is_known(tag) {
                return Err(format!("presentation references unknown kind tag {tag}"));
            }
            if !slash_tags.insert(tag) {
                return Err(format!("duplicate slash presentation kind tag {tag}"));
            }
            if descriptor.slash.icon.is_empty()
                || descriptor.slash.label.is_empty()
                || descriptor.slash.description.is_empty()
                || descriptor.slash.keywords.is_empty()
            {
                return Err(format!("incomplete slash presentation for kind tag {tag}"));
            }
            if let Some(transform) = descriptor.transform {
                if !transform_orders.insert(transform.order) {
                    return Err(format!(
                        "duplicate transform presentation order {}",
                        transform.order
                    ));
                }
                if transform.icon.is_empty() || transform.label.is_empty() {
                    return Err(format!(
                        "incomplete transform presentation for kind tag {tag}"
                    ));
                }
            }
        }

        let transform_count = transform_orders.len();
        if transform_orders
            != (0..transform_count)
                .map(|order| order as u16)
                .collect::<HashSet<_>>()
        {
            return Err("transform presentation order must be contiguous".to_owned());
        }
        Ok(())
    }
}

pub(crate) fn block_presentation_registry() -> &'static BlockPresentationRegistry {
    static REGISTRY: OnceLock<BlockPresentationRegistry> = OnceLock::new();
    REGISTRY.get_or_init(BlockPresentationRegistry::builtin)
}

const fn slash(
    icon: &'static str,
    label: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
) -> SlashMetadata {
    SlashMetadata {
        icon,
        label,
        description,
        keywords,
    }
}

const fn transform(order: u16, icon: &'static str, label: &'static str) -> TransformMetadata {
    TransformMetadata { order, icon, label }
}

const BUILTIN_PRESENTATIONS: &[BlockPresentationDescriptor] = &[
    BlockPresentationDescriptor {
        kind: RichBlockKind::Paragraph,
        slash: slash(
            "T",
            "Text",
            "Just start writing with plain text.",
            &["paragraph", "text"],
        ),
        transform: Some(transform(0, "T", "正文")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Heading { level: 1 },
        slash: slash(
            "H1",
            "Heading 1",
            "Big section heading.",
            &["h1", "heading"],
        ),
        transform: Some(transform(1, "H1", "标题 1")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Heading { level: 2 },
        slash: slash(
            "H2",
            "Heading 2",
            "Medium section heading.",
            &["h2", "heading"],
        ),
        transform: Some(transform(2, "H2", "标题 2")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Heading { level: 3 },
        slash: slash(
            "H3",
            "Heading 3",
            "Small section heading.",
            &["h3", "heading"],
        ),
        transform: Some(transform(3, "H3", "标题 3")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Todo { checked: false },
        slash: slash(
            "[]",
            "Todo",
            "Track a task with a checkbox.",
            &["task", "checkbox"],
        ),
        transform: Some(transform(6, "☑", "待办事项")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::BulletedList,
        slash: slash(
            "*",
            "Bulleted list",
            "Create a simple bulleted list.",
            &["bullet", "ul", "list"],
        ),
        transform: Some(transform(4, "•", "项目符号列表")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::NumberedList,
        slash: slash(
            "1.",
            "Numbered list",
            "Create a list with numbering.",
            &["number", "ol", "list"],
        ),
        transform: Some(transform(5, "1.", "有序列表")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Toggle,
        slash: slash(">", "Toggle", "Hide content inside a toggle.", &["details"]),
        transform: Some(transform(7, "▸", "折叠列表")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Quote,
        slash: slash("\"", "Quote", "Capture a quote.", &["blockquote"]),
        transform: Some(transform(8, "❝", "引用")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Callout {
            variant: CalloutVariant::Note,
        },
        slash: slash("!", "Callout", "Make writing stand out.", &["note"]),
        transform: Some(transform(9, "!", "标注")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Code { language: None },
        slash: slash("</>", "Code", "Capture a code snippet.", &["code block"]),
        transform: Some(transform(10, "</>", "代码块")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Math,
        slash: slash("fx", "Math", "Write a block equation.", &["equation"]),
        transform: Some(transform(11, "Σ", "公式区块")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Mermaid,
        slash: slash("M", "Mermaid", "Create a Mermaid diagram.", &["diagram"]),
        transform: Some(transform(12, "◇", "Mermaid 图表")),
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Html,
        slash: slash("<>", "HTML", "Embed an HTML snippet.", &["html"]),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Table,
        slash: slash("#", "Table", "Add a simple table.", &["grid"]),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Whiteboard,
        slash: slash(
            "WB",
            "Whiteboard",
            "Sketch and arrange ideas on a canvas.",
            &["board", "canvas", "draw", "diagram", "白板"],
        ),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Video,
        slash: slash(
            "Video",
            "Video",
            "Add a playable video block.",
            &["video", "movie", "media"],
        ),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Divider,
        slash: slash("---", "Divider", "Visually divide blocks.", &["hr", "line"]),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Separator,
        slash: slash("|", "Separator", "Add a section separator.", &["separator"]),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::FootnoteDefinition,
        slash: slash(
            "fn",
            "Footnote",
            "Add a footnote definition.",
            &["footnote"],
        ),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::Comment,
        slash: slash("//", "Comment", "Add a comment block.", &["comment"]),
        transform: None,
    },
    BlockPresentationDescriptor {
        kind: RichBlockKind::RawMarkdown,
        slash: slash(
            "MD",
            "Raw Markdown",
            "Keep text as raw Markdown.",
            &["markdown", "md"],
        ),
        transform: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_preserves_complete_stable_menu_order() {
        let registry = block_presentation_registry();
        let slash = registry.slash_presentations();
        let transforms = registry.transform_presentations();

        assert_eq!(slash.len(), 22);
        assert_eq!(transforms.len(), 13);
        assert_eq!(slash[0].kind, RichBlockKind::Paragraph);
        assert_eq!(slash[21].kind, RichBlockKind::RawMarkdown);
        assert_eq!(transforms[0].kind, RichBlockKind::Paragraph);
        assert_eq!(transforms[12].kind, RichBlockKind::Mermaid);
        assert!(slash.iter().all(|item| {
            !item.icon.is_empty()
                && !item.label.is_empty()
                && !item.description.is_empty()
                && !item.keywords.is_empty()
        }));
    }

    #[test]
    fn registry_maps_every_presentation_to_core_semantics() {
        let registry = block_presentation_registry();
        let core = builtin_block_registry();

        for item in registry.slash_presentations() {
            let tag = kind_tag_for_rich_block_kind(&item.kind);
            assert!(core.is_known(tag));
            assert_eq!(core.descriptor_for_kind(&item.kind).kind_tag, tag);
        }
        for (expected_order, item) in registry.transform_presentations().iter().enumerate() {
            assert_eq!(usize::from(item.order), expected_order);
            assert_eq!(kind_tag_for_rich_block_kind(&item.kind), item.kind_tag);
        }
    }

    #[test]
    fn transform_lookup_handles_parameterized_and_unpresented_kinds() {
        let registry = block_presentation_registry();

        assert!(
            registry
                .transform_for_kind(&RichBlockKind::Todo { checked: true })
                .is_some()
        );
        assert!(registry.transform_for_kind(&RichBlockKind::Image).is_none());
        assert!(registry.transform_by_tag(u16::MAX).is_none());
    }

    #[test]
    fn validation_rejects_unknown_and_duplicate_kind_tags() {
        const UNKNOWN: &[BlockPresentationDescriptor] = &[BlockPresentationDescriptor {
            kind: RichBlockKind::Custom(String::new()),
            slash: slash("?", "Unknown", "Unknown block.", &["unknown"]),
            transform: None,
        }];
        const DUPLICATE: &[BlockPresentationDescriptor] = &[
            BlockPresentationDescriptor {
                kind: RichBlockKind::Todo { checked: false },
                slash: slash("[]", "Todo", "Unchecked todo.", &["todo"]),
                transform: None,
            },
            BlockPresentationDescriptor {
                kind: RichBlockKind::Todo { checked: true },
                slash: slash("[x]", "Done", "Checked todo.", &["done"]),
                transform: None,
            },
        ];

        assert!(
            BlockPresentationRegistry {
                descriptors: UNKNOWN
            }
            .validate_with_known_kind(|_| false)
            .unwrap_err()
            .contains("unknown kind tag")
        );
        assert!(
            BlockPresentationRegistry {
                descriptors: DUPLICATE
            }
            .validate()
            .unwrap_err()
            .contains("duplicate slash presentation kind tag")
        );
    }

    #[test]
    fn validation_rejects_duplicate_and_non_contiguous_transform_order() {
        const DUPLICATE_ORDER: &[BlockPresentationDescriptor] = &[
            test_descriptor(RichBlockKind::Paragraph, 0),
            test_descriptor(RichBlockKind::Quote, 0),
        ];
        const ORDER_GAP: &[BlockPresentationDescriptor] = &[
            test_descriptor(RichBlockKind::Paragraph, 0),
            test_descriptor(RichBlockKind::Quote, 2),
        ];

        assert!(
            BlockPresentationRegistry {
                descriptors: DUPLICATE_ORDER
            }
            .validate()
            .unwrap_err()
            .contains("duplicate transform presentation order")
        );
        assert_eq!(
            BlockPresentationRegistry {
                descriptors: ORDER_GAP
            }
            .validate()
            .unwrap_err(),
            "transform presentation order must be contiguous"
        );
    }

    const fn test_descriptor(kind: RichBlockKind, order: u16) -> BlockPresentationDescriptor {
        BlockPresentationDescriptor {
            kind,
            slash: slash("T", "Test", "Test block.", &["test"]),
            transform: Some(transform(order, "T", "Test")),
        }
    }
}
