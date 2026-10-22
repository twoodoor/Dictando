//! Microphone capture via `cpal`.
//!
//! Records from the selected input device, downmixes to mono as audio
//! arrives, and on stop resamples the whole take (windowed-sinc, anti-aliased)
//! to the 16 kHz mono `f32` that the transcription engine expects.
//!
//! `cpal::Stream` is `!Send` on Windows (WASAPI), so the stream is created,
//! driven, and dropped entirely on a dedicated capture thread; only `Send`
//! handles (flags, the shared sample buffer) cross thread boundaries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const TARGET_RATE: u32 = 16_000;

/// Keep capturing this long after the hotkey is released: people let go while
/// still finishing the last word.
pub const TAIL_MS: u64 = 250;

/// How long `MicReady::wait` waits for the first audio callback.
const READY_TIMEOUT_MS: u64 = 1500;

struct Capture {
    running: Arc<AtomicBool>,
    /// Mono samples at the device's native rate (`src_rate`).
    buffer: Arc<Mutex<Vec<f32>>>,
    src_rate: u32,
    handle: JoinHandle<()>,
}

/// Resolves once the microphone is actually delivering audio (or failed).
pub struct MicReady(std::sync::mpsc::Receiver<Result<u128, String>>);

impl MicReady {
    /// Block until the first audio callback arrives. `Ok(ms)` = startup time.
    pub fn wait(self) -> Result<u128, String> {
        match self.0.recv_timeout(std::time::Duration::from_millis(READY_TIMEOUT_MS)) {
            Ok(r) => r,
            Err(_) => Err(format!("microphone delivered no audio within {READY_TIMEOUT_MS} ms")),
        }
    }
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
    ///
    /// Returns immediately (the shortcut handler must not block); the returned
    /// [`MicReady`] resolves once audio is really flowing, which is when the
    /// user should be told to speak. Nothing is captured before that point.
    pub fn start(
        &self,
        microphone_id: &str,
        on_level: Option<Box<dyn Fn(f32) + Send + 'static>>,
    ) -> Result<MicReady, String> {
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

        let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(src_rate as usize * 30)));
        let running = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u128, String>>();
        let started = std::time::Instant::now();

        // Drive the (non-Send) stream on its own thread.
        let thread_buffer = buffer.clone();
        let thread_running = running.clone();
        let handle = std::thread::spawn(move || {
            let err_fn = |e| log::error!("audio stream error: {e}");
            let buf_for_cb = thread_buffer.clone();
            let last_level = Mutex::new(std::time::Instant::now());
            let first_audio = Mutex::new(Some(ready_tx.clone()));
            let push = move |samples: &[f32]| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mono = downmix_to_mono(samples, channels);
                    if let Ok(mut b) = buf_for_cb.lock() {
                        b.extend_from_slice(&mono);
                    }
                    if let Some(tx) = first_audio.lock().ok().and_then(|mut t| t.take()) {
                        let _ = tx.send(Ok(started.elapsed().as_millis()));
                    }
                    if let Some(ref cb) = on_level {
                        if let Ok(mut last) = last_level.lock() {
                            if last.elapsed() >= std::time::Duration::from_millis(25) {
                                let sum_sq: f32 = mono.iter().map(|&s| s * s).sum();
                                let rms = (sum_sq / mono.len().max(1) as f32).sqrt();
                                // Sensitive gain so typical speech RMS (0.01..0.08) scales vibrantly
                                let level = (rms * 20.0).min(1.0);
                                cb(level);
                                *last = std::time::Instant::now();
                            }
                        }
                    }
                }));
            };

            let stream = build_stream(&device, &config, sample_format, push, err_fn);
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("failed to build input stream: {e}")));
                    thread_running.store(false, Ordering::SeqCst);
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!("failed to start stream: {e}")));
                thread_running.store(false, Ordering::SeqCst);
                return;
            }
            while thread_running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // Stream dropped here, on the owning thread.
            drop(stream);
        });

        *guard = Some(Capture { running, buffer, src_rate, handle });
        Ok(MicReady(ready_rx))
    }

    /// Stop recording and return 16 kHz mono f32 samples.
    pub fn stop(&self) -> Result<Vec<f32>, String> {
        let capture = self.capture.lock().unwrap().take().ok_or("not recording")?;
        capture.running.store(false, Ordering::SeqCst);
        let _ = capture.handle.join();
        let raw = std::mem::take(&mut *capture.buffer.lock().unwrap());
        Ok(resample(&raw, capture.src_rate, TARGET_RATE))
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

/// Windowed-sinc resampler for a whole recording.
///
/// Replaces per-callback linear interpolation, which had no low-pass filter:
/// at 48 kHz → 16 kHz everything between 8 and 24 kHz (the hiss of "s", "f",
/// "th") folded back into the speech band as noise, and every ~10 ms callback
/// restarted the interpolation phase. Here a Blackman-windowed sinc low-pass
/// at 0.95 × the target Nyquist runs over the full take in one pass. The
/// kernel is precomputed per fractional phase (1 phase for 48 kHz, 160 for
/// 44.1 kHz), so the hot loop is ~100 multiply-adds per output sample.
pub(crate) fn resample(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    const ZERO_CROSSINGS: f64 = 16.0;
    let g = gcd(src_rate, dst_rate);
    let (step, phases) = ((src_rate / g) as usize, (dst_rate / g) as usize);
    let ratio = dst_rate as f64 / src_rate as f64;
    // Cutoff as a fraction of the source sample rate (1.0 = source Nyquist).
    let cutoff = 0.95 * ratio.min(1.0);
    let half_width = ZERO_CROSSINGS / cutoff; // in input samples
    let reach = half_width.ceil() as isize;
    let taps = (2 * reach + 1) as usize;

    // kernel[p * taps + j] weights input[base - reach + j] for output phase p
    // (fractional source position p / phases), normalised to unity DC gain.
    let mut kernel = vec![0.0f32; phases * taps];
    for p in 0..phases {
        let frac = p as f64 / phases as f64;
        let row = &mut kernel[p * taps..(p + 1) * taps];
        let mut sum = 0.0;
        for (j, w) in row.iter_mut().enumerate() {
            let t = (j as isize - reach) as f64 - frac;
            let v = sinc(cutoff * t) * blackman(t / half_width);
            *w = v as f32;
            sum += v;
        }
        row.iter_mut().for_each(|w| *w = (*w as f64 / sum) as f32);
    }

    let out_len = input.len() * phases / step;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i * step; // source position × phases
        let (base, p) = ((pos / phases) as isize, pos % phases);
        let row = &kernel[p * taps..(p + 1) * taps];
        let start = base - reach;
        let mut acc = 0.0f32;
        for (j, &w) in row.iter().enumerate() {
            let k = start + j as isize;
            if k >= 0 && (k as usize) < input.len() {
                acc += input[k as usize] * w;
            }
        }
        out.push(acc);
    }
    out
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Blackman window over x ∈ [-1, 1] (0 outside).
fn blackman(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        return 0.0;
    }
    let a = std::f64::consts::PI * (x + 1.0); // 0..2π
    0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        let n = (rate as f64 * secs) as usize;
        (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    /// RMS of the middle 80 % (skips the filter's edge transients).
    fn rms(x: &[f32]) -> f32 {
        let x = &x[x.len() / 10..x.len() * 9 / 10];
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn resample_keeps_speech_band() {
        for src in [44_100, 48_000] {
            let out = resample(&tone(1_000.0, src, 1.0), src, TARGET_RATE);
            assert!((out.len() as i64 - 16_000).abs() <= 1, "length {}", out.len());
            let level = rms(&out);
            assert!((level - 0.3536).abs() < 0.01, "1 kHz level {level} at {src} Hz");
        }
    }

    #[test]
    fn resample_rejects_aliases() {
        // 12 kHz is above the 8 kHz output Nyquist. Linear interpolation folded
        // it into a loud 4 kHz tone; it must be filtered out instead.
        let out = resample(&tone(12_000.0, 48_000, 1.0), 48_000, TARGET_RATE);
        assert!(rms(&out) < 0.005, "alias leaked: rms {}", rms(&out));
    }

    #[test]
    fn resample_passthrough_at_target_rate() {
        let x = tone(440.0, 16_000, 0.1);
        assert_eq!(resample(&x, 16_000, 16_000), x);
    }
}
