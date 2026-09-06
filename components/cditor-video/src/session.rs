use std::{
    collections::VecDeque,
    io::{self, Read},
    path::Path,
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Deserialize;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use crate::audio::{AudioControl, AudioPlayback};
use crate::stderr::{
    STDERR_MAX_DIAGNOSTIC_BYTES, STDERR_MAX_LINE_BYTES, read_bounded_lines, truncate_utf8,
};
use crate::{
    DEFAULT_PLAYBACK_RATE, MAX_PLAYBACK_RATE, MIN_PLAYBACK_RATE, VideoCommand, VideoDimensions,
    VideoError, VideoFrame, VideoFrameStore, VideoPlaybackSnapshot, VideoSessionConfig,
    types::fit_dimensions,
};

const STDERR_RING_LINES: usize = 24;
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_MAX_STDOUT_BYTES: usize = 64 * 1024;
const INITIAL_FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const INITIAL_FRAME_POLL_INTERVAL: Duration = Duration::from_millis(2);
const VIDEO_START_WAIT_INTERVAL: Duration = Duration::from_millis(2);
const VIDEO_FRAME_TIMING_EPSILON_SECONDS: f64 = 0.002;

/// A reusable FFmpeg-backed playback session adapted from Frame's preview engine.
///
/// The session owns all child processes and worker threads. Dropping it is enough
/// to stop decoding, which is important when a virtualized Cditor block leaves
/// the render window.
pub struct VideoSession {
    config: VideoSessionConfig,
    dimensions: VideoDimensions,
    duration_seconds: Option<f64>,
    frames: VideoFrameStore,
    process: Mutex<Option<RunningVideoProcess>>,
    playback: Arc<Mutex<PlaybackState>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    has_audio: bool,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    audio_control: Arc<AudioControl>,
}

/// Cooperative cancellation for probing and decoder startup.
///
/// Dropping a thread handle does not stop its work. Viewport caches use this
/// token to terminate video processes when their block is evicted while it is
/// still loading.
#[derive(Debug, Default)]
struct VideoCancellationState {
    cancelled: AtomicBool,
    // A session has one cancellable startup wait (the asset-resolution race).
    // Keeping a single replaceable waker avoids retaining every executor waker
    // ever used to poll that future while preserving the cancellation wake-up
    // guarantee for the active waiter.
    waker: Mutex<Option<std::task::Waker>>,
}

#[derive(Clone, Debug, Default)]
pub struct VideoCancellationToken(Arc<VideoCancellationState>);

impl VideoCancellationToken {
    pub fn cancel(&self) {
        if self.0.cancelled.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(waker) = lock(&self.0.waker).take() {
            waker.wake();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }

    /// Resolves as soon as cancellation is requested. This lets callers race
    /// cancellable async asset resolution against viewport eviction without a
    /// polling timer.
    pub fn cancelled(&self) -> VideoCancellationFuture {
        VideoCancellationFuture {
            state: Arc::clone(&self.0),
            registered: false,
            waker: None,
        }
    }
}

/// A dropped losing-side waiter must unregister its waker. Otherwise a
/// successful asset-resolution race leaves an executor task waker retained by
/// the cancellation token until the whole video entry is retired.
pub struct VideoCancellationFuture {
    state: Arc<VideoCancellationState>,
    registered: bool,
    waker: Option<std::task::Waker>,
}

impl std::future::Future for VideoCancellationFuture {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let state = Arc::clone(&self.state);
        if state.cancelled.load(Ordering::SeqCst) {
            self.registered = false;
            return std::task::Poll::Ready(());
        }
        let mut waker = lock(&state.waker);
        if state.cancelled.load(Ordering::SeqCst) {
            self.registered = false;
            return std::task::Poll::Ready(());
        }
        if !waker
            .as_ref()
            .is_some_and(|current| current.will_wake(cx.waker()))
        {
            *waker = Some(cx.waker().clone());
        }
        self.waker = Some(cx.waker().clone());
        self.registered = true;
        std::task::Poll::Pending
    }
}

impl Drop for VideoCancellationFuture {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        let mut waker = lock(&self.state.waker);
        if self.waker.as_ref().is_some_and(|registered| {
            waker
                .as_ref()
                .is_some_and(|current| current.will_wake(registered))
        }) {
            waker.take();
        }
    }
}

struct RunningVideoProcess {
    child: Child,
    stdout_worker: Option<JoinHandle<()>>,
    stderr_worker: Option<JoinHandle<()>>,
    stop_requested: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    audio: Option<AudioPlayback>,
}

impl RunningVideoProcess {
    fn stop(&mut self) -> Result<(), VideoError> {
        self.stop_requested.store(true, Ordering::SeqCst);
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if let Some(mut audio) = self.audio.take() {
            let _ = audio.stop();
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(worker) = self.stdout_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.stderr_worker.take() {
            let _ = worker.join();
        }
        Ok(())
    }
}

impl Drop for RunningVideoProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Clone, Copy)]
struct PlaybackState {
    generation: u64,
    position: f64,
    playing: bool,
    ended: bool,
    started_at: Option<Instant>,
    playback_rate: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackStreamState {
    Ready,
    Waiting,
    Stale,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(default)]
    tags: Option<ProbeStreamTags>,
    #[serde(default)]
    side_data_list: Vec<ProbeSideData>,
}

#[derive(Debug, Deserialize)]
struct ProbeStreamTags {
    rotate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeSideData {
    rotation: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

impl VideoSession {
    /// Opens the source and blocks until FFmpeg publishes its first frame.
    pub fn start(config: VideoSessionConfig) -> Result<Arc<Self>, VideoError> {
        Self::start_cancellable(config, &VideoCancellationToken::default())
    }

    /// Opens the source while observing viewport-driven cancellation.
    pub fn start_cancellable(
        config: VideoSessionConfig,
        cancellation: &VideoCancellationToken,
    ) -> Result<Arc<Self>, VideoError> {
        if cancellation.is_cancelled() {
            return Err(VideoError::Cancelled);
        }
        config.validate()?;
        let metadata = probe_metadata(
            &config.source,
            config.max_width,
            config.max_height,
            Some(cancellation),
        )?;
        if cancellation.is_cancelled() {
            return Err(VideoError::Cancelled);
        }
        let dimensions = metadata.as_ref().map_or_else(
            || {
                fit_dimensions(
                    config.max_width,
                    config.max_height,
                    config.max_width,
                    config.max_height,
                )
            },
            |metadata| metadata.dimensions,
        );
        let duration_seconds = metadata.and_then(|metadata| metadata.duration_seconds);
        let has_audio = metadata.is_some_and(|metadata| metadata.has_audio);
        let session = Arc::new(Self {
            config,
            dimensions,
            duration_seconds,
            frames: VideoFrameStore::default(),
            process: Mutex::new(None),
            playback: Arc::new(Mutex::new(PlaybackState {
                generation: 0,
                position: 0.0,
                playing: false,
                ended: false,
                started_at: None,
                playback_rate: DEFAULT_PLAYBACK_RATE,
            })),
            stderr: Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_RING_LINES))),
            has_audio,
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            audio_control: Arc::new(AudioControl::default()),
        });
        session.start_process(0.0, false)?;
        if let Err(error) = session.wait_for_initial_frame(Some(cancellation)) {
            let _ = session.stop_process();
            return Err(error);
        }
        // The first frame is the paused poster. Reap the one-shot FFmpeg
        // process immediately; playback starts a fresh rate-limited process.
        session.stop_process()?;
        Ok(session)
    }

    pub const fn dimensions(&self) -> VideoDimensions {
        self.dimensions
    }

    pub const fn duration_seconds(&self) -> Option<f64> {
        self.duration_seconds
    }

    pub fn frame_store(&self) -> VideoFrameStore {
        self.frames.clone()
    }

    pub fn latest_frame(&self) -> Option<crate::LatestVideoFrame> {
        self.frames.latest()
    }

    pub fn take_latest_frame_for_presentation(&self) -> Option<crate::LatestVideoFrame> {
        self.frames.take_latest_for_presentation()
    }

    pub fn claim_latest_frame_for_presentation_after(
        &self,
        last_presented_generation: u64,
    ) -> Option<crate::VideoFrameLease> {
        self.frames
            .claim_latest_for_presentation_after(last_presented_generation)
    }

    pub fn take_latest_frame_for_presentation_after(
        &self,
        last_presented_generation: u64,
    ) -> Option<crate::LatestVideoFrame> {
        self.frames
            .take_latest_for_presentation_after(last_presented_generation)
    }

    pub fn mark_frame_presented(&self, generation: u64) {
        self.frames.mark_presented(generation);
    }

    pub fn release_presented_frame(&self, generation: u64) -> bool {
        self.frames.clear_if_generation(generation)
    }

    pub fn resident_frame_bytes(&self) -> usize {
        self.frames.resident_bytes()
    }

    pub fn stderr(&self) -> Vec<String> {
        lock(&self.stderr).iter().cloned().collect()
    }

    fn stderr_diagnostic(&self) -> String {
        truncate_utf8(
            &lock(&self.stderr)
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
            STDERR_MAX_DIAGNOSTIC_BYTES,
        )
    }

    pub const fn has_audio(&self) -> bool {
        self.has_audio
    }

    pub fn volume(&self) -> f32 {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        return self.audio_control.volume();
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        0.0
    }

    pub fn muted(&self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        return self.audio_control.muted();
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        true
    }

    pub fn snapshot(&self) -> VideoPlaybackSnapshot {
        let state = *lock(&self.playback);
        let position = if state.playing {
            state.position
                + state
                    .started_at
                    .map_or(0.0, |at| at.elapsed().as_secs_f64() * state.playback_rate)
        } else {
            state.position
        };
        // Don't clamp position to duration - let it reflect actual playback
        // Duration from metadata may be inaccurate for some videos
        VideoPlaybackSnapshot {
            position_seconds: position,
            duration_seconds: self.duration_seconds,
            playing: state.playing && !state.ended,
            ended: state.ended,
            volume: self.volume(),
            muted: self.muted(),
            playback_rate: state.playback_rate,
        }
    }

    pub fn command(self: &Arc<Self>, command: VideoCommand) -> Result<(), VideoError> {
        match command {
            VideoCommand::Play => self.play(),
            VideoCommand::Pause => self.pause(),
            VideoCommand::Seek(seconds) => self.seek(seconds),
            VideoCommand::SetVolume(volume) => self.set_volume(volume),
            VideoCommand::SetMuted(muted) => {
                self.set_muted(muted);
                Ok(())
            }
            VideoCommand::SetPlaybackRate(playback_rate) => self.set_playback_rate(playback_rate),
        }
    }

    fn set_volume(&self, volume: f32) -> Result<(), VideoError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(VideoError::InvalidInput(
                "video volume must be finite and between 0 and 1".into(),
            ));
        }
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        self.audio_control.set_volume(volume);
        Ok(())
    }

    fn set_muted(&self, muted: bool) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        self.audio_control.set_muted(muted);
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let _ = muted;
    }

    fn set_playback_rate(self: &Arc<Self>, playback_rate: f64) -> Result<(), VideoError> {
        validate_playback_rate(playback_rate)?;
        let snapshot = self.snapshot();
        if (snapshot.playback_rate - playback_rate).abs() <= f64::EPSILON {
            return Ok(());
        }
        lock(&self.playback).playback_rate = playback_rate;
        if snapshot.playing {
            self.start_process(snapshot.position_seconds, true)?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<(), VideoError> {
        self.stop_process()?;
        let position = self.snapshot().position_seconds;
        let mut state = lock(&self.playback);
        state.position = position;
        state.playing = false;
        state.started_at = None;
        Ok(())
    }

    fn play(self: &Arc<Self>) -> Result<(), VideoError> {
        let snapshot = self.snapshot();
        if snapshot.playing {
            return Ok(());
        }
        let position = if snapshot.ended {
            0.0
        } else {
            snapshot.position_seconds
        };
        self.start_process(position, true)
    }

    fn pause(&self) -> Result<(), VideoError> {
        let snapshot = self.snapshot();
        if !snapshot.playing {
            return Ok(());
        }
        self.stop_process()?;
        let mut state = lock(&self.playback);
        state.position = snapshot.position_seconds;
        state.playing = false;
        state.started_at = None;
        Ok(())
    }

    fn seek(self: &Arc<Self>, seconds: f64) -> Result<(), VideoError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(VideoError::InvalidInput(
                "seek position must be finite and non-negative".into(),
            ));
        }
        // Don't clamp seek position - metadata duration may be inaccurate
        // Let ffmpeg handle invalid seeks naturally
        let was_playing = self.snapshot().playing;
        self.start_process(seconds, was_playing)
    }

    fn start_process(
        self: &Arc<Self>,
        start_seconds: f64,
        realtime: bool,
    ) -> Result<(), VideoError> {
        self.stop_process()?;
        let (generation, playback_rate) = {
            let mut state = lock(&self.playback);
            state.generation = state.generation.saturating_add(1);
            state.position = start_seconds;
            state.playing = false;
            state.ended = false;
            state.started_at = None;
            (state.generation, state.playback_rate)
        };
        let ffmpeg = crate::ffmpeg_executable();
        let args = build_ffmpeg_args(
            &self.config.source,
            self.dimensions,
            self.config.fps,
            start_seconds,
            realtime,
            playback_rate,
        );
        let mut child = crate::media_command(ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| VideoError::Process(error.to_string()))?;
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VideoError::Process("FFmpeg stdout was unavailable".into()));
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VideoError::Process("FFmpeg stderr was unavailable".into()));
        };
        let stop_requested = Arc::new(AtomicBool::new(false));
        let mut process = RunningVideoProcess {
            child,
            stdout_worker: None,
            stderr_worker: None,
            stop_requested: Arc::clone(&stop_requested),
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            audio: None,
        };
        process.stderr_worker = Some(spawn_stderr_worker(
            stderr,
            Arc::clone(&self.stderr),
            Arc::clone(&stop_requested),
        )?);

        let dimensions = self.dimensions;
        let frames = self.frames.clone();
        let fps = self.config.fps;
        let playback = Arc::clone(&self.playback);
        let duration_seconds = self.duration_seconds;
        let stdout_stop = Arc::clone(&stop_requested);
        let timestamp_offset_us = seconds_to_micros(start_seconds);
        process.stdout_worker = Some(
            thread::Builder::new()
                .name("cditor-video-ffmpeg-stdout".into())
                .stack_size(crate::MEDIA_IO_THREAD_STACK_BYTES)
                .spawn(move || {
                    let frame_len = dimensions.width as usize * dimensions.height as usize * 4;
                    let mut bytes = vec![0; frame_len];
                    let mut index = 0_u64;
                    loop {
                        if stdout_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let timestamp_us = timestamp_offset_us
                            .saturating_add(index.saturating_mul(1_000_000) / u64::from(fps));
                        if realtime
                            && wait_for_frame_presentation_time(
                                &playback,
                                &stdout_stop,
                                generation,
                                timestamp_us as f64 / 1_000_000.0,
                            ) != PlaybackStreamState::Ready
                        {
                            break;
                        }
                        if stdout.read_exact(&mut bytes).is_err() {
                            break;
                        }
                        if frames.has_unpresented_frame() {
                            index = index.saturating_add(1);
                            continue;
                        }
                        // Keep the decoder scratch buffer reusable. A true
                        // move here would require allocating a new scratch
                        // buffer for every accepted frame because GPUI's
                        // `RenderImage` owns the upload bytes and exposes no
                        // buffer-return API. The bounded clone is therefore
                        // intentional until a stable GPU-surface API exists;
                        // it avoids unbounded allocation churn while the
                        // one-slot mailbox provides backpressure.
                        if let Ok(frame) = VideoFrame::bgra(
                            dimensions.width,
                            dimensions.height,
                            dimensions.width * 4,
                            timestamp_us,
                            bytes.clone(),
                        ) {
                            frames.publish(frame);
                        }
                        index = index.saturating_add(1);
                    }
                    if realtime && !stdout_stop.load(Ordering::SeqCst) {
                        let mut state = lock(&playback);
                        if state.generation == generation {
                            state.position = duration_seconds
                                .unwrap_or_else(|| start_seconds + index as f64 / f64::from(fps));
                            state.playing = false;
                            state.ended = true;
                            state.started_at = None;
                        }
                    }
                })
                .map_err(|error| VideoError::Process(error.to_string()))?,
        );

        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            process.audio = if realtime && self.has_audio {
                match AudioPlayback::start(
                    &self.config.source,
                    start_seconds,
                    Arc::clone(&self.audio_control),
                    Arc::clone(&self.stderr),
                    playback_rate,
                ) {
                    Ok(audio) => Some(audio),
                    Err(error) => {
                        push_stderr_line(&self.stderr, format!("audio playback disabled: {error}"));
                        None
                    }
                }
            } else {
                None
            };
        }

        *lock(&self.process) = Some(process);
        if realtime {
            let mut state = lock(&self.playback);
            if state.generation == generation {
                state.position = start_seconds;
                state.playing = true;
                state.ended = false;
                state.started_at = Some(Instant::now());
            }
        }
        Ok(())
    }

    fn wait_for_initial_frame(
        &self,
        cancellation: Option<&VideoCancellationToken>,
    ) -> Result<(), VideoError> {
        let deadline = Instant::now() + INITIAL_FRAME_TIMEOUT;
        loop {
            if cancellation.is_some_and(VideoCancellationToken::is_cancelled) {
                return Err(VideoError::Cancelled);
            }
            if self.latest_frame().is_some() {
                return Ok(());
            }
            let exited = {
                let mut process = lock(&self.process);
                process
                    .as_mut()
                    .and_then(|process| process.child.try_wait().ok().flatten())
            };
            if let Some(status) = exited {
                let diagnostics = self.stderr_diagnostic();
                return Err(VideoError::Process(format!(
                    "FFmpeg exited before producing a frame ({status}){}",
                    if diagnostics.is_empty() {
                        String::new()
                    } else {
                        format!(": {diagnostics}")
                    }
                )));
            }
            if Instant::now() >= deadline {
                let diagnostics = self.stderr_diagnostic();
                return Err(VideoError::Process(format!(
                    "timed out waiting for the first video frame{}",
                    if diagnostics.is_empty() {
                        String::new()
                    } else {
                        format!(": {diagnostics}")
                    }
                )));
            }
            thread::sleep(INITIAL_FRAME_POLL_INTERVAL);
        }
    }

    fn stop_process(&self) -> Result<(), VideoError> {
        let process = lock(&self.process).take();
        if let Some(mut process) = process {
            process.stop()?;
        }
        Ok(())
    }
}

fn validate_playback_rate(playback_rate: f64) -> Result<(), VideoError> {
    if playback_rate.is_finite() && (MIN_PLAYBACK_RATE..=MAX_PLAYBACK_RATE).contains(&playback_rate)
    {
        Ok(())
    } else {
        Err(VideoError::InvalidInput(format!(
            "video playback rate must be between {MIN_PLAYBACK_RATE} and {MAX_PLAYBACK_RATE}"
        )))
    }
}

fn frame_presentation_state(
    playback: &Arc<Mutex<PlaybackState>>,
    generation: u64,
    timestamp_seconds: f64,
) -> PlaybackStreamState {
    let state = lock(playback);
    if state.generation != generation || state.ended {
        return PlaybackStreamState::Stale;
    }
    let Some(started_at) = state.started_at.filter(|_| state.playing) else {
        return PlaybackStreamState::Waiting;
    };
    let clock_seconds = state.position + started_at.elapsed().as_secs_f64() * state.playback_rate;
    if clock_seconds + VIDEO_FRAME_TIMING_EPSILON_SECONDS >= timestamp_seconds {
        PlaybackStreamState::Ready
    } else {
        PlaybackStreamState::Waiting
    }
}

fn wait_for_frame_presentation_time(
    playback: &Arc<Mutex<PlaybackState>>,
    stop_requested: &AtomicBool,
    generation: u64,
    timestamp_seconds: f64,
) -> PlaybackStreamState {
    loop {
        if stop_requested.load(Ordering::SeqCst) {
            return PlaybackStreamState::Stale;
        }
        match frame_presentation_state(playback, generation, timestamp_seconds) {
            PlaybackStreamState::Ready => return PlaybackStreamState::Ready,
            PlaybackStreamState::Stale => return PlaybackStreamState::Stale,
            PlaybackStreamState::Waiting => thread::sleep(VIDEO_START_WAIT_INTERVAL),
        }
    }
}

impl Drop for VideoSession {
    fn drop(&mut self) {
        let _ = self.stop_process();
    }
}

#[derive(Clone, Copy)]
struct VideoMetadata {
    dimensions: VideoDimensions,
    duration_seconds: Option<f64>,
    has_audio: bool,
}

fn probe_metadata(
    source: &Path,
    max_width: u32,
    max_height: u32,
    cancellation: Option<&VideoCancellationToken>,
) -> Result<Option<VideoMetadata>, VideoError> {
    let Ok(mut child) = crate::media_command(crate::ffprobe_executable())
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height:stream_tags=rotate:stream_side_data=rotation:format=duration",
            "-of",
            "json",
        ])
        .arg(source)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Ok(None);
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(None);
    };
    // `wait_with_output` drains into an unconstrained Vec and can also deadlock
    // if a producer fills the pipe before it exits. Drain concurrently with a
    // fixed cap, and let the polling loop terminate a pathological probe.
    let output_exceeded = Arc::new(AtomicBool::new(false));
    let output_exceeded_for_reader = Arc::clone(&output_exceeded);
    let stdout_reader = thread::Builder::new()
        .name("cditor-video-ffprobe-stdout".into())
        .stack_size(crate::MEDIA_IO_THREAD_STACK_BYTES)
        .spawn(move || {
            read_bounded_probe_stdout(stdout, PROBE_MAX_STDOUT_BYTES, &output_exceeded_for_reader)
        })
        .ok();
    if stdout_reader.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(None);
    }
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut timed_out = false;
    loop {
        if cancellation.is_some_and(VideoCancellationToken::is_cancelled) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader
                .expect("reader existence checked above")
                .join();
            return Err(VideoError::Cancelled);
        }
        if output_exceeded.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let output = stdout_reader
        .expect("reader existence checked above")
        .join()
        .ok()
        .and_then(Result::ok);
    if timed_out || output_exceeded.load(Ordering::Acquire) {
        return Ok(None);
    }
    let status = child.try_wait().ok().flatten();
    if !status.is_some_and(|status| status.success()) {
        return Ok(None);
    }
    Ok(output.and_then(|output| parse_probe_output(&output, max_width, max_height)))
}

fn read_bounded_probe_stdout<R: Read>(
    mut stdout: R,
    max_bytes: usize,
    exceeded: &AtomicBool,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(max_bytes.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        let bytes_read = stdout.read(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(output);
        }
        let Some(next_len) = output.len().checked_add(bytes_read) else {
            exceeded.store(true, Ordering::Release);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffprobe output length overflow",
            ));
        };
        if next_len > max_bytes {
            exceeded.store(true, Ordering::Release);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffprobe output exceeded the diagnostic limit",
            ));
        }
        output.extend_from_slice(&buffer[..bytes_read]);
    }
}

fn parse_probe_output(bytes: &[u8], max_width: u32, max_height: u32) -> Option<VideoMetadata> {
    let probe: ProbeOutput = serde_json::from_slice(bytes).ok()?;
    let stream = probe.streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video")
            && stream.width.is_some()
            && stream.height.is_some()
    })?;
    let (source_width, source_height) = display_dimensions(stream)?;
    let dimensions = fit_dimensions(source_width, source_height, max_width, max_height);
    let duration_seconds = probe
        .format
        .and_then(|format| format.duration)
        .and_then(|duration| duration.parse::<f64>().ok())
        .filter(|duration| duration.is_finite() && *duration > 0.0);
    let has_audio = probe
        .streams
        .iter()
        .any(|stream| stream.codec_type.as_deref() == Some("audio"));
    Some(VideoMetadata {
        dimensions,
        duration_seconds,
        has_audio,
    })
}

fn display_dimensions(stream: &ProbeStream) -> Option<(u32, u32)> {
    let width = stream.width?;
    let height = stream.height?;
    let rotation = stream
        .side_data_list
        .iter()
        .find_map(|side_data| side_data.rotation)
        .or_else(|| {
            stream
                .tags
                .as_ref()
                .and_then(|tags| tags.rotate.as_deref())
                .and_then(|rotation| rotation.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    let normalized = (rotation.round() as i32).rem_euclid(360);
    if matches!(normalized, 90 | 270) {
        Some((height, width))
    } else {
        Some((width, height))
    }
}

fn build_ffmpeg_args(
    source: &Path,
    dimensions: VideoDimensions,
    fps: u32,
    start_seconds: f64,
    realtime: bool,
    playback_rate: f64,
) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
    ];
    if start_seconds > 0.0 {
        args.extend(["-ss".into(), format!("{start_seconds:.3}")]);
    }
    if realtime {
        args.extend(["-readrate".into(), format_playback_rate_arg(playback_rate)]);
    }
    args.extend([
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-vf".into(),
        format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
            dimensions.width, dimensions.height, dimensions.width, dimensions.height
        ),
        "-r".into(),
        fps.to_string(),
        "-an".into(),
        "-sn".into(),
        "-dn".into(),
        "-pix_fmt".into(),
        "bgra".into(),
        "-f".into(),
        "rawvideo".into(),
    ]);
    if !realtime {
        args.extend(["-frames:v".into(), "1".into()]);
    }
    args.push("pipe:1".into());
    args
}

fn format_playback_rate_arg(playback_rate: f64) -> String {
    format!("{playback_rate:.3}")
}

fn spawn_stderr_worker(
    stderr: impl Read + Send + 'static,
    lines: Arc<Mutex<VecDeque<String>>>,
    stop_requested: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, VideoError> {
    thread::Builder::new()
        .name("cditor-video-ffmpeg-stderr".into())
        .stack_size(crate::MEDIA_IO_THREAD_STACK_BYTES)
        .spawn(move || {
            let _ = read_bounded_lines(stderr, &stop_requested, |line| {
                push_stderr_line(&lines, line);
            });
        })
        .map_err(|error| VideoError::Process(error.to_string()))
}

fn push_stderr_line(lines: &Mutex<VecDeque<String>>, line: String) {
    let mut lines = lock(lines);
    if lines.len() == STDERR_RING_LINES {
        lines.pop_front();
    }
    lines.push_back(truncate_utf8(&line, STDERR_MAX_LINE_BYTES));
}

fn seconds_to_micros(seconds: f64) -> u64 {
    if seconds <= 0.0 {
        0
    } else {
        (seconds * 1_000_000.0).round() as u64
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::io::Cursor;
    use std::sync::atomic::AtomicUsize;
    use std::task::{Context, Wake};

    struct WakeCounter(AtomicUsize);

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn cancelled_start_exits_before_validating_or_spawning() {
        let cancellation = VideoCancellationToken::default();
        cancellation.cancel();
        let result = VideoSession::start_cancellable(VideoSessionConfig::default(), &cancellation);
        assert!(matches!(result, Err(VideoError::Cancelled)));
    }

    #[test]
    fn cancellation_future_is_woken_exactly_once() {
        let cancellation = VideoCancellationToken::default();
        let wake_counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let waker = std::task::Waker::from(Arc::clone(&wake_counter));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(cancellation.cancelled());

        assert!(future.as_mut().poll(&mut context).is_pending());
        cancellation.cancel();
        cancellation.cancel();
        assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
        assert!(future.as_mut().poll(&mut context).is_ready());
    }

    #[test]
    fn ffprobe_stdout_reader_rejects_output_beyond_the_fixed_limit() {
        let exceeded = AtomicBool::new(false);
        let input = vec![b'x'; PROBE_MAX_STDOUT_BYTES + 1];

        assert!(
            read_bounded_probe_stdout(Cursor::new(input), PROBE_MAX_STDOUT_BYTES, &exceeded)
                .is_err()
        );
        assert!(exceeded.load(Ordering::Acquire));
    }

    #[test]
    fn ffprobe_stdout_reader_keeps_valid_small_output() {
        let exceeded = AtomicBool::new(false);
        let input = br#"{"streams":[]}"#;

        assert_eq!(
            read_bounded_probe_stdout(Cursor::new(input), PROBE_MAX_STDOUT_BYTES, &exceeded)
                .unwrap(),
            input
        );
        assert!(!exceeded.load(Ordering::Acquire));
    }

    use std::path::PathBuf;

    #[test]
    fn paused_ffmpeg_plan_limits_frames_before_output() {
        let args = build_ffmpeg_args(
            &PathBuf::from("sample.mp4"),
            VideoDimensions {
                width: 640,
                height: 360,
            },
            30,
            1.25,
            false,
            DEFAULT_PLAYBACK_RATE,
        );
        let frame_limit = args.iter().position(|arg| arg == "-frames:v").unwrap();
        let output = args.iter().position(|arg| arg == "pipe:1").unwrap();
        assert!(frame_limit < output);
        assert_eq!(&args[frame_limit + 1], "1");
        assert!(args.windows(2).any(|args| args == ["-ss", "1.250"]));
        assert!(!args.iter().any(|arg| arg == "-readrate"));
    }

    #[test]
    fn realtime_ffmpeg_plan_is_rate_limited_without_frame_limit() {
        let args = build_ffmpeg_args(
            &PathBuf::from("sample.mp4"),
            VideoDimensions {
                width: 640,
                height: 360,
            },
            30,
            0.0,
            true,
            1.5,
        );
        assert!(args.windows(2).any(|args| args == ["-readrate", "1.500"]));
        assert!(!args.iter().any(|arg| arg == "-frames:v"));
        assert_eq!(args.last().map(String::as_str), Some("pipe:1"));
    }

    fn playback_state(
        generation: u64,
        position: f64,
        playing: bool,
        started_at: Option<Instant>,
    ) -> Arc<Mutex<PlaybackState>> {
        Arc::new(Mutex::new(PlaybackState {
            generation,
            position,
            playing,
            ended: false,
            started_at,
            playback_rate: DEFAULT_PLAYBACK_RATE,
        }))
    }

    #[test]
    fn playback_clock_advances_media_time_at_selected_rate() {
        let started_at = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .expect("test timestamp should be representable");
        let playback = playback_state(1, 10.0, true, Some(started_at));
        lock(&playback).playback_rate = 2.0;

        assert_eq!(
            frame_presentation_state(&playback, 1, 10.15),
            PlaybackStreamState::Ready
        );
        assert_eq!(
            frame_presentation_state(&playback, 1, 10.3),
            PlaybackStreamState::Waiting
        );
    }

    #[test]
    fn playback_rate_rejects_non_finite_and_out_of_range_values() {
        assert!(validate_playback_rate(f64::NAN).is_err());
        assert!(validate_playback_rate(f64::INFINITY).is_err());
        assert!(validate_playback_rate(0.25).is_err());
        assert!(validate_playback_rate(3.0).is_err());
        assert!(validate_playback_rate(MIN_PLAYBACK_RATE).is_ok());
        assert!(validate_playback_rate(MAX_PLAYBACK_RATE).is_ok());
    }

    #[test]
    fn frame_presentation_waits_until_playback_clock_starts() {
        let playback = playback_state(1, 2.5, false, None);

        assert_eq!(
            frame_presentation_state(&playback, 1, 2.5),
            PlaybackStreamState::Waiting
        );
    }

    #[test]
    fn frame_presentation_waits_for_future_timestamp() {
        let playback = playback_state(1, 10.0, true, Some(Instant::now()));

        assert_eq!(
            frame_presentation_state(&playback, 1, 10.5),
            PlaybackStreamState::Waiting
        );
    }

    #[test]
    fn frame_presentation_allows_due_timestamp() {
        let started_at = Instant::now()
            .checked_sub(Duration::from_millis(40))
            .expect("test timestamp should be representable");
        let playback = playback_state(1, 10.0, true, Some(started_at));

        assert_eq!(
            frame_presentation_state(&playback, 1, 10.033),
            PlaybackStreamState::Ready
        );
    }

    #[test]
    fn frame_presentation_rejects_stale_generation() {
        let playback = playback_state(2, 0.0, true, Some(Instant::now()));

        assert_eq!(
            frame_presentation_state(&playback, 1, 0.0),
            PlaybackStreamState::Stale
        );
    }

    #[test]
    fn frame_presentation_wait_exits_when_stop_is_requested() {
        let playback = playback_state(1, 0.0, false, None);
        let stop_requested = AtomicBool::new(true);

        assert_eq!(
            wait_for_frame_presentation_time(&playback, &stop_requested, 1, 1.0),
            PlaybackStreamState::Stale
        );
    }

    #[test]
    fn parses_ffprobe_dimensions_and_duration() {
        let metadata = parse_probe_output(
            br#"{"streams":[{"codec_type":"video","width":1920,"height":1080},{"codec_type":"audio"}],"format":{"duration":"12.500000"}}"#,
            1280,
            720,
        )
        .unwrap();
        assert_eq!(
            metadata.dimensions,
            VideoDimensions {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(metadata.duration_seconds, Some(12.5));
        assert!(metadata.has_audio);
    }

    #[test]
    fn parses_phone_rotation_as_portrait_dimensions() {
        let metadata = parse_probe_output(
            br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}],"format":{"duration":"3.0"}}"#,
            1280,
            720,
        )
        .unwrap();
        assert_eq!(
            metadata.dimensions,
            VideoDimensions {
                width: 404,
                height: 720
            }
        );
    }

    #[test]
    fn seek_timestamp_keeps_source_offset() {
        assert_eq!(seconds_to_micros(2.5), 2_500_000);
    }

    #[test]
    fn stderr_lines_are_bounded_without_splitting_utf8() {
        let line = "汉".repeat(STDERR_MAX_LINE_BYTES);
        let mutex = Mutex::new(VecDeque::new());
        push_stderr_line(&mutex, line);
        let mut lines = lock(&mutex).clone();

        let output = lines.pop_front().unwrap();
        assert!(output.len() <= STDERR_MAX_LINE_BYTES);
        assert!(output.ends_with("..."));
        assert!(output.is_char_boundary(output.len()));
    }

    #[test]
    fn stderr_ring_keeps_a_fixed_number_of_bounded_lines() {
        let mutex = Mutex::new(VecDeque::new());
        for _ in 0..(STDERR_RING_LINES + 4) {
            push_stderr_line(&mutex, "x".repeat(STDERR_MAX_LINE_BYTES + 100));
        }

        let lines = lock(&mutex);
        assert_eq!(lines.len(), STDERR_RING_LINES);
        assert!(lines.iter().all(|line| line.len() <= STDERR_MAX_LINE_BYTES));
    }

    #[test]
    fn bundled_ffmpeg_generates_and_decodes_a_real_video_frame_when_available() {
        let ffmpeg = crate::ffmpeg_executable();
        if crate::media_command(&ffmpeg)
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let source =
            std::env::temp_dir().join(format!("cditor-video-smoke-{}.mp4", std::process::id()));
        let generated = crate::media_command(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x48:r=10:d=0.8",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&source)
            .status()
            .expect("bundled FFmpeg should start");
        assert!(generated.success());

        let session = VideoSession::start(VideoSessionConfig {
            source: source.clone(),
            max_width: 64,
            max_height: 48,
            fps: 10,
        })
        .expect("generated video should decode");
        assert_eq!(
            session.dimensions(),
            VideoDimensions {
                width: 64,
                height: 48
            }
        );
        assert!(session.latest_frame().is_some());
        assert!(
            lock(&session.process).is_none(),
            "poster decoder must be reaped"
        );
        session
            .command(VideoCommand::Play)
            .expect("playback decoder should start");
        assert!(lock(&session.process).is_some());
        let playback_started_at = Instant::now();
        while !session.snapshot().ended && playback_started_at.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(10));
        }
        let normal_elapsed = playback_started_at.elapsed();
        assert!(
            session.snapshot().ended,
            "playback should reach end of stream"
        );
        assert!(
            normal_elapsed >= Duration::from_millis(650),
            "real-time playback must not consume an 800ms stream early"
        );

        session
            .command(VideoCommand::SetPlaybackRate(2.0))
            .expect("2x playback rate should be accepted");
        session
            .command(VideoCommand::Play)
            .expect("ended playback should restart at 2x");
        let fast_started_at = Instant::now();
        while !session.snapshot().ended && fast_started_at.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        let fast_elapsed = fast_started_at.elapsed();
        assert!(session.snapshot().ended, "2x playback should reach end");
        assert!(
            fast_elapsed >= Duration::from_millis(300),
            "2x playback must still honor the media clock"
        );
        assert!(
            fast_elapsed < normal_elapsed.saturating_sub(Duration::from_millis(150)),
            "2x playback should complete materially faster than 1x: 1x={normal_elapsed:?}, 2x={fast_elapsed:?}"
        );
        session.stop().expect("playback decoder should stop");
        assert!(lock(&session.process).is_none());
        drop(session);
        let _ = std::fs::remove_file(source);
    }
}
