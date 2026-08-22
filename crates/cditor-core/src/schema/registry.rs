//! Block 能力注册表（P1-009，总设计 7.3）。
//!
//! 每个 kind 由唯一 descriptor 声明 payload schema 版本、capabilities 和
//! migrator。GUI 与 Runtime 禁止各自维护 `match kind` 能力表——一律向本
//! 注册表查询。未知 kind 落到 unknown fallback descriptor：安全占位显示、
//! 禁止编辑、payload 无损保留（P1-010）。

use std::{collections::HashMap, sync::OnceLock};

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
    /// Runtime 可将安全导出的纯文本构造成该 kind。
    pub plain_text_conversion_target: bool,
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
            plain_text_conversion_target: false,
            lossless_unknown: false,
        }
    }
}

/// 单个 kind 的 descriptor。
#[derive(Debug, Clone)]
pub struct BlockDescriptor {
    pub kind_tag: u16,
    pub name: &'static str,
    /// Canonical semantic value for a parameterized kind.
    pub default_kind: RichBlockKind,
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

/// Shared immutable registry used by Core, Runtime, and GUI command queries.
pub fn builtin_block_registry() -> &'static BlockRegistry {
    static REGISTRY: OnceLock<BlockRegistry> = OnceLock::new();
    REGISTRY.get_or_init(BlockRegistry::builtin)
}

fn unknown_descriptor() -> BlockDescriptor {
    BlockDescriptor {
        kind_tag: u16::MAX,
        name: "unknown",
        default_kind: RichBlockKind::Custom("unknown".to_owned()),
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
    let text_container = BlockCapabilities {
        container: true,
        ..text
    };
    let media = BlockCapabilities::atomic_media();
    let version = CURRENT_BLOCK_PAYLOAD;
    let describe =
        |kind: &Kind, name: &'static str, capabilities: BlockCapabilities| BlockDescriptor {
            kind_tag: kind_tag_for_rich_block_kind(kind),
            name,
            default_kind: kind.clone(),
            payload_version: version,
            capabilities: BlockCapabilities {
                plain_text_conversion_target: matches!(
                    kind,
                    Kind::Paragraph
                        | Kind::Heading { level: 1..=3 }
                        | Kind::BulletedList
                        | Kind::NumberedList
                        | Kind::Todo { .. }
                        | Kind::Toggle
                        | Kind::Quote
                        | Kind::Callout { .. }
                        | Kind::Code { .. }
                        | Kind::Math
                        | Kind::Mermaid
                        | Kind::Html
                        | Kind::Table
                        | Kind::Whiteboard
                        | Kind::Video
                        | Kind::Divider
                        | Kind::Separator
                        | Kind::FootnoteDefinition
                        | Kind::Comment
                        | Kind::RawMarkdown
                ),
                ..capabilities
            },
            migrator: None,
        };

    let mut descriptors = vec![
        describe(&Kind::Paragraph, "paragraph", text),
        describe(&Kind::Quote, "quote", text_container),
        describe(
            &Kind::Callout {
                variant: crate::rich_text::CalloutVariant::Note,
            },
            "callout",
            text_container,
        ),
        describe(&Kind::Todo { checked: false }, "todo", text_container),
        describe(&Kind::BulletedList, "bulleted_list", text_container),
        describe(&Kind::NumberedList, "numbered_list", text_container),
        describe(&Kind::Toggle, "toggle", text_container),
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
                text_surface: true,
                soft_enter: true,
                block_selection: true,
                stable_box: true,
                collaborative_text: true,
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
        describe(
            &Kind::Video,
            "video",
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
mod tests;
