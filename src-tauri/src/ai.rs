//! Hybrid AI enhancement layer — local-first, cloud-optional.
//!
//! Tier 1 (instant, <1 ms): Local cleanup — filler removal, consecutive
//! deduplication (words *and* short phrases), standalone-"I" capitalisation,
//! sentence boundary capitalisation, trailing punctuation.  Applied on every
//! path before paste happens.
//!
//! Tier 2 (async, ~300–1000 ms): Cloud rewrite for "Polished" / "Concise"
//! styles or custom instructions.  Supports two providers:
//!   - Gemini 2.5 Flash-Lite  (Google)
//!   - Grok 4.1 Fast          (xAI)   -- selectable in Settings
//!
//! Two-phase paste: Tier-1 output is pasted immediately after key release.
//! If Tier-2 is needed, the caller fires a background thread that awaits the
//! cloud result and re-injects the polished version if it differs.

use serde::Deserialize;

// --- Provider model IDs ---

/// Tried in order; a 404 (model retired or not enabled for this key) moves on
/// to the next. `gemini-2.5-flash-lite` alone used to 404 for keys that never
/// used it, which silently disabled cloud cleanup.
const GEMINI_MODELS: &[&str] = &["gemini-3.5-flash-lite", "gemini-3.1-flash-lite", "gemini-2.5-flash-lite"];
const GROK_MODEL: &str = "grok-4-1-fast"; // xAI Grok 4.1 Fast

/// Filler words/phrases removed wherever they appear (matched
/// case-insensitively as whole words). Multi-word fillers are tried first,
/// longest first.
const FILLERS: &[&str] = &[
    // Hesitation sounds
    "um", "uh", "uhh", "umm", "hmm", "hm", "er", "err", "eh", "ah", "ahh", "ehm", "ahem",
    // Meta-commentary fillers (multi-word, processed first)
    "you know what i mean",
    "what i'm trying to say is",
    "if that makes sense",
    "does that make sense",
    "like i said",
    "as i said",
    "sort of like",
    "kind of like",
    "i mean to say",
    "so basically",
    "so essentially",
    "so actually",
    "basically speaking",
    "simply put",
    "you know",
    "i mean",
    "you see",
    "if you will",
    "okay so",
    "so yeah",
    "yeah so",
    "right so",
    "sort of",
    "kind of",
    "moving on",
    // Single-word padding
    "kinda",
    "sorta",
    "essentially",
    "basically",
];

/// Discourse words that are also ordinary words: "I like it", "is that
/// right?", "it went well", "speak clearly", "translate it literally".
/// Removed only when comma-delimited ("Well, …", "…, like, …").
const CONTEXT_FILLERS: &[&str] = &["like", "right", "well", "clearly", "literally"];

/// Sentence adverbs and interjections, also ordinary words: "did it actually
/// work?", "do it anyway", "Yeah." Removed when comma-delimited, or when they
/// open a sentence that goes on ("Actually I think…", "Yeah I know…").
const SENTENCE_ADVERBS: &[&str] = &[
    "actually", "honestly", "frankly", "truthfully", "obviously", "anyway", "anyways", "yeah",
    "yep",
];

pub struct AiEnhanceOptions<'a> {
    /// Cloud provider: "gemini" | "grok" | "" (local-only)
    pub provider: &'a str,
    pub api_key: &'a str,
    pub custom_words: &'a [String],
    pub fix_punctuation: bool,
    pub remove_fillers: bool,
    pub remove_repetitions: bool,
    pub style_preset: &'a str,
    pub custom_instructions: &'a str,
}

// --- Tier 1: Local cleanup (instant) ---

/// Run all enabled local cleanup passes. Returns the cleaned text in <1 ms.
pub fn local_cleanup(text: &str, opts: &AiEnhanceOptions) -> String {
    let mut out = text.to_string();

    if opts.remove_fillers {
        out = remove_filler_words(&out);
    }
    if opts.remove_repetitions {
        out = remove_consecutive_duplicates(&out);
        out = remove_duplicate_phrases(&out);
    }
    if opts.fix_punctuation {
        out = capitalize_standalone_i(&out);
        out = fix_punctuation(&out);
    }

    // Preserve any custom-dictionary terms (exact casing).
    if !opts.custom_words.is_empty() {
        out = preserve_custom_words(&out, opts.custom_words);
    }

    // Final whitespace normalisation.
    collapse_whitespace(&out)
}

/// Remove filler words/phrases (case-insensitive, whole-word).
///
/// Works on whitespace tokens, so a dropped filler hands its sentence ending
/// to the word before it ("…check this, um?" → "…check this?").
fn remove_filler_words(text: &str) -> String {
    // Multi-word fillers, longest first so "you know what i mean" beats "you know".
    let mut phrases: Vec<Vec<&str>> = FILLERS
        .iter()
        .filter(|f| f.contains(' '))
        .map(|f| f.split(' ').collect())
        .collect();
    phrases.sort_by_key(|p| std::cmp::Reverse(p.len()));

    let tokens: Vec<&str> = text.split_whitespace().collect();
    let cores: Vec<String> = tokens.iter().map(|t| normalize_word(t)).collect();
    let mut kept: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let word = cores[i].as_str();
        let prev = kept.last().map(String::as_str);
        let span = phrases
            .iter()
            .find(|p| phrase_at(&tokens, &cores, i, p))
            .map(Vec::len)
            .or_else(|| {
                let filler = FILLERS.contains(&word)
                    || ((CONTEXT_FILLERS.contains(&word) || SENTENCE_ADVERBS.contains(&word))
                        && is_context_filler(word, tokens[i], prev, tokens.get(i + 1).copied()));
                filler.then_some(1)
            });
        let Some(n) = span else {
            kept.push(tokens[i].to_string());
            i += 1;
            continue;
        };
        // A trailing comma was the filler's own pause; anything else ("?", ".")
        // ends the sentence and has to stay.
        let last = tokens[i + n - 1];
        if let Some(prev) = kept.last_mut() {
            if trailing_punct(last).contains(|c: char| c != ',') {
                *prev = carry_trailing_punct(prev, last);
            }
        }
        i += n;
    }
    kept.join(" ")
}

/// Does the multi-word filler `phrase` start at token `i`? Punctuation inside
/// the span breaks the match: "you, know" is not the filler "you know".
fn phrase_at(tokens: &[&str], cores: &[String], i: usize, phrase: &[&str]) -> bool {
    let end = i + phrase.len();
    end <= tokens.len()
        && cores[i..end].iter().zip(phrase).all(|(c, p)| c == p)
        && tokens[i..end - 1].iter().all(|t| trailing_punct(t).is_empty())
}

/// Is the context-dependent filler `word` (raw token `token`) a filler here?
/// Only when comma-delimited at a sentence start or mid-sentence ("Well, …",
/// "…, like, …"), or for a sentence adverb opening a sentence that goes on
/// ("Actually I think…"). Never as a verb, adjective or sentence-final tag:
/// "look like?", "is that right?", "you're well?", "…project, right?".
fn is_context_filler(word: &str, token: &str, prev: Option<&str>, next: Option<&str>) -> bool {
    let tail = trailing_punct(token);
    let sentence_start = prev.map_or(true, |p| p.ends_with(['.', '!', '?']));
    if tail.starts_with(',') {
        return sentence_start || prev.is_some_and(|p| p.ends_with(','));
    }
    tail.is_empty()
        && sentence_start
        && SENTENCE_ADVERBS.contains(&word)
        // "Obviously not." keeps its adverb.
        && next.is_some_and(|n| normalize_word(n) != "not")
}

/// The run of ASCII punctuation at the end of a token ("right?" → "?").
fn trailing_punct(token: &str) -> &str {
    &token[token.trim_end_matches(|c: char| c.is_ascii_punctuation()).len()..]
}

/// Words that are grammatical when doubled: "I think that that works",
/// "she had had enough", "what it is is…", "very very good", "bye bye".
/// Stutters happen on other words ("the the", "I I", "to to").
const LEGIT_DOUBLES: &[&str] = &[
    "that", "had", "is", "do", "very", "really", "so", "no", "bye", "yeah", "ha", "well", "now",
    "there", "knock", "blah", "far", "many", "long",
];

/// Remove consecutive duplicate words: "the the" -> "the", "I I I" -> "I".
/// A word in `LEGIT_DOUBLES` may appear twice (never three times).
fn remove_consecutive_duplicates(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut kept = vec![words[0].to_string()];
    let mut run = 1; // length of the current run of identical words in the input
    for w in &words[1..] {
        let last = kept.last_mut().unwrap();
        let prev = last.trim_matches(|c: char| c.is_ascii_punctuation());
        let curr = w.trim_matches(|c: char| c.is_ascii_punctuation());
        // A sentence break between them is not a stutter: "No. No."
        let same = prev.eq_ignore_ascii_case(curr)
            && !trailing_punct(last).contains(['.', '!', '?']);
        run = if same { run + 1 } else { 1 };
        if same && run == 2 && LEGIT_DOUBLES.contains(&curr.to_lowercase().as_str()) {
            kept.push(w.to_string());
        } else if same {
            *last = carry_trailing_punct(last, w);
        } else {
            kept.push(w.to_string());
        }
    }
    kept.join(" ")
}

/// `kept` plus the trailing punctuation of the `dropped` word, so removing a
/// word never takes the sentence ending with it: deduping "this this?" keeps
/// the "?". A comma left dangling on `kept` gives way to stronger punctuation
/// ("this, this?" → "this?", "project, um?" → "project?").
fn carry_trailing_punct(kept: &str, dropped: &str) -> String {
    let tail = trailing_punct(dropped);
    let strong = tail.trim_start_matches(',');
    if kept.ends_with(',') && !strong.is_empty() {
        format!("{}{strong}", kept.trim_end_matches(','))
    } else if tail.is_empty() || kept.ends_with(|c: char| c.is_ascii_punctuation()) {
        kept.to_string()
    } else {
        format!("{kept}{tail}")
    }
}

/// Remove short repeated phrases (2-5 words) that appear back-to-back.
/// Catches stutter-restart patterns like "I want to I want to go to the store".
fn remove_duplicate_phrases(text: &str) -> String {
    let mut kept: Vec<String> = text.split_whitespace().map(str::to_string).collect();

    // Try window sizes from 5 down to 2.
    for window in (2..=5usize).rev() {
        let mut new_kept: Vec<String> = Vec::with_capacity(kept.len());
        let mut i = 0;
        while i < kept.len() {
            if i + window * 2 <= kept.len() {
                let a = &kept[i..i + window];
                let b = &kept[i + window..i + window * 2];
                let matches = a.iter().zip(b.iter()).all(|(x, y)| {
                    let xc = x.trim_matches(|c: char| c.is_ascii_punctuation());
                    let yc = y.trim_matches(|c: char| c.is_ascii_punctuation());
                    xc.eq_ignore_ascii_case(yc)
                });
                if matches {
                    new_kept.extend_from_slice(&a[..window - 1]);
                    new_kept.push(carry_trailing_punct(&a[window - 1], &b[window - 1]));
                    i += window * 2;
                    continue;
                }
            }
            new_kept.push(kept[i].clone());
            i += 1;
        }
        kept = new_kept;
    }
    kept.join(" ")
}

/// Capitalise the standalone pronoun "i" -> "I".
/// Handles "i" attached to trailing punctuation (e.g. "i,") but ignores
/// "i" that is part of a longer word.
fn capitalize_standalone_i(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let stripped = w.trim_matches(|c: char| c.is_ascii_punctuation());
            if stripped == "i" {
                let prefix_len = w.len() - w.trim_start_matches(|c: char| c.is_ascii_punctuation()).len();
                let suffix_len = w.len() - w.trim_end_matches(|c: char| c.is_ascii_punctuation()).len();
                let prefix = &w[..prefix_len];
                let suffix = &w[w.len() - suffix_len..];
                format!("{prefix}I{suffix}")
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// --- Question detection (English heuristics) ---
//
// Some engines (notably SenseVoice) end spoken questions with "." instead of
// "?". These lists let `mark_questions` recognise a question from how the
// sentence opens ("Can you…", "Is the…", "How's…") or a closing tag
// ("…, right"). Patterns are deliberately two-word so statements such as
// "Do not change it" or "Have a look" are left alone.

/// Auxiliaries that open a question before any subject ("Is the…", "Can it…").
const AUX_ANY_SUBJECT: &[&str] = &[
    "is", "are", "am", "was", "were", "does", "did", "can", "could", "will", "would",
    "should", "has", "isn't", "aren't", "wasn't", "weren't", "doesn't", "didn't", "can't",
    "couldn't", "won't", "wouldn't", "shouldn't", "hasn't",
];
/// Auxiliaries that only open a question before a personal pronoun — they also
/// start imperatives ("Do it now", "Have a look", "May the best team win").
const AUX_PRONOUN_SUBJECT: &[&str] = &["do", "don't", "have", "haven't", "may", "shall"];
const PRONOUNS: &[&str] = &["i", "you", "we", "they"];
const SUBJECTS: &[&str] = &[
    "i", "you", "we", "they", "he", "she", "it", "there", "this", "that", "these", "those",
    "the", "a", "an", "my", "your", "our", "their", "his", "her", "its", "any", "anyone",
    "anybody", "anything", "someone", "somebody", "something", "everyone", "everybody",
    "everything", "all", "some", "every", "each",
];
const WH_WORDS: &[&str] = &["what", "when", "where", "why", "who", "whom", "whose", "which", "how"];
/// WH-words that also introduce relative clauses (", which is…", ", who can…"),
/// so they only count at the start of a sentence.
const RELATIVE_WH: &[&str] = &["which", "who", "whom", "whose"];
/// Non-auxiliary words that still make a WH-opener a question ("How about…").
const WH_FOLLOWERS: &[&str] = &[
    "about", "much", "many", "long", "often", "far", "come", "one", "ones", "of", "if", "not",
];
/// Lead-in words skipped before checking the opener ("So can you…", "Or is it…").
const LEAD_INS: &[&str] = &[
    "and", "but", "or", "so", "also", "okay", "ok", "oh", "hey", "please", "alright", "um", "uh",
];
/// Closing clauses that turn a statement into a question ("…, right").
const QUESTION_TAGS: &[&str] = &[
    "right", "correct", "is that right", "isn't it", "isn't that right", "aren't you",
    "aren't they", "aren't we", "don't you", "don't you think", "doesn't it", "didn't you",
    "didn't it", "didn't we", "won't you", "wouldn't you", "can't you",
];

/// Lowercase a word, normalise curly apostrophes, strip surrounding punctuation.
fn normalize_word(w: &str) -> String {
    w.replace('\u{2019}', "'")
        .trim_matches(|c: char| !(c.is_alphanumeric() || c == '\''))
        .to_lowercase()
}

fn clause_words(clause: &str) -> Vec<String> {
    clause
        .split_whitespace()
        .map(normalize_word)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Does this clause open like a question? `initial` = start of the sentence
/// (relative-clause WH-words like "which"/"who" only count there).
fn opens_question(words: &[String], initial: bool) -> bool {
    let mut i = 0;
    let mut dropped_so = false;
    while i < words.len() && LEAD_INS.contains(&words[i].as_str()) {
        dropped_so |= words[i] == "so";
        i += 1;
    }
    let w = &words[i..];
    // "So do I" / "So can we" are statements, not questions.
    if w.len() < 2 || (dropped_so && w.len() < 3) {
        return false;
    }
    let (first, second) = (w[0].as_str(), w[1].as_str());

    if AUX_ANY_SUBJECT.contains(&first) {
        return SUBJECTS.contains(&second);
    }
    if AUX_PRONOUN_SUBJECT.contains(&first) {
        return PRONOUNS.contains(&second);
    }
    // WH-contractions: "what's", "how's", "who'd", "where're"…
    let (wh, contracted) = match first.split_once('\'') {
        Some((stem, _)) => (stem, true),
        None => (first, false),
    };
    if !WH_WORDS.contains(&wh) || (!initial && RELATIVE_WH.contains(&wh)) {
        return false;
    }
    if contracted {
        return true;
    }
    if first == "which" && second == "is" && w.get(2).map(String::as_str) == Some("why") {
        return false; // "Which is why we left."
    }
    AUX_ANY_SUBJECT.contains(&second)
        || AUX_PRONOUN_SUBJECT.contains(&second)
        || WH_FOLLOWERS.contains(&second)
}

/// Is this sentence (without its terminator) a question?
fn is_question(sentence: &str) -> bool {
    let clauses: Vec<Vec<String>> = sentence
        .split(',')
        .map(clause_words)
        .filter(|c| !c.is_empty())
        .collect();
    let (Some(first), Some(last)) = (clauses.first(), clauses.last()) else {
        return false;
    };
    let multi = clauses.len() > 1;
    // "Should you need anything, let me know." is a conditional, not a question.
    let conditional = multi && first[0] == "should";
    if !conditional && opens_question(first, true) {
        return true;
    }
    multi && (opens_question(last, false) || QUESTION_TAGS.contains(&last.join(" ").as_str()))
}

/// Replace the "." (or missing terminator) of every question sentence with "?".
/// Also splits run-ons where a comma precedes a capitalised question
/// ("…a model thing, Can we fix it" → "…a model thing? Can we fix it?").
fn mark_questions(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((i, ch)) = iter.next() {
        let ends_sentence = matches!(ch, '.' | '!' | '?')
            && iter.peek().map_or(true, |&(_, next)| next.is_whitespace());
        if ends_sentence {
            push_sentence(&mut out, &text[start..i], Some(ch));
            start = i + ch.len_utf8();
        }
    }
    if start < text.len() {
        push_sentence(&mut out, &text[start..], None);
    }
    out
}

fn push_sentence(out: &mut String, body: &str, term: Option<char>) {
    // Only rewrite plain "." endings (not "!", "?" or an ellipsis) or none at all.
    let convertible = match term {
        Some('.') => !body.ends_with('.'),
        None => true,
        Some(_) => false,
    };
    if !convertible || body.trim().is_empty() {
        out.push_str(body);
        out.extend(term);
        return;
    }

    let mut body = body.to_string();
    let mut seg = 0; // start of the current sentence within `body`
    loop {
        let rest = &body[seg..];
        let split = rest.match_indices(", ").map(|(j, _)| j).find(|&j| {
            let after = &rest[j + 2..];
            let capitalised = after.chars().next().is_some_and(char::is_uppercase);
            let next_clause = after.split(',').next().unwrap_or("");
            capitalised
                && opens_question(&clause_words(next_clause), true)
                && is_question(&rest[..j])
        });
        let Some(j) = split else { break };
        body.replace_range(seg + j..seg + j + 1, "?");
        seg += j + 2;
    }

    let question = is_question(&body[seg..]);
    out.push_str(&body);
    match term {
        _ if question => out.push('?'),
        Some(t) => out.push(t),
        None => {}
    }
}

/// Basic punctuation fixes: turn questions' "." into "?", capitalise first
/// letter and after sentence endings, ensure the text ends with punctuation.
fn fix_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let marked = mark_questions(trimmed);

    let mut result = String::with_capacity(marked.len() + 2);
    let mut chars = marked.chars();

    // Capitalise the very first character.
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            result.push(c);
        }
    }
    result.extend(chars);

    // Capitalise after sentence-ending punctuation.
    let mut out = String::with_capacity(result.len());
    let mut cap_next = false;
    let mut iter = result.chars().peekable();
    while let Some(ch) = iter.next() {
        if cap_next && ch.is_alphabetic() {
            for c in ch.to_uppercase() {
                out.push(c);
            }
            cap_next = false;
        } else {
            out.push(ch);
            if ch == '.' || ch == '!' || ch == '?' {
                // Only a real sentence break: "FAL.ai" / "e.g." stay lowercase.
                cap_next = iter.peek().is_some_and(|n| n.is_whitespace());
            } else if ch != ' ' {
                cap_next = false;
            }
        }
    }

    // Ensure trailing punctuation.
    let last = out.trim_end().chars().last().unwrap_or('.');
    if !matches!(last, '.' | '!' | '?' | ':' | ';' | '"' | '\'' | ')' | '-') {
        out.push('.');
    }
    out
}

/// Restore exact spelling/casing for user-defined vocabulary terms.
fn preserve_custom_words(text: &str, custom_words: &[String]) -> String {
    let mut out = text.to_string();
    for word in custom_words {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        out = replace_whole_word_ci(&out, word);
    }
    out
}

/// Replace every case-insensitive, whole-word occurrence of `term` in `text`
/// with `term` itself ("iphone" → "iPhone"; "iphones" / "myiphone" untouched).
///
/// Walks `text` by char, so non-ASCII input ("café", "I’m") is safe, and copies
/// unmatched text through as `&str` slices. A word boundary is any char that is
/// not alphanumeric (Unicode-aware, so "éiphone" is not a match).
fn replace_whole_word_ci(text: &str, term: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0; // `text[..copied]` is already in `out`
    let mut skip_until = 0; // end of the last match; no new match starts inside it
    let mut prev_is_word = false;
    for (i, ch) in text.char_indices() {
        if i >= skip_until && !prev_is_word {
            if let Some(len) = ci_prefix_len(&text[i..], term) {
                let end = i + len;
                let next_is_word = text[end..].chars().next().is_some_and(char::is_alphanumeric);
                if !next_is_word {
                    out.push_str(&text[copied..i]);
                    out.push_str(term);
                    copied = end;
                    skip_until = end;
                }
            }
        }
        prev_is_word = ch.is_alphanumeric();
    }
    out.push_str(&text[copied..]);
    out
}

/// If `s` starts with `term`, compared case-insensitively char by char, return
/// the byte length of that prefix of `s`.
///
/// The length is measured in `s`, not in a lowercased copy: lowercasing can
/// change a char's UTF-8 width ('İ' 2 → 3 bytes, Kelvin 'K' 3 → 1 byte), so a
/// length taken from the lowercased string would not land on a char boundary.
fn ci_prefix_len(s: &str, term: &str) -> Option<usize> {
    let mut len = 0;
    let mut chars = s.chars();
    for t in term.chars() {
        let c = chars.next()?;
        if c != t && !c.to_lowercase().eq(t.to_lowercase()) {
            return None;
        }
        len += c.len_utf8();
    }
    Some(len)
}

/// Collapse runs of whitespace into single spaces and trim.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space && !result.is_empty() {
                result.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            result.push(ch);
        }
    }
    result.trim().to_string()
}

// --- Tier 2: Cloud AI ---

/// True if the current options require a cloud API call.
fn needs_cloud(opts: &AiEnhanceOptions) -> bool {
    let style_needs_ai = matches!(opts.style_preset, "polished" | "concise");
    let has_custom = !opts.custom_instructions.trim().is_empty();
    (style_needs_ai || has_custom) && !opts.provider.is_empty()
}

/// Main entry point. Runs local cleanup first, then optionally calls cloud API.
/// Returns the enhanced text, or an error (callers fall back to local text).
pub fn enhance(text: &str, opts: &AiEnhanceOptions) -> Result<String, String> {
    let cleaned = local_cleanup(text, opts);
    if cleaned.trim().is_empty() {
        return Ok(cleaned);
    }
    if !needs_cloud(opts) {
        log::info!("AI cleanup: local-only ({} -> {} chars)", text.len(), cleaned.len());
        return Ok(cleaned);
    }
    if opts.api_key.is_empty() {
        log::info!("AI cleanup: local-only (no API key)");
        return Ok(cleaned);
    }
    log::info!("AI cleanup: local + {} cloud (style={})", opts.provider, opts.style_preset);
    match opts.provider {
        "grok" => grok_enhance(&cleaned, opts),
        _      => gemini_enhance(&cleaned, opts),
    }
}

fn build_style_prompt(opts: &AiEnhanceOptions) -> String {
    let style_guide = match opts.style_preset {
        "polished" => "Elevate vocabulary, clarity, and sentence flow for professional communication while preserving core meaning.",
        "concise"  => "Make the text brief, punchy, and direct -- trim fluff without losing key information.",
        _          => "Maintain original voice and natural structure. Minimal changes only.",
    };
    let dict = if opts.custom_words.is_empty() {
        String::new()
    } else {
        format!("\n- Preserve exact spelling/casing of these terms: {}.", opts.custom_words.join(", "))
    };
    let user_rules = if opts.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!("\n- Additional rules: {}", opts.custom_instructions.trim())
    };
    format!(
        "You process dictated speech-to-text that has already had filler words removed.\n\
        Style: {style_guide}{dict}{user_rules}\n\n\
        Rules:\n\
        - Do NOT add false information or fabricate details.\n\
        - Do NOT add preamble, meta-commentary, or quotes around the output.\n\
        - Do NOT summarize away important details.\n\
        - Output ONLY the final enhanced text.\n\nText:\n"
    )
}

// -- Gemini ---

fn gemini_enhance(text: &str, opts: &AiEnhanceOptions) -> Result<String, String> {
    let prompt = format!("{}{}", build_style_prompt(opts), text);
    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "temperature": 0.15, "topP": 0.9 }
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut last_err = String::new();
    let mut resp = None;
    for model in GEMINI_MODELS {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );
        // Key in a header, not the URL, so it never lands in logs or proxies.
        let r = client
            .post(&url)
            .header("x-goog-api-key", opts.api_key)
            .json(&body)
            .send()
            .map_err(|e| e.to_string())?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            log::warn!("Gemini model '{model}' not available (404), trying next");
            last_err = format!("Gemini HTTP 404 for every model ({})", GEMINI_MODELS.join(", "));
            continue;
        }
        if !r.status().is_success() {
            return Err(format!("Gemini HTTP {} ({model})", r.status()));
        }
        resp = Some(r);
        break;
    }
    let resp = resp.ok_or(last_err)?;
    let parsed: GeminiResponse = resp.json().map_err(|e| e.to_string())?;
    let out = parsed
        .candidates.into_iter().next()
        .and_then(|c| c.content.parts.into_iter().next())
        .map(|p| p.text)
        .unwrap_or_default()
        .trim().to_string();
    if out.is_empty() { Err("empty Gemini response".into()) } else { Ok(out) }
}

// -- Grok (xAI) ---

fn grok_enhance(text: &str, opts: &AiEnhanceOptions) -> Result<String, String> {
    let prompt = format!("{}{}", build_style_prompt(opts), text);
    let body = serde_json::json!({
        "model": GROK_MODEL,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.15
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post("https://api.x.ai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", opts.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Grok HTTP {}", resp.status()));
    }
    let parsed: GrokResponse = resp.json().map_err(|e| e.to_string())?;
    let out = parsed
        .choices.into_iter().next()
        .map(|c| c.message.content)
        .unwrap_or_default()
        .trim().to_string();
    if out.is_empty() { Err("empty Grok response".into()) } else { Ok(out) }
}

// --- Response deserialization ---

#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}
#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}
#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}
#[derive(Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct GrokResponse {
    #[serde(default)]
    choices: Vec<GrokChoice>,
}
#[derive(Deserialize)]
struct GrokChoice {
    message: GrokMessage,
}
#[derive(Deserialize)]
struct GrokMessage {
    #[serde(default)]
    content: String,
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> AiEnhanceOptions<'static> {
        AiEnhanceOptions {
            provider: "",
            api_key: "",
            custom_words: &[],
            fix_punctuation: true,
            remove_fillers: true,
            remove_repetitions: true,
            style_preset: "clean",
            custom_instructions: "",
        }
    }

    #[test]
    fn removes_ums() {
        let o = opts();
        let r = local_cleanup("um so uh i want to basically go there", &o);
        assert!(!r.to_lowercase().contains("um"), "um should be removed: {r}");
        assert!(!r.to_lowercase().contains(" uh "), "uh should be removed: {r}");
        assert!(!r.to_lowercase().contains("basically"), "basically should be removed: {r}");
    }

    #[test]
    fn capitalizes_standalone_i() {
        let o = opts();
        let r = local_cleanup("i think i should go and i will", &o);
        assert!(!r.contains(" i "), "standalone i should become I: {r}");
    }

    #[test]
    fn removes_duplicate_phrase() {
        let o = opts();
        let r = local_cleanup("I want to go I want to go to the store", &o);
        let count = r.matches("want to go").count();
        assert!(count <= 1, "duplicate phrase should be removed: {r}");
    }

    #[test]
    fn removes_consecutive_word_dupes() {
        let o = opts();
        let r = local_cleanup("the the quick brown fox", &o);
        assert!(!r.contains("the the"), "consecutive dupe should be removed: {r}");
        assert_eq!(local_cleanup("I I I think so", &o), "I think so.");
    }

    #[test]
    fn keeps_grammatical_doubles() {
        let o = AiEnhanceOptions { fix_punctuation: false, ..opts() };
        for text in [
            "I think that that works",
            "she had had enough",
            "what it is is a timing bug",
            "it was very very slow",
            "No. No.",
        ] {
            assert_eq!(local_cleanup(text, &o), text);
        }
        assert_eq!(local_cleanup("that that that works", &o), "that that works");
    }

    #[test]
    fn first_letter_capitalised() {
        let o = opts();
        let r = local_cleanup("hello world", &o);
        assert!(r.starts_with("Hello"), "first letter should be capitalised: {r}");
    }

    /// Local cleanup with only punctuation fixing on (matches a common setup).
    fn punct(text: &str) -> String {
        let o = AiEnhanceOptions { remove_fillers: false, ..opts() };
        local_cleanup(text, &o)
    }

    #[test]
    fn questions_get_question_marks() {
        let cases = [
            ("Do you still need my help.", "Do you still need my help?"),
            ("are you sure it was merged", "Are you sure it was merged?"),
            ("Or can you merge it to yourself.", "Or can you merge it to yourself?"),
            ("Hey, mate, how's it going.", "Hey, mate, how's it going?"),
            ("What do you think about this.", "What do you think about this?"),
            ("Is there anything I can send today.", "Is there anything I can send today?"),
            ("You're working in the right project, right.", "You're working in the right project, right?"),
            (
                "I will leave in 40 minutes. Is there anything I can send to the client today.",
                "I will leave in 40 minutes. Is there anything I can send to the client today?",
            ),
            (
                "Is that a code thing or is that a model thing, Can we fix it.",
                "Is that a code thing or is that a model thing? Can we fix it?",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(punct(input), want, "input: {input}");
        }
    }

    #[test]
    fn statements_keep_periods() {
        let cases = [
            "Do not change the finishes.",
            "Do it now.",
            "Have a look at the file.",
            "If you have a better idea, let me know.",
            "Should you need anything, let me know.",
            "I talked to John, who is the manager.",
            "We got the file, which is great.",
            "Which is why we left.",
            "So do I.",
            "Please let me know if the plugins are easy to install.",
            "What postponed.",
            "Can you...",
            "Great!",
            "Swap Gemini for the FAL.ai API.",
        ];
        for input in cases {
            assert_eq!(punct(input), input, "statement changed");
        }
    }

    #[test]
    fn dedupe_keeps_question_mark() {
        let o = AiEnhanceOptions { fix_punctuation: false, ..opts() };
        assert_eq!(local_cleanup("Can you check this this?", &o), "Can you check this?");
        assert_eq!(local_cleanup("Is it ready is it ready?", &o), "Is it ready?");
        assert_eq!(local_cleanup("Can you check this, this?", &o), "Can you check this?");
    }

    #[test]
    fn fillers_keep_meaningful_words() {
        // "like", "right", "well"… as verbs, adjectives, adverbs or tags stay.
        let cases = [
            "What does it look like?",
            "You're in the right project, right?",
            "Is that right?",
            "You're well?",
            "I like it.",
            "It went well.",
            "I want this as well.",
            "Right now I'm busy.",
            "Well done.",
            "Turn right at the light.",
            "Speak clearly.",
            "Translate it literally.",
            "I'll do it anyway.",
            "Did it actually work?",
            "I was like, what?",
            "Yeah.",
            "Obviously not.",
        ];
        for input in cases {
            assert_eq!(local_cleanup(input, &opts()), input, "meaningful word removed");
        }
    }

    #[test]
    fn real_fillers_still_removed() {
        let cases = [
            ("um so uh I want to basically go there", "So I want to go there."),
            ("Um, I think it works.", "I think it works."),
            ("uh hmm the build is green", "The build is green."),
            ("Basically, it works.", "It works."),
            ("Well, I think so.", "I think so."),
            ("Like, what is this?", "What is this?"),
            ("Actually I think we should go.", "I think we should go."),
            ("Yeah, that works.", "That works."),
            ("Okay, right, let's go.", "Okay, let's go."),
            ("So, like, it's done.", "So, it's done."),
            // A filler between list items leaves the list intact.
            ("We need eggs, um, milk, and bread.", "We need eggs, milk, and bread."),
        ];
        for (input, want) in cases {
            assert_eq!(local_cleanup(input, &opts()), want, "input: {input}");
        }
    }

    #[test]
    fn dropped_filler_keeps_sentence_punctuation() {
        let cases = [
            ("What does it look like, uh?", "What does it look like?"),
            ("Can you check this, um?", "Can you check this?"),
            ("Is it ready um?", "Is it ready?"),
            ("It's done, you know.", "It's done."),
            ("That's the plan, you know?", "That's the plan?"),
            ("Is that the one, if that makes sense?", "Is that the one?"),
        ];
        for (input, want) in cases {
            assert_eq!(local_cleanup(input, &opts()), want, "input: {input}");
        }
    }

    #[test]
    fn filler_removal_handles_non_ascii() {
        // Used to panic: the phrase matcher sliced strings at byte offsets.
        assert_eq!(
            local_cleanup("Um, I’m at the café in Zürich — you know, the one by the lake.", &opts()),
            "I’m at the café in Zürich — the one by the lake."
        );
    }

    #[test]
    fn preserves_custom_words() {
        let words = vec!["iPhone".to_string(), "macOS".to_string()];
        let o = AiEnhanceOptions { custom_words: &words, ..opts() };
        let r = local_cleanup("I use iphone and macos every day", &o);
        assert!(r.contains("iPhone"), "iPhone casing should be preserved: {r}");
        assert!(r.contains("macOS"), "macOS casing should be preserved: {r}");
    }

    #[test]
    fn custom_words_handle_non_ascii() {
        // Used to panic: the matcher sliced the string at every byte offset.
        let words = vec!["iPhone".to_string(), "macOS".to_string(), "Kelvin".to_string()];
        let cases = [
            ("my café iphone", "my café iPhone"),
            ("I’m on macos", "I’m on macOS"),
            ("Zürich — iphone, macos.", "Zürich — iPhone, macOS."),
            // Lowercasing changes the UTF-8 width of these chars.
            ("İzmir iphone", "İzmir iPhone"),
            ("the \u{212A}elvin scale", "the Kelvin scale"),
        ];
        for (input, want) in cases {
            assert_eq!(preserve_custom_words(input, &words), want, "input: {input}");
        }
        // Same path through the full local cleanup.
        let o = AiEnhanceOptions { custom_words: &words, ..opts() };
        assert_eq!(local_cleanup("I’m on macos at the café", &o), "I’m on macOS at the café.");
    }

    #[test]
    fn custom_words_match_whole_words_only() {
        let words = vec!["iPhone".to_string()];
        let cases = [
            ("iphones and myiphone, iphone.", "iphones and myiphone, iPhone."),
            ("IPHONE’s case", "iPhone’s case"),
            // Non-ASCII letters count as word characters.
            ("éiphone iphoneé", "éiphone iphoneé"),
        ];
        for (input, want) in cases {
            assert_eq!(preserve_custom_words(input, &words), want, "input: {input}");
        }
    }
}
