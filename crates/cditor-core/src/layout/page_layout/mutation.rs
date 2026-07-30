use super::*;

impl PageLayoutIndex {
    pub fn split_page(
        &mut self,
        page: usize,
        split_at: usize,
        global: &BlockHeightIndex,
    ) -> Result<(), PageLayoutIndexError> {
        let Some(current) = self.pages.get(page).copied() else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };
        if split_at == 0 || split_at >= current.block_count {
            return Err(PageLayoutIndexError::InvalidPageSplit {
                page,
                split_at,
                block_count: current.block_count,
            });
        }

        let middle = current.block_start + split_at;
        let left = summarize_global_range(page, current.block_start..middle, global)?;
        let right = summarize_global_range(page + 1, middle..current.block_end(), global)?;
        self.pages.splice(page..page + 1, [left, right]);
        self.reindex_pages();
        Ok(())
    }

    pub fn merge_page_with_next(
        &mut self,
        page: usize,
        global: &BlockHeightIndex,
    ) -> Result<(), PageLayoutIndexError> {
        let Some(left) = self.pages.get(page).copied() else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page,
                len: self.pages.len(),
            });
        };
        let Some(right) = self.pages.get(page + 1).copied() else {
            return Err(PageLayoutIndexError::PageOutOfBounds {
                page: page + 1,
                len: self.pages.len(),
            });
        };
        let merged = summarize_global_range(page, left.block_start..right.block_end(), global)?;
        self.pages.splice(page..page + 2, [merged]);
        self.reindex_pages();
        Ok(())
    }

    fn reindex_pages(&mut self) {
        let mut block_start = 0;
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            page.page_index = page_index;
            page.block_start = block_start;
            block_start += page.block_count;
        }
        self.height_index = PageHeightFenwick::from_pages(&self.pages);
    }
}

fn summarize_global_range(
    page_index: usize,
    range: Range<usize>,
    global: &BlockHeightIndex,
) -> Result<PageLayout, PageLayoutIndexError> {
    if range.start > range.end || range.end > global.len() {
        return Err(PageLayoutIndexError::CoverageTailMismatch {
            covered: global.len(),
            total: range.end,
        });
    }
    let block_count = range.len();
    let height = global.heights[range.clone()].iter().sum();
    let measured = global.confidence[range.clone()]
        .iter()
        .filter(|confidence| **confidence == HeightConfidence::Exact)
        .count();
    let confidence = global.confidence[range]
        .iter()
        .copied()
        .fold(HeightConfidence::Exact, aggregate_confidence);
    Ok(build_page(
        page_index,
        0,
        block_count,
        height,
        measured,
        confidence,
        0.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::HeightEstimate;

    #[test]
    fn split_and_merge_reindex_suffix_pages_and_preserve_total_height() {
        let global =
            BlockHeightIndex::new((1..=8).map(|height| {
                HeightEstimate::new(height as f64 * 10.0, HeightConfidence::Exact, 0.0)
            }))
            .unwrap();
        let mut pages = PageLayoutIndex::from_block_height_index(
            &global,
            PagePolicy {
                max_blocks: 4,
                target_height: 10_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();

        pages.split_page(0, 2, &global).unwrap();
        assert_eq!(pages.page_count(), 3);
        assert_eq!(pages.pages[0].block_start, 0);
        assert_eq!(pages.pages[1].block_start, 2);
        assert_eq!(pages.pages[2].block_start, 4);
        assert_eq!(pages.total_height(), global.total_height());
        pages.validate_covers_blocks(global.len()).unwrap();

        pages.merge_page_with_next(0, &global).unwrap();
        assert_eq!(pages.page_count(), 2);
        assert_eq!(pages.pages[1].block_start, 4);
        assert_eq!(pages.total_height(), global.total_height());
        pages.validate_covers_blocks(global.len()).unwrap();
    }

    #[test]
    fn split_rejects_empty_child_pages() {
        let global = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let mut pages =
            PageLayoutIndex::from_block_height_index(&global, PagePolicy::default()).unwrap();
        assert!(matches!(
            pages.split_page(0, 0, &global),
            Err(PageLayoutIndexError::InvalidPageSplit { .. })
        ));
    }
}
