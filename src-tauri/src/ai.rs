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

const GEMINI_MODEL: &str = "gemini-2.5-flash-lite";
const GROK_MODEL: &str = "grok-4-1-fast"; // xAI Grok 4.1 Fast

/// Filler words/phrases to strip (matched case-insensitively as whole words).
/// Multi-word fillers are processed first (longest to shortest).
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
    // Single-word padding
    "sort of",
    "kind of",
    "kinda",
    "sorta",
    "essentially",
    "basically",
    "actually",
    "literally",
    "honestly",
    "frankly",
    "truthfully",
    "obviously",
    "clearly",
    "right",
    "yeah",
    "yep",
    "well",
    "anyway",
    "anyways",
    "moving on",
    "like",   // standalone filler "like"
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
fn remove_filler_words(text: &str) -> String {
    let mut result = text.to_string();

    // Multi-word fillers first (longest first to avoid partial matches).
    let mut multi_word: Vec<&&str> = FILLERS.iter().filter(|f| f.contains(' ')).collect();
    multi_word.sort_by(|a, b| b.len().cmp(&a.len()));
    for filler in multi_word {
        result = remove_phrase_ci(&result, filler);
    }

    // Single-word fillers via word-split.
    let single_word: Vec<&&str> = FILLERS.iter().filter(|f| !f.contains(' ')).collect();
    let words: Vec<&str> = result.split_whitespace().collect();
    let mut kept = Vec::with_capacity(words.len());
    for word in &words {
        let stripped = word.trim_matches(|c: char| c.is_ascii_punctuation());
        let lower = stripped.to_lowercase();
        if single_word.iter().any(|f| **f == lower) {
            continue;
        }
        kept.push(*word);
    }
    kept.join(" ")
}

/// Remove a multi-word phrase case-insensitively at word boundaries.
fn remove_phrase_ci(text: &str, phrase: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_phrase = phrase.to_lowercase();
    let phrase_len = lower_phrase.len();
    let text_len = text.len();
    let bytes = text.as_bytes();

    let mut result = String::with_capacity(text_len);
    let mut i = 0;
    while i < text_len {
        if i + phrase_len <= text_len && lower_text[i..i + phrase_len] == lower_phrase[..] {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_pos = i + phrase_len;
            let after_ok = after_pos >= text_len || !bytes[after_pos].is_ascii_alphanumeric();
            if before_ok && after_ok {
                i = after_pos;
                // Eat trailing comma/space after the removed phrase.
                while i < text_len && (bytes[i] == b',' || bytes[i] == b' ') {
                    i += 1;
                }
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Remove consecutive duplicate words: "the the" -> "the", "I I I" -> "I".
fn remove_consecutive_duplicates(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut kept = vec![words[0].to_string()];
    for w in &words[1..] {
        let last = kept.last_mut().unwrap();
        let prev = last.trim_matches(|c: char| c.is_ascii_punctuation());
        let curr = w.trim_matches(|c: char| c.is_ascii_punctuation());
        if prev.eq_ignore_ascii_case(curr) {
            *last = carry_trailing_punct(last, w);
        } else {
            kept.push(w.to_string());
        }
    }
    kept.join(" ")
}

/// `kept` plus the trailing punctuation of the `dropped` duplicate, when `kept`
/// has none of its own — so deduping "this this?" keeps the "?".
fn carry_trailing_punct(kept: &str, dropped: &str) -> String {
    let core = dropped.trim_end_matches(|c: char| c.is_ascii_punctuation());
    let tail = &dropped[core.len()..];
    if tail.is_empty() || kept.ends_with(|c: char| c.is_ascii_punctuation()) {
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
        let lower_word = word.to_lowercase();
        if !out.to_lowercase().contains(&lower_word) {
            continue;
        }
        let mut new_out = String::with_capacity(out.len());
        let bytes = out.as_bytes();
        let mut i = 0;
        while i < out.len() {
            let remaining = &out[i..];
            if remaining.to_lowercase().starts_with(&lower_word) {
                let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let after_pos = i + lower_word.len();
                let after_ok = after_pos >= out.len() || !bytes[after_pos].is_ascii_alphanumeric();
                if before_ok && after_ok {
                    new_out.push_str(word);
                    i += lower_word.len();
                    continue;
                }
            }
            new_out.push(bytes[i] as char);
            i += 1;
        }
        out = new_out;
    }
    out
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
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_MODEL}:generateContent?key={}",
        opts.api_key
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.post(&url).json(&body).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Gemini HTTP {}", resp.status()));
    }
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
    }

    #[test]
    fn preserves_custom_words() {
        let words = vec!["iPhone".to_string(), "macOS".to_string()];
        let o = AiEnhanceOptions { custom_words: &words, ..opts() };
        let r = local_cleanup("I use iphone and macos every day", &o);
        assert!(r.contains("iPhone"), "iPhone casing should be preserved: {r}");
        assert!(r.contains("macOS"), "macOS casing should be preserved: {r}");
    }
}
