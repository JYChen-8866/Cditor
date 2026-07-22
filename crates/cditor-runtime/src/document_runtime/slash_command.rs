use super::structure_payload::payload_for_converted_kind;
use super::*;

impl DocumentRuntime {
    pub fn can_apply_slash_block_kind(
        &self,
        block_id: BlockId,
        trigger_range: Range<usize>,
        kind: &RichBlockKind,
    ) -> bool {
        if self.focused_block_id() != Some(block_id) || trigger_range.is_empty() {
            return false;
        }
        let Some(record) = self.payload_window.get(block_id) else {
            return false;
        };
        let BlockPayload::RichText { .. } = &record.payload else {
            return false;
        };
        let text = record.plain_text();
        if trigger_range.start > trigger_range.end
            || trigger_range.end > text.len()
            || !text.is_char_boundary(trigger_range.start)
            || !text.is_char_boundary(trigger_range.end)
        {
            return false;
        }
        record.kind == *kind || self.can_convert_block_kind(block_id, kind)
    }

    pub(crate) fn apply_slash_block_kind(
        &mut self,
        block_id: BlockId,
        trigger_range: Range<usize>,
        kind: RichBlockKind,
    ) -> Result<bool, String> {
        if !self.can_apply_slash_block_kind(block_id, trigger_range.clone(), &kind) {
            return Err("slash command precondition failed".to_owned());
        }
        let before = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let current_spans = match &before.payload {
            BlockPayload::RichText { spans } => spans,
            _ => return Err("slash command requires a rich-text block".to_owned()),
        };
        let remaining_spans = super::text_payload::replace_rich_text_spans_preserving_marks(
            current_spans,
            trigger_range,
            "",
        );
        let remaining_text = plain_text_from_spans(&remaining_spans);
        let mut next_payload = payload_for_converted_kind(&kind, remaining_text);
        if matches!(next_payload, BlockPayload::RichText { .. }) {
            next_payload = BlockPayload::RichText {
                spans: remaining_spans,
            };
        }

        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::ExplicitCommand,
            kind,
            next_payload,
        )
    }
}
