# Routing latency fix: slow selections yield to live models

- **Date:** 2026-10-23
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- `src-tauri/src/models.rs`: `best_model_for_language` now considers latency. A model is *live* when `details.latency_ms` ≤ `LIVE_LATENCY_MS` (1000 ms for a ~4 s utterance). Order of choice:
  1. The selection, if live, or if it is a single-language model used for its own language (the Romanian Whisper for Romanian).
  2. An installed single-language specialist for the language.
  3. For a slow selection, the most accurate live model, provided its accuracy is within `LIVE_ACCURACY_SLACK` (0.02). Otherwise the slow selection stays.
  4. If the selection can't handle the language, the most accurate model that can, live ones first.
- `src-tauri/src/transcription.rs`: a `run` mutex in `Transcriber` serialises `ensure_loaded` and `transcribe`.
- Settings text for Smart model routing updated. Five routing unit tests added.

## Why
v0.3.17 made routing honour the selected model. The user had Whisper Turbo selected but had never actually run it, because the old routing always swapped in SenseVoice. Every dictation then took **26–34 s** (log, 2026-10-23). Re-pressing the hotkey while waiting made it worse: the second dictation found the engine checked out by the first, loaded a second Turbo copy, and both ran at once at half speed.

## Decisions & rationale
- Latency comes from the catalog (`details.latency_ms`, benchmark numbers) rather than measured History latency. It is deterministic, and this build is CPU-only. Routing on measured latency (`model_stats`) is a possible refinement for GPU builds.
- The 0.02 slack lets Parakeet V3 (0.92) replace Turbo (0.87), but not SenseVoice (0.80). The user complained about SenseVoice's accuracy, so a slow Turbo is the better fallback there.
- Specialists win for their language, per the Romanian model's author. Parakeet V3 is less accurate in Romanian (FLEURS 12.4 % vs ~9 %).
- With Auto-detect, a Whisper selection can now be routed to Parakeet V3, which covers 25 European languages. Speakers of other languages should pick their language or turn routing off.

## Follow-ups / open questions
- The installed v0.3.17 took 26 s for Turbo, against 13 s in the optimised benchmark build. Its whisper.cpp probably came from the pre-fix CI cache (see 9dddf56).
- Scaling Whisper's `audio_ctx` to the clip length could make Small/Turbo live. This needs a WER check with `bench_models`.
