use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;

use cditor_core::ids::SurfaceId;

use crate::text::RichTextPlatformLayout;

pub(in crate::app) const BLOCK_LAYOUT_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
pub(in crate::app) const TABLE_LAYOUT_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
pub(in crate::app) const AUX_LAYOUT_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct PlatformLayoutCache<K> {
    entries: HashMap<K, RichTextPlatformLayout>,
    last_insert: HashMap<K, u64>,
    estimated_bytes: usize,
    clock: u64,
    max_entries: usize,
    max_estimated_bytes: usize,
}

impl<K> PlatformLayoutCache<K>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn new(max_entries: usize, max_estimated_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            last_insert: HashMap::new(),
            estimated_bytes: 0,
            clock: 0,
            max_entries: max_entries.max(1),
            max_estimated_bytes: max_estimated_bytes.max(1),
        }
    }

    pub(crate) fn insert(
        &mut self,
        key: K,
        layout: RichTextPlatformLayout,
        pinned_surface: Option<SurfaceId>,
    ) {
        self.clock = self.clock.saturating_add(1);
        let bytes = estimated_platform_layout_bytes(&layout);
        if let Some(previous) = self.entries.insert(key.clone(), layout) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(estimated_platform_layout_bytes(&previous));
        }
        self.estimated_bytes = self.estimated_bytes.saturating_add(bytes);
        self.last_insert.insert(key, self.clock);
        self.trim(pinned_surface);
    }

    pub(in crate::app) fn remove(&mut self, key: &K) -> Option<RichTextPlatformLayout> {
        self.last_insert.remove(key);
        let removed = self.entries.remove(key)?;
        self.estimated_bytes = self
            .estimated_bytes
            .saturating_sub(estimated_platform_layout_bytes(&removed));
        Some(removed)
    }

    pub(in crate::app) fn retain(
        &mut self,
        mut keep: impl FnMut(&K, &mut RichTextPlatformLayout) -> bool,
    ) {
        let removed = self
            .entries
            .iter_mut()
            .filter_map(|(key, layout)| (!keep(key, layout)).then_some(key.clone()))
            .collect::<Vec<_>>();
        for key in removed {
            self.remove(&key);
        }
    }

    pub(in crate::app) fn clear(&mut self) {
        self.entries.clear();
        self.last_insert.clear();
        self.estimated_bytes = 0;
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }

    pub(crate) fn is_over_budget(&self) -> bool {
        self.entries.len() > self.max_entries || self.estimated_bytes > self.max_estimated_bytes
    }

    fn trim(&mut self, pinned_surface: Option<SurfaceId>) {
        while self.entries.len() > self.max_entries
            || self.estimated_bytes > self.max_estimated_bytes
        {
            let candidate = self
                .entries
                .iter()
                .filter(|(_, layout)| Some(layout.surface_id) != pinned_surface)
                .filter(|(key, _)| {
                    self.entries.len() > 1
                        && self.last_insert.get(*key).copied().unwrap_or(0) != self.clock
                })
                .min_by_key(|(key, _)| self.last_insert.get(*key).copied().unwrap_or(0))
                .map(|(key, _)| key.clone());
            let Some(candidate) = candidate else {
                break;
            };
            self.remove(&candidate);
        }
    }
}

impl<K> Deref for PlatformLayoutCache<K> {
    type Target = HashMap<K, RichTextPlatformLayout>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

pub(in crate::app) fn block_layout_cache<K>() -> PlatformLayoutCache<K>
where
    K: Clone + Eq + Hash,
{
    PlatformLayoutCache::new(1_024, BLOCK_LAYOUT_CACHE_MAX_BYTES)
}

pub(in crate::app) fn table_layout_cache<K>() -> PlatformLayoutCache<K>
where
    K: Clone + Eq + Hash,
{
    PlatformLayoutCache::new(4_096, TABLE_LAYOUT_CACHE_MAX_BYTES)
}

pub(in crate::app) fn auxiliary_layout_cache<K>() -> PlatformLayoutCache<K>
where
    K: Clone + Eq + Hash,
{
    PlatformLayoutCache::new(256, AUX_LAYOUT_CACHE_MAX_BYTES)
}

fn estimated_platform_layout_bytes(layout: &RichTextPlatformLayout) -> usize {
    std::mem::size_of::<RichTextPlatformLayout>().saturating_add(layout.snapshot.estimated_bytes())
}
