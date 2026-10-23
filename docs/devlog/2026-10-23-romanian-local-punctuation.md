# Local punctuation for Romanian (v0.3.22)

- **Date:** 2026-10-23
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- New module `src-tauri/src/punct_ro.rs`: offline, rule-based punctuation for Romanian transcripts.
  - It capitalises each sentence start, and ends each sentence with "." or "?".
  - A sentence is marked as a question when it opens with a strong opener: "de ce", "oare", "încotro", "a/al cui", "cât costă", "ce mai faci", and requests like "poți să…", "ați putea…" or "vrei să…". The match also accepts clitic forms such as "să-mi".
  - A short sentence of 5 words or fewer is also a question when it opens with "ce", "cine", "unde", "când", "cum", "care" or "cât…". Exclamations like "ce frumos" or "cât de bine" are excluded.
  - Closing tags like "nu-i așa", "nu crezi" and "sau nu" mark a question and get a comma before them.
  - A comma goes before "dar", "însă", "ci", "iar" and "deci" when they join two clauses. The exceptions: "un dar" (the noun, "a gift"), a final "iar" ("again"), and a conjunction right after a lead-in ("Bine dar…").
  - The old cedilla letters ş and ţ are matched like ș and ț.
- `src-tauri/src/lib.rs`: the pass runs on the raw transcript when `punct_ro::applies` is true, meaning the model that ran has `details.punctuation == false` and the dictation is Romanian (the language setting is `ro`, or the model is Romanian-only). It runs when AI cleanup is off, or when it is on with "Fix punctuation" enabled. Running before cleanup means the pasted text, History and the cloud polish all get the punctuated version.
- The SpeD ParakeetRo model card now says Mumblr adds basic punctuation itself.
- 10 unit tests.

## Why
SpeD ParakeetRo (v0.3.21) outputs bare lowercase text. With AI cleanup off, `lib.rs` pasted the raw transcript unchanged.

## Decisions & rationale
- The user chose rules over a neural model: 1-800-BAD-CODE `punct_cap_seg_47_language` is 233 MB of ONNX, plus SentencePiece in Rust, plus about 50–150 ms per dictation.
- Romanian yes/no questions are often marked only by intonation ("Ai mâncat?"), so the rules cannot see them. Only explicit openers and tags count. A missing "?" was judged less annoying than a wrong one.
- WH-words open statements too ("Când ajung acasă te sun"), hence the 5-word cap. Short WH statements of 5 words or fewer can still get a wrong "?".
- Checked on the model's real output for 28 FLEURS-ro sentences: no false question marks, and the 3 commas it added (before "dar" ×2 and "iar") are all correct.

## Follow-ups / open questions
- Proper nouns and sentence breaks inside long dictations are still unhandled. Next step: a neural punctuation and truecasing model (punct_cap_seg_47_language).
- Spelled-out numbers ("o mie nouă sute șaizeci și trei") could be turned into digits.
