# Dictation accuracy overhaul: routing, capture, resampling, Whisper build

- **Date:** 2026-10-22
- **Phase:** post-Phase A polish
- **Status:** done (needs a release build to reach the installed app)

## What changed
- **Model routing** (`models.rs`, `lib.rs`): `best_model_for_language` now takes the selected model and keeps it whenever it is installed and supports the language. Otherwise it falls back to the **most accurate** compatible model. It used to pick the **fastest** one. The Settings text for this option is updated, and Whisper Medium/Large are excluded because `load_engine` rejects them.
- **Parakeet v3 catalog entry**: it listed only `en`. It now lists the 25 European languages from the NVIDIA model card (including Romanian, French, German and Spanish).
- **Capture start** (`audio.rs`, `lib.rs`): `Recorder::start` returns a `MicReady` that resolves on the first audio callback. The overlay and start chime wait for it, so "talk now" means the mic is really capturing. A dead or muted mic now shows an error toast instead of silently recording nothing. The startup time is logged as `microphone live after N ms`.
- **Capture end**: recording continues for `TAIL_MS` = 250 ms after key-up, and the finish chime plays after the tail, so the mic doesn't record it.
- **Resampling**: per-callback linear interpolation is replaced with one Blackman-windowed sinc pass over the whole take, with precomputed polyphase kernels. The old path had no anti-alias filter (8–24 kHz folded into the speech band at 48 kHz) and restarted its phase every ~10 ms callback. Unit tests cover band level, alias rejection and passthrough.
- **Whisper build** (`.cargo/config.toml`, new): whisper.cpp/ggml had been compiled **without optimisation** (`/Od`) in every build, release included. With MSVC and the Visual Studio generator, the `cmake` crate overwrites `CMAKE_<LANG>_FLAGS_<CONFIG>` with cc-rs flags that have no `/O2`. Setting those variables through whisper-rs-sys's `CMAKE_*` env passthrough restores `/O2`.
- **Whisper decoding** (`transcription.rs`): the Dictionary is passed as Whisper's `initial_prompt`. The timeout went from 15 s to 60 s. An inference error no longer unloads the model.
- **Hallucination filter**: "Thank you." / "You" are discarded only for Whisper and only when the clip has no speech energy (`has_speech`), so a dictated "Thank you." survives. An echoed prompt is discarded too.
- **Repetition removal** (`ai.rs`): grammatical doubles are kept ("that that", "had had", "is is", "very very", "bye bye", …), and repeats across a sentence break ("No. No.") are left alone.
- **Gemini** (`ai.rs`): tries `gemini-3.5-flash-lite` → `gemini-3.1-flash-lite` → `gemini-2.5-flash-lite`, moving on only after an HTTP 404. The key goes in the `x-goog-api-key` header instead of the URL. A failed cloud pass now shows a toast instead of only a log line.
- **Benchmark** (`transcription::tests::bench_models`, ignored by default): WER and latency for every installed model over WAV clips. See CLAUDE.md for how to run it.

## Why
The user reported missing and misspelled words, and "like" disappearing mid-sentence. Evidence from the machine:
- The log showed **all 49 dictations since 2026-10-15 ran on SenseVoice**, although Whisper Turbo was selected and the language was English. Smart routing (on by default) always picked the fastest installed model. *Remove fillers* was off and no cloud provider was set, so the lost words came from the model and capture, not from cleanup.
- Every cloud pass had failed with `Gemini HTTP 404`: `gemini-2.5-flash-lite` is now restricted to keys that used it before. Only the log recorded this, so the user's custom instructions never ran.
- Whisper Small and Turbo **timed out at 15 s on a 4 s clip** in a release build on a Ryzen 9 5950X, because of the unoptimised C++ build. Fixing routing alone would have broken dictation for anyone who selected Whisper.

## Benchmark (Windows TTS, David + Zira, 12 sentences, 24 clips, 294 words, release build, Ryzen 9 5950X)

| Model | WER | Latency / utterance |
|---|---|---|
| SenseVoice (what actually ran) | 5.4 % | 0.13 s |
| Parakeet v2 / v3 | 3.4 % | 0.33 s |
| Whisper Small | 3.4 % (2.0 % with Dictionary prompt) | 2.6 s |
| Whisper Turbo | 3.4 % (2.7 % with Dictionary prompt) | 13 s |

Before the build fix, both Whisper models timed out on every clip. Most of the remaining errors are "three thirty" written as "3:30" (counted as 2 errors) and Dictionary names (Athmo, Zeea, Tauri). Every model transcribed "look like", "as well" and "went well" correctly. TTS speech is cleaner than a real voice, so real-world gaps between models are larger, not smaller.

## Decisions & rationale
- Routing keeps the selected model instead of being turned off, so a selection that can't handle the language (e.g. an English-only model with Romanian) still falls back to something usable.
- A custom resampler (~60 lines, tested) instead of adding `rubato`: no new dependency, and the whole take is resampled once on stop. 20 s of 48 kHz audio takes a few ms.
- The mic stream is not kept open between dictations (no pre-roll). That would capture speech started before the key press, but it keeps the OS microphone indicator on permanently. Waiting for real audio before the cue gets most of the benefit.
- The model choice is left to the user. On CPU, Parakeet v3 is the recommended pick: same benchmark accuracy as Turbo, ~40× faster.

## Follow-ups / open questions
- **Dictionary for Parakeet/SenseVoice**: ONNX engines have no prompt input, so names still come out as "Ithmo"/"Zia". Proposed: per-term "sounds like" aliases in the Dictionary (deterministic replacement), or phonetic matching guarded by an English word list.
- **Whisper speed**: scale `audio_ctx` to clip length (the encoder currently always processes 30 s), try flash-attention, consider a Vulkan/GPU build.
- **History**: store the raw transcript next to the cleaned text, so "who dropped the word" (model vs cleanup vs cloud) can be answered from History.
- Optional always-on mic with a ~300 ms pre-roll buffer.
- Run the benchmark on real recordings of the user's voice (and Romanian) to confirm the model recommendation.
