use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::core::document::BlockIndexRecord;
use crate::core::edit::{
    DocumentSelection, EditOperation, EditTransaction, EditTransactionKind, TextAffinity,
    TextPosition,
};
use crate::core::ids::{BlockId, DocumentId};
use crate::core::rich_text::{
    BlockAttrs, BlockPayload, CalloutVariant, EmbedPayload, FilePayload, ImagePayload, InlineMark,
    InlineSpan, RichBlockKind, TableCellPayload, TablePayload, TableRowPayload, TextAlign,
    WhiteboardPayload,
};
use crate::editor::scroll::virtual_scroll::ScrollAnchor;

mod attrs;
mod block_kind;
mod ids;
mod payload;
mod rows;
mod transactions;

pub use attrs::{DbBlockAttrs, DbTextAlign, decode_block_attrs, encode_block_attrs};
pub use block_kind::{rich_block_kind_from_db, rich_block_kind_to_db};
pub use ids::{
    PgBlockId, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime,
    runtime_block_id_from_pg, runtime_document_id_from_pg,
};
pub use payload::{DbBlockPayload, decode_block_payload, encode_block_payload};
pub use rows::{BlockPayloadRow, BlockRow, DocumentRow};
pub use transactions::{DbEditTransaction, decode_edit_transaction, encode_edit_transaction};

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
                        spans: vec![InlineSpan::plain("cell")],
                    }],
                }],
                header_rows: 1,
                header_cols: 0,
            }),
            BlockPayload::Image(ImagePayload {
                source: "a.png".to_owned(),
                alt: "alt".to_owned(),
                caption: "caption".to_owned(),
                display_width_ratio_milli: Some(760),
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
