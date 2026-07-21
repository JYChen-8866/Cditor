use std::collections::{BTreeSet, HashMap, VecDeque};

use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceState {
    Clean,
    Dirty,
    Saving,
    SaveFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPersistenceState {
    pub block_id: BlockId,
    pub persisted_version: u64,
    pub memory_version: u64,
    pub saving_version: Option<u64>,
    pub state: PersistenceState,
    pub last_error: Option<&'static str>,
}

impl BlockPersistenceState {
    pub fn clean(block_id: BlockId, version: u64) -> Self {
        Self {
            block_id,
            persisted_version: version,
            memory_version: version,
            saving_version: None,
            state: PersistenceState::Clean,
            last_error: None,
        }
    }

    pub fn apply_memory_edit(&mut self, new_memory_version: u64) {
        self.memory_version = self.memory_version.max(new_memory_version);
        if self.persisted_version == self.memory_version && self.saving_version.is_none() {
            self.state = PersistenceState::Clean;
        } else if self.saving_version.is_some() {
            self.state = PersistenceState::Saving;
        } else {
            self.state = PersistenceState::Dirty;
        }
        self.last_error = None;
    }

    pub fn begin_save(&mut self) -> Option<u64> {
        if self.persisted_version == self.memory_version {
            self.saving_version = None;
            self.state = PersistenceState::Clean;
            return None;
        }
        self.saving_version = Some(self.memory_version);
        self.state = PersistenceState::Saving;
        Some(self.memory_version)
    }

    pub fn save_succeeded(&mut self, saved_version: u64) {
        if self.saving_version == Some(saved_version) || saved_version > self.persisted_version {
            self.persisted_version = self.persisted_version.max(saved_version);
        }
        if self.saving_version == Some(saved_version) {
            self.saving_version = None;
        }
        self.last_error = None;
        self.state = if self.persisted_version == self.memory_version {
            PersistenceState::Clean
        } else {
            PersistenceState::Dirty
        };
    }

    pub fn save_failed(&mut self, failed_version: u64, error: &'static str) {
        if self.saving_version == Some(failed_version) {
            self.saving_version = None;
        }
        self.state = PersistenceState::SaveFailed;
        self.last_error = Some(error);
    }

    pub fn has_unsaved_content(&self) -> bool {
        self.persisted_version != self.memory_version || self.state == PersistenceState::SaveFailed
    }

    pub fn should_pin(&self) -> bool {
        self.has_unsaved_content()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceEvent {
    MemoryEdited {
        block_id: BlockId,
        memory_version: u64,
    },
    SaveStarted {
        block_id: BlockId,
        saving_version: u64,
    },
    SaveSucceeded {
        block_id: BlockId,
        saved_version: u64,
    },
    SaveFailed {
        block_id: BlockId,
        failed_version: u64,
        error: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseGuardReport {
    pub can_close_without_prompt: bool,
    pub dirty_blocks: Vec<BlockId>,
    pub save_failed_blocks: Vec<BlockId>,
    pub saving_blocks: Vec<BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptimisticPersistenceManager {
    blocks: HashMap<BlockId, BlockPersistenceState>,
    pinned_blocks: BTreeSet<BlockId>,
    recovery_queue: VecDeque<BlockId>,
}

impl OptimisticPersistenceManager {
    pub fn track_clean_block(&mut self, block_id: BlockId, version: u64) {
        self.blocks
            .insert(block_id, BlockPersistenceState::clean(block_id, version));
        self.update_pin(block_id);
    }

    pub fn state(&self, block_id: BlockId) -> Option<&BlockPersistenceState> {
        self.blocks.get(&block_id)
    }

    pub fn pinned_blocks(&self) -> &BTreeSet<BlockId> {
        &self.pinned_blocks
    }

    pub fn recovery_queue(&self) -> &VecDeque<BlockId> {
        &self.recovery_queue
    }

    pub fn apply_memory_edit(
        &mut self,
        block_id: BlockId,
        memory_version: u64,
    ) -> PersistenceEvent {
        let state = self
            .blocks
            .entry(block_id)
            .or_insert_with(|| BlockPersistenceState::clean(block_id, 0));
        state.apply_memory_edit(memory_version);
        self.update_pin(block_id);
        PersistenceEvent::MemoryEdited {
            block_id,
            memory_version,
        }
    }

    pub fn begin_save(&mut self, block_id: BlockId) -> Option<PersistenceEvent> {
        let state = self.blocks.get_mut(&block_id)?;
        let saving_version = state.begin_save()?;
        self.update_pin(block_id);
        Some(PersistenceEvent::SaveStarted {
            block_id,
            saving_version,
        })
    }

    pub fn save_succeeded(&mut self, block_id: BlockId, saved_version: u64) -> PersistenceEvent {
        let state = self
            .blocks
            .entry(block_id)
            .or_insert_with(|| BlockPersistenceState::clean(block_id, 0));
        state.save_succeeded(saved_version);
        if !state.has_unsaved_content() {
            self.recovery_queue.retain(|queued| *queued != block_id);
        }
        self.update_pin(block_id);
        PersistenceEvent::SaveSucceeded {
            block_id,
            saved_version,
        }
    }

    pub fn save_failed(
        &mut self,
        block_id: BlockId,
        failed_version: u64,
        error: &'static str,
    ) -> PersistenceEvent {
        let state = self
            .blocks
            .entry(block_id)
            .or_insert_with(|| BlockPersistenceState::clean(block_id, 0));
        state.save_failed(failed_version, error);
        if !self.recovery_queue.contains(&block_id) {
            self.recovery_queue.push_back(block_id);
        }
        self.update_pin(block_id);
        PersistenceEvent::SaveFailed {
            block_id,
            failed_version,
            error,
        }
    }

    pub fn close_guard_report(&self) -> CloseGuardReport {
        let mut dirty_blocks = Vec::new();
        let mut save_failed_blocks = Vec::new();
        let mut saving_blocks = Vec::new();
        for (block_id, state) in &self.blocks {
            match state.state {
                PersistenceState::Clean => {}
                PersistenceState::Dirty => dirty_blocks.push(*block_id),
                PersistenceState::Saving => saving_blocks.push(*block_id),
                PersistenceState::SaveFailed => save_failed_blocks.push(*block_id),
            }
        }
        dirty_blocks.sort_unstable();
        save_failed_blocks.sort_unstable();
        saving_blocks.sort_unstable();
        CloseGuardReport {
            can_close_without_prompt: dirty_blocks.is_empty()
                && save_failed_blocks.is_empty()
                && saving_blocks.is_empty(),
            dirty_blocks,
            save_failed_blocks,
            saving_blocks,
        }
    }

    fn update_pin(&mut self, block_id: BlockId) {
        if self
            .blocks
            .get(&block_id)
            .is_some_and(BlockPersistenceState::should_pin)
        {
            self.pinned_blocks.insert(block_id);
        } else {
            self.pinned_blocks.remove(&block_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_v5_success_while_edit_v6_does_not_mark_clean() {
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(42, 4);
        manager.apply_memory_edit(42, 5);
        assert_eq!(
            manager.begin_save(42),
            Some(PersistenceEvent::SaveStarted {
                block_id: 42,
                saving_version: 5
            })
        );
        manager.apply_memory_edit(42, 6);

        manager.save_succeeded(42, 5);

        let state = manager.state(42).unwrap();
        assert_eq!(state.persisted_version, 5);
        assert_eq!(state.memory_version, 6);
        assert_eq!(state.state, PersistenceState::Dirty);
        assert!(manager.pinned_blocks().contains(&42));
    }

    #[test]
    fn save_failed_does_not_drop_memory_content_and_enters_recovery_queue() {
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(42, 1);
        manager.apply_memory_edit(42, 2);
        manager.begin_save(42);

        manager.save_failed(42, 2, "sqlite write fail");

        let state = manager.state(42).unwrap();
        assert_eq!(state.persisted_version, 1);
        assert_eq!(state.memory_version, 2);
        assert_eq!(state.state, PersistenceState::SaveFailed);
        assert_eq!(state.last_error, Some("sqlite write fail"));
        assert!(manager.pinned_blocks().contains(&42));
        assert_eq!(
            manager.recovery_queue().iter().copied().collect::<Vec<_>>(),
            vec![42]
        );
    }

    #[test]
    fn clean_only_when_persisted_version_equals_memory_version() {
        let mut state = BlockPersistenceState::clean(7, 1);
        state.apply_memory_edit(2);
        assert_eq!(state.state, PersistenceState::Dirty);
        state.begin_save();
        assert_eq!(state.state, PersistenceState::Saving);
        state.save_succeeded(2);
        assert_eq!(state.state, PersistenceState::Clean);
        assert_eq!(state.persisted_version, state.memory_version);
    }

    #[test]
    fn close_with_dirty_blocks_reports_prompt_required() {
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(1, 1);
        manager.track_clean_block(2, 1);
        manager.apply_memory_edit(2, 2);

        let report = manager.close_guard_report();

        assert!(!report.can_close_without_prompt);
        assert_eq!(report.dirty_blocks, vec![2]);
        assert!(report.save_failed_blocks.is_empty());
    }

    #[test]
    fn successful_retry_removes_block_from_recovery_queue_and_unpins() {
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(42, 1);
        manager.apply_memory_edit(42, 2);
        manager.begin_save(42);
        manager.save_failed(42, 2, "sqlite write fail");

        manager.begin_save(42);
        manager.save_succeeded(42, 2);

        let state = manager.state(42).unwrap();
        assert_eq!(state.state, PersistenceState::Clean);
        assert!(manager.recovery_queue().is_empty());
        assert!(!manager.pinned_blocks().contains(&42));
    }

    #[test]
    fn close_with_saving_block_also_requires_prompt_or_flush() {
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(42, 1);
        manager.apply_memory_edit(42, 2);
        manager.begin_save(42);

        let report = manager.close_guard_report();

        assert!(!report.can_close_without_prompt);
        assert_eq!(report.saving_blocks, vec![42]);
    }
}
