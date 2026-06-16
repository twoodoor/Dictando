# Dictando Dev Log

Running record of significant changes, decisions, and their rationale during the rebuild of Dictando into a native Tauri app. Newest entries first.

**How to use:** for every meaningful change, add a dated entry file named `YYYY-MM-DD-<topic>.md` and link it in the index below. Each entry follows the template at the bottom. Keep `CLAUDE.md` (+ `GEMINI.md`/`AGENTS.md`) in sync when architecture changes.

## Index

- [2026-06-16 — Project rebuild kickoff: Tauri pivot, docs & devlog scaffolding](./2026-06-16-rebuild-kickoff.md)
- [2026-06-16 — Phase 1: Tauri backend — capture → transcribe → inject](./2026-06-16-phase1-tauri-backend.md)
- [2026-06-16 — Phase 2: Model catalog + download manager](./2026-06-16-phase2-model-downloads.md)
- [2026-06-16 — Phase 3 (part 1): Local SQLite history](./2026-06-16-phase3-history.md)
- [2026-06-16 — UI redesign: full-window shell, light/dark, Wispr-inspired](./2026-06-16-ui-redesign.md)
- [2026-06-16 — Phase 3b + 4: tray, autostart, overlay HUD, AI enhancement](./2026-06-16-phase3b-4-tray-overlay-ai.md)
- [2026-06-16 — Phase 5: deployment — installers for Windows & macOS](./2026-06-16-phase5-deployment.md)
- [2026-06-16 — macOS first-run UX: microphone + Accessibility](./2026-06-16-macos-first-run-ux.md)
- [2026-06-17 — Intel macOS support via hybrid engine (Parakeet / Whisper)](./2026-06-17-intel-mac-hybrid-engine.md)

---

## Entry template

```markdown
# <Title>

- **Date:** YYYY-MM-DD
- **Phase:** <0–5 / which roadmap phase>
- **Status:** planned | in progress | done

## What changed
<files/areas touched, concretely>

## Why
<the problem or goal>

## Decisions & rationale
<choices made and why; alternatives rejected>

## Follow-ups / open questions
<anything deferred>
```
