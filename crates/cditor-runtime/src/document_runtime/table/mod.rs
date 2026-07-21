use super::*;

const DEFAULT_TABLE_COLUMNS: usize = 3;

mod clipboard;
mod edit;
mod input;
mod layout;
mod navigation;
mod projection;
mod reorder;
mod resize;
mod runtime;
mod scroll;
mod selection;
mod transaction;

pub use clipboard::TableClipboardSnapshot;
pub(super) use layout::{span_size, table_payload_projected_height_px};
pub(super) use projection::table_view_state_from_payload;
pub(super) use runtime::TableRuntime;

pub(super) fn ensure_table_payload_for_kind(
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

pub(super) fn table_has_cells(table: &cditor_core::rich_text::TablePayload) -> bool {
    table.rows.iter().any(|row| !row.cells.is_empty())
}

pub(super) fn default_table_payload(first_cell_text: String) -> BlockPayload {
    BlockPayload::Table(cditor_core::rich_text::TablePayload {
        rows: vec![
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain(first_cell_text),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                ],
                height: Default::default(),
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain(""),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                ],
                height: Default::default(),
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain(""),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                    cditor_core::rich_text::TableCellPayload::plain(""),
                ],
                height: Default::default(),
            },
        ],
        // Use Auto width for new tables - they will fill available width evenly
        // User can manually resize columns to Px if needed
        columns: (0..DEFAULT_TABLE_COLUMNS)
            .map(|_| cditor_core::rich_text::TableColumnPayload {
                width: cditor_core::rich_text::TableTrackSize::Auto,
            })
            .collect(),
        header_rows: 0,
        header_cols: 0,
        header_style: cditor_core::rich_text::TableHeaderStyle::default(),
    })
}

impl DocumentRuntime {
    pub(super) fn sync_table_runtime_for_payload(&mut self, record: &mut BlockPayloadRecord) {
        if !matches!(record.kind, RichBlockKind::Table) {
            self.table_runtimes.remove(&record.block_id);
            self.table_horizontal_scroll_offsets
                .remove(&record.block_id);
            return;
        }

        record.payload = ensure_table_payload_for_kind(&record.kind, record.payload.clone());
        let next = match &record.payload {
            BlockPayload::Table(table) if table_has_cells(table) => {
                TableRuntime::from_payload(record.payload.clone())
            }
            _ => self
                .table_runtimes
                .get(&record.block_id)
                .cloned()
                .unwrap_or_else(|| {
                    TableRuntime::from_payload(default_table_payload(String::new()))
                }),
        };
        record.payload = next.payload();
        self.table_runtimes.insert(record.block_id, next);
        self.text_models.remove(&record.block_id);
    }

    pub(super) fn sync_table_runtime_from_loaded_record(
        &mut self,
        record: &mut BlockPayloadRecord,
    ) {
        self.sync_table_runtime_for_payload(record);
        sync_text_model_for_payload(&mut self.text_models, record);
    }

    pub(super) fn table_runtime_payload_record(
        &self,
        block_id: BlockId,
        mut record: BlockPayloadRecord,
    ) -> BlockPayloadRecord {
        if matches!(record.kind, RichBlockKind::Table)
            && let Some(runtime) = self.table_runtimes.get(&block_id)
        {
            record.payload = runtime.payload();
        }
        record
    }

    pub(super) fn table_runtime(&self, block_id: BlockId) -> Option<&TableRuntime> {
        self.table_runtimes.get(&block_id)
    }

    pub(super) fn table_runtime_mut(&mut self, block_id: BlockId) -> Option<&mut TableRuntime> {
        self.table_runtimes.get_mut(&block_id)
    }
}
