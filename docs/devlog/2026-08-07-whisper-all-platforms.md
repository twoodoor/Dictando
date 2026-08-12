# OpenAI Whisper on all platforms (hybrid engine everywhere)

- **Date:** 2026-08-07
- **Phase:** Engine follow-up
- **Status:** code-complete; **Windows build verified** (`cargo build` exit 0, whisper.cpp compiled + linked with ONNX). Not yet runtime-tested (download + dictate with a Whisper model).

## Context
The catalog already carried six OpenAI Whisper GGML models (`whisper-tiny` … `whisper-turbo`, `models.rs`), but they were only *loadable on Intel macOS*: whisper.cpp was compiled only for `cfg(all(macos, x86_64))` (see [2026-06-17](./2026-06-17-intel-mac-hybrid-engine.md)), and `list()` filtered the catalog down to the single format the compiled engine could load. On Windows / Apple-Silicon / Linux the engine was ONNX-only and `transcribe-rs` 0.3 has **no ONNX Whisper** module — so the Whisper entries were hidden and unusable there.

User wants Whisper (notably Whisper Medium) available on Windows, and it working on macOS is a bonus.

## What changed
- **`Cargo.toml`**: the non-Intel-mac target now builds `transcribe-rs` with `features = ["onnx", "whisper-cpp"]` (was `["onnx"]`). Intel-mac stays whisper-cpp-only.
- **`transcription.rs`**: `WhisperEngine` is imported on the non-Intel-mac targets too and boxed into the existing `Box<dyn SpeechModel + Send>` dispatch. `load_engine` gains a `model_id.starts_with("whisper")` branch that loads the single GGML `.bin`. Extracted `find_ggml_bin()` shared by both target variants. Whisper on these targets goes through the trait `transcribe()` path (Intel-mac keeps its `transcribe_with(WhisperInferenceParams)` path unchanged).
- **`models.rs`**: `engine_format() -> &str` replaced by `supported_formats() -> &[&str]` — `["ggml"]` on Intel-mac, `["onnx", "ggml"]` elsewhere. `list()` filters on membership, so Whisper + Parakeet/Moonshine/SenseVoice cards now all appear on non-Intel-mac.
- **`lib.rs`** setup guard: the "reset active model if this engine can't load it" check now tests membership in `supported_formats()` instead of equality with a single format.
- **Frontend**: no change — `ModelsView` renders whatever `list_models` returns; Whisper cards appear automatically.
- **Docs**: `CLAUDE.md` architecture row + build status updated, resolved the "whisper-cpp not wired" tech-debt bullet, added a CMake build-prerequisite note.

## Decisions & rationale
- **Keep Parakeet v3 as the non-Intel default.** It's faster and more accurate than Whisper for most dictation; Whisper is offered as an alternative (multilingual + translation, and the only option on Intel-mac). Whisper Medium works on Windows CPU — slower than tiny/base/small but fine for dictation-length clips.
- **Box `WhisperEngine` via `SpeechModel`** rather than a second `cfg`-selected concrete `Engine` type, so the non-Intel-mac path stays a single uniform dispatch across ONNX + Whisper.

## Follow-ups / open questions
- **Two new hard build deps on every host** (discovered while verifying): whisper-rs-sys compiles whisper.cpp via the `cmake` crate **and** generates bindings via `bindgen`, which needs `libclang`. On the Windows dev box both were missing; installed `Kitware.CMake` (4.4.2) and `LLVM.LLVM` (22.1.8, provides `libclang.dll`), with `LIBCLANG_PATH=C:\Program Files\LLVM\bin`. Documented in `CLAUDE.md` → Build & run.
- **CI:** the Windows `release.yml` job now compiles whisper.cpp too. GitHub `windows-latest` runners ship both CMake and LLVM/libclang, so this should be fine — confirm on the next tagged build (may still need to set `LIBCLANG_PATH`).
- Binary size / compile time on Windows grows (static whisper.cpp) — acceptable for the feature.
- **Unrelated:** app reportedly hangs on the user's (Apple-Silicon) Mac — separate investigation, not touched here.
