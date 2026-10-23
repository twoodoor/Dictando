# Model detail cards, Romanian model, update dot on the taskbar (v0.3.18)

- **Date:** 2026-10-23
- **Phase:** post-Phase A polish
- **Status:** done, shipped as v0.3.18

## What changed
- **Model cards** (`models.rs` `Details`, `ModelsView.tsx`): each catalog entry now carries parameters, architecture, typical latency (marked measured vs estimate), benchmark WER, RAM, "best for", plain-language limitations, punctuation / Dictionary-hint / translation flags, and the full language list. The card shows four stats (latency, latency on this PC, WER, size + RAM) and an expandable "Details, languages & limitations" section. Whisper Medium/Large are flagged "Too slow for dictation", because `load_engine` refuses them.
- **Language list:** `lang_name()` maps ISO codes to names. Whisper entries list all 99 Whisper languages (`WHISPER_LANGS`) for display only; routing still treats their empty `supported_lang_codes` as "everything". The Models filter now goes by language (e.g. "Romanian") instead of the old Multi-language / English Only split, which had hidden Parakeet v3.
- **Latency on this PC** (`history.rs`, `lib.rs`): History gets a `latency_ms` column (added with `ALTER TABLE`; old rows are 0 and ignored). `model_stats` returns the average over each model's last 50 timed dictations.
- **Romanian:** added `whisper-small-ro`, a community whisper.cpp q8_0 conversion of the Romanian Whisper Small fine-tune `IonGrozea/whisper-small_ro-80mel` (HF repo `YarosTN/whisper-small-ro-ggml`, Apache-2.0, 264 MB, sha256 pinned). The author reports 9.3% WER on their own validation set; stock Whisper Small scores 29.8% on FLEURS-ro. Parakeet v3's card now shows its Romanian FLEURS WER of 12.4%.
- **Update dot** (`updates.rs`, new): a background thread checks the updater endpoint 8 s after launch and then every 4 h. When an update is pending it:
  - draws a red dot as the Windows taskbar overlay icon, a "•" Dock badge on macOS, or a badge count on Linux;
  - composites the same dot onto the tray icon and sets the tray tooltip;
  - renames the tray item to "● Install Update vX";
  - emits `update-available`, which puts a dot on Settings in the sidebar and pre-loads the Install button.

  The indicators are re-applied in `show_main`, because Windows drops overlay icons when a taskbar button is recreated. Nothing downloads until the user clicks Install.

## Why
The user wanted to know what each model actually costs them (languages, exact latency, limitations) before choosing one. They also asked for Romanian models, "there is one specifically built for Romanian", and for a visible hint that a new version exists so they can update from inside the app.

## Decisions & rationale
- Latency numbers come from the 2026-10-22 benchmark (Ryzen 9 5950X, 4 s clips) where measured; the rest are labelled estimates. The per-machine average from History is the "exact" number for the user's own hardware.
- The dot is drawn in code (anti-aliased RGBA), not shipped as an asset. Red is used because a violet dot would disappear on the violet app icon.

## Follow-ups / open questions
- **SpeD ParakeetRo 110M** (POLITEHNICA Bucharest, `gabrielpirlo/Sped_ParakeetRomanian_110M_TDT-CTC`, Apache-2.0) is the strongest Romanian model: CV-21 3.57 / FLEURS 10.61 WER at 110M parameters. It is only published as `.nemo`, a CTC-only ONNX, or GGUF, and none of these match the `transcribe-rs` Parakeet layout. It needs a TDT export to `encoder-model.int8.onnx` + `decoder_joint-model.int8.onnx` + an 80-mel preprocessor saved as `nemo128.onnx` + `vocab.txt` with `<blk>`, and then hosting.
- Benchmark `whisper-small-ro` and Parakeet v3 on real Romanian recordings.
- When Language is Auto-detect, a single-language model the user selected is skipped by routing. Consider honouring it.
