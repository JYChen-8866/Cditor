use super::*;

fn table_payload_record(
    block_id: u64,
    table: cditor_core::rich_text::TablePayload,
) -> BlockPayloadRecord {
    BlockPayloadRecord {
        block_id,
        content_version: 1,
        kind: RichBlockKind::Table,
        payload: cditor_core::rich_text::BlockPayload::Table(table),
    }
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_integration_table_structure_survives_save_and_reopen() {
    use cditor_core::rich_text::{BlockPayload, InlineSpan, TablePayload};

    let stores = stores().await;
    let runtime_document_id = unique_runtime_document_id(190_000);
    let block_base = runtime_document_id * 10;
    let document = document_row(runtime_document_id);

    stores
        .document
        .save_document_metadata(&document)
        .await
        .unwrap();

    // Create a 3x3 table with varied content
    let mut table = TablePayload::default();
    table.rows.push(table.rows[0].clone());
    for row in &mut table.rows {
        row.cells.push(row.cells[0].clone());
    }
    table.columns.push(table.columns[0].clone());

    // Set cell content
    table.rows[0].cells[0].spans = vec![InlineSpan::plain("Header A")];
    table.rows[0].cells[1].spans = vec![InlineSpan::plain("Header B")];
    table.rows[0].cells[2].spans = vec![InlineSpan::plain("Header C")];
    table.rows[1].cells[0].spans = vec![InlineSpan::plain("Row 1 Cell 1")];
    table.rows[1].cells[1].spans = vec![InlineSpan::plain("Row 1 Cell 2")];
    table.rows[1].cells[2].spans = vec![InlineSpan::plain("Row 1 Cell 3")];
    table.rows[2].cells[0].spans = vec![InlineSpan::plain("Row 2 Cell 1")];
    table.rows[2].cells[1].spans = vec![InlineSpan::plain("Row 2 Cell 2")];
    table.rows[2].cells[2].spans = vec![InlineSpan::plain("Row 2 Cell 3")];

    let table_kind = kind_tag_for_rich_block_kind(&RichBlockKind::Table);
    let table_record = BlockIndexRecord::new(block_base, None, 0, table_kind, 0);
    let payload_record = table_payload_record(block_base, table);

    stores
        .document
        .save_block_index_records(document.id, &[table_record], 1)
        .await
        .unwrap();
    stores
        .payload
        .save_block_payloads(document.id, &[payload_record])
        .await
        .unwrap();

    // Reopen and verify structure
    let reopened_payloads = stores
        .payload
        .load_block_payloads(&[block_base])
        .await
        .unwrap();

    assert_eq!(reopened_payloads.records.len(), 1);
    let reopened = &reopened_payloads.records[0];
    assert_eq!(reopened.kind, RichBlockKind::Table);

    if let BlockPayload::Table(reopened_table) = &reopened.payload {
        assert_eq!(reopened_table.rows.len(), 3);
        assert_eq!(reopened_table.rows[0].cells.len(), 3);
        assert_eq!(
            reopened_table.cell_plain_text(0, 0).as_deref(),
            Some("Header A")
        );
        assert_eq!(
            reopened_table.cell_plain_text(1, 1).as_deref(),
            Some("Row 1 Cell 2")
        );
        assert_eq!(
            reopened_table.cell_plain_text(2, 2).as_deref(),
            Some("Row 2 Cell 3")
        );
    } else {
        panic!("Expected table payload");
    }
}

// P-008: 保存后重新打开，row/column sizes 一致
#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_integration_table_track_sizes_survive_save_and_reopen() {
    use cditor_core::rich_text::{BlockPayload, TablePayload, TableTrackSize};

    let stores = stores().await;
    let runtime_document_id = unique_runtime_document_id(200_000);
    let block_base = runtime_document_id * 10;
    let document = document_row(runtime_document_id);

    stores
        .document
        .save_document_metadata(&document)
        .await
        .unwrap();

    // Create table with custom sizes
    let mut table = TablePayload::default();
    table.columns[0].width = TableTrackSize::Px(180);
    table.columns[1].width = TableTrackSize::Px(240);
    table.rows[0].height = TableTrackSize::Px(60);
    table.rows[1].height = TableTrackSize::Auto;

    let table_kind = kind_tag_for_rich_block_kind(&RichBlockKind::Table);
    let table_record = BlockIndexRecord::new(block_base, None, 0, table_kind, 0);
    let payload_record = table_payload_record(block_base, table);

    stores
        .document
        .save_block_index_records(document.id, &[table_record], 1)
        .await
        .unwrap();
    stores
        .payload
        .save_block_payloads(document.id, &[payload_record])
        .await
        .unwrap();

    // Reopen and verify sizes
    let reopened_payloads = stores
        .payload
        .load_block_payloads(&[block_base])
        .await
        .unwrap();

    if let BlockPayload::Table(reopened_table) = &reopened_payloads.records[0].payload {
        assert_eq!(reopened_table.columns[0].width, TableTrackSize::Px(180));
        assert_eq!(reopened_table.columns[1].width, TableTrackSize::Px(240));
        assert_eq!(reopened_table.rows[0].height, TableTrackSize::Px(60));
        assert_eq!(reopened_table.rows[1].height, TableTrackSize::Auto);
    } else {
        panic!("Expected table payload");
    }
}

// P-009: 保存后重新打开，merge/align 一致
#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_integration_table_merge_and_align_survive_save_and_reopen() {
    use cditor_core::rich_text::{
        BlockPayload, InlineSpan, TableCellAlign, TableCellMerge, TablePayload,
    };

    let stores = stores().await;
    let runtime_document_id = unique_runtime_document_id(210_000);
    let block_base = runtime_document_id * 10;
    let document = document_row(runtime_document_id);

    stores
        .document
        .save_document_metadata(&document)
        .await
        .unwrap();

    // Create 3x3 table with merge and align
    let mut table = TablePayload::default();
    table.rows.push(table.rows[0].clone());
    for row in &mut table.rows {
        row.cells.push(row.cells[0].clone());
    }
    table.columns.push(table.columns[0].clone());

    // Merge cells [0,0] and [0,1]
    table.rows[0].cells[0].merge = TableCellMerge::Origin {
        row_span: 1,
        col_span: 2,
    };
    table.rows[0].cells[0].spans = vec![InlineSpan::plain("Merged Header")];
    table.rows[0].cells[1].merge = TableCellMerge::Covered {
        origin_row: 0,
        origin_col: 0,
    };
    table.rows[0].cells[1].spans = vec![];

    // Set alignments
    table.rows[0].cells[0].align = TableCellAlign::Center;
    table.rows[1].cells[0].align = TableCellAlign::Left;
    table.rows[1].cells[1].align = TableCellAlign::Center;
    table.rows[1].cells[2].align = TableCellAlign::Right;

    let table_kind = kind_tag_for_rich_block_kind(&RichBlockKind::Table);
    let table_record = BlockIndexRecord::new(block_base, None, 0, table_kind, 0);
    let payload_record = table_payload_record(block_base, table);

    stores
        .document
        .save_block_index_records(document.id, &[table_record], 1)
        .await
        .unwrap();
    stores
        .payload
        .save_block_payloads(document.id, &[payload_record])
        .await
        .unwrap();

    // Reopen and verify merge/align
    let reopened_payloads = stores
        .payload
        .load_block_payloads(&[block_base])
        .await
        .unwrap();

    if let BlockPayload::Table(reopened_table) = &reopened_payloads.records[0].payload {
        // Verify merge
        assert_eq!(
            reopened_table.rows[0].cells[0].merge,
            TableCellMerge::Origin {
                row_span: 1,
                col_span: 2
            }
        );
        assert_eq!(
            reopened_table.rows[0].cells[1].merge,
            TableCellMerge::Covered {
                origin_row: 0,
                origin_col: 0
            }
        );

        // Verify align
        assert_eq!(
            reopened_table.rows[0].cells[0].align,
            TableCellAlign::Center
        );
        assert_eq!(reopened_table.rows[1].cells[0].align, TableCellAlign::Left);
        assert_eq!(
            reopened_table.rows[1].cells[1].align,
            TableCellAlign::Center
        );
        assert_eq!(reopened_table.rows[1].cells[2].align, TableCellAlign::Right);
    } else {
        panic!("Expected table payload");
    }
}

// P-010: 保存后重新打开，layout cache 不导致表格高度错误
#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_integration_table_height_cache_does_not_corrupt_after_reopen() {
    use cditor_core::rich_text::{BlockPayload, InlineSpan, TablePayload};

    let stores = stores().await;
    let runtime_document_id = unique_runtime_document_id(220_000);
    let block_base = runtime_document_id * 10;
    let document = document_row(runtime_document_id);

    stores
        .document
        .save_document_metadata(&document)
        .await
        .unwrap();

    // Create table with multiline content
    let mut table = TablePayload::default();
    table.rows[0].cells[0].spans = vec![InlineSpan::plain("First line\nSecond line\nThird line")];

    let table_kind = kind_tag_for_rich_block_kind(&RichBlockKind::Table);
    let table_record = BlockIndexRecord::new(block_base, None, 0, table_kind, 0);
    let payload_record = table_payload_record(block_base, table);

    stores
        .document
        .save_block_index_records(document.id, &[table_record], 1)
        .await
        .unwrap();
    stores
        .payload
        .save_block_payloads(document.id, &[payload_record])
        .await
        .unwrap();

    // Save a layout cache entry
    let key = LayoutCacheKey {
        width_bucket: 800,
        exact_width_px: 800,
        content_version: 1,
        attrs_version: 1,
        style_version: 1,
        font_version: 1,
        theme_version: 1,
        scale_factor_milli: 1000,
    };
    let initial_height = 120.0;
    let row = BlockLayoutRow::new(
        block_base,
        key,
        HeightEstimate {
            height: initial_height,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
        },
    );
    stores
        .layout
        .save_block_layout(document.id, &row)
        .await
        .unwrap();

    // Reopen and verify payload is intact
    let reopened_payloads = stores
        .payload
        .load_block_payloads(&[block_base])
        .await
        .unwrap();

    if let BlockPayload::Table(reopened_table) = &reopened_payloads.records[0].payload {
        let text = reopened_table.cell_plain_text(0, 0).unwrap_or_default();
        assert_eq!(text, "First line\nSecond line\nThird line");
        assert_eq!(reopened_table.rows.len(), 2);
    } else {
        panic!("Expected table payload");
    }

    // Verify layout cache is still valid
    let cached = stores
        .layout
        .load_block_height(block_base, key)
        .await
        .unwrap();
    assert_eq!(cached.height, initial_height);
    assert_eq!(cached.source, CacheSource::ExactMatch);
}
