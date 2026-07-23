use std::collections::BTreeMap;

use super::*;

// P4-013：随机 edit 序列 -> undo all -> redo all 的状态还原 property test。
//
// 语义状态 = 结构（block id/parent/kind 顺序）+ 全部 payload 文本。
// 每轮：记录初始态 -> 执行 N 个随机编辑 -> undo 到栈空，必须回到初始态
// -> redo 到栈空，必须回到终态。随机源是确定性 LCG，失败可用 seed 复现。

fn semantic_state(runtime: &DocumentRuntime) -> String {
    // 结构：preorder 的 (id, parent, depth, kind_tag)。
    let mut state = String::new();
    for position in 0..runtime.document.index.total_count() {
        let block_id = runtime
            .document
            .index
            .id_at(position)
            .expect("index position");
        state.push_str(&format!(
            "{}:{:?}:{}:{};",
            block_id,
            runtime
                .document
                .index
                .parent_id_at(position)
                .expect("parent"),
            runtime.document.index.depth_at(position).expect("depth"),
            runtime.document.index.kind_tag_at(position).expect("kind"),
        ));
    }
    state.push('\n');
    // 内容：稳定排序的 (id -> kind + plain text)。
    let mut contents: BTreeMap<BlockId, String> = BTreeMap::new();
    for (block_id, record) in &runtime.document.payload_window.payloads {
        contents.insert(
            *block_id,
            format!("{:?}={}", record.kind, payload_text(record)),
        );
    }
    for (block_id, content) in contents {
        state.push_str(&format!("{block_id}->{content}\n"));
    }
    state
}

fn payload_text(record: &BlockPayloadRecord) -> String {
    match &record.payload {
        BlockPayload::RichText { spans } => spans.iter().map(|span| span.text.as_str()).collect(),
        BlockPayload::Table(table) => table
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| {
                        cell.spans
                            .iter()
                            .map(|span| span.text.as_str())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect::<Vec<_>>()
            .join("/"),
        other => format!("{other:?}"),
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

/// 对 runtime 执行一个随机编辑；返回是否真的做了编辑。
fn random_edit(runtime: &mut DocumentRuntime, rng: &mut Lcg) -> bool {
    let block_count = runtime.document.index.total_count();
    if block_count == 0 {
        return false;
    }
    let target_position = rng.next(block_count);
    let Some(block_id) = runtime.document.index.id_at(target_position) else {
        return false;
    };
    // 只编辑 rich text 块，保持编辑合法。
    let text_len = runtime
        .document
        .payload_window
        .get(block_id)
        .and_then(|record| match &record.payload {
            BlockPayload::RichText { spans } => {
                Some(spans.iter().map(|span| span.text.len()).sum::<usize>())
            }
            _ => None,
        });
    let Some(text_len) = text_len else {
        return false;
    };

    match rng.next(10) {
        // 50%：焦点处插入字符（覆盖 typing coalescing 与 grapheme 路径）。
        0..=4 => {
            let offset = clamp_to_char_boundary(runtime, block_id, rng.next(text_len + 1));
            if runtime.focus_block_at_offset(block_id, offset).is_err() {
                return false;
            }
            let ch = ['a', 'b', '中', '文', 'é', '🚀', ' '][rng.next(7)];
            runtime.insert_char(ch).is_ok()
        }
        // 20%：删除一个 grapheme。
        5..=6 => {
            if text_len == 0 {
                return false;
            }
            let offset = clamp_to_char_boundary(runtime, block_id, rng.next(text_len) + 1);
            if runtime.focus_block_at_offset(block_id, offset).is_err() {
                return false;
            }
            runtime.delete_backward().unwrap_or(false)
        }
        // 15%：replace range（模拟选区替换/IME commit 形态的编辑）。
        7 => {
            let start = clamp_to_char_boundary(runtime, block_id, rng.next(text_len + 1));
            let end =
                clamp_to_char_boundary(runtime, block_id, start + rng.next(text_len - start + 1));
            if runtime.focus_block_at_offset(block_id, start).is_err() {
                return false;
            }
            runtime
                .replace_text_in_focused_range(Some(start..end), "xy中")
                .unwrap_or(false)
        }
        // 10%：Enter split。
        8 => {
            let offset = clamp_to_char_boundary(runtime, block_id, rng.next(text_len + 1));
            if runtime.focus_block_at_offset(block_id, offset).is_err() {
                return false;
            }
            runtime.handle_enter().is_ok()
        }
        // 10%：merge into previous（首块时为 no-op）。
        _ => {
            if runtime.focus_block_at_offset(block_id, 0).is_err() {
                return false;
            }
            runtime.merge_focused_block_into_previous().unwrap_or(false)
        }
    }
}

fn clamp_to_char_boundary(runtime: &DocumentRuntime, block_id: BlockId, offset: usize) -> usize {
    let Some(record) = runtime.document.payload_window.get(block_id) else {
        return 0;
    };
    let text = payload_text(record);
    let mut clamped = offset.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn run_session(seed: u64, edits: usize) {
    let mut rng = Lcg(seed);
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "hello 世界"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 2 }, "标题 two"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "third block"),
            BlockPayloadRecord::rich_text(4, RichBlockKind::Quote, "quote 引用"),
        ],
        720.0,
    );

    let initial = semantic_state(&runtime);
    let mut applied = 0usize;
    for _ in 0..edits {
        if random_edit(&mut runtime, &mut rng) {
            applied += 1;
        }
    }
    let final_state = semantic_state(&runtime);

    // undo all：栈耗尽后必须回到初始态。
    let mut undo_steps = 0usize;
    while runtime.undo_focused_block().unwrap_or(false) {
        undo_steps += 1;
        assert!(undo_steps <= applied + edits, "undo runaway (seed {seed})");
    }
    assert_eq!(
        semantic_state(&runtime),
        initial,
        "undo-all must restore the initial document (seed {seed}, applied {applied})"
    );

    // redo all：栈耗尽后必须回到终态。
    let mut redo_steps = 0usize;
    while runtime.redo_focused_block().unwrap_or(false) {
        redo_steps += 1;
        assert!(redo_steps <= undo_steps, "redo beyond undo (seed {seed})");
    }
    assert_eq!(
        semantic_state(&runtime),
        final_state,
        "redo-all must restore the final document (seed {seed}, applied {applied})"
    );
    assert_eq!(
        undo_steps, redo_steps,
        "undo/redo step parity (seed {seed})"
    );
}

#[test]
fn randomized_edits_undo_all_redo_all_restore_states() {
    for seed in [0x5eed, 42, 0xfeed_beef, 7_777_777, 0x0123_4567] {
        run_session(seed, 60);
    }
}

#[test]
fn long_session_with_structure_edits_survives_undo_redo_cycles() {
    run_session(0xabad_cafe, 200);
}

#[test]
fn undo_all_on_untouched_runtime_is_a_clean_no_op() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "untouched",
        )],
        720.0,
    );
    let initial = semantic_state(&runtime);
    assert!(!runtime.undo_focused_block().unwrap());
    assert!(!runtime.redo_focused_block().unwrap());
    assert_eq!(semantic_state(&runtime), initial);
}

#[test]
fn new_edit_after_undo_clears_redo_branch() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "base",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 4).unwrap();
    runtime.insert_char('A').unwrap();
    runtime.insert_char('B').unwrap();
    assert!(runtime.undo_focused_block().unwrap());

    // 分叉：undo 后的新编辑必须清空 redo。
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.insert_char('Z').unwrap();
    assert!(
        !runtime.redo_focused_block().unwrap(),
        "redo branch must be gone"
    );
}
