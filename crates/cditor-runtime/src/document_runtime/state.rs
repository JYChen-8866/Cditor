use super::ai::RuntimeAiSession;
use super::document_state::DocumentState;
use super::editing_state::EditingState;
use super::history_state::HistoryState;
use super::layout_state::LayoutState;
use super::selection_state::SelectionState;
use super::transaction_state::TransactionState;
use super::*;

#[derive(Debug)]
pub struct DocumentRuntime {
    pub(super) document_id: DocumentId,
    pub(super) document: DocumentState,
    pub(super) layout: LayoutState,
    pub(super) editing: EditingState,
    pub(super) selection: SelectionState,
    pub(super) ai_session: Option<RuntimeAiSession>,
    pub(super) next_ai_request_id: u64,
    pub(super) history: HistoryState,
    pub(super) transactions: TransactionState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalScrollTarget {
    pub global_scroll_top: f64,
    pub block_index: usize,
    pub block_id: BlockId,
    pub block_top: f64,
    pub offset_in_block: f64,
    pub page_index: usize,
    pub page_top: f64,
    pub offset_in_page: f64,
    pub precision: cditor_viewport::scroll::ScrollPrecision,
}
