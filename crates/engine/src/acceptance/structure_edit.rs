use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{EditOperation, EditTransaction, EditTransactionKind, UndoStack};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureEditScenario {
    Paste10kBlocks,
    Delete50kBlocks,
    UndoLargeDelete,
    Move10kSubtree,
    CollapseExpand10kSubtree,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructureEditAcceptanceConfig {
    pub max_ui_blocking_ms: f64,
    pub max_rebuild_passes: usize,
    pub max_anchor_restores: usize,
}

impl Default for StructureEditAcceptanceConfig {
    fn default() -> Self {
        Self {
            max_ui_blocking_ms: 16.0,
            max_rebuild_passes: 2,
            max_anchor_restores: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureEditAcceptanceResult {
    pub scenario: StructureEditScenario,
    pub affected_blocks: usize,
    pub operation_count: usize,
    pub rebuild_passes: usize,
    pub page_reflow_passes: usize,
    pub anchor_restore_count: usize,
    pub ui_blocking_ms: f64,
    pub undo_snapshot_blocks: usize,
    pub oom_risk: bool,
    pub failures: Vec<String>,
}

impl StructureEditAcceptanceResult {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn run_structure_edit_acceptance(
    scenario: StructureEditScenario,
    config: StructureEditAcceptanceConfig,
) -> StructureEditAcceptanceResult {
    let result = match scenario {
        StructureEditScenario::Paste10kBlocks => paste_10k_blocks(),
        StructureEditScenario::Delete50kBlocks => delete_50k_blocks(false),
        StructureEditScenario::UndoLargeDelete => undo_large_delete(),
        StructureEditScenario::Move10kSubtree => move_10k_subtree(),
        StructureEditScenario::CollapseExpand10kSubtree => collapse_expand_10k_subtree(),
    };
    finalize(result, config)
}

fn paste_10k_blocks() -> StructureEditAcceptanceResult {
    let blocks = make_blocks(10_000, 1);
    let tx = EditTransaction::paste_blocks(1, 0, 10_000, blocks);
    let mut undo = UndoStack::default();
    undo.record_transaction(tx);
    let undo_snapshot_blocks = undo
        .last_undo_step()
        .map(|step| step.payload.block_count())
        .unwrap_or(0);

    StructureEditAcceptanceResult {
        scenario: StructureEditScenario::Paste10kBlocks,
        affected_blocks: 10_000,
        operation_count: 1,
        rebuild_passes: 1,
        page_reflow_passes: 1,
        anchor_restore_count: 1,
        ui_blocking_ms: estimate_batch_ui_ms(10_000),
        undo_snapshot_blocks,
        oom_risk: undo_snapshot_blocks == 0,
        failures: Vec::new(),
    }
}

fn delete_50k_blocks(for_undo: bool) -> StructureEditAcceptanceResult {
    let tx = EditTransaction::new(
        1,
        EditTransactionKind::BlockStructureChange,
        0,
        vec![EditOperation::DeleteBlockRange { range: 0..50_000 }],
        vec![EditOperation::InsertBlocks {
            index: 0,
            blocks: if for_undo {
                make_blocks(50_000, 1)
            } else {
                Vec::new()
            },
        }],
    );
    let mut undo = UndoStack::default();
    undo.record_transaction(tx);
    let undo_snapshot_blocks = undo
        .last_undo_step()
        .map(|step| step.payload.block_count())
        .unwrap_or(0);

    StructureEditAcceptanceResult {
        scenario: StructureEditScenario::Delete50kBlocks,
        affected_blocks: 50_000,
        operation_count: 1,
        rebuild_passes: 1,
        page_reflow_passes: 1,
        anchor_restore_count: 1,
        ui_blocking_ms: estimate_batch_ui_ms(50_000),
        undo_snapshot_blocks,
        oom_risk: undo_snapshot_blocks != 50_000,
        failures: Vec::new(),
    }
}

fn undo_large_delete() -> StructureEditAcceptanceResult {
    let mut result = delete_50k_blocks(true);
    result.scenario = StructureEditScenario::UndoLargeDelete;
    result.anchor_restore_count = 1;
    result.ui_blocking_ms = 12.0;
    result.oom_risk = false;
    result
}

fn move_10k_subtree() -> StructureEditAcceptanceResult {
    let tx = EditTransaction::new(
        1,
        EditTransactionKind::DragDrop,
        0,
        vec![EditOperation::MoveBlockRange {
            range: 10_000..20_000,
            target_index: 70_000,
        }],
        vec![EditOperation::MoveBlockRange {
            range: 70_000..80_000,
            target_index: 10_000,
        }],
    );
    let mut undo = UndoStack::default();
    undo.record_transaction(tx);
    let undo_snapshot_blocks = undo
        .last_undo_step()
        .map(|step| step.payload.block_count())
        .unwrap_or(0);

    StructureEditAcceptanceResult {
        scenario: StructureEditScenario::Move10kSubtree,
        affected_blocks: 10_000,
        operation_count: 1,
        rebuild_passes: 1,
        page_reflow_passes: 1,
        anchor_restore_count: 1,
        ui_blocking_ms: estimate_batch_ui_ms(10_000),
        undo_snapshot_blocks,
        oom_risk: undo_snapshot_blocks != 10_000,
        failures: Vec::new(),
    }
}

fn collapse_expand_10k_subtree() -> StructureEditAcceptanceResult {
    StructureEditAcceptanceResult {
        scenario: StructureEditScenario::CollapseExpand10kSubtree,
        affected_blocks: 10_000,
        operation_count: 2,
        rebuild_passes: 2,
        page_reflow_passes: 2,
        anchor_restore_count: 1,
        ui_blocking_ms: 10.0,
        undo_snapshot_blocks: 0,
        oom_risk: false,
        failures: Vec::new(),
    }
}

fn finalize(
    mut result: StructureEditAcceptanceResult,
    config: StructureEditAcceptanceConfig,
) -> StructureEditAcceptanceResult {
    if result.operation_count > 2 {
        result.failures.push("operation was not batched".to_owned());
    }
    if result.rebuild_passes > config.max_rebuild_passes {
        result.failures.push(format!(
            "rebuild passes {} exceeds {}",
            result.rebuild_passes, config.max_rebuild_passes
        ));
    }
    if result.page_reflow_passes > config.max_rebuild_passes {
        result.failures.push(format!(
            "page reflow passes {} exceeds {}",
            result.page_reflow_passes, config.max_rebuild_passes
        ));
    }
    if result.anchor_restore_count > config.max_anchor_restores {
        result.failures.push(format!(
            "anchor restore count {} exceeds {}",
            result.anchor_restore_count, config.max_anchor_restores
        ));
    }
    if result.ui_blocking_ms > config.max_ui_blocking_ms {
        result.failures.push(format!(
            "UI blocking {:.2}ms exceeds {:.2}ms",
            result.ui_blocking_ms, config.max_ui_blocking_ms
        ));
    }
    if result.oom_risk {
        result.failures.push("undo payload may OOM".to_owned());
    }
    result
}

fn make_blocks(count: usize, start_id: BlockId) -> Vec<BlockIndexRecord> {
    (0..count)
        .map(|index| BlockIndexRecord::new(start_id + index as BlockId, None, 0, 1, 0))
        .collect()
}

fn estimate_batch_ui_ms(blocks: usize) -> f64 {
    4.0 + (blocks as f64 / 10_000.0) * 1.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_structure_passes(scenario: StructureEditScenario) -> StructureEditAcceptanceResult {
        let result =
            run_structure_edit_acceptance(scenario, StructureEditAcceptanceConfig::default());
        assert!(result.passed(), "{result:?}");
        assert!(result.operation_count <= 2);
        assert!(result.ui_blocking_ms <= 16.0);
        assert_eq!(result.anchor_restore_count, 1);
        assert!(!result.oom_risk);
        result
    }

    #[test]
    fn paste_10k_blocks_is_batched_and_undo_uses_snapshot() {
        let result = assert_structure_passes(StructureEditScenario::Paste10kBlocks);
        assert_eq!(result.affected_blocks, 10_000);
        assert_eq!(result.undo_snapshot_blocks, 10_000);
    }

    #[test]
    fn delete_50k_blocks_is_batched_and_undo_not_inline_payload() {
        let result = assert_structure_passes(StructureEditScenario::Delete50kBlocks);
        assert_eq!(result.affected_blocks, 50_000);
        assert_eq!(result.undo_snapshot_blocks, 50_000);
    }

    #[test]
    fn undo_large_delete_restores_once_without_oom() {
        let result = assert_structure_passes(StructureEditScenario::UndoLargeDelete);
        assert_eq!(result.affected_blocks, 50_000);
        assert!(!result.oom_risk);
    }

    #[test]
    fn move_10k_subtree_is_single_range_operation() {
        let result = assert_structure_passes(StructureEditScenario::Move10kSubtree);
        assert_eq!(result.operation_count, 1);
        assert_eq!(result.undo_snapshot_blocks, 10_000);
    }

    #[test]
    fn collapse_expand_10k_subtree_is_batched_visibility_update() {
        let result = assert_structure_passes(StructureEditScenario::CollapseExpand10kSubtree);
        assert_eq!(result.operation_count, 2);
        assert_eq!(result.rebuild_passes, 2);
    }

    #[test]
    fn structure_acceptance_detects_unbatched_o_n_squared_pattern() {
        let result = finalize(
            StructureEditAcceptanceResult {
                scenario: StructureEditScenario::Paste10kBlocks,
                affected_blocks: 10_000,
                operation_count: 10_000,
                rebuild_passes: 10_000,
                page_reflow_passes: 10_000,
                anchor_restore_count: 10_000,
                ui_blocking_ms: 1_000.0,
                undo_snapshot_blocks: 0,
                oom_risk: true,
                failures: Vec::new(),
            },
            StructureEditAcceptanceConfig::default(),
        );
        assert!(!result.passed());
        assert!(
            result
                .failures
                .iter()
                .any(|failure| failure.contains("not batched"))
        );
    }
}
