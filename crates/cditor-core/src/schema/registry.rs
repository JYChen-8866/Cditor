//! Block 能力注册表（P1-009，总设计 7.3）。
//!
//! 每个 kind 由唯一 descriptor 声明 payload schema 版本、capabilities 和
//! migrator。GUI 与 Runtime 禁止各自维护 `match kind` 能力表——一律向本
//! 注册表查询。未知 kind 落到 unknown fallback descriptor：安全占位显示、
//! 禁止编辑、payload 无损保留（P1-010）。

use std::collections::HashMap;

use crate::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};

use super::{CURRENT_BLOCK_PAYLOAD, SchemaVersion};

/// payload 迁移函数：把旧版本 body 升级到当前版本。
pub type PayloadMigrator =
    fn(serde_json::Value, SchemaVersion) -> Result<serde_json::Value, String>;

/// Block 能力位（总设计 7.3 最小集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockCapabilities {
    /// 有可编辑 TextSurface。
    pub text_surface: bool,
    /// 是真实容器（有 children）。
    pub container: bool,
    /// 支持 inline marks。
    pub inline_marks: bool,
    /// 支持 soft enter（块内换行）。
    pub soft_enter: bool,
    /// 有 caption surface。
    pub caption: bool,
    /// 可调整尺寸。
    pub resizable: bool,
    /// 支持 full width。
    pub full_width: bool,
    /// 参与 block selection。
    pub block_selection: bool,
    /// 有内部选择模型（表格 cell、白板图形等）。
    pub inner_selection: bool,
    /// 渲染为 stable box（异步内容不得抖动外框）。
    pub stable_box: bool,
    /// 需要内部虚拟化（大 code/table）。
    pub internal_virtualization: bool,
    /// 有异步资源（图片、embed、渲染管线）。
    pub async_resource: bool,
    /// 可导出 Markdown。
    pub export_markdown: bool,
    /// 可导出 HTML。
    pub export_html: bool,
    /// 文本参与协作合并。
    pub collaborative_text: bool,
    /// payload 以未知 envelope 无损保留（unknown/plugin block）。
    pub lossless_unknown: bool,
}

impl BlockCapabilities {
    /// 普通富文本块的公共能力。
    const fn text_block() -> Self {
        Self {
            text_surface: true,
            inline_marks: true,
            soft_enter: true,
            block_selection: true,
            export_markdown: true,
            export_html: true,
            collaborative_text: true,
            ..Self::empty()
        }
    }

    /// 原子媒体块（无文本编辑）。
    const fn atomic_media() -> Self {
        Self {
            block_selection: true,
            stable_box: true,
            async_resource: true,
            export_markdown: true,
            export_html: true,
            ..Self::empty()
        }
    }

    const fn empty() -> Self {
        Self {
            text_surface: false,
            container: false,
            inline_marks: false,
            soft_enter: false,
            caption: false,
            resizable: false,
            full_width: false,
            block_selection: false,
            inner_selection: false,
            stable_box: false,
            internal_virtualization: false,
            async_resource: false,
            export_markdown: false,
            export_html: false,
            collaborative_text: false,
            lossless_unknown: false,
        }
    }
}

/// 单个 kind 的 descriptor。
#[derive(Debug, Clone)]
pub struct BlockDescriptor {
    pub kind_tag: u16,
    pub name: &'static str,
    pub payload_version: SchemaVersion,
    pub capabilities: BlockCapabilities,
    pub migrator: Option<PayloadMigrator>,
}

/// 注册表错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateTag { tag: u16, existing: &'static str },
    NoMigrator { tag: u16, from: SchemaVersion },
    Migration { tag: u16, message: String },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTag { tag, existing } => {
                write!(formatter, "kind tag {tag} already registered by {existing}")
            }
            Self::NoMigrator { tag, from } => {
                write!(formatter, "kind tag {tag} has no migrator from {from}")
            }
            Self::Migration { tag, message } => {
                write!(formatter, "kind tag {tag} migration failed: {message}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// kind tag -> descriptor 注册表。
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    by_tag: HashMap<u16, BlockDescriptor>,
    unknown: BlockDescriptor,
}

impl BlockRegistry {
    /// 注册全部内置 kind 的注册表。
    pub fn builtin() -> Self {
        let mut registry = Self {
            by_tag: HashMap::new(),
            unknown: unknown_descriptor(),
        };
        for descriptor in builtin_descriptors() {
            registry
                .register(descriptor)
                .expect("builtin descriptors must not conflict");
        }
        registry
    }

    /// 注册新 descriptor（插件 Block 入口）；tag 冲突拒绝。
    pub fn register(&mut self, descriptor: BlockDescriptor) -> Result<(), RegistryError> {
        if let Some(existing) = self.by_tag.get(&descriptor.kind_tag) {
            return Err(RegistryError::DuplicateTag {
                tag: descriptor.kind_tag,
                existing: existing.name,
            });
        }
        self.by_tag.insert(descriptor.kind_tag, descriptor);
        Ok(())
    }

    /// tag 的 descriptor；未注册返回 unknown fallback。
    pub fn descriptor_by_tag(&self, tag: u16) -> &BlockDescriptor {
        self.by_tag.get(&tag).unwrap_or(&self.unknown)
    }

    /// kind 的 descriptor。
    pub fn descriptor_for_kind(&self, kind: &RichBlockKind) -> &BlockDescriptor {
        self.descriptor_by_tag(kind_tag_for_rich_block_kind(kind))
    }

    pub fn is_known(&self, tag: u16) -> bool {
        self.by_tag.contains_key(&tag)
    }

    /// 把旧版本 payload body 迁移到当前版本。
    pub fn migrate_payload(
        &self,
        tag: u16,
        body: serde_json::Value,
        from: SchemaVersion,
    ) -> Result<serde_json::Value, RegistryError> {
        let descriptor = self.descriptor_by_tag(tag);
        if from == descriptor.payload_version {
            return Ok(body);
        }
        match descriptor.migrator {
            Some(migrator) => {
                migrator(body, from).map_err(|message| RegistryError::Migration { tag, message })
            }
            None => Err(RegistryError::NoMigrator { tag, from }),
        }
    }
}

fn unknown_descriptor() -> BlockDescriptor {
    BlockDescriptor {
        kind_tag: u16::MAX,
        name: "unknown",
        payload_version: CURRENT_BLOCK_PAYLOAD,
        capabilities: BlockCapabilities {
            block_selection: true,
            stable_box: true,
            lossless_unknown: true,
            ..BlockCapabilities::empty()
        },
        migrator: None,
    }
}

fn builtin_descriptors() -> Vec<BlockDescriptor> {
    use RichBlockKind as Kind;

    let text = BlockCapabilities::text_block();
    let media = BlockCapabilities::atomic_media();
    let version = CURRENT_BLOCK_PAYLOAD;
    let describe =
        |kind: &Kind, name: &'static str, capabilities: BlockCapabilities| BlockDescriptor {
            kind_tag: kind_tag_for_rich_block_kind(kind),
            name,
            payload_version: version,
            capabilities,
            migrator: None,
        };

    let mut descriptors = vec![
        describe(&Kind::Paragraph, "paragraph", text),
        describe(&Kind::Quote, "quote", text),
        describe(
            &Kind::Callout {
                variant: crate::rich_text::CalloutVariant::Note,
            },
            "callout",
            text,
        ),
        describe(&Kind::Todo { checked: false }, "todo", text),
        describe(&Kind::BulletedList, "bulleted_list", text),
        describe(&Kind::NumberedList, "numbered_list", text),
        describe(
            &Kind::Toggle,
            "toggle",
            BlockCapabilities {
                container: true,
                ..text
            },
        ),
        describe(
            &Kind::Code { language: None },
            "code",
            BlockCapabilities {
                inline_marks: false,
                collaborative_text: true,
                stable_box: true,
                internal_virtualization: true,
                ..BlockCapabilities {
                    text_surface: true,
                    soft_enter: true,
                    block_selection: true,
                    export_markdown: true,
                    export_html: true,
                    ..BlockCapabilities::empty()
                }
            },
        ),
        describe(
            &Kind::Math,
            "math",
            BlockCapabilities {
                text_surface: true,
                soft_enter: true,
                block_selection: true,
                stable_box: true,
                async_resource: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Mermaid,
            "mermaid",
            BlockCapabilities {
                text_surface: true,
                soft_enter: true,
                block_selection: true,
                stable_box: true,
                async_resource: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Html,
            "html",
            BlockCapabilities {
                block_selection: true,
                stable_box: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Table,
            "table",
            BlockCapabilities {
                block_selection: true,
                inner_selection: true,
                stable_box: true,
                internal_virtualization: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::ColumnsGroup,
            "columns_group",
            BlockCapabilities {
                container: true,
                block_selection: true,
                stable_box: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Column,
            "column",
            BlockCapabilities {
                container: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Image,
            "image",
            BlockCapabilities {
                caption: true,
                resizable: true,
                full_width: true,
                ..media
            },
        ),
        describe(&Kind::File, "file", media),
        describe(&Kind::Attachment, "attachment", media),
        describe(
            &Kind::Whiteboard,
            "whiteboard",
            BlockCapabilities {
                inner_selection: true,
                resizable: true,
                ..media
            },
        ),
        describe(
            &Kind::MindMap,
            "mindmap",
            BlockCapabilities {
                inner_selection: true,
                resizable: true,
                ..media
            },
        ),
        describe(
            &Kind::Embed,
            "embed",
            BlockCapabilities {
                resizable: true,
                ..media
            },
        ),
        describe(
            &Kind::Divider,
            "divider",
            BlockCapabilities {
                block_selection: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Separator,
            "separator",
            BlockCapabilities {
                block_selection: true,
                export_markdown: true,
                export_html: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(
            &Kind::Database,
            "database",
            BlockCapabilities {
                block_selection: true,
                inner_selection: true,
                stable_box: true,
                internal_virtualization: true,
                async_resource: true,
                ..BlockCapabilities::empty()
            },
        ),
        describe(&Kind::FootnoteDefinition, "footnote_definition", text),
        describe(&Kind::Comment, "comment", text),
        describe(
            &Kind::RawMarkdown,
            "raw_markdown",
            BlockCapabilities {
                inline_marks: false,
                ..text
            },
        ),
        describe(
            &Kind::Custom(String::new()),
            "custom",
            BlockCapabilities {
                block_selection: true,
                stable_box: true,
                lossless_unknown: true,
                ..BlockCapabilities::empty()
            },
        ),
    ];

    // Heading 1-6 各自独立 tag，共享文本能力。
    for level in 1..=6u8 {
        descriptors.push(describe(&Kind::Heading { level }, "heading", text));
    }

    descriptors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::CalloutVariant;

    fn all_kinds() -> Vec<RichBlockKind> {
        use RichBlockKind as Kind;
        let mut kinds = vec![
            Kind::Paragraph,
            Kind::Quote,
            Kind::Callout {
                variant: CalloutVariant::Warning,
            },
            Kind::Todo { checked: true },
            Kind::BulletedList,
            Kind::NumberedList,
            Kind::Toggle,
            Kind::Code {
                language: Some("rust".to_owned()),
            },
            Kind::Math,
            Kind::Mermaid,
            Kind::Html,
            Kind::Table,
            Kind::ColumnsGroup,
            Kind::Column,
            Kind::Image,
            Kind::File,
            Kind::Attachment,
            Kind::Whiteboard,
            Kind::MindMap,
            Kind::Embed,
            Kind::Divider,
            Kind::Separator,
            Kind::Database,
            Kind::FootnoteDefinition,
            Kind::Comment,
            Kind::RawMarkdown,
        ];
        for level in 1..=6 {
            kinds.push(Kind::Heading { level });
        }
        kinds
    }

    #[test]
    fn every_builtin_kind_has_a_registered_descriptor() {
        let registry = BlockRegistry::builtin();
        for kind in all_kinds() {
            let tag = kind_tag_for_rich_block_kind(&kind);
            assert!(
                registry.is_known(tag),
                "kind {kind:?} (tag {tag}) unregistered"
            );
            let descriptor = registry.descriptor_for_kind(&kind);
            assert_eq!(descriptor.kind_tag, tag);
            assert!(
                !descriptor.capabilities.lossless_unknown
                    || matches!(kind, RichBlockKind::Custom(_))
            );
        }
    }

    #[test]
    fn unknown_tag_falls_back_to_lossless_placeholder() {
        let registry = BlockRegistry::builtin();
        let descriptor = registry.descriptor_by_tag(31_337);
        assert_eq!(descriptor.name, "unknown");
        assert!(descriptor.capabilities.lossless_unknown);
        assert!(
            !descriptor.capabilities.text_surface,
            "unknown block must not be editable"
        );
        assert!(descriptor.capabilities.stable_box);
        assert!(!registry.is_known(31_337));
    }

    #[test]
    fn capability_spot_checks_match_product_semantics() {
        let registry = BlockRegistry::builtin();

        let paragraph = registry.descriptor_for_kind(&RichBlockKind::Paragraph);
        assert!(paragraph.capabilities.text_surface);
        assert!(paragraph.capabilities.inline_marks);

        let code = registry.descriptor_for_kind(&RichBlockKind::Code { language: None });
        assert!(code.capabilities.text_surface);
        assert!(!code.capabilities.inline_marks, "code has no rich marks");
        assert!(code.capabilities.internal_virtualization, "P10-002 target");

        let table = registry.descriptor_for_kind(&RichBlockKind::Table);
        assert!(
            !table.capabilities.text_surface,
            "cells own text, not the table shell"
        );
        assert!(table.capabilities.inner_selection);

        let image = registry.descriptor_for_kind(&RichBlockKind::Image);
        assert!(image.capabilities.caption);
        assert!(image.capabilities.stable_box);

        let toggle = registry.descriptor_for_kind(&RichBlockKind::Toggle);
        assert!(toggle.capabilities.container);
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = BlockRegistry::builtin();
        let error = registry
            .register(BlockDescriptor {
                kind_tag: 1,
                name: "imposter-paragraph",
                payload_version: CURRENT_BLOCK_PAYLOAD,
                capabilities: BlockCapabilities::empty(),
                migrator: None,
            })
            .unwrap_err();
        assert_eq!(
            error,
            RegistryError::DuplicateTag {
                tag: 1,
                existing: "paragraph",
            }
        );
    }

    #[test]
    fn migrate_payload_uses_descriptor_migrator() {
        fn rename_field(
            mut body: serde_json::Value,
            _from: SchemaVersion,
        ) -> Result<serde_json::Value, String> {
            let object = body.as_object_mut().ok_or("expected object")?;
            let value = object.remove("old_name").ok_or("missing old_name")?;
            object.insert("new_name".to_owned(), value);
            Ok(body)
        }

        let mut registry = BlockRegistry::builtin();
        registry
            .register(BlockDescriptor {
                kind_tag: 40_000,
                name: "migratable",
                payload_version: SchemaVersion::new(2, 0),
                capabilities: BlockCapabilities::empty(),
                migrator: Some(rename_field),
            })
            .unwrap();

        let migrated = registry
            .migrate_payload(
                40_000,
                serde_json::json!({"old_name": 7}),
                SchemaVersion::new(1, 0),
            )
            .unwrap();
        assert_eq!(migrated, serde_json::json!({"new_name": 7}));

        // 同版本直接透传。
        let same = registry
            .migrate_payload(
                40_000,
                serde_json::json!({"x": 1}),
                SchemaVersion::new(2, 0),
            )
            .unwrap();
        assert_eq!(same, serde_json::json!({"x": 1}));

        // 无 migrator 的旧版本拒绝。
        let error = registry
            .migrate_payload(1, serde_json::json!({}), SchemaVersion::new(0, 1))
            .unwrap_err();
        assert!(matches!(error, RegistryError::NoMigrator { tag: 1, .. }));
    }
}
