use std::collections::HashMap;

use cditor_core::ids::BlockId;
use cditor_sdk::document::SearchDecoration;

use crate::text::TextSearchRange;

#[derive(Debug, Default)]
pub(crate) struct SearchDecorationState {
    by_block: HashMap<BlockId, Vec<SearchDecoration>>,
}

impl SearchDecorationState {
    pub(crate) fn replace(&mut self, decorations: Vec<SearchDecoration>) {
        self.by_block.clear();
        for decoration in decorations {
            self.by_block
                .entry(decoration.block_id)
                .or_default()
                .push(decoration);
        }
        for decorations in self.by_block.values_mut() {
            decorations.sort_by_key(|item| (item.byte_range.start, item.byte_range.end));
        }
    }

    pub(crate) fn ranges_for(
        &self,
        block_id: BlockId,
        content_version: u64,
        text_len: usize,
        is_char_boundary: impl Fn(usize) -> bool,
    ) -> Vec<TextSearchRange> {
        self.by_block
            .get(&block_id)
            .into_iter()
            .flatten()
            .filter(|item| {
                item.content_version == content_version
                    && item.byte_range.start < item.byte_range.end
                    && item.byte_range.end <= text_len
                    && is_char_boundary(item.byte_range.start)
                    && is_char_boundary(item.byte_range.end)
            })
            .map(|item| TextSearchRange {
                byte_range: item.byte_range.clone(),
                current: item.current,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_stale_and_invalid_utf8_ranges() {
        let mut state = SearchDecorationState::default();
        state.replace(vec![
            SearchDecoration {
                block_id: 7,
                content_version: 2,
                byte_range: 0..3,
                current: true,
            },
            SearchDecoration {
                block_id: 7,
                content_version: 1,
                byte_range: 0..1,
                current: false,
            },
            SearchDecoration {
                block_id: 7,
                content_version: 2,
                byte_range: 1..2,
                current: false,
            },
        ]);
        let text = "世a";
        let ranges = state.ranges_for(7, 2, text.len(), |offset| text.is_char_boundary(offset));
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].byte_range, 0..3);
        assert!(ranges[0].current);
    }
}
