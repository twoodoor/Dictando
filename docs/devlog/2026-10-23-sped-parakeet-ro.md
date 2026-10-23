# SpeD ParakeetRo 110M: fast, accurate Romanian model (v0.3.21)

- **Date:** 2026-10-23
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- `src-tauri/src/models.rs`: new catalog entry `parakeet-tdt-ctc-110m-ro` ("SpeD ParakeetRo"), `supported_lang_codes: &["ro"]`. The `parakeet` prefix makes `transcription.rs` load it with transcribe-rs's `ParakeetModel`. It downloads from a GitHub release asset: `models-sped-ro-110m/sped-parakeet-ro-110m-int8.tar.gz`, 104.8 MB, sha256 `f30df4f7…`. That release is a prerelease that is not marked latest, so the updater's `releases/latest` still points at the app.
- Parakeet V3's Romanian note now points to the new model.
- New routing test: with the new model and the Romanian Whisper both installed, the new model is the Romanian specialist. It is never used for other languages or with auto-detect.
- Test fix: `fake_install` in the routing tests now gives every call its own temp dir. Parallel tests that called `route` with the same selection and language were deleting each other's dirs, and failed at random.
- `bench_models` takes optional `BENCH_LANG`, `BENCH_APP_DATA` and `BENCH_MODELS`, and lists the two Romanian models.
- `docs/models/export_sped_parakeet_ro.py`: reproducible conversion script.

## Conversion
Source: `gabrielpirlo/Sped_ParakeetRomanian_110M_TDT-CTC`. It is a NeMo hybrid TDT+CTC FastConformer, fine-tuned from `nvidia/parakeet-tdt_ctc-110m` (Apache-2.0, arXiv 2511.03361). No published export fits transcribe-rs: `AlinClaudiu/SpeD-ParakeetRo-110M-onnx` is CTC-only, and GGUF is a different engine. So:
1. NeMo `restore_from`, `change_decoding_strategy(decoder_type="rnnt")`, and `set_export_config({"decoder_type": "rnnt"})`. Then `model.export()` writes `encoder-model.onnx` and `decoder_joint-model.onnx`, the same way istupakov/onnx-asr exports parakeet-tdt.
2. `quantize_dynamic(weight_type=QUInt8)` produces `*.int8.onnx`: encoder 131 MB, decoder_joint 5 MB.
3. `vocab.txt` holds the 1,024 SentencePiece tokens plus `<blk> 1024`.
4. The preprocessor is istupakov's `nemo80.onnx`, saved as `nemo128.onnx` because transcribe-rs hard-codes that name. The model config is 80 mel, per_feature, 25/10 ms Hann and n_fft 512, which is what nemo80 computes.

The ONNX signatures match what transcribe-rs expects: states are `[1, B, 640]`, and the joint outputs 1030 logits (1025 vocab + 5 TDT durations). The engine keeps the first 1025 logits and decodes greedily, as it does for Parakeet V3.

## Benchmark
40 FLEURS ro_ro test clips (393 s of audio), Ryzen 9 5950X, `bench_models` release build:

| Model | WER | ms/clip | ms per audio-second |
|---|---|---|---|
| **SpeD ParakeetRo (ONNX int8, this export)** | **9.8 %** | 339 | 35 |
| SpeD ParakeetRo (NeMo fp32, reference) | 9.4 % | — | — |
| Parakeet V3 | 22.2 % | 783 | 80 |
| Whisper Small Română | 18.4 % | 2831 | 288 |

The references keep digits and punctuation, but every model spells numbers out, so absolute WERs run higher than the paper's. The authors report 10.61 on FLEURS-ro, and 12.44 for Parakeet V3. Exporting to ONNX int8 costs about 0.4 points.

## Decisions & rationale
- `wer_pct` is left unset, because the UI labels that field "English benchmark". The Romanian numbers go in the notes instead.
- `punctuation: false`: the model writes lowercase text without punctuation, so AI cleanup can add both.
- The model sits before `whisper-small-ro` in `CATALOG`, so specialist routing picks it first when both are installed. It is also live (150 ms), unlike the Whisper model (~2.8 s).

## Follow-ups / open questions
- A local punctuation and casing pass for Romanian would help when AI cleanup is off.
- The CTC head is unused. A CTC-only decode might be faster still.
