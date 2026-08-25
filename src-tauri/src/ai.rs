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
    let mut kept = vec![words[0]];
    for w in &words[1..] {
        let prev = kept.last().unwrap().trim_matches(|c: char| c.is_ascii_punctuation());
        let curr = w.trim_matches(|c: char| c.is_ascii_punctuation());
        if !prev.eq_ignore_ascii_case(curr) {
            kept.push(w);
        }
    }
    kept.join(" ")
}

/// Remove short repeated phrases (2-5 words) that appear back-to-back.
/// Catches stutter-restart patterns like "I want to I want to go to the store".
fn remove_duplicate_phrases(text: &str) -> String {
    let mut kept: Vec<&str> = text.split_whitespace().collect();

    // Try window sizes from 5 down to 2.
    for window in (2..=5usize).rev() {
        let mut new_kept: Vec<&str> = Vec::with_capacity(kept.len());
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
                    new_kept.extend_from_slice(a);
                    i += window * 2;
                    continue;
                }
            }
            new_kept.push(kept[i]);
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

/// Basic punctuation fixes: capitalise first letter and after sentence endings,
/// ensure the text ends with punctuation.
fn fix_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(trimmed.len() + 2);
    let mut chars = trimmed.chars();

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
    for ch in result.chars() {
        if cap_next && ch.is_alphabetic() {
            for c in ch.to_uppercase() {
                out.push(c);
            }
            cap_next = false;
        } else {
            out.push(ch);
            if ch == '.' || ch == '!' || ch == '?' {
                cap_next = true;
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

    #[test]
    fn preserves_custom_words() {
        let words = vec!["iPhone".to_string(), "macOS".to_string()];
        let o = AiEnhanceOptions { custom_words: &words, ..opts() };
        let r = local_cleanup("I use iphone and macos every day", &o);
        assert!(r.contains("iPhone"), "iPhone casing should be preserved: {r}");
        assert!(r.contains("macOS"), "macOS casing should be preserved: {r}");
    }
}
