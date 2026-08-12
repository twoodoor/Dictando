# Start/finish sound cues (synthesized water drops)

- **Date:** 2026-08-07
- **Phase:** UX polish
- **Status:** code-complete; frontend type-checks, `rodio` resolves against the existing `cpal 0.15` with no duplicate. Not yet runtime-verified (need to run the app and listen).

## What changed
- **New `src-tauri/src/sound.rs`** — `SoundPlayer`: a dedicated audio thread owns the `rodio` output stream (which is `!Send`) for the app's lifetime and receives `Cue::{Start,Finish}` over an `mpsc` channel. Both cues are **synthesized at startup** (no asset files) as short decaying sines with a rising pitch glide — the chirp that makes a tone read as a water droplet. Start = single rising drop (C5→G5); finish = quick high tap then a lower resolving drop (a two-note "call and answer" that lands back home, so it's distinct from the start by ear).
- **`Cargo.toml`** — added `rodio = { version = "0.20", default-features = false }`. Default features off drops the (unused) audio-file decoders; we only play raw `SamplesBuffer`s. Rides on `cpal 0.15.3`, the same version used for capture → single cpal in the tree.
- **`lib.rs`** — `mod sound;`, `SoundPlayer` field on `AppState`, constructed in `setup`. Start cue fires in `begin_recording` only after the mic stream actually opens; finish cue fires at the top of `finish_recording` (on key-up, before the transcription thread spawns). Both gated on the existing `audio_feedback` setting.
- **`SettingsView.tsx`** — wired the previously-dead `audioFeedback` flag to a real "Sound cues" toggle in the Audio section (native only).

## Why
User wanted discreet, satisfying press/release feedback for push-to-talk — "so you want to hear it again." An `audio_feedback: bool` setting already existed in `settings.rs` + `bridge.ts` but was never consumed.

## Decisions & rationale
- **Rust-side playback, not the webview.** The trigger lives in Rust, the window is often hidden/minimized, and a preloaded in-memory buffer via `rodio` is lower-latency and focus-independent vs. emitting an event to JS.
- **Procedural synthesis over sample files.** Zero assets, tiny, fully offline (on-brand), and start≠finish is a couple of constants — easy to tune the "addictive" feel by ear. `rodio` plays a recorded WAV identically if we ever want to swap one in.
- **Dedicated thread owns the stream.** `rodio::OutputStream` is `!Send`; a thread + channel keeps `AppState` cleanly `Send + Sync` and makes playback fire-and-forget (best-effort — no output device = silent no-op, never a user-visible error).

## Follow-ups / open questions
- **Runtime tuning:** dial pitch/decay/amplitude by ear; current peak amp ~0.3 to stay discreet.
- **Mic bleed:** the start cue plays the instant capture opens, so it can leak into the recording. Discreet + short mitigates it; the planned VAD / leading-silence trim should remove it. The finish cue plays after `stop()`, so no bleed.
- Consider a tiny "Preview" button next to the toggle so users can audition the pair without recording.
