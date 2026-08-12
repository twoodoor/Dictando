//! Discreet water-drop cues for record start / finish.
//!
//! The sounds are **synthesized** at startup — no asset files ship with the app.
//! Each cue is a short, decaying sine "drop" whose pitch glides *upward* over
//! its (very short) lifetime. That rising chirp is the acoustic signature that
//! makes a tone read as a water droplet ("bloop") rather than a flat beep.
//!
//! - **Start** — a single rising drop (C5→G5): "listening".
//! - **Finish** — two drops, high then resolving lower (G5→ then C5): "got it".
//!
//! rodio's output stream is `!Send`, so a dedicated thread owns it for the
//! app's lifetime and the rest of the app just posts play commands over a
//! channel. Playback is best-effort: if no output device is available the cues
//! are silently dropped (never an error the user sees).

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use rodio::buffer::SamplesBuffer;

const SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy)]
enum Cue {
    Start,
    Finish,
}

/// Owns the sound thread. `Send + Sync`, so it lives happily in Tauri's managed
/// `AppState`. Cheap to construct; playing a cue is a non-blocking channel send.
pub struct SoundPlayer {
    tx: Mutex<Sender<Cue>>,
}

impl SoundPlayer {
    /// Spawn the audio thread and pre-render both cues off the hot path. Never
    /// fails: if the output device can't be opened the thread exits and every
    /// later `play_*` call becomes a no-op.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Cue>();
        let start = render_start();
        let finish = render_finish();
        std::thread::Builder::new()
            .name("mumblr-sfx".into())
            .spawn(move || run(rx, start, finish))
            .ok();
        Self { tx: Mutex::new(tx) }
    }

    /// Play the record-start cue (fire-and-forget).
    pub fn play_start(&self) {
        self.send(Cue::Start);
    }

    /// Play the record-finish cue (fire-and-forget).
    pub fn play_finish(&self) {
        self.send(Cue::Finish);
    }

    fn send(&self, cue: Cue) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(cue); // Err only if the audio thread is gone; ignore.
        }
    }
}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// The audio thread: hold the output stream alive and mix cues on demand.
fn run(rx: Receiver<Cue>, start: Vec<f32>, finish: Vec<f32>) {
    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok(pair) => pair,
        Err(e) => {
            log::warn!("audio cues disabled (no output device): {e}");
            return;
        }
    };
    while let Ok(cue) = rx.recv() {
        let samples = match cue {
            Cue::Start => start.clone(),
            Cue::Finish => finish.clone(),
        };
        // play_raw mixes asynchronously and returns immediately, so overlapping
        // presses never block or cut each other off.
        if let Err(e) = handle.play_raw(SamplesBuffer::new(1, SAMPLE_RATE, samples)) {
            log::warn!("failed to play audio cue: {e}");
        }
    }
}

/// One decaying "drop": a sine gliding from `f0` up to `f1` (exponential pitch
/// sweep), shaped by a soft attack and an exponential-decay tail. A little 2nd
/// harmonic adds "plink" body. Appends samples to `out`.
fn render_drop(f0: f32, f1: f32, dur: f32, decay: f32, amp: f32, out: &mut Vec<f32>) {
    let n = (dur * SAMPLE_RATE as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SAMPLE_RATE as f32;
        let frac = t / dur;
        let freq = f0 * (f1 / f0).powf(frac); // rising chirp = the "drop" character
        phase += std::f32::consts::TAU * freq / SAMPLE_RATE as f32;
        let attack = (t / 0.004).min(1.0); // ~4 ms fade-in to avoid a click
        let env = attack * (-t * decay).exp(); // exponential decay tail
        let s = phase.sin() + 0.18 * (2.0 * phase).sin();
        out.push(s * env * amp);
    }
}

/// Start cue: a single soft rising drop (C5 → G5). Feels like an inhale.
fn render_start() -> Vec<f32> {
    let mut out = Vec::new();
    render_drop(523.25, 784.0, 0.12, 34.0, 0.25, &mut out);
    out
}

/// Finish cue: a quick high tap then a lower, resolving drop — a two-note
/// "call and answer" that lands *back home*, so it's clearly distinct from the
/// single rising start cue even with your eyes closed.
fn render_finish() -> Vec<f32> {
    let mut out = Vec::new();
    render_drop(784.0, 1046.5, 0.07, 46.0, 0.22, &mut out);
    let gap = (0.03 * SAMPLE_RATE as f32) as usize;
    out.extend(std::iter::repeat(0.0).take(gap));
    render_drop(523.25, 660.0, 0.12, 32.0, 0.26, &mut out);
    out
}
