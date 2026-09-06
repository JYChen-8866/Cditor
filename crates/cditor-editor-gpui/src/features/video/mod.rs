use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, RecvTimeoutError, SyncSender, TrySendError},
};
use std::thread::JoinHandle;

use cditor_component::{Upload, UploadStyle};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayloadView, RichBlockKind, VideoPayload};
use cditor_runtime::EditorViewProjection;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Styled, StyledImage, div, px,
};

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

#[cfg(any(target_os = "ios", target_os = "android"))]
const MAX_ACTIVE_VIDEO_SESSIONS: usize = 2;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const MAX_ACTIVE_VIDEO_SESSIONS: usize = 4;
/// A 1280x720 BGRA frame is about 3.5 MiB. Reserve space for the decoder
/// frame, the GPUI image, and bounded codec/audio overhead per active session.
#[cfg(any(target_os = "ios", target_os = "android"))]
const VIDEO_SESSION_MEMORY_RESERVATION_BYTES: usize = 12 * 1024 * 1024;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const VIDEO_SESSION_MEMORY_RESERVATION_BYTES: usize = 16 * 1024 * 1024;
#[cfg(any(target_os = "ios", target_os = "android"))]
const MAX_VIDEO_MEMORY_BYTES: usize = 24 * 1024 * 1024;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const MAX_VIDEO_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_BUDGETED_VIDEO_SESSIONS: usize =
    MAX_VIDEO_MEMORY_BYTES / VIDEO_SESSION_MEMORY_RESERVATION_BYTES;
const VIDEO_REAPER_QUEUE_CAPACITY: usize = 16;
// Bootstrap and reaper workers perform bounded I/O/drop work. Avoid reserving
// a platform-default multi-megabyte stack for every active video.
const VIDEO_WORKER_STACK_BYTES: usize = 512 * 1024;
static RESERVED_VIDEO_MEMORY_BYTES: AtomicUsize = AtomicUsize::new(0);
const VIDEO_PLAYER_HOVER_GROUP: &str = "video-player";
const VIDEO_CONTROLS_HIDE_UNTIL_HOVER: bool = !cfg!(any(target_os = "ios", target_os = "android"));

mod controls;
mod frame_surface;
mod import;
mod layout_cache;
mod window_overlay;

use frame_surface::{CachedVideoImage, RetiredVideoImage, VideoRenderImage};
pub(crate) use import::accepts_external_video_paths;
use layout_cache::VideoLayoutCache;

struct VideoEntry {
    source: String,
    state: Arc<Mutex<VideoEntryState>>,
    cancellation: cditor_video::VideoCancellationToken,
    _task: Option<JoinHandle<()>>,
}

impl VideoEntry {
    fn is_heavy(&self) -> bool {
        matches!(
            *self.state.lock().unwrap_or_else(|error| error.into_inner()),
            VideoEntryState::Loading | VideoEntryState::Ready { .. }
        )
    }
}

struct VideoMemoryReservation {
    bytes: usize,
}

impl Drop for VideoMemoryReservation {
    fn drop(&mut self) {
        RESERVED_VIDEO_MEMORY_BYTES.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

fn reserve_video_memory(
    bytes: usize,
    cancellation: &cditor_video::VideoCancellationToken,
) -> Option<VideoMemoryReservation> {
    if cancellation.is_cancelled() {
        return None;
    }
    if try_reserve_video_memory(&RESERVED_VIDEO_MEMORY_BYTES, MAX_VIDEO_MEMORY_BYTES, bytes) {
        Some(VideoMemoryReservation { bytes })
    } else {
        // A render pass will retry deferred entries after an older session is
        // evicted. Never park a worker thread polling a global memory limit.
        None
    }
}

fn try_reserve_video_memory(counter: &AtomicUsize, budget: usize, bytes: usize) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes) else {
            return false;
        };
        if next > budget {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

enum VideoEntryState {
    Loading,
    Deferred,
    Ready {
        session: Arc<cditor_video::VideoSession>,
        render_image: CachedVideoImage,
        _memory_reservation: VideoMemoryReservation,
    },
    Failed(String),
}

#[derive(Default)]
pub(crate) struct VideoPlaybackCache {
    entries: Mutex<HashMap<BlockId, VideoEntry>>,
    layout_dimensions: Mutex<VideoLayoutCache>,
    imports: Mutex<HashMap<BlockId, String>>,
    uploads: Mutex<HashMap<BlockId, Entity<Upload>>>,
    requested: Mutex<HashSet<BlockId>>,
    control_bounds: controls::ControlBounds,
}

impl VideoPlaybackCache {
    pub(crate) fn diagnostics(&self) -> cditor_sdk::diagnostics::VideoDiagnostics {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut diagnostics = cditor_sdk::diagnostics::VideoDiagnostics {
            tracked_blocks: entries.len(),
            reserved_decoder_bytes: RESERVED_VIDEO_MEMORY_BYTES.load(Ordering::Acquire),
            decoder_budget_bytes: MAX_VIDEO_MEMORY_BYTES,
            max_active_sessions_per_editor: MAX_ACTIVE_VIDEO_SESSIONS,
            ..Default::default()
        };
        for entry in entries.values() {
            match &*entry.state.lock().unwrap_or_else(|e| e.into_inner()) {
                VideoEntryState::Loading => diagnostics.loading_sessions += 1,
                VideoEntryState::Deferred => diagnostics.deferred_sessions += 1,
                VideoEntryState::Failed(_) => diagnostics.failed_sessions += 1,
                VideoEntryState::Ready {
                    session,
                    render_image,
                    ..
                } => {
                    diagnostics.ready_sessions += 1;
                    diagnostics.playing_sessions += usize::from(session.snapshot().playing);
                    diagnostics.resident_cpu_frame_bytes = diagnostics
                        .resident_cpu_frame_bytes
                        .saturating_add(session.resident_frame_bytes());
                    if render_image.is_some() {
                        diagnostics.render_images += 1;
                        #[cfg(feature = "gpui-dynamic-image")]
                        {
                            diagnostics.dynamic_images += 1;
                        }
                        diagnostics.stable_gpu_slot_capacity = diagnostics
                            .stable_gpu_slot_capacity
                            .saturating_add(render_image.stable_slot_capacity());
                        let dimensions = session.dimensions();
                        diagnostics.resident_render_image_bytes =
                            diagnostics.resident_render_image_bytes.saturating_add(
                                usize::try_from(dimensions.width)
                                    .unwrap_or(usize::MAX)
                                    .saturating_mul(
                                        usize::try_from(dimensions.height).unwrap_or(usize::MAX),
                                    )
                                    .saturating_mul(4),
                            );
                    }
                }
            }
        }
        diagnostics
    }

    pub(crate) fn clear(&self) -> Vec<RetiredVideoImage> {
        let retired = self.clear_playback_entries();
        self.layout_dimensions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.imports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.uploads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.requested
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.control_bounds.clear();
        retired
    }

    /// Releases decoder sessions and their current frame surfaces that are
    /// outside the caller's protected set. Intrinsic dimensions remain in the
    /// separate layout cache, so reclaiming a session cannot move the block or
    /// disturb scroll anchors. This is intentionally a coarse per-editor
    /// pressure boundary; the process-wide reservation enforces the hard cap.
    pub(crate) fn apply_memory_pressure(
        &self,
        pressure: crate::memory_pressure::CditorMemoryPressure,
        protected: &HashSet<BlockId>,
    ) -> Vec<RetiredVideoImage> {
        if matches!(
            pressure,
            crate::memory_pressure::CditorMemoryPressure::Normal
        ) {
            return Vec::new();
        }
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let victims = entries
            .keys()
            .copied()
            .filter(|block_id| !protected.contains(block_id))
            .filter(|block_id| {
                entries.get(block_id).is_some_and(|entry| {
                    !matches!(
                        *entry.state.lock().unwrap_or_else(|e| e.into_inner()),
                        VideoEntryState::Deferred | VideoEntryState::Failed(_)
                    )
                })
            })
            .collect::<Vec<_>>();
        let victims = if matches!(
            pressure,
            crate::memory_pressure::CditorMemoryPressure::Critical
        ) {
            victims
        } else {
            let target = victims.len().saturating_div(2).max(1);
            victims.into_iter().take(target).collect()
        };
        let retired = victims
            .into_iter()
            .filter_map(|block_id| entries.remove(&block_id))
            .collect::<Vec<_>>();
        drop(entries);
        retire_video_entries(retired)
    }

    pub(crate) fn sync_visible_window(
        &self,
        projection: &EditorViewProjection,
        asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
        pinned_block_id: Option<BlockId>,
        cx: &mut Context<CditorV2View>,
    ) {
        let video_ids = projection
            .blocks
            .iter()
            .filter(|block| matches!(block.kind, RichBlockKind::Video))
            .map(|block| block.block_id)
            .collect::<HashSet<_>>();
        self.uploads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|block_id, _| video_ids.contains(block_id));
        // Import progress is document-window state as well. Keeping entries
        // for every video ever touched made repeated drag/drop operations grow
        // the map even after the blocks were deleted or left the projection.
        // The durable block payload is the source of truth; stale progress is
        // safe to discard and will be recreated when the block returns.
        self.imports
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|block_id, _| video_ids.contains(block_id));
        self.control_bounds.retain(&video_ids);
        let requested = self
            .requested
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let candidates = projection
            .blocks
            .iter()
            .filter(|block| matches!(block.kind, RichBlockKind::Video))
            // `projection.blocks` is the render window, including bounded
            // overscan. Keep admission tied to that window rather than the
            // physical payload-visible core: the latter is a data-fetch
            // range, and using it to evict media would destroy playback
            // continuity every time a block crosses the core boundary.
            .filter_map(|block| {
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let cditor_core::rich_text::BlockPayload::Video(video) = &payload.payload else {
                    return None;
                };
                (!video.source.trim().is_empty()).then_some((block.block_id, video.source.clone()))
            })
            .collect::<Vec<_>>();
        let mut candidates = candidates;
        // A fullscreen overlay can outlive the document render window. Keep
        // its already-resolved source in the bounded candidate set even when
        // the corresponding block is currently outside the projection.
        if let Some(pinned_block_id) = pinned_block_id
            && !candidates
                .iter()
                .any(|(block_id, _)| *block_id == pinned_block_id)
            && let Some(source) = self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&pinned_block_id)
                .map(|entry| entry.source.clone())
        {
            candidates.push((pinned_block_id, source));
        }
        let visible = select_active_video_candidates(
            candidates,
            &requested,
            MAX_ACTIVE_VIDEO_SESSIONS.min(MAX_BUDGETED_VIDEO_SESSIONS),
        );
        let ids = visible
            .iter()
            .map(|(block_id, _)| *block_id)
            .collect::<HashSet<_>>();
        self.requested
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|block_id| video_ids.contains(block_id));
        let mut retired_entries = Vec::new();
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let stale_ids = entries
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect::<Vec<_>>();
        for id in stale_ids {
            if let Some(entry) = entries.remove(&id) {
                retired_entries.push(entry);
            }
        }
        for (block_id, source) in visible {
            if entries
                .get(&block_id)
                .is_some_and(|entry| entry.source != source)
                && let Some(entry) = entries.remove(&block_id)
            {
                retired_entries.push(entry);
            }
            let should_retry_deferred = entries.get(&block_id).is_some_and(|entry| {
                matches!(
                    *entry.state.lock().unwrap_or_else(|e| e.into_inner()),
                    VideoEntryState::Deferred
                )
            });
            if should_retry_deferred {
                entries.remove(&block_id);
            }
            if !entries.contains_key(&block_id) {
                let state = Arc::new(Mutex::new(VideoEntryState::Loading));
                let state_for_task = Arc::clone(&state);
                let cancellation = cditor_video::VideoCancellationToken::default();
                let Some(memory_reservation) =
                    reserve_video_memory(VIDEO_SESSION_MEMORY_RESERVATION_BYTES, &cancellation)
                else {
                    *state.lock().unwrap_or_else(|e| e.into_inner()) = VideoEntryState::Deferred;
                    entries.insert(
                        block_id,
                        VideoEntry {
                            source,
                            state,
                            cancellation,
                            _task: None,
                        },
                    );
                    continue;
                };
                let cancellation_for_task = cancellation.clone();
                let source_for_task = source.clone();
                let provider_for_task = asset_provider.clone();
                let should_autoplay = requested.contains(&block_id);
                let task = std::thread::Builder::new()
                    .name("cditor-video-session".into())
                    .stack_size(VIDEO_WORKER_STACK_BYTES)
                    .spawn(move || {
                        let result = futures_lite::future::block_on(async {
                            if cancellation_for_task.is_cancelled() {
                                return Err(cditor_video::VideoError::Cancelled.to_string());
                            }
                            let resolved = if source_for_task.starts_with("assets/")
                                && let Some(provider) = provider_for_task
                            {
                                let asset = cditor_core::rich_text::AssetRef::local(
                                    source_for_task.clone(),
                                );
                                let resolve = async {
                                    crate::provider_io::resolve_asset(provider, asset)
                                        .await
                                        .map(Some)
                                        .map_err(|error| error.to_string())
                                };
                                let cancel = async {
                                    cancellation_for_task.cancelled().await;
                                    Ok(None)
                                };
                                futures_lite::future::race(resolve, cancel)
                                    .await?
                                    .ok_or_else(|| cditor_video::VideoError::Cancelled.to_string())?
                                    .local_path
                                    .ok_or_else(|| {
                                        "asset provider returned no local path".to_owned()
                                    })?
                            } else {
                                PathBuf::from(source_for_task)
                            };
                            if cancellation_for_task.is_cancelled() {
                                return Err(cditor_video::VideoError::Cancelled.to_string());
                            }
                            cditor_video::VideoSession::start_cancellable(
                                cditor_video::VideoSessionConfig {
                                    source: resolved,
                                    ..Default::default()
                                },
                                &cancellation_for_task,
                            )
                            .map(|session| {
                                if should_autoplay {
                                    let _ = session.command(cditor_video::VideoCommand::Play);
                                }
                                (session, memory_reservation)
                            })
                            .map_err(|error| error.to_string())
                        });
                        let mut state = state_for_task.lock().unwrap_or_else(|e| e.into_inner());
                        *state = match result {
                            Ok((session, memory_reservation)) => VideoEntryState::Ready {
                                session,
                                render_image: CachedVideoImage::default(),
                                _memory_reservation: memory_reservation,
                            },
                            Err(error) => VideoEntryState::Failed(error.to_string()),
                        };
                    })
                    .ok();
                if task.is_none() {
                    *state.lock().unwrap_or_else(|e| e.into_inner()) =
                        VideoEntryState::Failed("failed to start video loading task".into());
                }
                entries.insert(
                    block_id,
                    VideoEntry {
                        source,
                        state: Arc::clone(&state),
                        cancellation: cancellation.clone(),
                        _task: task,
                    },
                );
                let state_for_notify = Arc::clone(&state);
                let cancellation_for_notify = cancellation.clone();
                cx.spawn(async move |view, cx| {
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(33))
                            .await;
                        let keep_notifying = match &*state_for_notify
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                        {
                            VideoEntryState::Loading => true,
                            VideoEntryState::Ready { session, .. } => session.snapshot().playing,
                            VideoEntryState::Deferred | VideoEntryState::Failed(_) => false,
                        };
                        if cancellation_for_notify.is_cancelled() {
                            break;
                        }
                        let _ = view.update(cx, |_, cx| cx.notify());
                        if !keep_notifying {
                            break;
                        }
                    }
                })
                .detach();
            }
        }
        drop(entries);
        retire_video_resources_after_effect(retire_video_entries(retired_entries), [], cx);
    }

    fn clear_playback_entries(&self) -> Vec<RetiredVideoImage> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        retire_video_entries(entries)
    }

    fn upload_for(
        &self,
        block_id: BlockId,
        style: UploadStyle,
        view: Entity<CditorV2View>,
        provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
        cx: &mut App,
    ) -> Entity<Upload> {
        let mut uploads = self
            .uploads
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(upload) = uploads.get(&block_id) {
            upload.update(cx, |upload, cx| upload.set_style(style, cx));
            return upload.clone();
        }
        let upload = cx.new(|_| {
            Upload::new(format!("video-upload-{block_id}"), style)
                .drag(true)
                .limit(1)
                .accept(".mp4,.mov,.m4v,.webm,.mkv,.avi")
                .max_size(import::MAX_VIDEO_IMPORT_BYTES)
                .title("拖放视频或点击选择")
                .hint("支持 MP4、MOV、M4V、WebM、MKV、AVI，最大 512 MiB")
                .on_select(move |paths, cx| {
                    if let Some(path) = paths.into_iter().next() {
                        import::replace_video_from_path(
                            view.clone(),
                            provider.clone(),
                            block_id,
                            path,
                            cx,
                        );
                    }
                })
        });
        uploads.insert(block_id, upload.clone());
        upload
    }

    fn render_image(&self, block_id: BlockId, cx: &mut App) -> Option<Arc<VideoRenderImage>> {
        let (current, retired) = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let entry = entries.get(&block_id)?;
            let mut state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
            let VideoEntryState::Ready {
                session,
                render_image,
                ..
            } = &mut *state
            else {
                return None;
            };
            let retired = session
                .claim_latest_frame_for_presentation_after(render_image.presented_generation())
                .and_then(|mut lease| {
                    let generation = lease.generation();
                    let frame = lease.take_frame()?;
                    match cditor_video::render_image_from_owned_frame_recoverable(frame) {
                        Ok(next) => {
                            // Acknowledge only after the image owns the
                            // claimed pixels. Conversion errors leave the
                            // frame available for a later render attempt.
                            let retired = render_image.replace(generation, next);
                            lease.commit();
                            Some(retired)
                        }
                        Err(error) => {
                            let (_, frame) = error.into_parts();
                            lease.return_frame(frame);
                            None
                        }
                    }
                })
                .flatten();
            (render_image.current(), retired)
        };
        retire_video_resources_after_effect([], retired.into_iter(), cx);
        current
    }

    fn status(&self, block_id: BlockId) -> Option<String> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let entry = entries.get(&block_id)?;
        let state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*state {
            VideoEntryState::Loading => Some("正在加载视频…".into()),
            VideoEntryState::Deferred => Some("等待视频解码资源…".into()),
            VideoEntryState::Failed(message) => Some(message.clone()),
            VideoEntryState::Ready { .. } => None,
        }
    }

    fn command(&self, block_id: BlockId, command: cditor_video::VideoCommand) {
        if matches!(command, cditor_video::VideoCommand::Play) {
            self.requested
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(block_id);
        }
        let (target, other_playing) = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let mut target = None;
            let mut other_playing = Vec::new();
            for (entry_block_id, entry) in entries.iter() {
                let state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
                let VideoEntryState::Ready { session, .. } = &*state else {
                    continue;
                };
                if *entry_block_id == block_id {
                    target = Some(Arc::clone(session));
                } else if matches!(command, cditor_video::VideoCommand::Play)
                    && session.snapshot().playing
                {
                    other_playing.push(Arc::clone(session));
                }
            }
            (target, other_playing)
        };
        for session in other_playing {
            let _ = session.command(cditor_video::VideoCommand::Pause);
        }
        if let Some(session) = target {
            let _ = session.command(command);
        }
    }

    fn snapshot(&self, block_id: BlockId) -> Option<cditor_video::VideoPlaybackSnapshot> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let entry = entries.get(&block_id)?;
        let state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
        match &*state {
            VideoEntryState::Ready { session, .. } => Some(session.snapshot()),
            _ => None,
        }
    }

    fn dimensions_for_source(
        &self,
        block_id: BlockId,
        source: &str,
    ) -> Option<cditor_video::VideoDimensions> {
        let decoded = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.get(&block_id).and_then(|entry| {
                if entry.source != source {
                    return None;
                }
                let state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
                match &*state {
                    VideoEntryState::Ready { session, .. } => Some(session.dimensions()),
                    _ => None,
                }
            })
        };
        if let Some(dimensions) = decoded {
            self.layout_dimensions
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(block_id, source.to_owned(), dimensions);
            return Some(dimensions);
        }
        self.layout_dimensions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(block_id, source)
    }

    fn dimensions(&self, block_id: BlockId) -> Option<cditor_video::VideoDimensions> {
        let source = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&block_id)
            .map(|entry| entry.source.clone());
        source.map_or_else(
            || {
                self.layout_dimensions
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .get_any(block_id)
            },
            |source| self.dimensions_for_source(block_id, &source),
        )
    }

    fn failed(&self, block_id: BlockId) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = entries.get(&block_id) else {
            return false;
        };
        matches!(
            *entry.state.lock().unwrap_or_else(|e| e.into_inner()),
            VideoEntryState::Failed(_)
        )
    }

    fn set_import_status(&self, block_id: BlockId, status: Option<String>) {
        let mut imports = self.imports.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(status) = status {
            imports.insert(block_id, status);
        } else {
            imports.remove(&block_id);
        }
    }

    fn import_status(&self, block_id: BlockId) -> Option<String> {
        self.imports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&block_id)
            .cloned()
    }
}

fn select_active_video_candidates(
    mut candidates: Vec<(BlockId, String)>,
    requested: &HashSet<BlockId>,
    limit: usize,
) -> Vec<(BlockId, String)> {
    // Explicit playback wins a decoder slot even when several compact video
    // blocks fit in one projection. Stable sorting preserves document order.
    candidates.sort_by_key(|(block_id, _)| !requested.contains(block_id));
    candidates.truncate(limit);
    candidates
}

impl Drop for VideoPlaybackCache {
    fn drop(&mut self) {
        let entries = self
            .entries
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retired = entries.drain().map(|(_, entry)| entry).collect();
        retire_video_entries(retired);
    }
}

fn retire_video_entries(entries: Vec<VideoEntry>) -> Vec<RetiredVideoImage> {
    if entries.is_empty() {
        return Vec::new();
    }
    let retired_images = entries
        .iter()
        .filter_map(|entry| {
            entry.cancellation.cancel();
            take_entry_render_image(entry)
        })
        .collect();
    // Stopping FFmpeg and joining its pipe workers can briefly block. Never do
    // that while GPUI is producing a frame or holding the playback map lock.
    enqueue_video_reap(entries);
    retired_images
}

struct VideoReaper {
    primary: SyncSender<Vec<VideoEntry>>,
    overflow: Arc<Mutex<Vec<VideoEntry>>>,
}

fn video_reaper() -> &'static VideoReaper {
    static REAPER: OnceLock<VideoReaper> = OnceLock::new();
    REAPER.get_or_init(|| {
        let (primary, primary_receiver) =
            mpsc::sync_channel::<Vec<VideoEntry>>(VIDEO_REAPER_QUEUE_CAPACITY);
        let overflow = Arc::new(Mutex::new(Vec::new()));
        spawn_video_reaper_worker(
            "cditor-video-reaper",
            primary_receiver,
            Arc::clone(&overflow),
        );
        VideoReaper { primary, overflow }
    })
}

fn spawn_video_reaper_worker(
    name: &'static str,
    receiver: mpsc::Receiver<Vec<VideoEntry>>,
    overflow: Arc<Mutex<Vec<VideoEntry>>>,
) {
    let _ = std::thread::Builder::new()
        .name(name.into())
        .stack_size(VIDEO_WORKER_STACK_BYTES)
        .spawn(move || {
            loop {
                match receiver.recv_timeout(std::time::Duration::from_millis(25)) {
                    Ok(entries) => {
                        reap_video_entries(entries);
                        drain_video_reaper_overflow(&overflow);
                    }
                    // An idle reaper must stay alive. The next eviction may
                    // arrive long after the previous batch was drained.
                    Err(RecvTimeoutError::Timeout) => {
                        drain_video_reaper_overflow(&overflow);
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        drain_video_reaper_overflow(&overflow);
                        break;
                    }
                }
            }
        });
}

fn enqueue_video_reap(entries: Vec<VideoEntry>) {
    let reaper = video_reaper();
    match reaper.primary.try_send(entries) {
        Ok(()) => {}
        Err(TrySendError::Disconnected(entries)) => {
            // There is no receiver left that could drain an overflow slot.
            // This is only a thread-start/panic fallback. Reap synchronously
            // so a detached startup task cannot outlive the editor forever.
            reap_video_entries(entries);
        }
        Err(TrySendError::Full(entries)) => {
            append_video_reaper_overflow(&reaper.overflow, entries);
        }
    }
}

fn append_video_reaper_overflow(overflow: &Mutex<Vec<VideoEntry>>, entries: Vec<VideoEntry>) {
    let (heavy, light): (Vec<_>, Vec<_>) = entries.into_iter().partition(VideoEntry::is_heavy);
    // Failed/deferred entries do not own an FFmpeg process or reservation; they
    // can be released immediately and never contribute to the overflow.
    drop(light);
    if heavy.is_empty() {
        return;
    }
    let excess = {
        let mut pending = overflow.lock().unwrap_or_else(|error| error.into_inner());
        // Every Loading/Ready entry owns one reservation from the process-wide
        // decoder budget. Keep the bound explicit in release builds too: if a
        // future lifecycle path ever violates that accounting invariant, the
        // excess is reaped rather than retained indefinitely.
        let available = MAX_BUDGETED_VIDEO_SESSIONS.saturating_sub(pending.len());
        let mut heavy = heavy;
        let keep = available.min(heavy.len());
        let excess = heavy.split_off(keep);
        pending.extend(heavy);
        excess
    };
    if !excess.is_empty() {
        // This is a defensive fallback for an accounting bug. Release the
        // lock before joining startup workers so other evictions can proceed.
        reap_video_entries(excess);
    }
}

fn drain_video_reaper_overflow(overflow: &Mutex<Vec<VideoEntry>>) {
    let pending = {
        let mut pending = overflow.lock().unwrap_or_else(|error| error.into_inner());
        std::mem::take(&mut *pending)
    };
    reap_video_entries(pending);
}

/// Stop and join startup workers on the dedicated reaper thread. Dropping a
/// `JoinHandle` detaches the worker, which would let a cancelled asset lookup
/// retain its source, reservation and session state after viewport eviction.
/// Joining here is safe because this path never holds the editor playback map
/// or runs on the GPUI frame thread.
fn reap_video_entries(entries: Vec<VideoEntry>) {
    for mut entry in entries {
        entry.cancellation.cancel();
        if let Some(task) = entry._task.take() {
            let _ = task.join();
        }
        drop(entry);
    }
}

fn take_entry_render_image(entry: &VideoEntry) -> Option<RetiredVideoImage> {
    let mut state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
    match &mut *state {
        VideoEntryState::Ready { render_image, .. } => render_image.take(),
        VideoEntryState::Loading | VideoEntryState::Deferred | VideoEntryState::Failed(_) => None,
    }
}

fn retire_video_resources_after_effect(
    retired_images: impl IntoIterator<Item = RetiredVideoImage>,
    fallback_images: impl IntoIterator<Item = Arc<gpui::RenderImage>>,
    cx: &mut App,
) {
    let retired_images = retired_images.into_iter().collect::<Vec<_>>();
    let fallback_images = fallback_images.into_iter().collect::<Vec<_>>();
    if retired_images.is_empty() && fallback_images.is_empty() {
        return;
    }
    // The current scene can still reference the immutable fallback tile. Keep
    // retirement after the update so atlas removal cannot race the scene.
    cx.defer(move |cx| {
        #[cfg(feature = "gpui-dynamic-image")]
        for retired in retired_images {
            let (dynamic, fallback) = retired.into_parts();
            cx.drop_dynamic_image(dynamic, None);
            cx.drop_image(fallback, None);
        }
        #[cfg(not(feature = "gpui-dynamic-image"))]
        for retired in retired_images {
            cx.drop_image(retired.into_parts(), None);
        }
        for image in fallback_images {
            cx.drop_image(image, None);
        }
    });
}

#[cfg(feature = "gpui-dynamic-image")]
fn render_video_frame(image: Arc<VideoRenderImage>, width: f32, height: f32) -> AnyElement {
    gpui::dynamic_img(image)
        .w(px(width))
        .h(px(height))
        .into_any_element()
}

#[cfg(not(feature = "gpui-dynamic-image"))]
fn render_video_frame(image: Arc<VideoRenderImage>, width: f32, height: f32) -> AnyElement {
    gpui::img(image)
        .w(px(width))
        .h(px(height))
        .into_any_element()
}

pub(crate) fn render_video_block(
    block_id: BlockId,
    content_version: u64,
    video: &VideoPayload,
    available_width_px: f64,
    cache: &VideoPlaybackCache,
    workers: &crate::app::worker_admission::EditorWorkerAdmission,
    asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    _focus: FocusHandle,
    cx: &mut App,
) -> AnyElement {
    // CrabMD keeps the black player surface at the full document width, while
    // sizing the contained frame from a 560px media width. This preserves a
    // strong block shape without enlarging the actual video content.
    let width = available_width_px.max(1.0) as f32;
    // The payload may not carry dimensions for newly imported videos. Once
    // ffprobe has opened the source, prefer the decoder's dimensions so the
    // layout and the rendered frame use the same aspect ratio.
    let decoded_dimensions = cache.dimensions_for_source(block_id, &video.source);
    let block_height = video_block_height_px(video, decoded_dimensions, f64::from(width));
    let height = (block_height - cditor_core::layout::VIDEO_BLOCK_CHROME_HEIGHT_PX) as f32;
    let content_width = cditor_core::layout::video_content_width_px(f64::from(width)) as f32;
    let display_dimensions = decoded_dimensions.unwrap_or(cditor_video::VideoDimensions {
        width: video.intrinsic_width.unwrap_or(16),
        height: video.intrinsic_height.unwrap_or(9),
    });
    let (frame_width, frame_height) = fullscreen_video_size(
        display_dimensions.width,
        display_dimensions.height,
        content_width,
        height,
    );
    crate::features::media::schedule_rendered_media_height_report(
        view.clone(),
        block_id,
        content_version,
        block_height,
        cx,
    );
    let can_choose_source = video.source.is_empty() || cache.failed(block_id);
    let poster = video.poster.as_deref().and_then(|poster| {
        crate::image_loader::load_render_image(
            poster,
            block_id,
            content_version,
            workers,
            asset_provider.clone(),
            view.clone(),
            cx,
        )
    });
    let upload = cache.upload_for(
        block_id,
        UploadStyle {
            background: theme.action_background,
            border: theme.border,
            hover_border: theme.focused,
            text: theme.text,
            muted: theme.muted,
            icon: theme.muted,
        },
        view.clone(),
        asset_provider,
        cx,
    );
    let mut surface = div()
        .relative()
        .group(VIDEO_PLAYER_HOVER_GROUP)
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgb(theme.code_background));
    if let Some(status) = cache.import_status(block_id) {
        surface = surface.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(gpui::rgb(theme.muted))
                .child(status),
        );
    } else if can_choose_source {
        surface = surface.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(upload),
        );
    } else if let Some(image) = cache.render_image(block_id, cx) {
        surface = surface.child(render_video_frame(image, frame_width, frame_height));
    } else if let Some(poster) = poster {
        surface = surface.child(div().w(px(content_width)).h_full().child(
            crate::image_loader::RasterImageElement::new(poster, gpui::ObjectFit::Contain, px(0.0)),
        ));
    } else if let Some(status) = cache.status(block_id) {
        surface = surface.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(gpui::rgb(theme.muted))
                .child(status),
        );
    } else {
        surface = surface.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(gpui::rgb(theme.muted))
                .child("视频已保留布局，播放时加载"),
        );
    }
    let controls = controls::render_video_controls(
        block_id,
        width,
        cache.snapshot(block_id),
        cache.control_bounds.clone(),
        theme,
        view.clone(),
        false,
    );
    let drop_surface = surface
        .id(("video-drop-target", block_id))
        .rounded(px(6.0))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .opacity(video_controls_idle_opacity())
                .group_hover(VIDEO_PLAYER_HOVER_GROUP, |style| style.opacity(1.0))
                .child(controls),
        );
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .child(drop_surface)
        .child(
            div()
                .text_size(px(13.0))
                .text_color(gpui::rgb(theme.muted))
                .child(video.title.clone()),
        )
        .into_any_element()
}

/// Renders the existing playback session in a window-level layer. The host
/// window separately enters native fullscreen, so playback remains continuous
/// while the video covers editor chrome and sibling application panels.
pub(crate) fn render_fullscreen_video_overlay(
    block_id: BlockId,
    cache: &VideoPlaybackCache,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    window_width: f32,
    window_height: f32,
    cx: &mut App,
) -> Option<AnyElement> {
    let image = cache.render_image(block_id, cx);
    let dimensions = cache.dimensions(block_id);
    if image.is_none() && dimensions.is_none() && cache.status(block_id).is_none() {
        return None;
    }
    let dimensions = dimensions.unwrap_or(cditor_video::VideoDimensions {
        width: 16,
        height: 9,
    });
    let surface_width = window_width.max(1.0);
    let surface_height = window_height.max(1.0);
    let (frame_width, frame_height) = fullscreen_video_size(
        dimensions.width,
        dimensions.height,
        surface_width,
        surface_height,
    );
    let controls_bounds = controls::ControlBounds::default();
    let mut surface = div()
        .relative()
        .group(VIDEO_PLAYER_HOVER_GROUP)
        .w(px(surface_width))
        .h(px(surface_height))
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgb(theme.code_background))
        .overflow_hidden();
    if let Some(image) = image {
        surface = surface.child(render_video_frame(image, frame_width, frame_height));
    }
    let controls = controls::render_video_controls(
        block_id,
        surface_width,
        cache.snapshot(block_id),
        controls_bounds,
        theme,
        view,
        true,
    );
    surface = surface.child(
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .opacity(video_controls_idle_opacity())
            .group_hover(VIDEO_PLAYER_HOVER_GROUP, |style| style.opacity(1.0))
            .child(controls),
    );
    Some(
        window_overlay::WindowOverlay::new(
            div()
                .absolute()
                .left_0()
                .top_0()
                .w(px(surface_width))
                .h(px(surface_height))
                .bg(gpui::rgba(0x000000f5))
                .occlude()
                .child(surface),
        )
        .into_any_element(),
    )
}

impl CditorV2View {
    pub(crate) fn enter_fullscreen_video(
        &mut self,
        block_id: BlockId,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let window_was_fullscreen = window.is_fullscreen();
        if begin_fullscreen_video_state(&mut self.overlay, block_id, window_was_fullscreen) {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    pub(crate) fn exit_fullscreen_video(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if end_fullscreen_video_state(&mut self.overlay) {
            // Toggle even while AppKit is still animating the enter request.
            // macOS queues the matching exit, preventing a late transition
            // from leaving the application fullscreen after the overlay closes.
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    pub(crate) fn reconcile_fullscreen_video_window(&mut self, window: &gpui::Window) {
        reconcile_fullscreen_video_state(&mut self.overlay, window.is_fullscreen());
    }
}

fn begin_fullscreen_video_state(
    overlay: &mut crate::editor_view::OverlayUiState,
    block_id: BlockId,
    window_is_fullscreen: bool,
) -> bool {
    overlay.fullscreen_video_block_id = Some(block_id);
    overlay.fullscreen_video_requested_window = !window_is_fullscreen;
    overlay.fullscreen_video_observed_window = window_is_fullscreen;
    !window_is_fullscreen
}

fn end_fullscreen_video_state(overlay: &mut crate::editor_view::OverlayUiState) -> bool {
    let restore_window = overlay.fullscreen_video_requested_window;
    overlay.fullscreen_video_block_id = None;
    overlay.fullscreen_video_requested_window = false;
    overlay.fullscreen_video_observed_window = false;
    restore_window
}

fn reconcile_fullscreen_video_state(
    overlay: &mut crate::editor_view::OverlayUiState,
    window_is_fullscreen: bool,
) {
    if overlay.fullscreen_video_block_id.is_none() {
        overlay.fullscreen_video_requested_window = false;
        overlay.fullscreen_video_observed_window = false;
    } else if window_is_fullscreen {
        overlay.fullscreen_video_observed_window = true;
    } else if overlay.fullscreen_video_observed_window {
        // The user left native fullscreen through the operating system.
        end_fullscreen_video_state(overlay);
    }
}

fn fullscreen_video_size(
    source_width: u32,
    source_height: u32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    // Preserve aspect ratio, fit video inside viewport (letterbox/pillarbox)
    let source_aspect = source_width as f32 / source_height.max(1) as f32;
    let viewport_aspect = viewport_width / viewport_height.max(1.0);

    if !source_aspect.is_finite() || source_aspect <= 0.0 {
        // Fallback if aspect ratio is invalid
        return (viewport_width.max(1.0), viewport_height.max(1.0));
    }

    if source_aspect > viewport_aspect {
        // Video is wider than viewport, fit to width
        let width = viewport_width.max(1.0);
        let height = width / source_aspect;
        (width, height)
    } else {
        // Video is taller than viewport, fit to height
        let height = viewport_height.max(1.0);
        let width = height * source_aspect;
        (width, height)
    }
}

fn video_block_height_px(
    payload: &VideoPayload,
    decoded_dimensions: Option<cditor_video::VideoDimensions>,
    width_px: f64,
) -> f64 {
    let aspect_ratio = decoded_dimensions
        .map(|dimensions| f64::from(dimensions.width) / f64::from(dimensions.height.max(1)))
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .or_else(|| {
            payload
                .intrinsic_width
                .map(f64::from)
                .zip(payload.intrinsic_height.map(f64::from))
                .map(|(width, height)| width / height.max(1.0))
        })
        .unwrap_or(cditor_core::layout::VIDEO_DEFAULT_ASPECT_RATIO);
    let (_, height) = cditor_core::layout::video_viewport_size_px(width_px, aspect_ratio);
    height + cditor_core::layout::VIDEO_BLOCK_CHROME_HEIGHT_PX
}

const fn video_controls_idle_opacity() -> f32 {
    if VIDEO_CONTROLS_HIDE_UNTIL_HOVER {
        0.0
    } else {
        1.0
    }
}

impl CditorV2View {
    fn start_video_ticker(&mut self, block_id: BlockId, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(33))
                    .await;
                let keep_ticking = view
                    .update(cx, |view, cx| {
                        let playing = view
                            .cache
                            .video_playbacks
                            .snapshot(block_id)
                            .is_some_and(|snapshot| snapshot.playing);
                        cx.notify();
                        playing
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_surface_preserves_landscape_aspect_ratio() {
        let (width, height) = fullscreen_video_size(1920, 1080, 1200.0, 800.0);
        assert_eq!((width, height), (1200.0, 675.0));
    }

    #[test]
    fn decoded_dimensions_are_bounded_for_portrait_video() {
        let payload = VideoPayload::default();
        let height = video_block_height_px(
            &payload,
            Some(cditor_video::VideoDimensions {
                width: 720,
                height: 1280,
            }),
            640.0,
        );
        assert_eq!(
            height,
            400.0 + cditor_core::layout::VIDEO_BLOCK_CHROME_HEIGHT_PX
        );
    }

    #[test]
    fn portrait_frame_is_fitted_inside_the_complete_inline_surface() {
        let (width, height) = fullscreen_video_size(404, 720, 560.0, 400.0);
        assert!((width - 224.44444).abs() < 0.001);
        assert_eq!(height, 400.0);
    }

    #[test]
    fn payload_dimensions_remain_the_fallback_before_decode() {
        let payload = VideoPayload {
            intrinsic_width: Some(1920),
            intrinsic_height: Some(1080),
            ..Default::default()
        };
        assert_eq!(
            video_block_height_px(&payload, None, 640.0),
            315.0 + cditor_core::layout::VIDEO_BLOCK_CHROME_HEIGHT_PX
        );
    }

    #[test]
    fn video_fullscreen_restores_only_a_window_it_changed() {
        let mut overlay = crate::editor_view::OverlayUiState::default();

        assert!(begin_fullscreen_video_state(&mut overlay, 7, false));
        assert_eq!(overlay.fullscreen_video_block_id, Some(7));
        assert!(overlay.fullscreen_video_requested_window);
        assert!(!overlay.fullscreen_video_observed_window);
        assert!(end_fullscreen_video_state(&mut overlay));

        assert!(!begin_fullscreen_video_state(&mut overlay, 8, true));
        assert!(overlay.fullscreen_video_observed_window);
        assert!(!end_fullscreen_video_state(&mut overlay));
    }

    #[test]
    fn video_fullscreen_waits_for_native_entry_and_tracks_system_exit() {
        let mut overlay = crate::editor_view::OverlayUiState::default();
        begin_fullscreen_video_state(&mut overlay, 7, false);

        reconcile_fullscreen_video_state(&mut overlay, false);
        assert_eq!(overlay.fullscreen_video_block_id, Some(7));

        reconcile_fullscreen_video_state(&mut overlay, true);
        assert!(overlay.fullscreen_video_observed_window);

        reconcile_fullscreen_video_state(&mut overlay, false);
        assert!(overlay.fullscreen_video_block_id.is_none());
        assert!(!overlay.fullscreen_video_requested_window);
        assert!(!overlay.fullscreen_video_observed_window);
    }

    #[test]
    fn video_controls_do_not_add_to_reported_block_height() {
        let payload = VideoPayload {
            intrinsic_width: Some(16),
            intrinsic_height: Some(9),
            ..Default::default()
        };
        assert_eq!(
            cditor_core::layout::video_payload_block_height_px(&payload, 640.0),
            315.0 + cditor_core::layout::VIDEO_BLOCK_CHROME_HEIGHT_PX
        );
    }

    #[test]
    fn video_controls_stay_visible_without_hover_on_touch_platforms() {
        let expected = if cfg!(any(target_os = "ios", target_os = "android")) {
            1.0
        } else {
            0.0
        };
        assert_eq!(video_controls_idle_opacity(), expected);
    }

    #[test]
    fn fullscreen_surface_preserves_portrait_aspect_ratio() {
        let (width, height) = fullscreen_video_size(720, 1280, 1200.0, 800.0);
        assert_eq!((width, height), (450.0, 800.0));
    }

    #[test]
    fn video_memory_budget_rejects_a_fifth_default_session() {
        let counter = AtomicUsize::new(0);
        for _ in 0..MAX_BUDGETED_VIDEO_SESSIONS {
            assert!(try_reserve_video_memory(
                &counter,
                MAX_VIDEO_MEMORY_BYTES,
                VIDEO_SESSION_MEMORY_RESERVATION_BYTES,
            ));
        }
        assert_eq!(counter.load(Ordering::Acquire), MAX_VIDEO_MEMORY_BYTES);
        assert!(!try_reserve_video_memory(
            &counter,
            MAX_VIDEO_MEMORY_BYTES,
            VIDEO_SESSION_MEMORY_RESERVATION_BYTES,
        ));
    }

    #[test]
    fn video_memory_budget_rejects_overflow() {
        let counter = AtomicUsize::new(usize::MAX);
        assert!(!try_reserve_video_memory(&counter, usize::MAX, 1,));
    }

    #[test]
    fn reaper_overflow_drops_cheap_entries_and_bounds_heavy_pressure() {
        let overflow = Mutex::new(Vec::new());
        let entries = (0..MAX_BUDGETED_VIDEO_SESSIONS)
            .map(|block_id| VideoEntry {
                source: format!("video-{block_id}.mp4"),
                state: Arc::new(Mutex::new(VideoEntryState::Loading)),
                cancellation: cditor_video::VideoCancellationToken::default(),
                _task: None,
            })
            .chain([
                VideoEntry {
                    source: "deferred.mp4".into(),
                    state: Arc::new(Mutex::new(VideoEntryState::Deferred)),
                    cancellation: cditor_video::VideoCancellationToken::default(),
                    _task: None,
                },
                VideoEntry {
                    source: "failed.mp4".into(),
                    state: Arc::new(Mutex::new(VideoEntryState::Failed("decode".into()))),
                    cancellation: cditor_video::VideoCancellationToken::default(),
                    _task: None,
                },
            ])
            .collect();

        append_video_reaper_overflow(&overflow, entries);

        let pending = overflow.lock().unwrap();
        assert_eq!(pending.len(), MAX_BUDGETED_VIDEO_SESSIONS);
        assert!(pending.iter().all(VideoEntry::is_heavy));
    }

    #[test]
    fn reaper_overflow_hard_caps_heavy_entries_in_release() {
        let overflow = Mutex::new(Vec::new());
        let entries = (0..MAX_BUDGETED_VIDEO_SESSIONS + 2)
            .map(|block_id| VideoEntry {
                source: format!("video-{block_id}.mp4"),
                state: Arc::new(Mutex::new(VideoEntryState::Loading)),
                cancellation: cditor_video::VideoCancellationToken::default(),
                _task: None,
            })
            .collect();

        append_video_reaper_overflow(&overflow, entries);

        assert_eq!(overflow.lock().unwrap().len(), MAX_BUDGETED_VIDEO_SESSIONS);
    }

    #[test]
    fn clearing_playback_entries_cancels_inflight_loads() {
        let cache = VideoPlaybackCache::default();
        let cancellation = cditor_video::VideoCancellationToken::default();
        cache.entries.lock().unwrap().insert(
            7,
            VideoEntry {
                source: "video.mp4".into(),
                state: Arc::new(Mutex::new(VideoEntryState::Loading)),
                cancellation: cancellation.clone(),
                _task: None,
            },
        );

        assert!(cache.clear_playback_entries().is_empty());
        assert!(cancellation.is_cancelled());
        assert!(cache.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn dropping_playback_cache_cancels_inflight_loads() {
        let cancellation = cditor_video::VideoCancellationToken::default();
        let cache = VideoPlaybackCache::default();
        cache.entries.lock().unwrap().insert(
            9,
            VideoEntry {
                source: "video.mp4".into(),
                state: Arc::new(Mutex::new(VideoEntryState::Loading)),
                cancellation: cancellation.clone(),
                _task: None,
            },
        );

        drop(cache);
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn diagnostics_distinguish_loading_deferred_and_failed_entries() {
        let cache = VideoPlaybackCache::default();
        let insert = |block_id, state| {
            cache.entries.lock().unwrap().insert(
                block_id,
                VideoEntry {
                    source: format!("video-{block_id}.mp4"),
                    state: Arc::new(Mutex::new(state)),
                    cancellation: cditor_video::VideoCancellationToken::default(),
                    _task: None,
                },
            );
        };
        insert(1, VideoEntryState::Loading);
        insert(2, VideoEntryState::Deferred);
        insert(3, VideoEntryState::Failed("decode failed".into()));

        let diagnostics = cache.diagnostics();
        assert_eq!(diagnostics.tracked_blocks, 3);
        assert_eq!(diagnostics.loading_sessions, 1);
        assert_eq!(diagnostics.deferred_sessions, 1);
        assert_eq!(diagnostics.failed_sessions, 1);
        assert_eq!(diagnostics.ready_sessions, 0);
        assert_eq!(diagnostics.decoder_budget_bytes, MAX_VIDEO_MEMORY_BYTES);
        assert_eq!(
            diagnostics.max_active_sessions_per_editor,
            MAX_ACTIVE_VIDEO_SESSIONS
        );
    }

    #[test]
    fn explicit_play_request_is_remembered_before_a_session_exists() {
        let cache = VideoPlaybackCache::default();
        cache.command(42, cditor_video::VideoCommand::Play);

        assert!(cache.requested.lock().unwrap().contains(&42));
    }

    #[test]
    fn hundred_video_blocks_only_admit_the_bounded_active_window() {
        let candidates = (0..100)
            .map(|block_id| (block_id, format!("video-{block_id}.mp4")))
            .collect();
        let requested = HashSet::from([99]);

        let selected = select_active_video_candidates(candidates, &requested, 4);

        assert_eq!(selected.len(), 4);
        assert_eq!(selected[0].0, 99);
        assert_eq!(
            selected[1..].iter().map(|item| item.0).collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }
}
