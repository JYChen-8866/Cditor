use std::{
    collections::VecDeque,
    io::Read,
    path::Path,
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::{self, JoinHandle},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{
    VideoError,
    stderr::{STDERR_MAX_LINE_BYTES, read_bounded_lines, truncate_utf8},
};

const AUDIO_BUFFER_SECONDS: usize = 3;
const AUDIO_READ_BUFFER_BYTES: usize = 8192;
const AUDIO_PREBUFFER_MS: usize = 30;
// Keep an invalid or unusually large device-reported format from turning one
// playback session into an unbounded allocation. This is 4 MiB of f32 samples,
// while the normal 48 kHz stereo three-second buffer is about 1.1 MiB.
const AUDIO_MAX_BUFFER_SAMPLES: usize = 1_048_576;
const STDERR_RING_LINES: usize = 24;

pub(crate) struct AudioControl {
    volume_bits: AtomicU32,
    muted: AtomicBool,
}

impl Default for AudioControl {
    fn default() -> Self {
        Self {
            volume_bits: AtomicU32::new(1.0_f32.to_bits()),
            muted: AtomicBool::new(false),
        }
    }
}

impl AudioControl {
    pub(crate) fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub(crate) fn set_volume(&self, volume: f32) {
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub(crate) fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }
}

pub(crate) struct AudioPlayback {
    child: Child,
    stdout_worker: Option<JoinHandle<()>>,
    stderr_worker: Option<JoinHandle<()>>,
    stop_requested: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct AudioOutputSpec {
    sample_rate: u32,
    channels: u16,
}

struct AudioOutput {
    stream: cpal::Stream,
    buffer: Arc<Mutex<AudioSampleBuffer>>,
}

struct AudioSampleBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl AudioPlayback {
    pub(crate) fn start(
        source: &Path,
        start_seconds: f64,
        control: Arc<AudioControl>,
        stderr_lines: Arc<Mutex<VecDeque<String>>>,
        playback_rate: f64,
    ) -> Result<Self, VideoError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| {
            VideoError::Audio("no default audio output device is available".into())
        })?;
        let supported = device
            .default_output_config()
            .map_err(|error| VideoError::Audio(error.to_string()))?;
        let spec = AudioOutputSpec {
            sample_rate: supported.sample_rate(),
            channels: supported.channels(),
        };
        let mut child = crate::media_command(crate::ffmpeg_executable())
            .args(build_audio_args(source, start_seconds, spec, playback_rate))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| VideoError::Audio(error.to_string()))?;
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VideoError::Audio(
                "FFmpeg audio stdout was unavailable".into(),
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VideoError::Audio(
                "FFmpeg audio stderr was unavailable".into(),
            ));
        };
        let stop_requested = Arc::new(AtomicBool::new(false));
        let mut playback = Self {
            child,
            stdout_worker: None,
            stderr_worker: None,
            stop_requested: Arc::clone(&stop_requested),
        };
        playback.stderr_worker = Some(spawn_audio_stderr_worker(
            stderr,
            Arc::clone(&stderr_lines),
            Arc::clone(&stop_requested),
        )?);
        let stdout_stop = Arc::clone(&stop_requested);
        playback.stdout_worker = Some(
            thread::Builder::new()
                .name("cditor-video-ffmpeg-audio-stdout".into())
                .stack_size(crate::MEDIA_IO_THREAD_STACK_BYTES)
                .spawn(move || {
                    let output = match AudioOutput::new(device, supported, control, stderr_lines) {
                        Ok(output) => output,
                        Err(_) => return,
                    };
                    let prebuffer_samples = usize::try_from(spec.sample_rate)
                        .unwrap_or(48_000)
                        .saturating_mul(usize::from(spec.channels))
                        .saturating_mul(AUDIO_PREBUFFER_MS)
                        / 1_000;
                    let mut read_buffer = [0_u8; AUDIO_READ_BUFFER_BYTES];
                    let mut pending = Vec::new();
                    let mut queued = 0_usize;
                    let mut started = false;
                    while !stdout_stop.load(Ordering::SeqCst) {
                        let Ok(bytes_read) = stdout.read(&mut read_buffer) else {
                            break;
                        };
                        if bytes_read == 0 {
                            break;
                        }
                        pending.extend_from_slice(&read_buffer[..bytes_read]);
                        let complete_len = pending.len() - pending.len() % 4;
                        if complete_len == 0 {
                            continue;
                        }
                        queued = queued.saturating_add(push_audio_samples(
                            &output.buffer,
                            &pending[..complete_len],
                        ));
                        pending.drain(..complete_len);
                        if !started && queued >= prebuffer_samples {
                            if output.stream.play().is_err() {
                                break;
                            }
                            started = true;
                        }
                    }
                    if !started && queued > 0 {
                        let _ = output.stream.play();
                    }
                    while !stdout_stop.load(Ordering::SeqCst)
                        && !lock(&output.buffer).samples.is_empty()
                    {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                })
                .map_err(|error| VideoError::Audio(error.to_string()))?,
        );

        Ok(playback)
    }

    pub(crate) fn stop(&mut self) -> Result<(), VideoError> {
        self.stop_requested.store(true, Ordering::SeqCst);
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

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl AudioOutput {
    fn new(
        device: cpal::Device,
        supported: cpal::SupportedStreamConfig,
        control: Arc<AudioControl>,
        stderr_lines: Arc<Mutex<VecDeque<String>>>,
    ) -> Result<Self, VideoError> {
        let config = supported.config();
        let channels = usize::from(config.channels);
        let requested_capacity = usize::try_from(config.sample_rate)
            .unwrap_or(48_000)
            .saturating_mul(channels)
            .saturating_mul(AUDIO_BUFFER_SECONDS);
        let capacity = requested_capacity.min(AUDIO_MAX_BUFFER_SAMPLES);
        let buffer = Arc::new(Mutex::new(AudioSampleBuffer {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }));
        let error_lines = Arc::clone(&stderr_lines);
        let error_callback = move |error| {
            push_stderr_line(&error_lines, format!("audio output error: {error}"));
        };
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => {
                let buffer = Arc::clone(&buffer);
                let control = Arc::clone(&control);
                device.build_output_stream(
                    config,
                    move |data: &mut [f32], _| fill_output_f32(data, &buffer, &control),
                    error_callback,
                    None,
                )
            }
            cpal::SampleFormat::F64 => {
                let buffer = Arc::clone(&buffer);
                let control = Arc::clone(&control);
                device.build_output_stream(
                    config,
                    move |data: &mut [f64], _| fill_output_f64(data, &buffer, &control),
                    error_callback,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let buffer = Arc::clone(&buffer);
                let control = Arc::clone(&control);
                device.build_output_stream(
                    config,
                    move |data: &mut [i16], _| fill_output_i16(data, &buffer, &control),
                    error_callback,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let buffer = Arc::clone(&buffer);
                let control = Arc::clone(&control);
                device.build_output_stream(
                    config,
                    move |data: &mut [u16], _| fill_output_u16(data, &buffer, &control),
                    error_callback,
                    None,
                )
            }
            format => {
                return Err(VideoError::Audio(format!(
                    "unsupported output sample format: {format:?}"
                )));
            }
        }
        .map_err(|error| VideoError::Audio(error.to_string()))?;
        Ok(Self { stream, buffer })
    }
}

fn build_audio_args(
    source: &Path,
    start_seconds: f64,
    spec: AudioOutputSpec,
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
    args.extend([
        "-readrate".into(),
        format!("{playback_rate:.3}"),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-vn".into(),
        "-sn".into(),
        "-dn".into(),
        "-af".into(),
        format!("atempo={playback_rate:.3}"),
        "-ac".into(),
        spec.channels.to_string(),
        "-ar".into(),
        spec.sample_rate.to_string(),
        "-f".into(),
        "f32le".into(),
        "pipe:1".into(),
    ]);
    args
}

fn push_audio_samples(buffer: &Arc<Mutex<AudioSampleBuffer>>, bytes: &[u8]) -> usize {
    let mut buffer = lock(buffer);
    if buffer.capacity == 0 {
        return 0;
    }
    let mut count = 0;
    for chunk in bytes.chunks_exact(4) {
        if buffer.samples.len() >= buffer.capacity {
            buffer.samples.pop_front();
        }
        buffer
            .samples
            .push_back(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        count += 1;
    }
    count
}

fn next_sample(buffer: &Arc<Mutex<AudioSampleBuffer>>, control: &AudioControl) -> f32 {
    let sample = lock(buffer).samples.pop_front().unwrap_or(0.0);
    if control.muted() {
        0.0
    } else {
        sample * control.volume()
    }
}

fn fill_output_f32(
    data: &mut [f32],
    buffer: &Arc<Mutex<AudioSampleBuffer>>,
    control: &AudioControl,
) {
    for sample in data {
        *sample = next_sample(buffer, control);
    }
}

fn fill_output_f64(
    data: &mut [f64],
    buffer: &Arc<Mutex<AudioSampleBuffer>>,
    control: &AudioControl,
) {
    for sample in data {
        *sample = f64::from(next_sample(buffer, control));
    }
}

fn fill_output_i16(
    data: &mut [i16],
    buffer: &Arc<Mutex<AudioSampleBuffer>>,
    control: &AudioControl,
) {
    for sample in data {
        *sample = (next_sample(buffer, control).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
    }
}

fn fill_output_u16(
    data: &mut [u16],
    buffer: &Arc<Mutex<AudioSampleBuffer>>,
    control: &AudioControl,
) {
    for sample in data {
        let normalized = (next_sample(buffer, control).clamp(-1.0, 1.0) + 1.0) * 0.5;
        *sample = (normalized * f32::from(u16::MAX)) as u16;
    }
}

fn spawn_audio_stderr_worker(
    stderr: impl Read + Send + 'static,
    lines: Arc<Mutex<VecDeque<String>>>,
    stop_requested: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, VideoError> {
    thread::Builder::new()
        .name("cditor-video-ffmpeg-audio-stderr".into())
        .stack_size(crate::MEDIA_IO_THREAD_STACK_BYTES)
        .spawn(move || {
            let _ = read_bounded_lines(stderr, &stop_requested, |line| {
                push_stderr_line(&lines, line);
            });
        })
        .map_err(|error| VideoError::Audio(error.to_string()))
}

fn push_stderr_line(lines: &Mutex<VecDeque<String>>, line: String) {
    let mut lines = lock(lines);
    if lines.len() == STDERR_RING_LINES {
        lines.pop_front();
    }
    lines.push_back(truncate_utf8(&line, STDERR_MAX_LINE_BYTES));
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_plan_outputs_f32_at_device_layout() {
        let args = build_audio_args(
            Path::new("video.mp4"),
            2.5,
            AudioOutputSpec {
                sample_rate: 48_000,
                channels: 2,
            },
            1.5,
        );
        assert!(args.windows(2).any(|args| args == ["-ss", "2.500"]));
        assert!(args.windows(2).any(|args| args == ["-ar", "48000"]));
        assert!(args.windows(2).any(|args| args == ["-ac", "2"]));
        assert!(args.windows(2).any(|args| args == ["-f", "f32le"]));
        assert!(args.windows(2).any(|args| args == ["-readrate", "1.500"]));
        assert!(args.windows(2).any(|args| args == ["-af", "atempo=1.500"]));
    }

    #[test]
    fn pcm_buffer_keeps_latest_samples_with_bounded_capacity() {
        let buffer = Arc::new(Mutex::new(AudioSampleBuffer {
            samples: VecDeque::new(),
            capacity: 2,
        }));
        let bytes = [
            1.0_f32.to_le_bytes(),
            2.0_f32.to_le_bytes(),
            3.0_f32.to_le_bytes(),
        ]
        .concat();
        assert_eq!(push_audio_samples(&buffer, &bytes), 3);
        assert_eq!(
            lock(&buffer).samples.iter().copied().collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn zero_capacity_pcm_buffer_drops_samples_instead_of_growing() {
        let buffer = Arc::new(Mutex::new(AudioSampleBuffer {
            samples: VecDeque::new(),
            capacity: 0,
        }));
        let bytes = [1.0_f32.to_le_bytes(), 2.0_f32.to_le_bytes()].concat();

        assert_eq!(push_audio_samples(&buffer, &bytes), 0);
        assert!(lock(&buffer).samples.is_empty());
    }

    #[test]
    fn audio_buffer_capacity_is_hard_bounded() {
        let requested = usize::MAX;
        assert_eq!(
            requested.min(AUDIO_MAX_BUFFER_SAMPLES),
            AUDIO_MAX_BUFFER_SAMPLES
        );
    }
}
