use super::*;

pub(super) use crate::content::payload_preparation::{
    default_table_payload, ensure_table_payload_for_kind, table_has_cells,
};

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

impl DocumentRuntime {
    pub(super) fn sync_table_runtime_for_payload(&mut self, record: &mut BlockPayloadRecord) {
        if !matches!(record.kind, RichBlockKind::Table) {
            self.document.table_runtimes.remove(&record.block_id);
            self.layout
                .table_horizontal_scroll_offsets
                .remove(&record.block_id);
            return;
        }

        record.payload = ensure_table_payload_for_kind(&record.kind, record.payload.clone());
        let next = match &record.payload {
            BlockPayload::Table(table) if table_has_cells(table) => {
                TableRuntime::from_payload(record.payload.clone())
            }
            _ => self
                .document
                .table_runtimes
                .get(&record.block_id)
                .cloned()
                .unwrap_or_else(|| {
                    TableRuntime::from_payload(default_table_payload(String::new()))
                }),
        };
        record.payload = next.payload();
        self.document.table_runtimes.insert(record.block_id, next);
        self.document.text_models.remove(&record.block_id);
    }

    pub(super) fn sync_table_runtime_from_loaded_record(
        &mut self,
        record: &mut BlockPayloadRecord,
    ) {
        self.sync_table_runtime_for_payload(record);
        sync_text_model_for_payload(&mut self.document.text_models, record);
    }

    pub(super) fn table_runtime_payload_record(
        &self,
        block_id: BlockId,
        mut record: BlockPayloadRecord,
    ) -> BlockPayloadRecord {
        if matches!(record.kind, RichBlockKind::Table)
            && let Some(runtime) = self.document.table_runtimes.get(&block_id)
        {
            record.payload = runtime.payload();
        }
        record
    }

    pub(super) fn table_runtime(&self, block_id: BlockId) -> Option<&TableRuntime> {
        self.document.table_runtimes.get(&block_id)
    }

    pub(super) fn table_runtime_mut(&mut self, block_id: BlockId) -> Option<&mut TableRuntime> {
        self.document.table_runtimes.get_mut(&block_id)
    }
}
