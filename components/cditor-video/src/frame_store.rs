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

/// A claimed mailbox frame whose presentation acknowledgement is deferred
/// until the renderer has successfully consumed it.
///
/// Claiming removes the frame from the decoder mailbox immediately, which
/// keeps the one-slot backpressure invariant. If the lease is dropped before
/// [`Self::commit`] and no newer generation has arrived, the frame is put back
/// into the mailbox. A newer generation always wins, so an old renderer can
/// never overwrite a frame that the decoder published after the claim.
#[derive(Debug)]
pub struct VideoFrameLease {
    store: VideoFrameStore,
    generation: u64,
    frame: Option<Arc<VideoFrame>>,
    committed: bool,
}

impl VideoFrameLease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn take_frame(&mut self) -> Option<Arc<VideoFrame>> {
        self.frame.take()
    }

    /// Returns a frame to this lease after a fallible conversion. The lease
    /// will restore it on drop if the decoder has not published a newer one.
    pub fn return_frame(&mut self, frame: Arc<VideoFrame>) {
        debug_assert!(self.frame.is_none());
        if self.frame.is_none() {
            self.frame = Some(frame);
        }
    }

    /// Acknowledges the claimed generation as presented.
    pub fn commit(mut self) {
        self.committed = true;
        let mut state = self
            .store
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.generation > state.last_presented_generation && self.generation <= state.generation
        {
            state.last_presented_generation = self.generation;
            state.stats.presented_frames = state.stats.presented_frames.saturating_add(1);
        }
        self.frame = None;
    }
}

impl Drop for VideoFrameLease {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let Some(frame) = self.frame.take() else {
            return;
        };
        let mut state = self
            .store
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation == self.generation && state.frame.is_none() {
            state.frame = Some(frame);
        } else if state.generation > self.generation {
            // A newer frame superseded the failed presentation. Keep the
            // stats honest without replacing the newer mailbox contents.
            state.stats.overwritten_before_present =
                state.stats.overwritten_before_present.saturating_add(1);
        }
    }
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

    /// Atomically removes the newest frame from the single-slot mailbox.
    ///
    /// Taking and acknowledging happen under the same lock. Once this returns,
    /// the decoder may publish the next generation while the caller converts
    /// the claimed `Arc<VideoFrame>` into a render resource. A later publish
    /// cannot invalidate the returned frame, and acknowledging this generation
    /// never acknowledges a newer one.
    pub fn take_latest_for_presentation(&self) -> Option<LatestVideoFrame> {
        self.take_latest_for_presentation_after(0)
    }

    /// Claims the newest frame without acknowledging it as presented yet.
    pub fn claim_latest_for_presentation_after(
        &self,
        last_presented_generation: u64,
    ) -> Option<VideoFrameLease> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation <= last_presented_generation {
            return None;
        }
        let frame = state.frame.take()?;
        Some(VideoFrameLease {
            store: self.clone(),
            generation: state.generation,
            frame: Some(frame),
            committed: false,
        })
    }

    /// Claims the mailbox only when it contains a generation newer than the
    /// caller's already-rendered generation. The generation check and take
    /// are one atomic operation, so a stale renderer cannot consume a frame
    /// that a newer renderer still needs.
    pub fn take_latest_for_presentation_after(
        &self,
        last_presented_generation: u64,
    ) -> Option<LatestVideoFrame> {
        let mut lease = self.claim_latest_for_presentation_after(last_presented_generation)?;
        let generation = lease.generation;
        let frame = lease.take_frame()?;
        lease.commit();
        Some(LatestVideoFrame {
            generation,
            frame,
            stats: self.stats(),
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

    #[test]
    fn atomic_take_reopens_the_slot_without_acknowledging_the_next_generation() {
        let store = VideoFrameStore::default();
        let first = store.publish(VideoFrame::bgra(2, 2, 8, 1, vec![1; 16]).unwrap());

        let claimed = store.take_latest_for_presentation().unwrap();
        assert_eq!(claimed.generation, first.generation);
        assert_eq!(claimed.stats.presented_frames, 1);
        assert!(!store.has_unpresented_frame());
        assert_eq!(store.resident_bytes(), 0);

        let second = store.publish(VideoFrame::bgra(2, 2, 8, 2, vec![2; 16]).unwrap());
        store.mark_presented(first.generation);

        assert_eq!(store.latest().unwrap().generation, second.generation);
        assert!(store.has_unpresented_frame());
        assert_eq!(store.stats().presented_frames, 1);
        assert_eq!(claimed.frame.bytes(), [1; 16]);
    }

    #[test]
    fn atomic_take_preserves_external_frame_readers_for_copy_fallback() {
        let store = VideoFrameStore::default();
        let published = store.publish(VideoFrame::bgra(1, 1, 4, 1, vec![4; 4]).unwrap());
        let generation = published.generation;
        drop(published);
        let external_reader = store.latest().unwrap();

        let claimed = store.take_latest_for_presentation().unwrap();

        assert_eq!(claimed.generation, generation);
        assert_eq!(external_reader.frame.bytes(), [4; 4]);
        assert_eq!(Arc::strong_count(&claimed.frame), 2);
        assert!(store.latest().is_none());
    }

    #[test]
    fn stale_take_does_not_consume_the_newest_frame() {
        let store = VideoFrameStore::default();
        let published = store.publish(VideoFrame::bgra(1, 1, 4, 1, vec![6; 4]).unwrap());

        assert!(
            store
                .take_latest_for_presentation_after(published.generation)
                .is_none()
        );
        assert_eq!(store.latest().unwrap().generation, published.generation);
        assert!(store.has_unpresented_frame());
    }

    #[test]
    fn dropped_lease_restores_frame_and_does_not_acknowledge_it() {
        let store = VideoFrameStore::default();
        let published = store.publish(VideoFrame::bgra(1, 1, 4, 1, vec![7; 4]).unwrap());

        let lease = store
            .claim_latest_for_presentation_after(0)
            .expect("new frame should be claimable");
        assert_eq!(lease.generation(), published.generation);
        assert!(store.latest().is_none());
        drop(lease);

        assert_eq!(store.latest().unwrap().generation, published.generation);
        assert_eq!(store.stats().presented_frames, 0);
    }

    #[test]
    fn committed_lease_acknowledges_only_the_claimed_generation() {
        let store = VideoFrameStore::default();
        let first = store.publish(VideoFrame::bgra(1, 1, 4, 1, vec![8; 4]).unwrap());
        let mut lease = store
            .claim_latest_for_presentation_after(0)
            .expect("new frame should be claimable");
        let frame = lease.take_frame().unwrap();
        lease.return_frame(frame);
        let second = store.publish(VideoFrame::bgra(1, 1, 4, 2, vec![9; 4]).unwrap());
        lease.commit();

        assert_eq!(store.stats().presented_frames, 1);
        assert_eq!(store.latest().unwrap().generation, second.generation);
        assert_ne!(first.generation, second.generation);
        assert!(store.has_unpresented_frame());
    }

    #[test]
    fn failed_lease_does_not_overwrite_a_newer_generation() {
        let store = VideoFrameStore::default();
        store.publish(VideoFrame::bgra(1, 1, 4, 1, vec![1; 4]).unwrap());
        let lease = store
            .claim_latest_for_presentation_after(0)
            .expect("new frame should be claimable");
        let newer = store.publish(VideoFrame::bgra(1, 1, 4, 2, vec![2; 4]).unwrap());
        drop(lease);

        assert_eq!(store.latest().unwrap().generation, newer.generation);
        assert_eq!(store.stats().overwritten_before_present, 1);
    }
}
