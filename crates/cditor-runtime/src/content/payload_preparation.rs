use std::sync::Arc;

use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, CollectionPayload, RichBlockKind, TableColumnPayload,
    TableHeaderStyle, TablePayload, TableRowPayload, TableTrackSize,
};

use super::payload_cache::estimated_payload_record_bytes;

const DEFAULT_TABLE_COLUMNS: usize = 3;

/// A storage payload that is ready for the bounded visible-residency commit.
///
/// Construction performs all schema normalization and memory accounting before
/// the value reaches the GPUI update callback. The callback can therefore move
/// the shared record into the resident map without walking payload contents.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedPayloadRecord {
    record: Arc<BlockPayloadRecord>,
    estimated_bytes: usize,
}

impl PreparedPayloadRecord {
    pub fn prepare(record: BlockPayloadRecord) -> Self {
        let record = normalize_payload_record_for_kind(record);
        let estimated_bytes = estimated_payload_record_bytes(&record);
        Self {
            record: Arc::new(record),
            estimated_bytes,
        }
    }

    pub fn block_id(&self) -> cditor_core::ids::BlockId {
        self.record.block_id
    }

    pub fn content_version(&self) -> u64 {
        self.record.content_version
    }

    pub fn record(&self) -> &Arc<BlockPayloadRecord> {
        &self.record
    }

    pub fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }

    pub(crate) fn into_parts(self) -> (Arc<BlockPayloadRecord>, usize) {
        (self.record, self.estimated_bytes)
    }
}

pub fn prepare_payload_records(records: Vec<BlockPayloadRecord>) -> Vec<PreparedPayloadRecord> {
    records
        .into_iter()
        .map(PreparedPayloadRecord::prepare)
        .collect()
}

pub(crate) fn normalize_payload_record_for_kind(
    mut record: BlockPayloadRecord,
) -> BlockPayloadRecord {
    record.payload = ensure_table_payload_for_kind(&record.kind, record.payload);
    if matches!(record.kind, RichBlockKind::Database) {
        record.payload = match record.payload {
            BlockPayload::Collection(mut collection) => {
                if collection.collection_id == 0 {
                    let title = collection.title.plain_text();
                    let active_view_id = collection.active_view_id;
                    let mut normalized = CollectionPayload::for_block(record.block_id, title);
                    if !collection.properties.is_empty() {
                        normalized.properties = std::mem::take(&mut collection.properties);
                    }
                    if !collection.views.is_empty() {
                        normalized.views = std::mem::take(&mut collection.views);
                    }
                    normalized.active_view_id = active_view_id
                        .filter(|view_id| {
                            normalized.views.iter().any(|view| view.view_id == *view_id)
                        })
                        .or_else(|| normalized.views.first().map(|view| view.view_id));
                    normalized.schema_version = collection.schema_version.max(1);
                    BlockPayload::Collection(normalized)
                } else {
                    collection.active_view_id = collection
                        .active_view_id
                        .filter(|view_id| {
                            collection.views.iter().any(|view| view.view_id == *view_id)
                        })
                        .or_else(|| collection.views.first().map(|view| view.view_id));
                    BlockPayload::Collection(collection)
                }
            }
            other => BlockPayload::Collection(CollectionPayload::for_block(
                record.block_id,
                other.plain_text(),
            )),
        };
    }
    record
}

pub(crate) fn ensure_table_payload_for_kind(
    kind: &RichBlockKind,
    payload: BlockPayload,
) -> BlockPayload {
    match kind {
        RichBlockKind::Table => match payload {
            BlockPayload::Table(mut table) if table_has_cells(&table) => {
                table.normalize();
                BlockPayload::Table(table)
            }
            BlockPayload::Table(_) => default_table_payload(String::new()),
            other => default_table_payload(other.plain_text()),
        },
        _ => payload,
    }
}

pub(crate) fn table_has_cells(table: &TablePayload) -> bool {
    table.rows.iter().any(|row| !row.cells.is_empty())
}

pub(crate) fn default_table_payload(first_cell_text: String) -> BlockPayload {
    let mut rows = Vec::with_capacity(DEFAULT_TABLE_COLUMNS);
    let mut first_cell_text = Some(first_cell_text);
    for row_index in 0..DEFAULT_TABLE_COLUMNS {
        let mut cells = Vec::with_capacity(DEFAULT_TABLE_COLUMNS);
        for column_index in 0..DEFAULT_TABLE_COLUMNS {
            let text = if row_index == 0 && column_index == 0 {
                first_cell_text.take().unwrap_or_default()
            } else {
                String::new()
            };
            cells.push(cditor_core::rich_text::TableCellPayload::plain(text));
        }
        rows.push(TableRowPayload {
            cells,
            height: Default::default(),
        });
    }

    BlockPayload::Table(TablePayload {
        rows,
        columns: (0..DEFAULT_TABLE_COLUMNS)
            .map(|_| TableColumnPayload {
                width: TableTrackSize::Auto,
            })
            .collect(),
        header_rows: 0,
        header_cols: 0,
        header_style: TableHeaderStyle::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{CollectionViewLayout, CollectionViewPayload, InlineSpan};

    #[test]
    fn preparation_repairs_table_shape_and_accounts_for_it() {
        let prepared = PreparedPayloadRecord::prepare(BlockPayloadRecord {
            block_id: 9,
            content_version: 3,
            kind: RichBlockKind::Table,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain("first")],
            },
        });

        let BlockPayload::Table(table) = &prepared.record().payload else {
            panic!("table kind must have a table payload")
        };
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("first"));
        assert!(prepared.estimated_bytes() > std::mem::size_of::<BlockPayloadRecord>());
    }

    #[test]
    fn preparation_repairs_database_identity_and_active_view() {
        let mut collection = CollectionPayload::for_block(0, "Tasks");
        collection.collection_id = 0;
        collection.schema_version = 0;
        collection.views = vec![CollectionViewPayload {
            view_id: 41,
            name: "All".to_owned(),
            layout: CollectionViewLayout::Table,
        }];
        collection.active_view_id = Some(99);

        let prepared = PreparedPayloadRecord::prepare(BlockPayloadRecord {
            block_id: 17,
            content_version: 2,
            kind: RichBlockKind::Database,
            payload: BlockPayload::Collection(collection),
        });

        let BlockPayload::Collection(collection) = &prepared.record().payload else {
            panic!("database kind must have a collection payload")
        };
        assert_ne!(collection.collection_id, 0);
        assert_eq!(collection.active_view_id, Some(41));
        assert_eq!(collection.schema_version, 1);
    }
}
