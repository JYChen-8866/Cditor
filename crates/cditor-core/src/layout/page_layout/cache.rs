use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachedPageMismatchPolicy {
    Reject,
    ReuseHistorical,
    Rebuild,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CachedPageRestore {
    Exact(PageLayoutIndex),
    Historical(PageLayoutIndex),
    RebuildRequired,
}

impl CachedPageRestore {
    pub fn into_index(self) -> Option<PageLayoutIndex> {
        match self {
            Self::Exact(index) | Self::Historical(index) => Some(index),
            Self::RebuildRequired => None,
        }
    }
}

impl PageLayoutIndex {
    pub fn restore_cached_pages(
        pages: Vec<PageLayout>,
        policy: PagePolicy,
        total_visible_blocks: usize,
        cached_identity: PageLayoutIdentity,
        expected_identity: PageLayoutIdentity,
        mismatch_policy: CachedPageMismatchPolicy,
    ) -> Result<CachedPageRestore, PageLayoutIndexError> {
        let mut index = Self::from_cached_pages(pages, policy, total_visible_blocks)?;
        if cached_identity.same_layout_context(&expected_identity) {
            index.identity = Some(expected_identity.for_page_index(0));
            return Ok(CachedPageRestore::Exact(index));
        }

        match mismatch_policy {
            CachedPageMismatchPolicy::Reject => Err(PageLayoutIndexError::PageIdentityMismatch {
                cached: cached_identity,
                expected: expected_identity,
            }),
            CachedPageMismatchPolicy::ReuseHistorical => {
                for page in &mut index.pages {
                    page.confidence = HeightConfidence::Historical;
                    page.measured_ratio = 0.0;
                    page.max_error_hint = page.max_error_hint.max(32.0);
                    page.dirty = true;
                }
                index.identity = Some(expected_identity.for_page_index(0));
                Ok(CachedPageRestore::Historical(index))
            }
            CachedPageMismatchPolicy::Rebuild => Ok(CachedPageRestore::RebuildRequired),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(structure_version: u64) -> PageLayoutIdentity {
        PageLayoutIdentity::for_page(1, structure_version, 3, 4, PAGE_POLICY_VERSION, 0)
    }

    fn cached_page() -> PageLayout {
        PageLayout {
            page_index: 0,
            block_start: 0,
            block_count: 2,
            height: 48.0,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
        }
    }

    #[test]
    fn exact_cache_restore_preserves_confidence_and_derives_page_identity() {
        let restore = PageLayoutIndex::restore_cached_pages(
            vec![cached_page()],
            PagePolicy::default(),
            2,
            identity(7),
            identity(7),
            CachedPageMismatchPolicy::Reject,
        )
        .unwrap();
        let CachedPageRestore::Exact(index) = restore else {
            panic!("matching identity must restore exactly");
        };
        assert_eq!(index.pages[0].confidence, HeightConfidence::Exact);
        assert_eq!(index.page_identity(0), Some(identity(7)));
    }

    #[test]
    fn stale_cache_can_be_rejected_downgraded_or_rebuilt() {
        let reject = PageLayoutIndex::restore_cached_pages(
            vec![cached_page()],
            PagePolicy::default(),
            2,
            identity(6),
            identity(7),
            CachedPageMismatchPolicy::Reject,
        );
        assert!(matches!(
            reject,
            Err(PageLayoutIndexError::PageIdentityMismatch { .. })
        ));

        let historical = PageLayoutIndex::restore_cached_pages(
            vec![cached_page()],
            PagePolicy::default(),
            2,
            identity(6),
            identity(7),
            CachedPageMismatchPolicy::ReuseHistorical,
        )
        .unwrap()
        .into_index()
        .unwrap();
        assert_eq!(historical.pages[0].confidence, HeightConfidence::Historical);
        assert_eq!(historical.pages[0].measured_ratio, 0.0);
        assert!(historical.pages[0].dirty);
        assert!(historical.pages[0].max_error_hint >= 32.0);

        assert_eq!(
            PageLayoutIndex::restore_cached_pages(
                vec![cached_page()],
                PagePolicy::default(),
                2,
                identity(6),
                identity(7),
                CachedPageMismatchPolicy::Rebuild,
            )
            .unwrap(),
            CachedPageRestore::RebuildRequired
        );
    }
}
