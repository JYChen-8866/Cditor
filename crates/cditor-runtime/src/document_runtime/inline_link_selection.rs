use super::*;

impl DocumentRuntime {
    /// Sets, re-targets, or clears (`href: None`) the inline link on the
    /// focused selection. `replacement_text: Some(_)` additionally replaces
    /// the selected label with the given text carrying the link — the link
    /// popup edits label and destination together. Selection and input-session
    /// state are preserved across the transaction, mirroring
    /// `set_inline_color_on_selection`: the popup steals platform focus, so
    /// the restored selection is what the user returns to.
    pub(crate) fn set_inline_link_on_selection(
        &mut self,
        href: Option<&str>,
        replacement_text: Option<&str>,
    ) -> Result<bool, String> {
        if let Some(
            surface_id @ (cditor_core::ids::SurfaceId::ImageCaption { .. }
            | cditor_core::ids::SurfaceId::CollectionTitle { .. }),
        ) = self.focused_text_surface_id()
        {
            let Some(range) = self.input_session_selected_range() else {
                return Ok(false);
            };
            let snapshot = self
                .text_surface_base_snapshot(surface_id)
                .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
            let range = safe_char_range(&snapshot.plain_text(), range);
            if range.is_empty() {
                return Ok(false);
            }
            let spans = match replacement_text {
                Some(text) => replace_range_with_linked_text(&snapshot.spans, range, text, href),
                None => set_link_mark_for_range(&snapshot.spans, range, href),
            };
            return self.replace_auxiliary_text_surface_spans(surface_id, spans);
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(range) = self.focused_text_selection_range() else {
            return Ok(false);
        };
        let text = self
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let range = safe_char_range(&text, range);
        if range.is_empty() {
            return Ok(false);
        }
        let current_spans = self
            .document
            .payload_window
            .get(block_id)
            .and_then(|payload| match &payload.payload {
                BlockPayload::RichText { spans } => Some(spans.clone()),
                _ => None,
            })
            .ok_or_else(|| format!("block {block_id} does not support inline links"))?;
        let spans = match replacement_text {
            Some(text) => replace_range_with_linked_text(&current_spans, range.clone(), text, href),
            None => set_link_mark_for_range(&current_spans, range.clone(), href),
        };
        if spans == current_spans {
            return Ok(false);
        }
        let selection_reversed = self.input_session_selection_reversed();
        let restored_range = match replacement_text {
            Some(text) => range.start..range.start + text.len(),
            None => range.clone(),
        };
        self.apply_text_surface_format_transaction(
            SurfaceId::Block(block_id),
            current_spans,
            spans,
        )?;
        self.selection.focused_text_selection = Some(FocusedTextSelection {
            anchor: restored_range.start,
            focus: restored_range.end,
        });
        if let Some(editing) = self
            .editing
            .session
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_selected_range(restored_range, selection_reversed);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked_runtime(text: &str) -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                text,
            )],
            720.0,
        )
    }

    fn spans(runtime: &DocumentRuntime) -> Vec<InlineSpan> {
        match &runtime.document.payload_window.get(1).unwrap().payload {
            BlockPayload::RichText { spans } => spans.clone(),
            other => panic!("expected rich text payload, got {other:?}"),
        }
    }

    #[test]
    fn selection_gains_a_link_and_keeps_the_selection_for_the_editor() {
        let mut runtime = linked_runtime("visit example now");
        runtime.set_document_text_selection(1, 6, 1, 13).unwrap();

        assert!(
            runtime
                .set_inline_link_on_selection(Some("https://example.com"), None)
                .unwrap()
        );

        let spans = spans(&runtime);
        assert_eq!(spans[1].text, "example");
        assert_eq!(
            spans[1].marks,
            vec![InlineMark::Link {
                href: "https://example.com".to_owned()
            }]
        );
        assert_eq!(runtime.input_session_selected_range(), Some(6..13));
    }

    #[test]
    fn clearing_and_relabelling_round_trip() {
        let mut runtime = linked_runtime("visit example now");
        runtime.set_document_text_selection(1, 6, 1, 13).unwrap();
        runtime
            .set_inline_link_on_selection(Some("https://example.com"), None)
            .unwrap();

        // Re-target with a new label in one step.
        runtime.set_document_text_selection(1, 6, 1, 13).unwrap();
        assert!(
            runtime
                .set_inline_link_on_selection(Some("https://new.example"), Some("the docs"))
                .unwrap()
        );
        let updated = spans(&runtime);
        assert_eq!(updated[1].text, "the docs");
        assert_eq!(
            updated[1].marks,
            vec![InlineMark::Link {
                href: "https://new.example".to_owned()
            }]
        );
        assert_eq!(runtime.focused_text(), Some("visit the docs now"));
        assert_eq!(runtime.input_session_selected_range(), Some(6..14));

        // Clear the link, keep the label.
        runtime.set_document_text_selection(1, 6, 1, 14).unwrap();
        assert!(runtime.set_inline_link_on_selection(None, None).unwrap());
        assert!(spans(&runtime).iter().all(|span| {
            !span
                .marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Link { .. }))
        }));
    }
}
