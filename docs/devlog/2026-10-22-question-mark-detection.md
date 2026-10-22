# Question-mark detection in local cleanup

- **Date:** 2026-10-22
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- `src-tauri/src/ai.rs`: new `mark_questions` pass at the start of `fix_punctuation`. It turns a sentence's `.` (or missing ending) into `?` when the sentence opens like an English question ("Can you…", "Is the…", "How's…", "Or did you…") or ends with a tag ("…, right"). A comma followed by a capitalised question ("…a model thing, Can we fix it") becomes a sentence break.
- `ai.rs`: when repetition removal drops a duplicate word or phrase, it keeps the duplicate's trailing punctuation ("check this this?" → "check this?", not "check this").
- `ai.rs`: capitalisation after `.`/`!`/`?` now needs a following space ("FAL.ai" and "e.g." are no longer turned into "FAL.Ai"/"e.G.").
- `src-tauri/src/lib.rs`: the History `engine` field now records the model that actually ran (`effective_model_id`), not the selected one.

## Why
Spoken questions ended in "." with no "?". The logs showed **Smart Model Routing** was sending English dictation to **SenseVoice** (fastest installed model) even though Whisper Turbo was selected. Raw and pasted character counts matched, so the "." came from SenseVoice itself, not from our cleanup. History said `whisper-turbo` for every entry, which hid this. Parakeet history had 46 "?" in 392 entries; SenseVoice had 0 in 50.

## Decisions & rationale
- Fix it in Tier-1 local cleanup (instant, offline, works with any engine) instead of blocking SenseVoice from routing.
- The rules are conservative two-word patterns, so imperatives ("Do it now", "Have a look", "Do not…"), conditionals ("Should you need anything, …"), relative clauses (", which is…", ", who is…") and "So do I" stay as statements. Tested against 80 real dictations with no false positives.
- English-only word lists. Other languages are unaffected because they don't match the patterns.

## Follow-ups / open questions
- Bare-noun subjects are missed ("Will agents remember…?"), on purpose, to avoid false positives.
- Runs only when AI enhancement is on with *Fix punctuation* (`ai_enhance_enabled` defaults to off).
- With *Remove fillers* on, a filler at the end of a question ("…look like?", "…, right?") is dropped together with its "?". This bug existed before and is not fixed yet.
- Consider whether Smart Model Routing should prefer Parakeet over SenseVoice for English, because Parakeet punctuates better.
