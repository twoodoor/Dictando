# Context-aware filler removal that keeps punctuation

- **Date:** 2026-10-22
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- `src-tauri/src/ai.rs`: `remove_filler_words` now works on whitespace tokens in a single pass. It replaces the old two-pass approach (byte-level `remove_phrase_ci` for phrases, then a word split for single words).
- When a filler is dropped, its sentence ending moves to the previous word, and a dangling comma before it is removed: "Can you check this, um?" → "Can you check this?", "It's done, you know." → "It's done." The filler's own trailing comma is not carried, because it only marked the pause.
- Risky single words have moved out of the always-removed `FILLERS` list:
  - `CONTEXT_FILLERS` (like, right, well, clearly, literally) are removed only when comma-delimited: at a sentence start ("Well, …") or between commas ("…, like, …").
  - `SENTENCE_ADVERBS` (actually, honestly, frankly, truthfully, obviously, anyway(s), yeah, yep) follow the same rule. They are also removed when they open a sentence that goes on ("Actually I think…"), except before "not" ("Obviously not.").
  - In every other position these words are kept, for example as a verb, adjective, adverb or sentence-final tag: "What does it look like?", "Is that right?", "You're well?", "…the right project, right?", "Right now…", "Well done.", "as well", "Speak clearly.", "Yeah."
- `carry_trailing_punct` (shared with repetition removal) now lets stronger punctuation replace a dangling comma, so "check this, this?" → "check this?".
- Four new unit tests: meaningful words kept, real fillers still removed, punctuation carried, and non-ASCII input. The dedupe test has a new case. The ai suite now has 13 tests.

## Why
- Removing a filler dropped the whole token, punctuation included. Because "like", "right" and "well" were always treated as fillers, questions lost words and their "?": "What does it look like?" → "What does it look." and "…the right project, right?" → "…the right project,". The previous entry listed this as an open bug.
- `remove_phrase_ci` stepped through the text one byte at a time and sliced the string at each offset. It panicked on any non-ASCII character ("I’m", "café", "—") once there were a few bytes after it. As a result, *Remove fillers* crashed the cleanup on most non-English dictation.

## Decisions & rationale
- For a parenthetical filler, one comma is kept: "eggs, um, milk, and bread" → "eggs, milk, and bread". Dropping both commas would read better in "I think, um, we should" but would merge list items. Keeping one comma never changes meaning, and it matches the old behaviour.
- "…, you know?" → "…?": the "?" is carried as instructed. This matches the codebase's view that a question tag makes the sentence a question (`QUESTION_TAGS` in `mark_questions`).
- Hesitation sounds and pure padding (um, uh, hmm, basically, essentially, kinda, sorta) are still removed everywhere.
- Comma-delimited is the main signal. Parakeet and Whisper put commas around spoken discourse markers. Without commas, these words are usually doing grammatical work.

## Follow-ups / open questions
- Some multi-word fillers are still removed everywhere and can drop meaning: "Do you know where it is?" → "Do where it is?", "Can you see it?" → "Can it?", "what kind of car", "that's what I mean", "we're moving on", "do it right so…". "Does that make sense?" on its own is removed entirely. These need the same context rules.
- `preserve_custom_words` uses the same byte-offset slicing and panics on non-ASCII text when a Dictionary term is present ("my café iphone" + "iPhone"). Fixed in [Unicode-safe Dictionary term matching](./2026-10-22-custom-words-utf8-fix.md).
- "er" is an always-removed hesitation, but it is also German for "he".
