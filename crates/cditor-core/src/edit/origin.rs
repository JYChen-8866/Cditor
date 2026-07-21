//! 统一 ChangeOrigin（P4-007，总设计 12.2）。
//!
//! 每个文档变更事务必须携带唯一 origin：它决定 undo 归属（remote/plugin 变
//! 更不进本地 undo）、save 状态、telemetry 维度和协作回放语义。core 是唯一
//! 定义点；App/SDK 层的旧 `ChangeOrigin`（Local/Host/...）映射到本类型，禁
//! 止再新增第二套 origin 枚举。

use serde::{Deserialize, Serialize};

/// 文档变更来源。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOrigin {
    /// 用户直接输入（键盘/鼠标/触控板产生的本地编辑）。
    #[default]
    User,
    /// IME composition commit。
    Ime,
    /// 撤销。
    Undo,
    /// 重做。
    Redo,
    /// 远端协作 operation 应用。
    Remote,
    /// AI apply。
    Ai,
    /// 插件通过 SDK 写入。
    Plugin,
    /// 宿主应用通过 SDK 写入。
    Host,
    /// 导入（文件/剪贴板批量导入管线）。
    Import,
    /// 数据迁移写入。
    Migration,
}

impl ChangeOrigin {
    /// 是否进入本地 undo 栈（总设计 12.4：remote 不进本地 undo；undo/redo
    /// 本身是栈操作不再入栈；迁移不可撤销）。
    pub const fn records_local_undo(self) -> bool {
        matches!(
            self,
            Self::User | Self::Ime | Self::Ai | Self::Plugin | Self::Host | Self::Import
        )
    }

    /// 是否打断 typing coalescing（P4-008：非连续用户输入边界）。
    pub const fn breaks_typing_coalescing(self) -> bool {
        !matches!(self, Self::User)
    }

    /// 是否标记文档为本地 dirty 并进入保存管线。
    pub const fn marks_document_dirty(self) -> bool {
        // Remote 变更已在服务端持久化，本地只需物化，不产生 outbox 条目；
        // 其余全部来源都需要本地保存。
        !matches!(self, Self::Remote)
    }

    /// 稳定 wire tag（telemetry/journal 使用）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Ime => "ime",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Remote => "remote",
            Self::Ai => "ai",
            Self::Plugin => "plugin",
            Self::Host => "host",
            Self::Import => "import",
            Self::Migration => "migration",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ChangeOrigin; 10] = [
        ChangeOrigin::User,
        ChangeOrigin::Ime,
        ChangeOrigin::Undo,
        ChangeOrigin::Redo,
        ChangeOrigin::Remote,
        ChangeOrigin::Ai,
        ChangeOrigin::Plugin,
        ChangeOrigin::Host,
        ChangeOrigin::Import,
        ChangeOrigin::Migration,
    ];

    #[test]
    fn undo_membership_follows_master_design() {
        // remote 不进本地 undo；undo/redo 是栈操作；迁移不可撤销。
        assert!(!ChangeOrigin::Remote.records_local_undo());
        assert!(!ChangeOrigin::Undo.records_local_undo());
        assert!(!ChangeOrigin::Redo.records_local_undo());
        assert!(!ChangeOrigin::Migration.records_local_undo());
        // 用户/IME/AI/插件/宿主/导入的变更必须可撤销。
        for origin in [
            ChangeOrigin::User,
            ChangeOrigin::Ime,
            ChangeOrigin::Ai,
            ChangeOrigin::Plugin,
            ChangeOrigin::Host,
            ChangeOrigin::Import,
        ] {
            assert!(origin.records_local_undo(), "{origin:?} must be undoable");
        }
    }

    #[test]
    fn only_user_input_coalesces_typing() {
        for origin in ALL {
            assert_eq!(
                origin.breaks_typing_coalescing(),
                origin != ChangeOrigin::User,
                "{origin:?}"
            );
        }
    }

    #[test]
    fn remote_changes_do_not_mark_dirty() {
        for origin in ALL {
            assert_eq!(
                origin.marks_document_dirty(),
                origin != ChangeOrigin::Remote,
                "{origin:?}"
            );
        }
    }

    #[test]
    fn wire_tags_are_stable_snake_case() {
        for origin in ALL {
            let tag = origin.as_str();
            assert!(tag.chars().all(|ch| ch.is_ascii_lowercase()));
            let json = serde_json::to_value(origin).unwrap();
            assert_eq!(json, tag, "serde tag must equal as_str for {origin:?}");
        }
    }

    #[test]
    fn serde_round_trip() {
        for origin in ALL {
            let json = serde_json::to_string(&origin).unwrap();
            let back: ChangeOrigin = serde_json::from_str(&json).unwrap();
            assert_eq!(back, origin);
        }
    }
}
