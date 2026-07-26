use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;

use cditor_core::ids::SurfaceId;

use crate::text::RichTextPlatformLayout;

pub(crate) struct PlatformGeometryRegistry<K> {
    entries: HashMap<K, RichTextPlatformLayout>,
    last_insert: HashMap<K, u64>,
    clock: u64,
    max_entries: usize,
}

impl<K> PlatformGeometryRegistry<K>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            last_insert: HashMap::new(),
            clock: 0,
            max_entries: max_entries.max(1),
        }
    }

    pub(crate) fn insert(
        &mut self,
        key: K,
        layout: RichTextPlatformLayout,
        pinned_surface: Option<SurfaceId>,
    ) {
        self.clock = self.clock.saturating_add(1);
        self.entries.insert(key.clone(), layout);
        self.last_insert.insert(key, self.clock);
        self.trim(pinned_surface);
    }

    pub(crate) fn remove(&mut self, key: &K) -> Option<RichTextPlatformLayout> {
        self.last_insert.remove(key);
        self.entries.remove(key)
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K, &mut RichTextPlatformLayout) -> bool) {
        let removed = self
            .entries
            .iter_mut()
            .filter_map(|(key, layout)| (!keep(key, layout)).then_some(key.clone()))
            .collect::<Vec<_>>();
        for key in removed {
            self.remove(&key);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.last_insert.clear();
    }

    pub(crate) fn estimated_metadata_bytes(&self) -> usize {
        self.entries
            .len()
            .saturating_mul(std::mem::size_of::<RichTextPlatformLayout>())
    }

    pub(crate) fn is_over_budget(&self) -> bool {
        self.entries.len() > self.max_entries
    }

    fn trim(&mut self, pinned_surface: Option<SurfaceId>) {
        while self.entries.len() > self.max_entries {
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

impl<K> Deref for PlatformGeometryRegistry<K> {
    type Target = HashMap<K, RichTextPlatformLayout>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

pub(crate) fn block_geometry_registry<K>() -> PlatformGeometryRegistry<K>
where
    K: Clone + Eq + Hash,
{
    PlatformGeometryRegistry::new(1_024)
}

pub(crate) fn table_geometry_registry<K>() -> PlatformGeometryRegistry<K>
where
    K: Clone + Eq + Hash,
{
    PlatformGeometryRegistry::new(4_096)
}

pub(crate) fn auxiliary_geometry_registry<K>() -> PlatformGeometryRegistry<K>
where
    K: Clone + Eq + Hash,
{
    PlatformGeometryRegistry::new(256)
}
