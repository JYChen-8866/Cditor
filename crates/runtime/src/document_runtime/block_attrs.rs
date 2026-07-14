use super::*;

impl DocumentRuntime {
    pub fn block_attrs(&self, block_id: BlockId) -> BlockAttrs {
        let mut attrs = self.block_attrs.get(&block_id).cloned().unwrap_or_default();
        attrs.folded = self.visible_index.is_folded(block_id);
        attrs
    }

    pub fn set_block_color(
        &mut self,
        block_id: BlockId,
        target: InlineColorTarget,
        value: Option<&str>,
    ) -> Result<bool, String> {
        let before = self.block_attrs(block_id);
        trace_block_color(
            "set.begin",
            format_args!(
                "block_id={block_id} target={target:?} value={value:?} index_present={} payload_loaded={} before={before:?}",
                self.index.index_of(block_id).is_some(),
                self.payload_window.get(block_id).is_some(),
            ),
        );
        if self.index.index_of(block_id).is_none() {
            trace_block_color("set.error", format_args!("missing block_id={block_id}"));
            return Err(format!("missing block {block_id}"));
        }

        // Older gutter coloring was persisted as an inline mark spanning the
        // entire block. Inline marks correctly override a block's base color,
        // so that legacy representation would make the new block attribute
        // appear ineffective. Migrate only a uniform, full-block mark of the
        // same family; partial inline styling remains an intentional override.
        let legacy_inline_range =
            self.payload_window
                .get(block_id)
                .and_then(|record| match &record.payload {
                    BlockPayload::RichText { spans }
                        if has_uniform_full_block_color_mark(spans, target) =>
                    {
                        Some(0..spans.iter().map(|span| span.text.len()).sum())
                    }
                    _ => None,
                });
        let migrated_legacy_inline = if let Some(range) = legacy_inline_range {
            trace_block_color(
                "set.migrate_legacy_inline",
                format_args!("block_id={block_id} target={target:?} range={range:?}"),
            );
            self.set_inline_color_for_range(block_id, range, target, None)?
        } else {
            false
        };

        let attrs = self.block_attrs.entry(block_id).or_default();
        let slot = match target {
            InlineColorTarget::Text => &mut attrs.color,
            InlineColorTarget::Background => &mut attrs.background_color,
        };
        let next = value.map(str::to_owned);
        if *slot == next {
            trace_block_color(
                "set.noop",
                format_args!(
                    "block_id={block_id} migrated_legacy_inline={migrated_legacy_inline} attrs={:?}",
                    self.block_attrs(block_id),
                ),
            );
            return Ok(migrated_legacy_inline);
        }
        *slot = next;
        if *attrs == BlockAttrs::default() {
            self.block_attrs.remove(&block_id);
        }
        trace_block_color(
            "set.commit",
            format_args!(
                "block_id={block_id} migrated_legacy_inline={migrated_legacy_inline} after={:?}",
                self.block_attrs(block_id),
            ),
        );
        Ok(true)
    }

    pub fn block_attrs_snapshot(&self) -> Vec<(BlockId, BlockAttrs)> {
        self.block_attrs
            .iter()
            .filter(|(block_id, _)| self.index.index_of(**block_id).is_some())
            .map(|(block_id, attrs)| (*block_id, attrs.clone()))
            .collect()
    }
}

fn has_uniform_full_block_color_mark(spans: &[InlineSpan], target: InlineColorTarget) -> bool {
    let mut uniform_value: Option<&str> = None;
    let mut saw_text = false;

    for span in spans.iter().filter(|span| !span.text.is_empty()) {
        saw_text = true;
        let mut values = span.marks.iter().filter_map(|mark| match (target, mark) {
            (InlineColorTarget::Text, InlineMark::Color(value))
            | (InlineColorTarget::Background, InlineMark::Background(value)) => {
                Some(value.as_str())
            }
            _ => None,
        });
        let Some(value) = values.next() else {
            return false;
        };
        if values.next().is_some() {
            return false;
        }
        match uniform_value {
            None => uniform_value = Some(value),
            Some(uniform) if uniform == value => {}
            Some(_) => return false,
        }
    }

    saw_text && uniform_value.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_colors_are_independent_from_inline_marks_and_projected() {
        let mut runtime = DocumentRuntime::demo();
        assert!(
            runtime
                .set_block_color(2, InlineColorTarget::Text, Some("#d44c47"))
                .unwrap()
        );
        assert!(
            runtime
                .set_block_color(2, InlineColorTarget::Background, Some("#fdebec"))
                .unwrap()
        );

        let block = runtime
            .projection()
            .blocks
            .into_iter()
            .find(|block| block.block_id == 2)
            .unwrap();
        assert_eq!(block.attrs.color.as_deref(), Some("#d44c47"));
        assert_eq!(block.attrs.background_color.as_deref(), Some("#fdebec"));
        assert!(matches!(
            runtime.block_payload_record(2).unwrap().payload,
            BlockPayload::RichText { ref spans } if spans.iter().all(|span| span.marks.is_empty())
        ));
    }

    #[test]
    fn clearing_both_colors_removes_sparse_attrs_entry() {
        let mut runtime = DocumentRuntime::demo();
        runtime
            .set_block_color(2, InlineColorTarget::Text, Some("#d44c47"))
            .unwrap();
        runtime
            .set_block_color(2, InlineColorTarget::Text, None)
            .unwrap();
        assert!(runtime.block_attrs_snapshot().is_empty());
    }

    #[test]
    fn block_color_migrates_only_uniform_full_block_legacy_inline_color() {
        let mut runtime = DocumentRuntime::demo();
        let text_len = runtime.block_payload_record(2).unwrap().plain_text().len();
        runtime
            .set_inline_color_for_range(2, 0..text_len, InlineColorTarget::Text, Some("#337ea9"))
            .unwrap();

        assert!(
            runtime
                .set_block_color(2, InlineColorTarget::Text, Some("#d44c47"))
                .unwrap()
        );

        let record = runtime.block_payload_record(2).unwrap();
        let BlockPayload::RichText { spans } = record.payload else {
            panic!("demo block 2 should be rich text");
        };
        assert!(spans.iter().all(|span| {
            span.marks
                .iter()
                .all(|mark| !InlineColorTarget::Text.matches(mark))
        }));
        assert_eq!(runtime.block_attrs(2).color.as_deref(), Some("#d44c47"));
    }

    #[test]
    fn block_color_preserves_partial_inline_color_override() {
        let mut runtime = DocumentRuntime::demo();
        runtime
            .set_inline_color_for_range(2, 0..2, InlineColorTarget::Text, Some("#337ea9"))
            .unwrap();

        runtime
            .set_block_color(2, InlineColorTarget::Text, Some("#d44c47"))
            .unwrap();

        let record = runtime.block_payload_record(2).unwrap();
        let BlockPayload::RichText { spans } = record.payload else {
            panic!("demo block 2 should be rich text");
        };
        assert!(spans.iter().any(|span| {
            span.marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Color(value) if value == "#337ea9"))
        }));
        assert_eq!(runtime.block_attrs(2).color.as_deref(), Some("#d44c47"));
    }
}
