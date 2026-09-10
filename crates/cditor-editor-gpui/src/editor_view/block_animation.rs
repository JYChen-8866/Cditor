use cditor_core::ids::BlockId;
use cditor_editor_protocol::{
    command::{CditorCommand, CommandSource},
    projection::ProjectionRequest,
};
use cditor_runtime::EditorViewProjection;
use cditor_session::EditorSessionHandle;
use gpui::{Context, Window};
use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

use crate::editor_view::CditorV2View;

const INSERTION_MOTION_DURATION: Duration = Duration::from_millis(180);
const INSERTED_BLOCK_FADE_START: f32 = 0.65;
const BLOCK_LAYOUT_MOTION_KEY: BlockId = 0;

/// 新 block 的纯视觉入场状态。
///
/// runtime 在插入事务中已经提交最终 block 高度和位置；这里记录动画时钟和
/// 命令执行前的 block tops。渲染层让新 block 完整绘制，并把后续 block 从旧
/// top 插值到新 top，绝不回写 `BlockHeightIndex` 或 `BlockLayoutMeta`。
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectionTruthSnapshot {
    pub block_tops: HashMap<BlockId, f64>,
    pub scroll_top: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct BlockInsertionMotion {
    started: Instant,
    before: ProjectionTruthSnapshot,
    inserted_block_id: Option<BlockId>,
}

impl BlockInsertionMotion {
    pub(crate) fn new_inserted(
        started: Instant,
        before: ProjectionTruthSnapshot,
        inserted_block_id: BlockId,
    ) -> Self {
        Self {
            started,
            before,
            inserted_block_id: Some(inserted_block_id),
        }
    }

    pub(crate) fn new_layout(started: Instant, before: ProjectionTruthSnapshot) -> Self {
        Self {
            started,
            before,
            inserted_block_id: None,
        }
    }

    pub(crate) fn progress_at(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= INSERTION_MOTION_DURATION {
            return 1.0;
        }
        let t = elapsed.as_secs_f32() / INSERTION_MOTION_DURATION.as_secs_f32();
        t * t * (3.0 - 2.0 * t)
    }

    pub(crate) fn is_animating(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) < INSERTION_MOTION_DURATION
    }

    pub(crate) fn opacity_at(&self, progress: f32) -> f32 {
        let t = ((progress - INSERTED_BLOCK_FADE_START) / (1.0 - INSERTED_BLOCK_FADE_START))
            .clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    pub(crate) fn block_offset(
        &self,
        block_id: BlockId,
        current_truth_top: f64,
        progress: f32,
    ) -> Option<f64> {
        let before_top = self.before.block_tops.get(&block_id)?;
        Some((before_top - current_truth_top) * f64::from((1.0 - progress).clamp(0.0, 1.0)))
    }

    pub(crate) fn before_scroll_top(&self) -> f64 {
        self.before.scroll_top
    }

    pub(crate) fn before_top(&self, block_id: BlockId) -> Option<f64> {
        self.before.block_tops.get(&block_id).copied()
    }

    pub(crate) fn scroll_top_at(&self, current_truth_scroll_top: f64, now: Instant) -> f64 {
        let progress = self.progress_at(now);
        self.before.scroll_top
            + (current_truth_scroll_top - self.before.scroll_top) * f64::from(progress)
    }

    pub(crate) fn started(&self) -> Instant {
        self.started
    }

    pub(crate) fn inserted_block_id(&self) -> Option<BlockId> {
        self.inserted_block_id
    }
}

pub(crate) fn projection_truth_snapshot(
    projection: &EditorViewProjection,
) -> ProjectionTruthSnapshot {
    let mut top = projection.before_window_height;
    let mut tops = HashMap::with_capacity(projection.blocks.len());
    for block in &projection.blocks {
        tops.insert(block.block_id, top);
        top += block.layout.effective_height();
    }
    ProjectionTruthSnapshot {
        block_tops: tops,
        scroll_top: projection.scroll.global_scroll_top,
    }
}

pub(crate) fn document_top_from_projection_slice(before_window_height: f64, slice_top: f64) -> f64 {
    before_window_height + slice_top
}

pub(crate) fn snapshot_projection_truth(session: &EditorSessionHandle) -> ProjectionTruthSnapshot {
    let viewport_revision = session
        .snapshot()
        .map(|snapshot| snapshot.revision)
        .unwrap_or(0);
    session
        .projection(ProjectionRequest {
            viewport_revision,
            include_diagnostics: false,
        })
        .map(|projection| projection_truth_snapshot(&projection))
        .unwrap_or_default()
}

pub(crate) fn command_can_insert_block(command: &CditorCommand) -> bool {
    matches!(
        command,
        CditorCommand::InsertParagraphAfterBlock { .. }
            | CditorCommand::InsertParagraphAfterFocused
            | CditorCommand::HandleEnter
            | CditorCommand::EnsureTrailingParagraph
    )
}

pub(crate) fn command_can_animate_block_layout(command: &CditorCommand) -> bool {
    command_can_insert_block(command)
        || matches!(
            command,
            CditorCommand::DeleteBackward
                | CditorCommand::DeleteForward
                | CditorCommand::DeleteBlock { .. }
                | CditorCommand::DeleteSelectedBlocks
        )
}

/// 判断本次成功的结构命令是否新增并聚焦了一个 block。
///
/// `CommandOutcome::affected_blocks` 对显式插入命令包含新 block，但
/// `HandleEnter` 的 runtime 结果只保证 revision 变化。块数增长 + 焦点落到
/// 新 id 这两条约束能把 split、复杂块 Enter、尾部补位统一识别出来，同时
/// 排除“文档标题 Enter 跳到已有块”这类仅改焦点的情况。
pub(crate) fn inserted_block_id(
    command: &CditorCommand,
    affected_blocks: &[BlockId],
    before_block_count: usize,
    after_block_count: usize,
    before_focused_block_id: Option<BlockId>,
    after_focused_block_id: Option<BlockId>,
) -> Option<BlockId> {
    if after_block_count <= before_block_count {
        return None;
    }

    let candidate = match command {
        CditorCommand::InsertParagraphAfterBlock { .. }
        | CditorCommand::InsertParagraphAfterFocused => affected_blocks.last().copied(),
        CditorCommand::HandleEnter | CditorCommand::EnsureTrailingParagraph => {
            after_focused_block_id
        }
        _ => None,
    }?;

    (Some(candidate) != before_focused_block_id).then_some(candidate)
}

/// 只对直接操作用户界面的来源播放结构动画。
///
/// SDK、自动化、导入、插件和 AI 可能一次插入大量 block；逐块补间会让文档
/// 分多帧落位，因此这些来源继续走原子瞬时更新。
pub(crate) fn source_animates_block_insert(source: CommandSource) -> bool {
    matches!(
        source,
        CommandSource::Keyboard
            | CommandSource::Toolbar
            | CommandSource::SlashMenu
            | CommandSource::ContextMenu
    )
}

impl CditorV2View {
    pub(crate) fn presented_scroll_top_for_frame(
        &self,
        truth_scroll_top: f64,
        now: Instant,
    ) -> f64 {
        self.overlay
            .block_insertion_motions
            .values()
            .max_by_key(|motion| motion.started())
            .map(|motion| motion.scroll_top_at(truth_scroll_top, now))
            .unwrap_or(truth_scroll_top)
    }

    /// 启动新 block 的纯视觉入场动画。
    ///
    /// runtime 高度和顺序已经一次性提交为最终真相。这里保存命令执行前的
    /// truth projection tops；渲染每帧把同一个 block 从 before top 投影到
    /// 当前 truth top。它不锁测量、不写高度，也不产生额外文档修订。
    pub(crate) fn start_block_insertion_animation(
        &mut self,
        block_id: BlockId,
        before: ProjectionTruthSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.overlay.block_insertion_motions.contains_key(&block_id) {
            return;
        }

        if self
            .ready_session()
            .and_then(|session| session.block_layout_context(block_id).ok())
            .flatten()
            .is_none()
        {
            return;
        }

        let now = Instant::now();
        let before_top_count = before.block_tops.len();
        let before_scroll_top = before.scroll_top;
        self.overlay.block_insertion_motions.insert(
            block_id,
            BlockInsertionMotion::new_inserted(now, before, block_id),
        );
        crate::diagnostics::block_motion::trace(
            "start",
            format_args!(
                "block={block_id} truth=document-layout-final before_tops={before_top_count} before_scroll={before_scroll_top:.2}",
            ),
        );
        cx.notify();
    }

    pub(crate) fn start_block_layout_animation(
        &mut self,
        before: ProjectionTruthSnapshot,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.overlay.block_insertion_motions.insert(
            BLOCK_LAYOUT_MOTION_KEY,
            BlockInsertionMotion::new_layout(now, before),
        );
        crate::diagnostics::block_motion::trace(
            "start",
            format_args!("block-layout truth=document-layout-final"),
        );
        cx.notify();
    }

    /// 推进纯视觉插入动画，并在结束后释放测量所有权。
    pub(crate) fn advance_block_insertion_motions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay.block_insertion_motions.is_empty() {
            return;
        }

        let now = Instant::now();
        let mut finished = Vec::new();
        self.overlay
            .block_insertion_motions
            .retain(|block_id, motion| {
                if motion.is_animating(now) {
                    true
                } else {
                    finished.push(*block_id);
                    false
                }
            });

        for block_id in finished {
            crate::diagnostics::block_motion::trace(
                "settle",
                format_args!("block={block_id} duration_ms={INSERTION_MOTION_DURATION:?}"),
            );
        }

        if !self.overlay.block_insertion_motions.is_empty() {
            window.request_animation_frame();
        } else {
            // 释放后让文本布局再跑一帧，用自然测量校正估算；这里不经过补间通道。
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{RichBlockRecord, RichTextDocument};
    use cditor_editor_protocol::command::{CommandOutcomeStatus, CommandSource};
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, Entity, ParentElement, Render, Styled, TestAppContext, div};

    use super::*;

    struct MotionHost {
        editor: Entity<CditorV2View>,
    }

    impl Render for MotionHost {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            div().size_full().child(self.editor.clone())
        }
    }

    #[test]
    fn explicit_insert_uses_the_last_affected_block() {
        assert_eq!(
            inserted_block_id(
                &CditorCommand::InsertParagraphAfterFocused,
                &[4, 9],
                3,
                4,
                Some(4),
                Some(9),
            ),
            Some(9)
        );
        assert_eq!(
            inserted_block_id(
                &CditorCommand::InsertParagraphAfterBlock { block_id: 4 },
                &[4, 9],
                3,
                4,
                Some(4),
                Some(9),
            ),
            Some(9)
        );
    }

    #[test]
    fn handle_enter_uses_the_new_focus_only_when_the_document_grows() {
        assert_eq!(
            inserted_block_id(&CditorCommand::HandleEnter, &[4], 3, 4, Some(4), Some(9),),
            Some(9)
        );
        assert_eq!(
            inserted_block_id(&CditorCommand::HandleEnter, &[4], 3, 3, Some(4), Some(5),),
            None,
            "标题 Enter 跳到已有块时不能播放入场动画"
        );
    }

    #[test]
    fn unrelated_commands_never_start_insert_animation() {
        assert!(!command_can_insert_block(
            &CditorCommand::InsertSoftLineBreak
        ));
        assert_eq!(
            inserted_block_id(
                &CditorCommand::InsertSoftLineBreak,
                &[4],
                3,
                4,
                Some(4),
                Some(4),
            ),
            None
        );
    }

    #[test]
    fn only_direct_user_commands_animate_structural_inserts() {
        assert!(source_animates_block_insert(CommandSource::Keyboard));
        assert!(source_animates_block_insert(CommandSource::Toolbar));
        assert!(!source_animates_block_insert(CommandSource::Sdk));
        assert!(!source_animates_block_insert(CommandSource::Automation));
        assert!(!source_animates_block_insert(CommandSource::Import));
        assert!(!source_animates_block_insert(CommandSource::Ai));
    }

    #[test]
    fn visual_insertion_motion_has_zero_start_and_one_at_settle() {
        let start = Instant::now();
        let motion =
            BlockInsertionMotion::new_inserted(start, ProjectionTruthSnapshot::default(), 1);

        assert_eq!(motion.progress_at(start), 0.0);
        assert!(motion.progress_at(start + INSERTION_MOTION_DURATION / 2) > 0.0);
        assert!(motion.progress_at(start + INSERTION_MOTION_DURATION / 2) < 1.0);
        assert_eq!(motion.progress_at(start + INSERTION_MOTION_DURATION), 1.0);
        assert!(!motion.is_animating(start + INSERTION_MOTION_DURATION));
    }

    #[test]
    fn inserted_block_fades_in_after_the_following_block_has_moved() {
        let motion = BlockInsertionMotion::new_inserted(
            Instant::now(),
            ProjectionTruthSnapshot::default(),
            1,
        );

        assert_eq!(motion.opacity_at(0.0), 0.0);
        assert_eq!(motion.opacity_at(INSERTED_BLOCK_FADE_START), 0.0);
        assert!(motion.opacity_at(0.80) > 0.0);
        assert!(motion.opacity_at(0.80) < 1.0);
        assert_eq!(motion.opacity_at(1.0), 1.0);
    }

    #[test]
    fn block_removal_offset_moves_a_surviving_block_upward() {
        let motion = BlockInsertionMotion::new_layout(
            Instant::now(),
            ProjectionTruthSnapshot {
                block_tops: HashMap::from([(2, 182.0)]),
                scroll_top: 0.0,
            },
        );

        assert_eq!(motion.block_offset(2, 142.0, 0.0), Some(40.0));
        assert_eq!(motion.block_offset(2, 142.0, 0.5), Some(20.0));
        assert_eq!(motion.block_offset(2, 142.0, 1.0), Some(0.0));
    }

    #[gpui::test]
    fn delete_backward_starts_an_upward_layout_motion(cx: &mut TestAppContext) {
        let mut document = RichTextDocument::empty(13);
        document.push_root_block(RichBlockRecord::paragraph(1, "a"));
        document.push_root_block(RichBlockRecord::paragraph(2, "b"));
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, cx| {
            view.dispatch_command(
                CditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection::caret(
                        cditor_core::edit::TextPosition::downstream(2, 0),
                    ),
                },
                CommandSource::Keyboard,
                cx,
            )
            .unwrap();
            let before_count = view
                .ready_session()
                .unwrap()
                .document_snapshot()
                .unwrap()
                .block_count;
            let outcome = view
                .dispatch_command(CditorCommand::DeleteBackward, CommandSource::Keyboard, cx)
                .unwrap();
            assert!(outcome.changed());
            assert!(
                view.ready_session()
                    .unwrap()
                    .document_snapshot()
                    .unwrap()
                    .block_count
                    < before_count
            );
            let motion = view
                .overlay
                .block_insertion_motions
                .get(&BLOCK_LAYOUT_MOTION_KEY)
                .expect("delete merge must start a layout motion");
            assert_eq!(motion.inserted_block_id(), None);
        });
    }

    #[test]
    fn insertion_geometry_interpolates_each_block_from_its_before_top() {
        let final_height = 96.0;
        let inserted_final_top = 200.0;
        let following_final_top = inserted_final_top + final_height;
        let following_before_top = following_final_top - final_height;
        let motion = BlockInsertionMotion::new_inserted(
            Instant::now(),
            ProjectionTruthSnapshot {
                block_tops: HashMap::from([(2, following_before_top)]),
                scroll_top: 0.0,
            },
            1,
        );
        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let trailing_offset = motion
                .block_offset(2, following_final_top, progress)
                .expect("following block exists in the before projection");
            assert!(trailing_offset <= 0.0);
            assert!(trailing_offset >= -final_height);
            assert!(
                (following_before_top
                    + (following_final_top - following_before_top) * f64::from(progress)
                    - (following_final_top + trailing_offset))
                    .abs()
                    < 0.000_001
            );
        }

        // Split case: the source block shrinks by exactly the inserted height, so
        // the following block's truth top does not move and must not jump.
        let split_before_top = following_final_top;
        let split_motion = BlockInsertionMotion::new_inserted(
            Instant::now(),
            ProjectionTruthSnapshot {
                block_tops: HashMap::from([(2, split_before_top)]),
                scroll_top: 0.0,
            },
            1,
        );
        assert_eq!(
            split_motion
                .block_offset(2, following_final_top, 0.0)
                .unwrap(),
            0.0
        );
        assert_eq!(
            split_motion
                .block_offset(2, following_final_top, 0.5)
                .unwrap(),
            0.0
        );

        assert_eq!(
            motion.block_offset(9, following_final_top, 0.5),
            None,
            "blocks absent from the before projection are not guessed"
        );
    }

    #[test]
    fn projection_slice_top_is_converted_to_document_truth_coordinates() {
        assert_eq!(document_top_from_projection_slice(190.0, 616.0), 806.0);
        assert_eq!(document_top_from_projection_slice(0.0, 806.0), 806.0);
    }

    #[test]
    fn scroll_projection_uses_the_same_progress_as_blocks() {
        let before_scroll = 846.0;
        let truth_scroll = 886.0;
        let start = Instant::now();
        let motion = BlockInsertionMotion::new_layout(
            start,
            ProjectionTruthSnapshot {
                block_tops: HashMap::new(),
                scroll_top: before_scroll,
            },
        );

        assert_eq!(motion.scroll_top_at(truth_scroll, start), before_scroll);
        let midpoint = motion.scroll_top_at(truth_scroll, start + INSERTION_MOTION_DURATION / 2);
        assert!(midpoint > before_scroll);
        assert!(midpoint < truth_scroll);
        assert_eq!(
            motion.scroll_top_at(truth_scroll, start + INSERTION_MOTION_DURATION),
            truth_scroll
        );
    }

    #[gpui::test]
    fn insert_paragraph_keeps_final_truth_while_visual_motion_starts(cx: &mut TestAppContext) {
        let mut document = RichTextDocument::empty(9);
        document.push_root_block(RichBlockRecord::paragraph(1, "body"));
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, cx| {
            view.dispatch_command(
                CditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Keyboard,
                cx,
            )
            .unwrap();
            let outcome = view
                .dispatch_command(
                    CditorCommand::InsertParagraphAfterFocused,
                    CommandSource::Keyboard,
                    cx,
                )
                .unwrap();
            assert_eq!(outcome.status, CommandOutcomeStatus::Applied);
            let inserted_block_id = *outcome.affected_blocks.last().unwrap();
            assert_ne!(inserted_block_id, 1);

            assert!(
                view.overlay
                    .block_insertion_motions
                    .contains_key(&inserted_block_id),
                "visual insertion motion must start with the paragraph"
            );

            let context = view
                .ready_session()
                .unwrap()
                .block_layout_context(inserted_block_id)
                .unwrap()
                .unwrap();
            assert!(
                context.effective_height > 0.0,
                "runtime must expose the final inserted height immediately"
            );
        });
    }

    #[gpui::test]
    fn handle_enter_on_a_complex_block_animates_the_inserted_paragraph(cx: &mut TestAppContext) {
        let mut document = RichTextDocument::empty(10);
        document.push_root_block(RichBlockRecord::divider(1));
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, cx| {
            view.dispatch_command(
                CditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Keyboard,
                cx,
            )
            .unwrap();
            let outcome = view
                .dispatch_command(CditorCommand::HandleEnter, CommandSource::Keyboard, cx)
                .unwrap();

            assert_eq!(outcome.status, CommandOutcomeStatus::Applied);
            let inserted_block_id = outcome
                .affected_blocks
                .last()
                .copied()
                .filter(|block_id| *block_id != 1)
                .unwrap_or_else(|| {
                    view.ready_session()
                        .unwrap()
                        .document_snapshot()
                        .unwrap()
                        .focused_block_id
                        .unwrap()
                });
            assert!(
                view.overlay
                    .block_insertion_motions
                    .contains_key(&inserted_block_id)
            );
            assert!(
                view.ready_session()
                    .unwrap()
                    .block_layout_context(inserted_block_id)
                    .unwrap()
                    .unwrap()
                    .effective_height
                    > 0.0
            );
        });
    }

    #[gpui::test]
    fn split_enter_captures_following_block_before_top(cx: &mut TestAppContext) {
        let mut document = RichTextDocument::empty(12);
        document.push_root_block(RichBlockRecord::paragraph(1, "abcdef"));
        document.push_root_block(RichBlockRecord::paragraph(2, "following"));
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, cx| {
            view.dispatch_command(
                CditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection::caret(
                        cditor_core::edit::TextPosition::downstream(1, 3),
                    ),
                },
                CommandSource::Keyboard,
                cx,
            )
            .unwrap();
            let before_truth = snapshot_projection_truth(view.ready_session().unwrap());
            let outcome = view
                .dispatch_command(CditorCommand::HandleEnter, CommandSource::Keyboard, cx)
                .unwrap();
            let inserted_block_id = view
                .ready_session()
                .unwrap()
                .document_snapshot()
                .unwrap()
                .focused_block_id
                .unwrap();
            assert_ne!(inserted_block_id, 1);
            assert!(outcome.changed());

            let motion = view
                .overlay
                .block_insertion_motions
                .get(&inserted_block_id)
                .expect("split Enter must start insertion motion");
            assert_eq!(
                motion.before_top(2),
                before_truth.block_tops.get(&2).copied()
            );
        });
    }

    #[gpui::test]
    fn insertion_motion_render_path_logs_truth_and_projected_geometry(cx: &mut TestAppContext) {
        let (host, cx) = cx.add_window_view(|window, cx| {
            let mut document = RichTextDocument::empty(11);
            document.push_root_block(RichBlockRecord::paragraph(1, "body"));
            document.push_root_block(RichBlockRecord::paragraph(2, "following"));
            let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
            let editor = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));
            editor.update(cx, |view, cx| {
                view.dispatch_command(
                    CditorCommand::FocusBlock { block_id: 1 },
                    CommandSource::Keyboard,
                    cx,
                )
                .unwrap();
                view.dispatch_command(
                    CditorCommand::InsertParagraphAfterFocused,
                    CommandSource::Keyboard,
                    cx,
                )
                .unwrap();
            });
            window.refresh();
            MotionHost { editor }
        });

        let motion_started = host.read_with(cx, |host, cx| {
            host.editor.read_with(cx, |view, _| {
                !view.overlay.block_insertion_motions.is_empty()
            })
        });
        assert!(
            motion_started,
            "the render test must start inside the motion"
        );

        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
}
