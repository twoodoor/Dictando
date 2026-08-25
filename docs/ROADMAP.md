# Mumblr — Implementation Roadmap
<!-- AUTO-UPDATED: agent must keep this file current as work progresses -->
<!-- Last updated: 2026-08-25 -->

## Status Legend
- `[x]` Done & deployed
- `[/]` In progress
- `[ ]` Pending
- `[~]` Decision made, not yet started

---

## ?? North Star

Build a dictation experience that rivals **Wispr Flow** and **Glaido**:
- **Zero perceived latency** — text appears the instant you release the key
- **Reliable hotkeys** — the key combo is *always* caught
- **Stellar audio feedback** — you always know the app heard you
- **Clean output** — fillers gone, punctuation right, "I" capitalised, no repeats

---

## Phase A — Foundation Fixes ? DONE (2026-08-25)

| # | Task | File | Status |
|---|---|---|---|
| A1 | Audio feedback **on by default** | `settings.rs` | `[x]` |
| A2 | Settings migration v0?v1 (force `audioFeedback=true` for ALL existing users on upgrade) | `settings.rs` | `[x]` |
| A3 | **Warm-stream sound** — keep WASAPI stream alive 5 s after last cue, eliminates 50–300 ms open latency on Windows | `sound.rs` | `[x]` |
| A4 | Local cleanup overhaul — 40+ fillers, 2–5 word duplicate phrase detection, standalone-I caps, custom vocab preservation | `ai.rs` | `[x]` |
| A5 | **Grok 4.1 Fast** as second cloud cleanup provider (`ai_provider` + `grok_api_key` settings fields) | `ai.rs`, `settings.rs` | `[x]` |
| A6 | Hotkey **key-repeat debounce** (`hotkey_held: AtomicBool`) — stops Windows 30 Hz repeat from spamming `begin_recording` | `lib.rs` | `[x]` |

---

## Phase B — Latency & Reliability ?? NEXT UP

> **Owner decisions locked in:**
> - Two-phase paste: Wispr Flow style — instant local paste ? silent background AI re-injection
> - Hotkey: keep Ctrl+Space + add conflict detection UI warning badge

| # | Task | File(s) | Status |
|---|---|---|---|
| B1 | **Two-phase paste** — paste local-cleaned text instantly on key-up; `tokio::spawn` fires cloud call; if result differs, re-inject (select pasted chars + replace) | `lib.rs`, `inject.rs` | `[x]` |
| B2 | **Parallel cloud cleanup** — move `cloud_enhance` from blocking thread to `tokio::spawn`; surface result via channel | `lib.rs`, `ai.rs` | `[x]` |
| B3 | **Hotkey conflict detection** — check `RegisterHotKey` return value, emit `hotkey-conflict` event, show warning badge in Settings | `shortcuts.rs`, `lib.rs`, `SettingsView.tsx` | `[x]` |
| B4 | **Overlay appears immediately** — emit `recording` state synchronously before spawning transcription thread | `lib.rs` | `[~]` |

---

## Phase C — Model Expansion ?? PLANNED

> **Owner decisions locked in:**
> - SenseVoice Small first (multilingual), then Moonshine Tiny
> - Both Gemini and Grok available as selectable providers (already shipped in A5)

| # | Task | Notes | Status |
|---|---|---|---|
| C1 | **SenseVoice Small** (ONNX) — ~35 ms, 50+ langs, non-autoregressive | Already in catalog + engine; via `transcribe-rs` | `[x]` |
| C5 | **GigaAM v3** (ONNX) — ~60 ms, ultra-fast Russian & Slavic engine | Added to catalog + engine; via `transcribe-rs` | `[x]` |
| C2 | **Moonshine Tiny** (ONNX) — ~55 ms, English-only, 27 M params | Engine dispatch ready; needs hosted tar.gz | `[/]` |
| C3 | **Moonshine Base** (ONNX) — ~107 ms, English, higher accuracy | Already in catalog + engine | `[x]` |
| C4 | Model catalog tier labels (? Lightning / ?? Balanced) in descriptions | `models.rs` | `[x]` |

---

## Phase D — Settings UI ?? PLANNED

| # | Task | Status |
|---|---|---|
| D1 | Cloud provider selector: Gemini / Grok / Local only | `[x]` |
| D2 | Grok API key field (conditional, shown when Grok selected) | `[x]` |
| D3 | Audio feedback toggle more prominent + sound description text | `[x]` |
| D4 | Hotkey conflict warning badge (fed by B3) | `[x]` |
| D5 | Model tier labels in Models tab (fed by C4) | `[x]` |

---

## ?? Research Summary (August 2026)

### Local ASR Model Speeds (5 s audio clip, CPU)
| Model | Latency | Langs | Size | Notes |
|---|---|---|---|---|
| **SenseVoice Small** | ~35 ms | 50+ | 230 MB | Non-autoregressive, parallel decode |
| **Moonshine Tiny** | ~55 ms | English | 55 MB | Variable-length enc, no 30s padding |
| **Moonshine Base** | ~107 ms | English | 200 MB | Better accuracy |
| Parakeet TDT v3 *(current default)* | ~150–250 ms | 25 | 280 MB | Keep as multilingual default |
| Whisper Turbo (GGML) | ~250 ms | 100 | 1.6 GB | Already in catalog |

### Cloud Cleanup Cost (per call, ~200 tokens in + out)
| Provider | Model | Cost/call | Latency | Notes |
|---|---|---|---|---|
| **Gemini** *(current)* | Flash-Lite | ~$0.00009 | 600–1000 ms | Cheapest |
| **Grok** | 4.1 Fast | ~$0.00013 | 300–500 ms | Fastest cloud option |
| GPT-4o mini | — | ~$0.00014 | 500–800 ms | |
| Claude | Haiku 4.5 | ~$0.00115 | 400–700 ms | 10× pricier |

### Why Wispr Flow feels instant
1. **Paste immediately** with local cleanup — AI polish replaces text silently ~500 ms later
2. Non-autoregressive models — no per-token decode wait
3. Key-up is the hard stop — no VAD silence timeout adds latency
4. Audio capture starts on key-down, not key-up (pre-buffering)

---

## Devlog
- **2026-08-25 (Phase A)**: Audio feedback fixed — warm WASAPI stream + migration v0?v1 forces it on for existing users. 40+ filler words. Duplicate phrase detection. Standalone-I caps. Grok 4.1 Fast added. Hotkey debounce. 6 tests passing. ROADMAP.md created.
- **2026-08-25 (Decisions)**: Owner selected: two-phase Wispr-style paste (B1), keep Ctrl+Space + conflict badge (B3), SenseVoice first then Moonshine Tiny (C1+C2), both Gemini+Grok available.



