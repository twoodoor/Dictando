//! Microphone capture via `cpal`.
//!
//! Records from the selected input device, accumulates samples, and on stop
//! downmixes to mono and resamples to the 16 kHz mono `f32` that the
//! transcription engine expects.
//!
//! `cpal::Stream` is `!Send` on Windows (WASAPI), so the stream is created,
//! driven, and dropped entirely on a dedicated capture thread; only `Send`
//! handles (flags, the shared sample buffer) cross thread boundaries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const TARGET_RATE: u32 = 16_000;

struct Capture {
    running: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<f32>>>,
    channels: u16,
    src_rate: u32,
    handle: JoinHandle<()>,
}

/// Microphone recorder. Hold one instance in Tauri managed state.
pub struct Recorder {
    capture: Mutex<Option<Capture>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self { capture: Mutex::new(None) }
    }

    pub fn is_recording(&self) -> bool {
        self.capture.lock().unwrap().is_some()
    }

    /// List available input devices as (id, label). The id is the cpal device
    /// name, which is what `start` matches against.
    pub fn list_devices() -> Vec<(String, String)> {
        let host = cpal::default_host();
        let mut out = vec![("default".to_string(), "System Default".to_string())];
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if let Ok(name) = d.name() {
                    out.push((name.clone(), name));
                }
            }
        }
        out
    }

    /// Begin recording from `microphone_id` ("default" or a cpal device name).
    pub fn start(&self, microphone_id: &str) -> Result<(), String> {
        let mut guard = self.capture.lock().unwrap();
        if guard.is_some() {
            return Err("already recording".into());
        }

        let host = cpal::default_host();
        let device = if microphone_id == "default" || microphone_id.is_empty() {
            host.default_input_device()
        } else {
            host.input_devices()
                .ok()
                .and_then(|mut ds| ds.find(|d| d.name().map(|n| n == microphone_id).unwrap_or(false)))
                .or_else(|| host.default_input_device())
        }
        .ok_or("no input device available")?;

        let supported = device
            .default_input_config()
            .map_err(|e| format!("no default input config: {e}"))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels;
        let src_rate = config.sample_rate.0;

        let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(src_rate as usize * 4)));
        let running = Arc::new(AtomicBool::new(true));

        // Drive the (non-Send) stream on its own thread.
        let thread_buffer = buffer.clone();
        let thread_running = running.clone();
        let handle = std::thread::spawn(move || {
            let err_fn = |e| log::error!("audio stream error: {e}");
            let buf_for_cb = thread_buffer.clone();
            let push = move |samples: &[f32]| {
                if let Ok(mut b) = buf_for_cb.lock() {
                    b.extend_from_slice(samples);
                }
            };

            let stream = build_stream(&device, &config, sample_format, push, err_fn);
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("failed to build input stream: {e}");
                    thread_running.store(false, Ordering::SeqCst);
                    return;
                }
            };
            if let Err(e) = stream.play() {
                log::error!("failed to start stream: {e}");
                thread_running.store(false, Ordering::SeqCst);
                return;
            }
            while thread_running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            // Stream dropped here, on the owning thread.
            drop(stream);
        });

        *guard = Some(Capture { running, buffer, channels, src_rate, handle });
        Ok(())
    }

    /// Stop recording and return 16 kHz mono f32 samples.
    pub fn stop(&self) -> Result<Vec<f32>, String> {
        let capture = self.capture.lock().unwrap().take().ok_or("not recording")?;
        capture.running.store(false, Ordering::SeqCst);
        let _ = capture.handle.join();
        let raw = capture.buffer.lock().unwrap().clone();
        let mono = downmix_to_mono(&raw, capture.channels);
        Ok(resample_linear(&mono, capture.src_rate, TARGET_RATE))
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Build an input stream for the given sample format, converting every sample
/// to `f32` in [-1, 1] before handing it to `push`.
fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    push: impl Fn(&[f32]) + Send + 'static,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    use cpal::SampleFormat;
    match format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &_| push(data),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &_| {
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                push(&f);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &_| {
                let f: Vec<f32> =
                    data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).collect();
                push(&f);
            },
            err_fn,
            None,
        ),
        other => {
            log::error!("unsupported sample format: {other:?}");
            Err(cpal::BuildStreamError::StreamConfigNotSupported)
        }
    }
}

/// Average interleaved channels down to a single mono channel.
fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Linear-interpolation resampler (adequate for 16 kHz speech). A higher
/// quality sinc/FFT resampler (rubato) can replace this later.
fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = dst_rate as f64 / src_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
