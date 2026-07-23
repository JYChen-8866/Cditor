use super::*;

#[test]
fn block_and_table_text_edits_share_invalid_cjk_offset_normalization() {
    let mut block_runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "萨德",
        )],
        720.0,
    );
    block_runtime.focus_block_at_offset(1, 2).unwrap();
    assert!(
        block_runtime
            .replace_text_in_focused_range(Some(2..2), "中")
            .unwrap()
    );
    assert_eq!(block_runtime.focused_text(), Some("中萨德"));
    assert_eq!(block_runtime.caret_offset_for_block(1), Some("中".len()));

    let mut table_runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("萨德")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    table_runtime
        .focus_table_cell_at_offset(1, 0, 0, 2)
        .unwrap();
    assert!(
        table_runtime
            .replace_text_in_focused_range(Some(2..2), "中")
            .unwrap()
    );
    let BlockPayload::Table(table) = &table_runtime
        .document
        .payload_window
        .get(1)
        .unwrap()
        .payload
    else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("中萨德"));
    assert_eq!(
        table_runtime.focused_table_cell_offset(),
        Some((1, 0, 0, "中".len()))
    );
}

#[test]
fn table_composition_normalizes_invalid_cjk_caret_before_preview_and_commit() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("萨德")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 2).unwrap();
    runtime.begin_or_update_composition(1, 2..2, "中").unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input().unwrap().1,
        "中萨德"
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(0.."中".len())
    );
    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.document.payload_window.get(1).unwrap().payload
    else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("中萨德"));
}
