# Phase 3 (part 1): Local SQLite history

- **Date:** 2026-06-16
- **Phase:** 3 — Feature parity + settings depth
- **Status:** code-complete & build-verified

## What changed
- New `history.rs`: `History` store over bundled SQLite at `<app_data>/history.db` (`dictations` table + timestamp index). Methods: `insert`, `list(limit)`, `delete`, `clear`, `prune(limit)`.
- `lib.rs`: `History` added to `AppState` and opened in setup; every completed transcription is inserted and the table pruned to `history_limit`; `list_history`/`delete_history`/`clear_history` commands now back onto SQLite. Added `unique_id()` (nanosecond clock) for collision-resistant ids.
- `HistoryView.tsx`: native path loads from `backend.listHistory`, live-appends from `transcription` events, deletes via `backend.deleteHistory`, and renders numeric (epoch-ms) timestamps.

## Why
Makes transcription history real and local-first natively (the History tab was a Firestore-only stub on web). Persists across restarts; respects the user's history limit.

## Verification
- `cargo build` exit 0 (bundled SQLite compiled via MSVC); `npm run lint` clean.

## Follow-ups (rest of Phase 3)
- Full settings surface: overlay window + position, tray icon, launch-on-startup, custom-words biasing, append-trailing-space (wired) , auto-delete recordings, audio feedback, mute-while-recording.
- Optional: store recordings for playback (History screenshot shows a player) under `<app_data>/recordings/`.
