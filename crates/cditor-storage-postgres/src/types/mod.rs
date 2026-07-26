use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::ScrollAnchor;
use cditor_core::edit::{
    AssetEditOperation, BlockEditOperation, CollectionEditOperation, CommentEditOperation,
    DocumentSelection, EditOperation, EditTransaction, EditTransactionKind, TableEditOperation,
    TextAffinity, TextEditOperation, TextPosition, TransactionPrecondition,
};
#[cfg(test)]
use cditor_core::edit::{AssetSnapshot, AssetState};
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::ColumnSpec;
use cditor_core::rich_text::{
    BlockAttrs, BlockPayload, BlockPayloadRecord, CalloutVariant, CollectionPayload,
    ColumnsGroupPayload, EmbedPayload, FilePayload, ImagePayload, InlineMark, InlineSpan,
    RichBlockKind, RichTextContent, TableCellAlign, TableCellMerge, TableCellPayload,
    TableCellStyle, TableColumnPayload, TableHeaderStyle, TablePayload, TableRange,
    TableRowPayload, TableTrackSize, TextAlign, WhiteboardPayload,
};

mod attrs;
mod block_kind;
mod ids;
mod payload;
mod rows;
mod transactions;

#[cfg(test)]
pub(crate) use attrs::{DbBlockAttrs, DbTextAlign};
pub(crate) use attrs::{decode_block_attrs, encode_block_attrs};
pub(crate) use block_kind::{rich_block_kind_from_db, rich_block_kind_to_db};
pub(crate) use ids::{
    PgBlockId, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime,
    runtime_block_id_from_pg, runtime_document_id_from_pg,
};
pub(crate) use payload::{DbBlockPayload, decode_block_payload, encode_block_payload};
pub(crate) use rows::DocumentRow;
#[cfg(test)]
pub(crate) use rows::{BlockPayloadRow, BlockRow};
pub(crate) use transactions::{
    DbEditTransaction, decode_edit_transaction, encode_edit_transaction,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ids_map_to_stable_postgres_uuid_namespace() {
        let document = pg_document_id_from_runtime(42);
        let block = pg_block_id_from_runtime(42);

        assert_ne!(document, block);
        assert_eq!(runtime_document_id_from_pg(document), Some(42));
        assert_eq!(runtime_block_id_from_pg(block), Some(42));
        assert_eq!(runtime_block_id_from_pg(document), None);
    }

    #[test]
    fn block_attrs_round_trip_through_json() {
        let mut attrs = BlockAttrs {
            color: Some("#ff0000".to_owned()),
            background_color: Some("#00ff00".to_owned()),
            text_align: TextAlign::Center,
            indent: 3,
            folded: true,
            locked: true,
            custom: BTreeMap::new(),
        };
        attrs.custom.insert("key".to_owned(), "value".to_owned());

        let encoded = encode_block_attrs(&attrs).unwrap();
        let decoded = decode_block_attrs(encoded).unwrap();

        assert_eq!(decoded, attrs);
    }

    #[test]
    fn block_payload_round_trips_through_json() {
        let payloads = vec![
            BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "hello".to_owned(),
                    marks: vec![
                        InlineMark::Bold,
                        InlineMark::Link {
                            href: "https://example.com".to_owned(),
                        },
                    ],
                }],
            },
            BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
            BlockPayload::Table(TablePayload {
                rows: vec![TableRowPayload {
                    cells: vec![TableCellPayload {
                        style: TableCellStyle {
                            background_color: Some("yellow".to_owned()),
                        },
                        ..TableCellPayload::plain("cell")
                    }],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 1,
                header_cols: 0,
                header_style: TableHeaderStyle {
                    row_background_color: Some("gray".to_owned()),
                    column_background_color: Some("blue".to_owned()),
                },
            }),
            BlockPayload::Image(ImagePayload {
                source: "a.png".to_owned(),
                alt: "alt".to_owned(),
                caption: RichTextContent {
                    spans: vec![InlineSpan {
                        text: "caption".to_owned(),
                        marks: vec![InlineMark::Italic],
                    }],
                },
                display_width_ratio_milli: Some(760),
            }),
            BlockPayload::Collection(CollectionPayload::for_block(77, "Projects")),
            BlockPayload::Columns(ColumnsGroupPayload {
                columns: vec![
                    ColumnSpec {
                        block_id: 81,
                        weight: 600_000,
                    },
                    ColumnSpec {
                        block_id: 82,
                        weight: 400_000,
                    },
                ],
                gap_milli: 24_000,
            }),
            BlockPayload::Empty,
        ];

        for payload in payloads {
            let encoded = encode_block_payload(&payload).unwrap();
            let decoded = decode_block_payload(encoded).unwrap();
            assert_eq!(decoded, payload);
        }
    }

    #[test]
    fn opaque_payload_cannot_accidentally_enter_the_jsonb_codec() {
        let payload = cditor_core::fixtures::unknown::unknown_plugin_payload(7).payload;
        let error = encode_block_payload(&payload).unwrap_err();
        assert!(error.to_string().contains("BYTEA lossless path"));
    }

    #[test]
    fn table_payload_schema_round_trips_structure_geometry_merge_and_align() {
        let payload = BlockPayload::Table(TablePayload {
            rows: vec![
                TableRowPayload {
                    cells: vec![
                        TableCellPayload {
                            spans: vec![InlineSpan {
                                text: "merged".to_owned(),
                                marks: vec![InlineMark::Bold],
                            }],
                            align: TableCellAlign::Center,
                            merge: TableCellMerge::Origin {
                                row_span: 2,
                                col_span: 2,
                            },
                            style: TableCellStyle {
                                background_color: Some("blue".to_owned()),
                            },
                        },
                        TableCellPayload {
                            merge: TableCellMerge::Covered {
                                origin_row: 0,
                                origin_col: 0,
                            },
                            ..TableCellPayload::plain("")
                        },
                    ],
                    height: TableTrackSize::Px(48),
                },
                TableRowPayload {
                    cells: vec![
                        TableCellPayload {
                            merge: TableCellMerge::Covered {
                                origin_row: 0,
                                origin_col: 0,
                            },
                            ..TableCellPayload::plain("")
                        },
                        TableCellPayload {
                            align: TableCellAlign::Right,
                            merge: TableCellMerge::Covered {
                                origin_row: 0,
                                origin_col: 0,
                            },
                            ..TableCellPayload::plain("")
                        },
                    ],
                    height: TableTrackSize::Auto,
                },
            ],
            columns: vec![
                TableColumnPayload {
                    width: TableTrackSize::Px(180),
                },
                TableColumnPayload {
                    width: TableTrackSize::Auto,
                },
            ],
            header_rows: 1,
            header_cols: 1,
            header_style: TableHeaderStyle {
                row_background_color: Some("gray".to_owned()),
                column_background_color: Some("slate".to_owned()),
            },
        });

        let encoded = encode_block_payload(&payload).unwrap();
        let decoded = decode_block_payload(encoded).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn edit_transaction_encodes_to_json() {
        let tx = EditTransaction::new(
            7,
            EditTransactionKind::Typing,
            123,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 0,
                text: "A".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 0..1,
            }],
        )
        .with_selection(
            Some(DocumentSelection::caret(TextPosition::downstream(1, 0))),
            Some(DocumentSelection::caret(TextPosition::downstream(1, 1))),
        );

        let encoded = encode_edit_transaction(&tx).unwrap();

        assert_eq!(encoded["id"], 7);
        assert_eq!(encoded["kind"], "typing");
        assert_eq!(encoded["ops"][0]["type"], "insert_text");
        assert_eq!(encoded["inverse_ops"][0]["type"], "delete_text");
        assert_eq!(encoded["after_selection"]["focus"]["offset"], 1);
    }

    #[test]
    fn table_edit_transaction_round_trips_through_json() {
        let tx = EditTransaction::new(
            9,
            EditTransactionKind::ExplicitCommand,
            125,
            vec![
                EditOperation::Table(TableEditOperation::ResizeColumn {
                    block_id: 10,
                    column: 1,
                    old_width: TableTrackSize::Auto,
                    new_width: TableTrackSize::Px(180),
                }),
                EditOperation::Table(TableEditOperation::SetCellAlign {
                    block_id: 10,
                    range: TableRange::normalized(0, 0, 1, 1),
                    old_aligns: vec![vec![TableCellAlign::Left, TableCellAlign::Right]],
                    new_align: TableCellAlign::Center,
                }),
                EditOperation::Table(TableEditOperation::MergeCells {
                    block_id: 10,
                    range: TableRange::normalized(0, 0, 1, 1),
                    before: TablePayload::default(),
                    after: TablePayload::default(),
                }),
            ],
            vec![EditOperation::Table(TableEditOperation::ResizeColumn {
                block_id: 10,
                column: 1,
                old_width: TableTrackSize::Px(180),
                new_width: TableTrackSize::Auto,
            })],
        );

        let encoded = encode_edit_transaction(&tx).unwrap();
        let decoded = decode_edit_transaction(encoded.clone()).unwrap();

        assert_eq!(encoded["ops"][0]["type"], "table");
        assert_eq!(encoded["ops"][0]["op"]["type"], "resize_column");
        assert_eq!(encoded["ops"][1]["op"]["type"], "set_cell_align");
        assert_eq!(encoded["ops"][2]["op"]["type"], "merge_cells");
        assert_eq!(decoded, tx);
    }

    #[test]
    fn typed_domain_operations_round_trip_through_transaction_json() {
        let asset = AssetSnapshot {
            asset_id: 91,
            file_name: "cover.png".to_owned(),
            media_type: "image/png".to_owned(),
            size_bytes: 2048,
            source: "asset://91".to_owned(),
            checksum: Some("sha256:test".to_owned()),
            state: AssetState::Ready,
        };
        let tx = EditTransaction::new(
            10,
            EditTransactionKind::ExplicitCommand,
            126,
            vec![
                EditOperation::Text(TextEditOperation::ReplaceSpans {
                    surface_id: cditor_core::ids::SurfaceId::ImageCaption { block_id: 10 },
                    range: 0..0,
                    old_spans: Vec::new(),
                    new_spans: vec![InlineSpan::plain("caption")],
                }),
                EditOperation::Collection(CollectionEditOperation::SetTitle {
                    block_id: 12,
                    collection_id: 120,
                    before: RichTextContent::plain("Old"),
                    after: RichTextContent::plain("New"),
                }),
                EditOperation::Asset(AssetEditOperation::Attach {
                    block_id: 14,
                    asset,
                }),
            ],
            Vec::new(),
        );

        let encoded = encode_edit_transaction(&tx).unwrap();
        let decoded = decode_edit_transaction(encoded.clone()).unwrap();

        assert_eq!(encoded["ops"][0]["type"], "text");
        assert_eq!(encoded["ops"][1]["type"], "collection");
        assert_eq!(encoded["ops"][2]["type"], "asset");
        assert_eq!(decoded, tx);
    }

    #[test]
    fn move_block_to_parent_transaction_encodes_to_json() {
        let tx = EditTransaction::new(
            8,
            EditTransactionKind::BlockStructureChange,
            124,
            vec![EditOperation::MoveBlockToParent {
                block_id: 10,
                parent_id: Some(3),
                sibling_index: 2,
            }],
            vec![EditOperation::MoveBlockToParent {
                block_id: 10,
                parent_id: None,
                sibling_index: 4,
            }],
        );

        let encoded = encode_edit_transaction(&tx).unwrap();

        assert_eq!(encoded["kind"], "block_structure_change");
        assert_eq!(encoded["ops"][0]["type"], "move_block_to_parent");
        assert_eq!(encoded["ops"][0]["block_id"], 10);
        assert_eq!(encoded["ops"][0]["parent_id"], 3);
        assert_eq!(encoded["ops"][0]["sibling_index"], 2);
        assert_eq!(
            encoded["inverse_ops"][0]["parent_id"],
            serde_json::Value::Null
        );
        assert_eq!(encoded["inverse_ops"][0]["sibling_index"], 4);
    }

    #[test]
    fn rich_block_kind_round_trips_through_db_string() {
        let kinds = [
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 3 },
            RichBlockKind::Callout {
                variant: CalloutVariant::Warning,
            },
            RichBlockKind::Todo { checked: true },
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            RichBlockKind::Database,
            RichBlockKind::Custom("chart".to_owned()),
        ];

        for kind in kinds {
            let encoded = rich_block_kind_to_db(&kind);
            let decoded = rich_block_kind_from_db(&encoded);
            assert_eq!(decoded, kind);
        }
    }
}
