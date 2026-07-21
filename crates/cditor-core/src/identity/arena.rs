//! RuntimeHandle 与 PersistentId 双向 arena（P1-002，ADR-006）。
//!
//! 渲染/布局/索引热路径继续使用紧凑 `u64` handle；arena 提供 128 位持久 ID
//! 与 handle 的双向映射。handle 单调分配、永不复用，从 1 开始（与现有
//! fixture/测试的 `BlockId` 约定一致，0 保留为非法值）。

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::persistent_id::PersistentId;

/// 进程内热路径 handle；跨进程/持久化禁止使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeHandle(u64);

impl RuntimeHandle {
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RuntimeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.0)
    }
}

/// PersistentId <-> RuntimeHandle 双向 arena。
///
/// 泛型参数 `T` 是 typed persistent ID（如 [`super::BlockUid`]），保证不同
/// 实体各用各的 arena，handle 不跨实体混用。
#[derive(Debug, Clone, Default)]
pub struct IdArena<T> {
    next_handle: u64,
    handle_to_id: HashMap<RuntimeHandle, T>,
    id_to_handle: HashMap<PersistentId, RuntimeHandle>,
}

impl<T> IdArena<T>
where
    T: Copy + Into<PersistentId>,
{
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            handle_to_id: HashMap::new(),
            id_to_handle: HashMap::new(),
        }
    }

    /// 返回该持久 ID 的 handle；首次出现时分配新 handle。
    pub fn intern(&mut self, id: T) -> RuntimeHandle {
        let key: PersistentId = id.into();
        if let Some(handle) = self.id_to_handle.get(&key) {
            return *handle;
        }
        let handle = RuntimeHandle(self.next_handle);
        self.next_handle += 1;
        self.id_to_handle.insert(key, handle);
        self.handle_to_id.insert(handle, id);
        handle
    }

    /// 已注册 ID 的 handle；未注册返回 `None`（不分配）。
    pub fn handle_of(&self, id: T) -> Option<RuntimeHandle> {
        self.id_to_handle.get(&id.into()).copied()
    }

    /// handle 对应的持久 ID；未注册或已移除返回 `None`。
    pub fn id_of(&self, handle: RuntimeHandle) -> Option<T> {
        self.handle_to_id.get(&handle).copied()
    }

    /// 移除映射；handle 编号不会被复用。
    pub fn remove(&mut self, handle: RuntimeHandle) -> Option<T> {
        let id = self.handle_to_id.remove(&handle)?;
        self.id_to_handle.remove(&id.into());
        Some(id)
    }

    pub fn len(&self) -> usize {
        self.handle_to_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handle_to_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::persistent_id::BlockUid;
    use super::*;

    fn uid(byte: u8) -> BlockUid {
        BlockUid::new(PersistentId::from_bytes([byte; 16]))
    }

    #[test]
    fn intern_is_idempotent_and_bidirectional() {
        let mut arena = IdArena::new();
        let first = arena.intern(uid(1));
        let second = arena.intern(uid(2));

        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
        assert_eq!(arena.intern(uid(1)), first);
        assert_eq!(arena.handle_of(uid(2)), Some(second));
        assert_eq!(arena.id_of(first), Some(uid(1)));
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn handles_are_never_reused_after_removal() {
        let mut arena = IdArena::new();
        let first = arena.intern(uid(1));
        assert_eq!(arena.remove(first), Some(uid(1)));
        assert_eq!(arena.id_of(first), None);
        assert_eq!(arena.handle_of(uid(1)), None);

        let again = arena.intern(uid(1));
        assert_ne!(again, first, "removed handle must not be reused");
        assert_eq!(again.get(), 2);
    }

    #[test]
    fn zero_handle_is_never_allocated() {
        let mut arena = IdArena::new();
        let handle = arena.intern(uid(9));
        assert!(handle.get() >= 1);
        assert_eq!(arena.remove(RuntimeHandle(0)), None);
    }
}
