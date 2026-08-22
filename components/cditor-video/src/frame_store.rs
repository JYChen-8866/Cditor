use std::sync::{Arc, Mutex};

use crate::types::VideoFrame;

#[derive(Clone, Debug, Default)]
pub struct VideoFrameStore {
    state: Arc<Mutex<VideoFrameState>>,
}

#[derive(Debug, Default)]
struct VideoFrameState {
    generation: u64,
    frame: Option<Arc<VideoFrame>>,
    last_presented_generation: u64,
    stats: VideoFrameStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoFrameStats {
    pub published_frames: u64,
    pub presented_frames: u64,
    pub overwritten_before_present: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatestVideoFrame {
    pub generation: u64,
    pub frame: Arc<VideoFrame>,
    pub stats: VideoFrameStats,
}

impl VideoFrameStore {
    pub fn publish(&self, frame: VideoFrame) -> LatestVideoFrame {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.frame.is_some() && state.last_presented_generation < state.generation {
            state.stats.overwritten_before_present += 1;
        }
        state.generation += 1;
        state.stats.published_frames += 1;
        let frame = Arc::new(frame);
        state.frame = Some(frame.clone());
        LatestVideoFrame {
            generation: state.generation,
            frame,
            stats: state.stats,
        }
    }

    pub fn latest(&self) -> Option<LatestVideoFrame> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(LatestVideoFrame {
            generation: state.generation,
            frame: state.frame.clone()?,
            stats: state.stats,
        })
    }

    pub fn stats(&self) -> VideoFrameStats {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stats
    }

    pub fn has_unpresented_frame(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.frame.is_some() && state.last_presented_generation < state.generation
    }

    pub fn mark_presented(&self, generation: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation > state.last_presented_generation && generation <= state.generation {
            state.last_presented_generation = generation;
            state.stats.presented_frames += 1;
        }
    }

    /// Releases the CPU pixel buffer after the frame has been uploaded to the
    /// renderer. The generation remains monotonic, so a decoder can publish a
    /// new frame immediately without retaining two full BGRA buffers.
    pub fn clear_if_generation(&self, generation: u64) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation != generation {
            return false;
        }
        state.frame = None;
        true
    }

    pub fn resident_bytes(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .frame
            .as_ref()
            .map_or(0, |frame| frame.bytes().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoFrame;

    #[test]
    fn latest_frame_store_replaces_old_frames_and_tracks_presentations() {
        let store = VideoFrameStore::default();
        let frame = |timestamp| VideoFrame::bgra(2, 2, 8, timestamp, vec![0; 16]).unwrap();
        let first = store.publish(frame(1));
        let second = store.publish(frame(2));

        assert_eq!(first.generation, 1);
        assert_eq!(second.generation, 2);
        assert_eq!(second.stats.overwritten_before_present, 1);
        assert!(store.has_unpresented_frame());
        store.mark_presented(second.generation);
        assert!(!store.has_unpresented_frame());
        assert_eq!(store.stats().presented_frames, 1);
        assert_eq!(store.latest().unwrap().frame.timestamp_us, 2);
    }

    #[test]
    fn presented_frame_can_release_its_cpu_pixels_without_resetting_generation() {
        let store = VideoFrameStore::default();
        let frame = store.publish(VideoFrame::bgra(2, 2, 8, 1, vec![0; 16]).unwrap());

        assert_eq!(store.resident_bytes(), 16);
        store.mark_presented(frame.generation);
        assert!(store.clear_if_generation(frame.generation));
        assert_eq!(store.resident_bytes(), 0);
        assert!(store.latest().is_none());

        let next = store.publish(VideoFrame::bgra(2, 2, 8, 2, vec![0; 16]).unwrap());
        assert_eq!(next.generation, frame.generation + 1);
        assert!(store.has_unpresented_frame());
    }

    #[test]
    fn stale_renderer_cannot_clear_a_newer_frame() {
        let store = VideoFrameStore::default();
        let first = store.publish(VideoFrame::bgra(2, 2, 8, 1, vec![0; 16]).unwrap());
        let second = store.publish(VideoFrame::bgra(2, 2, 8, 2, vec![0; 16]).unwrap());

        assert!(!store.clear_if_generation(first.generation));
        assert_eq!(store.latest().unwrap().generation, second.generation);
    }
}
