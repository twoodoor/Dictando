# Unicode-safe Dictionary term matching (`preserve_custom_words`)

- **Date:** 2026-10-22
- **Phase:** post-Phase A polish
- **Status:** done

## What changed
- `src-tauri/src/ai.rs`: `preserve_custom_words` now calls a new `replace_whole_word_ci` for each Dictionary term. That function walks the text with `char_indices`, so it only stops on char boundaries, and it copies unmatched text through as `&str` slices.
- A new helper, `ci_prefix_len`, compares the term against the text one char at a time, ignoring case. It returns the byte length of the match as measured in the original text, not in a lowercased copy.
- The whole-word checks are kept and are now Unicode-aware: `char::is_alphanumeric` replaces `u8::is_ascii_alphanumeric`. As a result, "éiphone" and "iphoneé" no longer count as matches for "iPhone".
- Two new unit tests:
  - `custom_words_handle_non_ascii` covers "my café iphone" → "my café iPhone", "I’m on macos" → "I’m on macOS", 'İ', the Kelvin sign, and one full `local_cleanup` pass.
  - `custom_words_match_whole_words_only` checks whole-word matching.
- The existing `preserves_custom_words` test still passes. The ai suite now has 16 tests.

## Why
- The old code stepped through the text one byte at a time with `&out[i..]` and `i += 1`. Once the text contained a Dictionary term, any non-ASCII character made it panic: `preserve_custom_words("my café iphone", &["iPhone"])` failed with *"byte index 7 is not a char boundary; it is inside 'é'"*. This crashed local cleanup for most non-English dictation, and also for English with curly apostrophes ("I’m") or em-dashes.
- `new_out.push(bytes[i] as char)` turned each UTF-8 byte into a Latin-1 char, which would produce mojibake ("cafÃ©") even when nothing panicked.
- The old code was also O(n²): it lowercased the whole remaining string at every byte position.
- The [context-aware filler removal entry](./2026-10-22-context-aware-filler-removal.md) listed this as a follow-up. It had fixed the same byte-slicing bug in `remove_phrase_ci`.

## Decisions & rationale
- **Char-by-char case folding instead of lowercasing the remainder.** `to_lowercase` can change a char's byte width: 'İ' grows from 2 to 3 bytes and the Kelvin sign 'K' shrinks from 3 to 1. A length taken from the lowercased string therefore cannot be used to index the original. Comparing `c.to_lowercase()` with `t.to_lowercase()` per char, and adding up `c.len_utf8()` from the source, means the match end always falls on a real boundary.
- **Matches are only tried at word starts**, where the previous char is not alphanumeric. Most positions are rejected after one char comparison, so a term pass is effectively linear (worst case O(n·m)).
- **Locale-specific casing is not handled.** Turkish dotless/dotted i and Greek final sigma ('ς' vs 'σ') follow plain Unicode simple lowercasing, as they did before. A transcript "istanbul" will not match a term typed as "İstanbul".

## Follow-ups / open questions
- Text in decomposed form (NFD), such as "cafe" + U+0301, has a combining mark that is not alphanumeric. A term "cafe" would therefore match before the accent. Parakeet and Whisper output NFC, so this is not handled for now.
- The multi-word filler context rules listed in the filler-removal entry are still open.
