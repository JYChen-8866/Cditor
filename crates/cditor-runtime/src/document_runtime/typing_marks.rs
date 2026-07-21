//! Explicit typing attributes at a collapsed caret. They are required when an
//! auto-format closes a marked span: subsequent input must not accidentally
//! inherit the mark from the span immediately to its left.

use super::*;

impl DocumentRuntime {
    pub(super) fn typing_marks_for(
        &self,
        surface_id: SurfaceId,
        offset: usize,
    ) -> Option<Vec<InlineMark>> {
        self.typing_mark_override
            .as_ref()
            .filter(|override_| override_.surface_id == surface_id && override_.offset == offset)
            .map(|override_| override_.marks.clone())
    }

    pub(super) fn set_typing_mark_override(
        &mut self,
        surface_id: SurfaceId,
        offset: usize,
        marks: Vec<InlineMark>,
    ) {
        self.typing_mark_override = Some(TypingMarkOverride {
            surface_id,
            offset,
            marks,
        });
    }

    pub(super) fn advance_typing_mark_override(
        &mut self,
        surface_id: SurfaceId,
        previous_offset: usize,
        next_offset: usize,
    ) {
        if let Some(override_) = self.typing_mark_override.as_mut()
            && override_.surface_id == surface_id
            && override_.offset == previous_offset
        {
            override_.offset = next_offset;
        }
    }
}

pub(super) fn sync_payload_after_replace_with_typing_marks(
    payload_window: &mut PayloadWindow,
    block_id: BlockId,
    content_version: u64,
    model: &PieceTableTextModel,
    replaced_range: Range<usize>,
    inserted_text: &str,
    typing_marks: Option<Vec<InlineMark>>,
) {
    let Some(marks) = typing_marks.filter(|_| !inserted_text.is_empty()) else {
        sync_payload_from_model_after_replace(
            payload_window,
            block_id,
            content_version,
            model,
            replaced_range,
            inserted_text,
        );
        return;
    };
    let Some(record) = payload_window.payloads.get(&block_id) else {
        return;
    };
    if !matches!(record.payload, BlockPayload::RichText { .. }) {
        sync_payload_from_model_after_replace(
            payload_window,
            block_id,
            content_version,
            model,
            replaced_range,
            inserted_text,
        );
        return;
    }
    let record = payload_window
        .payloads
        .get_mut(&block_id)
        .expect("payload existence checked above");
    let BlockPayload::RichText { spans } = &record.payload else {
        unreachable!("payload kind checked above");
    };
    let next_spans = replace_rich_text_spans_with_spans(
        spans,
        replaced_range,
        &[InlineSpan {
            text: inserted_text.to_owned(),
            marks,
        }],
    );
    record.content_version = content_version;
    record.payload = BlockPayload::RichText { spans: next_spans };
}
