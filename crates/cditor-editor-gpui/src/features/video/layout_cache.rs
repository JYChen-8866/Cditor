use std::collections::{HashMap, VecDeque};

use cditor_core::ids::BlockId;

const MAX_VIDEO_LAYOUT_ENTRIES: usize = 4096;

struct VideoLayoutEntry {
    source: String,
    dimensions: cditor_video::VideoDimensions,
}

#[derive(Default)]
pub(super) struct VideoLayoutCache {
    entries: HashMap<BlockId, VideoLayoutEntry>,
    insertion_order: VecDeque<BlockId>,
}

impl VideoLayoutCache {
    pub(super) fn insert(
        &mut self,
        block_id: BlockId,
        source: String,
        dimensions: cditor_video::VideoDimensions,
    ) {
        let entry = VideoLayoutEntry { source, dimensions };
        if self.entries.contains_key(&block_id) {
            self.entries.insert(block_id, entry);
            return;
        }
        if self.entries.len() >= MAX_VIDEO_LAYOUT_ENTRIES
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(block_id, entry);
        self.insertion_order.push_back(block_id);
    }

    pub(super) fn get(
        &self,
        block_id: BlockId,
        source: &str,
    ) -> Option<cditor_video::VideoDimensions> {
        self.entries
            .get(&block_id)
            .filter(|entry| entry.source == source)
            .map(|entry| entry.dimensions)
    }

    pub(super) fn get_any(&self, block_id: BlockId) -> Option<cditor_video::VideoDimensions> {
        self.entries.get(&block_id).map(|entry| entry.dimensions)
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_dimensions_survive_playback_session_eviction() {
        let mut layouts = VideoLayoutCache::default();
        layouts.insert(
            7,
            "assets/portrait.mp4".into(),
            cditor_video::VideoDimensions {
                width: 720,
                height: 1280,
            },
        );

        assert_eq!(
            layouts.get(7, "assets/portrait.mp4"),
            Some(cditor_video::VideoDimensions {
                width: 720,
                height: 1280,
            })
        );
    }

    #[test]
    fn decoded_dimensions_are_not_reused_after_video_source_changes() {
        let mut layouts = VideoLayoutCache::default();
        layouts.insert(
            7,
            "assets/old.mp4".into(),
            cditor_video::VideoDimensions {
                width: 1920,
                height: 1080,
            },
        );

        assert!(layouts.get(7, "assets/new.mp4").is_none());
    }
}
