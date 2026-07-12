use std::ops::Range;
use std::time::{Duration, Instant};

use crate::{EditingPriority, EditingSession};
use cditor_core::edit::{DocumentSelection, EditTransaction, TextPosition};
use cditor_core::ids::BlockId;
use cditor_editor::scroll::{CaretAnchor, ScrollAnchor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRun {
    pub range: Range<usize>,
    pub attrs: InlineAttrs,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineAttrs {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PieceTableTextModel {
    text: String,
    pub inline_runs: Vec<InlineRun>,
}

impl PieceTableTextModel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            inline_runs: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn insert(
        &mut self,
        byte_offset: usize,
        text: &str,
    ) -> Result<Range<usize>, InputHotPathError> {
        if byte_offset > self.text.len() || !self.text.is_char_boundary(byte_offset) {
            return Err(InputHotPathError::InvalidTextOffset(byte_offset));
        }
        self.text.insert_str(byte_offset, text);
        let inserted = byte_offset..byte_offset + text.len();
        self.shift_inline_runs_after_insert(byte_offset, text.len());
        Ok(inserted)
    }

    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        text: &str,
    ) -> Result<Range<usize>, InputHotPathError> {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err(InputHotPathError::InvalidTextRange(range));
        }
        let removed_len = range.end - range.start;
        self.text.replace_range(range.clone(), text);
        let inserted = range.start..range.start + text.len();
        self.shift_inline_runs_after_replace(range.start, removed_len, text.len());
        Ok(inserted)
    }

    pub fn add_inline_run(&mut self, run: InlineRun) {
        self.inline_runs.push(run);
    }

    fn shift_inline_runs_after_insert(&mut self, at: usize, len: usize) {
        for run in &mut self.inline_runs {
            if run.range.start >= at {
                run.range.start += len;
                run.range.end += len;
            } else if run.range.end > at {
                run.range.end += len;
            }
        }
    }

    fn shift_inline_runs_after_replace(
        &mut self,
        at: usize,
        removed_len: usize,
        inserted_len: usize,
    ) {
        let removed_end = at + removed_len;
        let delta = inserted_len as isize - removed_len as isize;
        for run in &mut self.inline_runs {
            if run.range.start >= removed_end {
                run.range.start = run.range.start.saturating_add_signed(delta);
                run.range.end = run.range.end.saturating_add_signed(delta);
            } else if run.range.end > at {
                run.range.end = run
                    .range
                    .end
                    .saturating_add_signed(delta)
                    .max(at + inserted_len);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirtyReason {
    InsertText,
    DeleteText,
    CompositionCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutDirtyRange {
    pub block_id: BlockId,
    pub text_range: Range<usize>,
    pub reason: LayoutDirtyReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalLayoutRequest {
    pub block_id: BlockId,
    pub dirty_range: LayoutDirtyRange,
    pub visual_line_hint: Range<usize>,
    pub priority: EditingPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTaskKind {
    PersistTransaction,
    FtsUpdate,
    SyntaxHighlight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledAsyncTask {
    pub kind: AsyncTaskKind,
    pub block_id: BlockId,
    pub transaction_id: u64,
    pub priority: EditingPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AsyncTaskQueue {
    pub tasks: Vec<ScheduledAsyncTask>,
}

impl AsyncTaskQueue {
    pub fn schedule(&mut self, task: ScheduledAsyncTask) {
        self.tasks.push(task);
    }

    pub fn contains_kind(&self, kind: AsyncTaskKind) -> bool {
        self.tasks.iter().any(|task| task.kind == kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ForbiddenSyncWorkGuard {
    pub sqlite_write: bool,
    pub fts_update: bool,
    pub full_block_shaping: bool,
    pub page_reflow: bool,
    pub waited_async_result: bool,
}

impl ForbiddenSyncWorkGuard {
    pub fn assert_clean(&self) -> Result<(), InputHotPathError> {
        if self.sqlite_write {
            return Err(InputHotPathError::ForbiddenSyncWork("sqlite_write"));
        }
        if self.fts_update {
            return Err(InputHotPathError::ForbiddenSyncWork("fts_update"));
        }
        if self.full_block_shaping {
            return Err(InputHotPathError::ForbiddenSyncWork("full_block_shaping"));
        }
        if self.page_reflow {
            return Err(InputHotPathError::ForbiddenSyncWork("page_reflow"));
        }
        if self.waited_async_result {
            return Err(InputHotPathError::ForbiddenSyncWork("waited_async_result"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputHotPathConfig {
    pub visual_line_context_bytes: usize,
    pub p95_budget: Duration,
    pub p99_budget: Duration,
}

impl Default for InputHotPathConfig {
    fn default() -> Self {
        Self {
            visual_line_context_bytes: 128,
            p95_budget: Duration::from_millis(8),
            p99_budget: Duration::from_millis(16),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputHotPathResult {
    pub dirty_range: LayoutDirtyRange,
    pub layout_request: IncrementalLayoutRequest,
    pub next_caret_anchor: CaretAnchor,
    pub transaction: EditTransaction,
    pub scheduled_tasks: Vec<ScheduledAsyncTask>,
    pub elapsed: Duration,
}

#[derive(Debug)]
pub struct SingleCharInputHotPath {
    next_transaction_id: u64,
    pub async_queue: AsyncTaskQueue,
    pub forbidden_sync_work: ForbiddenSyncWorkGuard,
    pub config: InputHotPathConfig,
}

impl SingleCharInputHotPath {
    pub fn new(config: InputHotPathConfig) -> Self {
        Self {
            next_transaction_id: 1,
            async_queue: AsyncTaskQueue::default(),
            forbidden_sync_work: ForbiddenSyncWorkGuard::default(),
            config,
        }
    }

    pub fn handle_insert_char(
        &mut self,
        session: &mut EditingSession,
        model: &mut PieceTableTextModel,
        byte_offset: usize,
        ch: char,
    ) -> Result<InputHotPathResult, InputHotPathError> {
        let started = Instant::now();
        self.forbidden_sync_work.assert_clean()?;

        let inserted_text = ch.to_string();
        let inserted_range = model.insert(byte_offset, &inserted_text)?;
        let dirty_range = LayoutDirtyRange {
            block_id: session.block_id,
            text_range: inserted_range.clone(),
            reason: LayoutDirtyReason::InsertText,
        };
        let visual_line_hint = visual_line_hint(
            model.text(),
            inserted_range.start,
            self.config.visual_line_context_bytes,
        );
        let next_caret_anchor = CaretAnchor {
            block_id: session.block_id,
            text_offset: (inserted_range.end) as u64,
            caret_rect_y_in_block: session.caret_anchor.caret_rect_y_in_block,
            viewport_y: session.caret_anchor.viewport_y,
        };
        session.apply_content_edit(next_caret_anchor);
        session
            .ensure_layout_and_caret_same_version()
            .map_err(InputHotPathError::EditingSession)?;

        let layout_request = IncrementalLayoutRequest {
            block_id: session.block_id,
            dirty_range: dirty_range.clone(),
            visual_line_hint,
            priority: EditingPriority::Realtime,
        };
        let tx_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let before_selection =
            DocumentSelection::caret(TextPosition::downstream(session.block_id, byte_offset));
        let after_selection = DocumentSelection::caret(TextPosition::downstream(
            session.block_id,
            inserted_range.end,
        ));
        let before_anchor = ScrollAnchor {
            block_id: session.block_id,
            offset_in_block: session.caret_anchor.caret_rect_y_in_block,
            viewport_y: session.caret_anchor.viewport_y,
        };
        let after_anchor = ScrollAnchor {
            block_id: session.block_id,
            offset_in_block: next_caret_anchor.caret_rect_y_in_block,
            viewport_y: next_caret_anchor.viewport_y,
        };
        let transaction = EditTransaction::insert_text(
            tx_id,
            tx_id,
            session.block_id,
            byte_offset,
            inserted_text,
        )
        .with_selection(Some(before_selection), Some(after_selection))
        .with_anchor(Some(before_anchor), Some(after_anchor));

        self.schedule_async_followups(session.block_id, tx_id);
        self.forbidden_sync_work.assert_clean()?;

        Ok(InputHotPathResult {
            dirty_range,
            layout_request,
            next_caret_anchor,
            transaction,
            scheduled_tasks: self.async_queue.tasks.clone(),
            elapsed: started.elapsed(),
        })
    }

    fn schedule_async_followups(&mut self, block_id: BlockId, transaction_id: u64) {
        for kind in [
            AsyncTaskKind::PersistTransaction,
            AsyncTaskKind::FtsUpdate,
            AsyncTaskKind::SyntaxHighlight,
        ] {
            self.async_queue.schedule(ScheduledAsyncTask {
                kind,
                block_id,
                transaction_id,
                priority: EditingPriority::Background,
            });
        }
    }
}

impl Default for SingleCharInputHotPath {
    fn default() -> Self {
        Self::new(InputHotPathConfig::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputHotPathError {
    InvalidTextOffset(usize),
    InvalidTextRange(Range<usize>),
    ForbiddenSyncWork(&'static str),
    EditingSession(crate::EditingSessionError),
}

fn visual_line_hint(text: &str, offset: usize, context: usize) -> Range<usize> {
    let start_limit = offset.saturating_sub(context);
    let end_limit = (offset + context).min(text.len());
    let start = text[..offset]
        .rfind('\n')
        .map(|line_start| line_start + 1)
        .unwrap_or(start_limit)
        .max(start_limit);
    let end = text[offset..]
        .find('\n')
        .map(|relative| offset + relative)
        .unwrap_or(end_limit)
        .min(end_limit);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_editor::scroll::CaretAnchor;

    #[test]
    fn keydown_updates_memory_model_before_transaction_and_async_tasks() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut hot_path = SingleCharInputHotPath::default();

        let result = hot_path
            .handle_insert_char(&mut session, &mut model, 1, 'x')
            .unwrap();

        assert_eq!(model.text(), "axb");
        assert_eq!(result.transaction.ops.len(), 1);
        assert_eq!(result.transaction.affected_blocks, vec![session.block_id]);
        assert!(result.transaction.before_selection.is_some());
        assert!(result.transaction.after_selection.is_some());
        assert!(
            hot_path
                .async_queue
                .contains_kind(AsyncTaskKind::PersistTransaction)
        );
        assert!(hot_path.async_queue.contains_kind(AsyncTaskKind::FtsUpdate));
        assert!(
            hot_path
                .async_queue
                .contains_kind(AsyncTaskKind::SyntaxHighlight)
        );
    }

    #[test]
    fn updates_inline_runs_piece_table_and_dirty_range() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("abcd");
        model.add_inline_run(InlineRun {
            range: 2..4,
            attrs: InlineAttrs {
                bold: true,
                italic: false,
                code: false,
            },
        });
        let mut hot_path = SingleCharInputHotPath::default();

        let result = hot_path
            .handle_insert_char(&mut session, &mut model, 1, '中')
            .unwrap();

        assert_eq!(model.text(), "a中bcd");
        assert_eq!(model.inline_runs[0].range, 5..7);
        assert_eq!(result.dirty_range.block_id, 42);
        assert_eq!(result.dirty_range.text_range, 1..4);
        assert_eq!(result.dirty_range.reason, LayoutDirtyReason::InsertText);
    }

    #[test]
    fn layouts_only_current_block_and_current_visual_line_neighborhood() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("line1\nline2\nline3");
        let mut hot_path = SingleCharInputHotPath::default();

        let result = hot_path
            .handle_insert_char(&mut session, &mut model, 8, 'x')
            .unwrap();

        assert_eq!(result.layout_request.block_id, 42);
        assert_eq!(result.layout_request.priority, EditingPriority::Realtime);
        assert!(result.layout_request.visual_line_hint.start >= 6);
        assert!(result.layout_request.visual_line_hint.end <= 12);
    }

    #[test]
    fn updates_caret_geometry_same_version_as_text_layout() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut hot_path = SingleCharInputHotPath::default();

        let result = hot_path
            .handle_insert_char(&mut session, &mut model, 2, 'c')
            .unwrap();

        assert_eq!(result.next_caret_anchor.text_offset, 3);
        assert!(session.ensure_layout_and_caret_same_version().is_ok());
    }

    #[test]
    fn forbidden_sync_work_is_rejected() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut hot_path = SingleCharInputHotPath::default();
        hot_path.forbidden_sync_work.sqlite_write = true;

        let error = hot_path
            .handle_insert_char(&mut session, &mut model, 1, 'x')
            .unwrap_err();

        assert_eq!(error, InputHotPathError::ForbiddenSyncWork("sqlite_write"));
    }

    #[test]
    fn typing_trace_replay_1000_chars_keeps_caret_stable_and_latency_budget() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("");
        let mut hot_path = SingleCharInputHotPath::default();
        let mut samples = Vec::new();

        for index in 0..1_000 {
            let result = hot_path
                .handle_insert_char(&mut session, &mut model, index, 'a')
                .unwrap();
            samples.push(result.elapsed);
            assert_eq!(session.caret_anchor.viewport_y, 100.0);
        }

        samples.sort_unstable();
        let p95 = samples[(samples.len() as f64 * 0.95).floor() as usize];
        let p99 = samples[(samples.len() as f64 * 0.99).floor() as usize];
        assert!(p95 < Duration::from_millis(8), "p95 was {p95:?}");
        assert!(p99 < Duration::from_millis(16), "p99 was {p99:?}");
        assert_eq!(model.len(), 1_000);
        assert_eq!(session.caret_anchor.text_offset, 1_000);
    }

    #[test]
    fn debug_overlay_can_report_edit_transaction_time() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("a");
        let mut hot_path = SingleCharInputHotPath::default();

        let result = hot_path
            .handle_insert_char(&mut session, &mut model, 1, 'b')
            .unwrap();

        assert!(result.elapsed < Duration::from_millis(16));
        assert_eq!(result.transaction.id, 1);
    }

    fn session() -> EditingSession {
        EditingSession::start(
            42,
            1,
            CaretAnchor {
                block_id: 42,
                text_offset: 0,
                caret_rect_y_in_block: 20.0,
                viewport_y: 100.0,
            },
        )
    }
}
