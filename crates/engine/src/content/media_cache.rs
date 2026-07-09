use std::collections::{HashMap, VecDeque};

use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaCachePolicy {
    pub max_decoded_bytes: usize,
    pub max_thumbnail_bytes: usize,
    pub prefer_viewport_distance: f64,
}

impl Default for MediaCachePolicy {
    fn default() -> Self {
        Self {
            max_decoded_bytes: 128 * 1024 * 1024,
            max_thumbnail_bytes: 32 * 1024 * 1024,
            prefer_viewport_distance: 2_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MediaResourceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDecodeKind {
    Thumbnail,
    Original,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDecodeTrigger {
    ViewportNear,
    UserExplicitView,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDecodeLane {
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDecodeRequest {
    pub resource_id: MediaResourceId,
    pub block_id: BlockId,
    pub kind: MediaDecodeKind,
    pub lane: MediaDecodeLane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaDecodeDecision {
    Scheduled(MediaDecodeRequest),
    MetadataMissing,
    TooFarFromViewport,
    OriginalRequiresExplicitView,
    AlreadyCached,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaMetadata {
    pub width: u32,
    pub height: u32,
    pub encoded_bytes: usize,
    pub mime: String,
}

impl MediaMetadata {
    pub fn new(width: u32, height: u32, encoded_bytes: usize, mime: impl Into<String>) -> Self {
        Self {
            width,
            height,
            encoded_bytes,
            mime: mime.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaStableBox {
    pub estimated_height: f64,
    pub min_height: f64,
    pub max_height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaCacheEntry {
    pub resource_id: MediaResourceId,
    pub block_id: BlockId,
    pub metadata: Option<MediaMetadata>,
    pub stable_box: MediaStableBox,
    pub viewport_distance: f64,
    pub thumbnail_bytes: usize,
    pub decoded_bytes: usize,
    pub entity_pin_count: usize,
    pub resource_pin_count: usize,
    last_access: u64,
}

impl MediaCacheEntry {
    pub fn has_payload_and_layout_box(&self) -> bool {
        self.metadata.is_some() && self.stable_box.estimated_height > 0.0
    }

    pub fn has_thumbnail(&self) -> bool {
        self.thumbnail_bytes > 0
    }

    pub fn has_decoded_original(&self) -> bool {
        self.decoded_bytes > 0
    }

    fn resource_pinned(&self) -> bool {
        self.resource_pin_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCacheStats {
    pub entries: usize,
    pub decoded_bytes: usize,
    pub thumbnail_bytes: usize,
    pub scheduled_decode_requests: usize,
    pub evicted_decoded_resources: usize,
    pub evicted_thumbnails: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaCache {
    policy: MediaCachePolicy,
    entries: HashMap<MediaResourceId, MediaCacheEntry>,
    decoded_lru: VecDeque<MediaResourceId>,
    thumbnail_lru: VecDeque<MediaResourceId>,
    clock: u64,
    scheduled_decode_requests: usize,
    evicted_decoded_resources: usize,
    evicted_thumbnails: usize,
}

impl MediaCache {
    pub fn new(policy: MediaCachePolicy) -> Self {
        Self {
            policy,
            entries: HashMap::new(),
            decoded_lru: VecDeque::new(),
            thumbnail_lru: VecDeque::new(),
            clock: 0,
            scheduled_decode_requests: 0,
            evicted_decoded_resources: 0,
            evicted_thumbnails: 0,
        }
    }

    pub fn policy(&self) -> MediaCachePolicy {
        self.policy
    }

    pub fn upsert_metadata(
        &mut self,
        resource_id: MediaResourceId,
        block_id: BlockId,
        metadata: MediaMetadata,
        stable_box: MediaStableBox,
    ) {
        self.clock = self.clock.saturating_add(1);
        let entry = self.entries.entry(resource_id).or_insert(MediaCacheEntry {
            resource_id,
            block_id,
            metadata: None,
            stable_box,
            viewport_distance: f64::INFINITY,
            thumbnail_bytes: 0,
            decoded_bytes: 0,
            entity_pin_count: 0,
            resource_pin_count: 0,
            last_access: self.clock,
        });
        entry.block_id = block_id;
        entry.metadata = Some(metadata);
        entry.stable_box = stable_box;
        entry.last_access = self.clock;
    }

    pub fn entry(&self, resource_id: MediaResourceId) -> Option<&MediaCacheEntry> {
        self.entries.get(&resource_id)
    }

    pub fn set_viewport_distance(&mut self, resource_id: MediaResourceId, distance: f64) {
        if let Some(entry) = self.entries.get_mut(&resource_id) {
            entry.viewport_distance = distance.abs();
        }
    }

    pub fn request_thumbnail_if_near(
        &mut self,
        resource_id: MediaResourceId,
    ) -> MediaDecodeDecision {
        let Some(entry) = self.entries.get(&resource_id) else {
            return MediaDecodeDecision::MetadataMissing;
        };
        if entry.metadata.is_none() {
            return MediaDecodeDecision::MetadataMissing;
        }
        if entry.thumbnail_bytes > 0 {
            return MediaDecodeDecision::AlreadyCached;
        }
        if entry.viewport_distance > self.policy.prefer_viewport_distance {
            return MediaDecodeDecision::TooFarFromViewport;
        }

        self.scheduled_decode_requests = self.scheduled_decode_requests.saturating_add(1);
        MediaDecodeDecision::Scheduled(MediaDecodeRequest {
            resource_id,
            block_id: entry.block_id,
            kind: MediaDecodeKind::Thumbnail,
            lane: MediaDecodeLane::Background,
        })
    }

    pub fn request_original(
        &mut self,
        resource_id: MediaResourceId,
        trigger: MediaDecodeTrigger,
    ) -> MediaDecodeDecision {
        let Some(entry) = self.entries.get(&resource_id) else {
            return MediaDecodeDecision::MetadataMissing;
        };
        if entry.metadata.is_none() {
            return MediaDecodeDecision::MetadataMissing;
        }
        if entry.decoded_bytes > 0 {
            return MediaDecodeDecision::AlreadyCached;
        }
        if trigger != MediaDecodeTrigger::UserExplicitView {
            return MediaDecodeDecision::OriginalRequiresExplicitView;
        }

        self.scheduled_decode_requests = self.scheduled_decode_requests.saturating_add(1);
        MediaDecodeDecision::Scheduled(MediaDecodeRequest {
            resource_id,
            block_id: entry.block_id,
            kind: MediaDecodeKind::Original,
            lane: MediaDecodeLane::Background,
        })
    }

    pub fn apply_decode_result(
        &mut self,
        resource_id: MediaResourceId,
        kind: MediaDecodeKind,
        bytes: usize,
    ) -> bool {
        self.clock = self.clock.saturating_add(1);
        let Some(entry) = self.entries.get_mut(&resource_id) else {
            return false;
        };
        entry.last_access = self.clock;
        match kind {
            MediaDecodeKind::Thumbnail => {
                entry.thumbnail_bytes = bytes;
                touch_lru(&mut self.thumbnail_lru, resource_id);
                self.enforce_thumbnail_limit();
            }
            MediaDecodeKind::Original => {
                entry.decoded_bytes = bytes;
                touch_lru(&mut self.decoded_lru, resource_id);
                self.enforce_decoded_limit();
            }
        }
        true
    }

    pub fn pin_entity(&mut self, resource_id: MediaResourceId) {
        if let Some(entry) = self.entries.get_mut(&resource_id) {
            entry.entity_pin_count = entry.entity_pin_count.saturating_add(1);
        }
    }

    pub fn pin_resource(&mut self, resource_id: MediaResourceId) {
        if let Some(entry) = self.entries.get_mut(&resource_id) {
            entry.resource_pin_count = entry.resource_pin_count.saturating_add(1);
        }
    }

    pub fn unpin_resource(&mut self, resource_id: MediaResourceId) {
        if let Some(entry) = self.entries.get_mut(&resource_id) {
            entry.resource_pin_count = entry.resource_pin_count.saturating_sub(1);
        }
    }

    pub fn apply_memory_pressure(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Normal => {}
            MemoryPressure::Warning => {
                self.evict_all_unpinned_decoded();
                self.evict_far_thumbnails();
            }
            MemoryPressure::Critical => {
                self.evict_all_unpinned_decoded();
                self.evict_all_unpinned_thumbnails();
            }
        }
    }

    pub fn stats(&self) -> MediaCacheStats {
        MediaCacheStats {
            entries: self.entries.len(),
            decoded_bytes: self.decoded_bytes(),
            thumbnail_bytes: self.thumbnail_bytes(),
            scheduled_decode_requests: self.scheduled_decode_requests,
            evicted_decoded_resources: self.evicted_decoded_resources,
            evicted_thumbnails: self.evicted_thumbnails,
        }
    }

    fn decoded_bytes(&self) -> usize {
        self.entries.values().map(|entry| entry.decoded_bytes).sum()
    }

    fn thumbnail_bytes(&self) -> usize {
        self.entries
            .values()
            .map(|entry| entry.thumbnail_bytes)
            .sum()
    }

    fn enforce_decoded_limit(&mut self) {
        while self.decoded_bytes() > self.policy.max_decoded_bytes {
            if !self.evict_one_decoded_lru() {
                break;
            }
        }
    }

    fn enforce_thumbnail_limit(&mut self) {
        while self.thumbnail_bytes() > self.policy.max_thumbnail_bytes {
            if !self.evict_one_thumbnail_lru() {
                break;
            }
        }
    }

    fn evict_one_decoded_lru(&mut self) -> bool {
        let len = self.decoded_lru.len();
        for _ in 0..len {
            let Some(resource_id) = self.decoded_lru.pop_front() else {
                return false;
            };
            let Some(entry) = self.entries.get_mut(&resource_id) else {
                continue;
            };
            if entry.decoded_bytes == 0 {
                continue;
            }
            if entry.resource_pinned() {
                self.decoded_lru.push_back(resource_id);
                continue;
            }
            entry.decoded_bytes = 0;
            self.evicted_decoded_resources = self.evicted_decoded_resources.saturating_add(1);
            return true;
        }
        false
    }

    fn evict_one_thumbnail_lru(&mut self) -> bool {
        let len = self.thumbnail_lru.len();
        for _ in 0..len {
            let Some(resource_id) = self.thumbnail_lru.pop_front() else {
                return false;
            };
            let Some(entry) = self.entries.get_mut(&resource_id) else {
                continue;
            };
            if entry.thumbnail_bytes == 0 {
                continue;
            }
            if entry.resource_pinned() {
                self.thumbnail_lru.push_back(resource_id);
                continue;
            }
            entry.thumbnail_bytes = 0;
            self.evicted_thumbnails = self.evicted_thumbnails.saturating_add(1);
            return true;
        }
        false
    }

    fn evict_all_unpinned_decoded(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.decoded_bytes > 0 && !entry.resource_pinned() {
                entry.decoded_bytes = 0;
                self.evicted_decoded_resources = self.evicted_decoded_resources.saturating_add(1);
            }
        }
        self.decoded_lru.retain(|resource_id| {
            self.entries
                .get(resource_id)
                .is_some_and(|entry| entry.decoded_bytes > 0)
        });
    }

    fn evict_far_thumbnails(&mut self) {
        let max_distance = self.policy.prefer_viewport_distance;
        for entry in self.entries.values_mut() {
            if entry.thumbnail_bytes > 0
                && entry.viewport_distance > max_distance
                && !entry.resource_pinned()
            {
                entry.thumbnail_bytes = 0;
                self.evicted_thumbnails = self.evicted_thumbnails.saturating_add(1);
            }
        }
        self.thumbnail_lru.retain(|resource_id| {
            self.entries
                .get(resource_id)
                .is_some_and(|entry| entry.thumbnail_bytes > 0)
        });
    }

    fn evict_all_unpinned_thumbnails(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.thumbnail_bytes > 0 && !entry.resource_pinned() {
                entry.thumbnail_bytes = 0;
                self.evicted_thumbnails = self.evicted_thumbnails.saturating_add(1);
            }
        }
        self.thumbnail_lru.retain(|resource_id| {
            self.entries
                .get(resource_id)
                .is_some_and(|entry| entry.thumbnail_bytes > 0)
        });
    }
}

fn touch_lru(lru: &mut VecDeque<MediaResourceId>, resource_id: MediaResourceId) {
    if let Some(index) = lru.iter().position(|id| *id == resource_id) {
        lru.remove(index);
    }
    lru.push_back(resource_id);
}

impl Default for MediaCache {
    fn default() -> Self {
        Self::new(MediaCachePolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_box() -> MediaStableBox {
        MediaStableBox {
            estimated_height: 240.0,
            min_height: 120.0,
            max_height: 480.0,
        }
    }

    fn metadata() -> MediaMetadata {
        MediaMetadata::new(4_000, 3_000, 5 * 1024 * 1024, "image/jpeg")
    }

    #[test]
    fn image_dense_scroll_schedules_thumbnail_not_original_on_ui_thread() {
        let mut cache = MediaCache::new(MediaCachePolicy {
            max_decoded_bytes: 20 * 1024 * 1024,
            max_thumbnail_bytes: 2 * 1024 * 1024,
            prefer_viewport_distance: 800.0,
        });
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());
        cache.set_viewport_distance(MediaResourceId(1), 120.0);

        let thumbnail = cache.request_thumbnail_if_near(MediaResourceId(1));
        assert_eq!(
            thumbnail,
            MediaDecodeDecision::Scheduled(MediaDecodeRequest {
                resource_id: MediaResourceId(1),
                block_id: 10,
                kind: MediaDecodeKind::Thumbnail,
                lane: MediaDecodeLane::Background,
            })
        );

        let original = cache.request_original(MediaResourceId(1), MediaDecodeTrigger::Scroll);
        assert_eq!(original, MediaDecodeDecision::OriginalRequiresExplicitView);
    }

    #[test]
    fn viewport_distance_priority_skips_far_thumbnail_decode() {
        let mut cache = MediaCache::new(MediaCachePolicy {
            max_decoded_bytes: 20 * 1024 * 1024,
            max_thumbnail_bytes: 2 * 1024 * 1024,
            prefer_viewport_distance: 800.0,
        });
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());
        cache.set_viewport_distance(MediaResourceId(1), 2_400.0);

        assert_eq!(
            cache.request_thumbnail_if_near(MediaResourceId(1)),
            MediaDecodeDecision::TooFarFromViewport
        );
    }

    #[test]
    fn original_decode_waits_for_explicit_view() {
        let mut cache = MediaCache::default();
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());

        assert_eq!(
            cache.request_original(MediaResourceId(1), MediaDecodeTrigger::ViewportNear),
            MediaDecodeDecision::OriginalRequiresExplicitView
        );
        assert!(matches!(
            cache.request_original(MediaResourceId(1), MediaDecodeTrigger::UserExplicitView),
            MediaDecodeDecision::Scheduled(MediaDecodeRequest {
                kind: MediaDecodeKind::Original,
                lane: MediaDecodeLane::Background,
                ..
            })
        ));
    }

    #[test]
    fn entity_pin_does_not_permanently_pin_original_resource() {
        let mut cache = MediaCache::new(MediaCachePolicy {
            max_decoded_bytes: 1,
            max_thumbnail_bytes: 1024,
            prefer_viewport_distance: 800.0,
        });
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());
        cache.pin_entity(MediaResourceId(1));

        assert!(cache.apply_decode_result(MediaResourceId(1), MediaDecodeKind::Original, 10));

        let entry = cache.entry(MediaResourceId(1)).unwrap();
        assert_eq!(entry.entity_pin_count, 1);
        assert!(!entry.has_decoded_original());
        assert!(entry.has_payload_and_layout_box());
    }

    #[test]
    fn resource_pin_protects_decoded_resource_until_unpinned() {
        let mut cache = MediaCache::new(MediaCachePolicy {
            max_decoded_bytes: 1,
            max_thumbnail_bytes: 1024,
            prefer_viewport_distance: 800.0,
        });
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());
        cache.pin_resource(MediaResourceId(1));

        assert!(cache.apply_decode_result(MediaResourceId(1), MediaDecodeKind::Original, 10));
        assert!(
            cache
                .entry(MediaResourceId(1))
                .unwrap()
                .has_decoded_original()
        );

        cache.unpin_resource(MediaResourceId(1));
        cache.apply_memory_pressure(MemoryPressure::Warning);
        assert!(
            !cache
                .entry(MediaResourceId(1))
                .unwrap()
                .has_decoded_original()
        );
    }

    #[test]
    fn high_resolution_image_memory_pressure_drops_decoded_not_metadata_or_stable_box() {
        let mut cache = MediaCache::default();
        cache.upsert_metadata(MediaResourceId(1), 10, metadata(), stable_box());
        assert!(cache.apply_decode_result(
            MediaResourceId(1),
            MediaDecodeKind::Original,
            48 * 1024 * 1024
        ));

        cache.apply_memory_pressure(MemoryPressure::Warning);

        let entry = cache.entry(MediaResourceId(1)).unwrap();
        assert!(!entry.has_decoded_original());
        assert!(entry.has_payload_and_layout_box());
    }

    #[test]
    fn decode_result_storm_keeps_lru_under_limits() {
        let mut cache = MediaCache::new(MediaCachePolicy {
            max_decoded_bytes: 30,
            max_thumbnail_bytes: 12,
            prefer_viewport_distance: 800.0,
        });
        for id in 0..10 {
            cache.upsert_metadata(MediaResourceId(id), id, metadata(), stable_box());
            assert!(cache.apply_decode_result(MediaResourceId(id), MediaDecodeKind::Original, 10));
            assert!(cache.apply_decode_result(MediaResourceId(id), MediaDecodeKind::Thumbnail, 4));
        }

        let stats = cache.stats();
        assert!(stats.decoded_bytes <= 30);
        assert!(stats.thumbnail_bytes <= 12);
        assert!(stats.evicted_decoded_resources > 0);
        assert!(stats.evicted_thumbnails > 0);
    }
}
