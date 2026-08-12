//! Hybrid AI enhancement layer — local-first, cloud-optional.
//!
//! Tier 1 (instant): Local regex-based cleanup handles filler word removal,
//! consecutive-word deduplication, and basic punctuation. This runs in <1ms
//! and is the default for "Clean & Natural" and "Casual" styles.
//!
//! Tier 2 (cloud): For "Polished" and "Concise" style presets, or when the
//! user has custom instructions, the locally-cleaned text is sent to Gemini
//! Flash Lite (~1s) for intelligent rewriting.

use serde::Deserialize;

const MODEL: &str = "gemini-2.5-flash-lite";

/// Filler words/phrases to strip (matched case-insensitively as whole words).
const FILLERS: &[&str] = &[
    "um", "uh", "uhh", "umm", "hmm", "hm", "er", "ah", "ehm",
    "like",              // standalone filler "like"
    "you know",
    "sort of",
    "kind of",
    "i mean",
    "basically",
    "actually",
    "literally",
    "right",
    "so yeah",
    "yeah",
    "okay so",
    "well",
    "anyway",
    "anyways",
];

pub struct AiEnhanceOptions<'a> {
    pub api_key: &'a str,
    pub custom_words: &'a [String],
    pub fix_punctuation: bool,
    pub remove_fillers: bool,
    pub remove_repetitions: bool,
    pub style_preset: &'a str,
    pub custom_instructions: &'a str,
}

// ─── Tier 1: Local cleanup (instant) ────────────────────────────────────────

/// Run all enabled local cleanup passes. Returns the cleaned text in <1ms.
pub fn local_cleanup(text: &str, opts: &AiEnhanceOptions) -> String {
    let mut out = text.to_string();

    if opts.remove_fillers {
        out = remove_filler_words(&out);
    }
    if opts.remove_repetitions {
        out = remove_consecutive_duplicates(&out);
    }
    if opts.fix_punctuation {
        out = fix_punctuation(&out);
    }

    // Collapse multiple spaces and trim.
    out = collapse_whitespace(&out);
    out
}

/// Remove filler words/phrases (case-insensitive, whole-word).
fn remove_filler_words(text: &str) -> String {
    let mut result = text.to_string();

    // Process multi-word fillers first (longest first to avoid partial matches).
    let mut multi_word: Vec<&&str> = FILLERS.iter().filter(|f| f.contains(' ')).collect();
    multi_word.sort_by(|a, b| b.len().cmp(&a.len()));

    for filler in multi_word {
        result = remove_phrase_case_insensitive(&result, filler);
    }

    // Process single-word fillers using word-boundary logic.
    let single_word: Vec<&&str> = FILLERS.iter().filter(|f| !f.contains(' ')).collect();
    let words: Vec<&str> = result.split_whitespace().collect();
    let mut kept = Vec::with_capacity(words.len());

    for word in &words {
        let stripped = word.trim_matches(|c: char| c.is_ascii_punctuation());
        let lower = stripped.to_lowercase();
        if single_word.iter().any(|f| **f == lower) {
            // Drop the filler word including any attached punctuation.
            continue;
        }
        kept.push(*word);
    }

    kept.join(" ")
}

/// Remove a multi-word phrase case-insensitively.
fn remove_phrase_case_insensitive(text: &str, phrase: &str) -> String {
    let lower = text.to_lowercase();
    let phrase_lower = phrase.to_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    let text_len = text.len();
    let phrase_len = phrase.len();

    while i < text_len {
        if i + phrase_len <= text_len && lower[i..i + phrase_len] == phrase_lower {
            // Check word boundaries.
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_pos = i + phrase_len;
            let after_ok = after_pos >= text_len || !bytes[after_pos].is_ascii_alphanumeric();

            if before_ok && after_ok {
                // Skip trailing punctuation + space after the filler phrase.
                i = after_pos;
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

/// Basic punctuation fixes: capitalize first letter, ensure ending punctuation.
fn fix_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(trimmed.len() + 1);
    let mut chars = trimmed.chars();

    // Capitalize first character.
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            result.push(c);
        }
    }
    result.extend(chars);

    // Capitalize after sentence-ending punctuation.
    let mut capitalized = String::with_capacity(result.len());
    let mut cap_next = false;
    for ch in result.chars() {
        if cap_next && ch.is_alphabetic() {
            for c in ch.to_uppercase() {
                capitalized.push(c);
            }
            cap_next = false;
        } else {
            capitalized.push(ch);
            if ch == '.' || ch == '!' || ch == '?' {
                cap_next = true;
            } else if ch != ' ' {
                cap_next = false;
            }
        }
    }

    // Ensure ending punctuation.
    let last = capitalized.trim_end().chars().last().unwrap_or('.');
    if !matches!(last, '.' | '!' | '?' | ':' | ';' | '"' | '\'' | ')') {
        capitalized.push('.');
    }

    capitalized
}

/// Collapse runs of whitespace into single spaces.
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

// ─── Tier 2: Cloud AI (optional, for polished/concise styles) ───────────────

/// Returns true if the current options require a cloud API call.
fn needs_cloud(opts: &AiEnhanceOptions) -> bool {
    // Polished and Concise styles need AI intelligence.
    let style_needs_ai = matches!(opts.style_preset, "polished" | "concise");
    // Custom instructions always need AI.
    let has_custom = !opts.custom_instructions.trim().is_empty();
    style_needs_ai || has_custom
}

/// Main entry point. Runs local cleanup first, then optionally calls cloud API.
/// Returns the enhanced text, or an error (callers fall back to raw text).
pub fn enhance(text: &str, opts: &AiEnhanceOptions) -> Result<String, String> {
    // Always run local cleanup first (instant).
    let cleaned = local_cleanup(text, opts);

    if cleaned.trim().is_empty() {
        return Ok(cleaned);
    }

    // Only call cloud API if the style/instructions require it.
    if !needs_cloud(opts) {
        log::info!("AI cleanup: local-only ({} chars -> {} chars)", text.len(), cleaned.len());
        return Ok(cleaned);
    }

    // Need API key for cloud tier.
    if opts.api_key.is_empty() {
        log::info!("AI cleanup: local-only (no API key for cloud tier)");
        return Ok(cleaned);
    }

    log::info!("AI cleanup: local + cloud (style={})", opts.style_preset);
    cloud_enhance(&cleaned, opts)
}

/// Call Gemini Flash Lite for intelligent style rewriting.
fn cloud_enhance(text: &str, opts: &AiEnhanceOptions) -> Result<String, String> {
    let style_guide = match opts.style_preset {
        "polished" => "Elevate vocabulary, clarity, and sentence flow for professional communication while preserving core meaning.",
        "concise" => "Make the text brief, punchy, and direct, trimming unnecessary fluff.",
        _ => "Maintain original voice and natural structure without unnecessary rewrite.",
    };

    let dict = if opts.custom_words.is_empty() {
        String::new()
    } else {
        format!(
            "\n- Preserve the exact spelling and casing of these custom vocabulary terms: {}.",
            opts.custom_words.join(", ")
        )
    };

    let user_rules = if opts.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!("\n- Follow these custom rules: {}", opts.custom_instructions.trim())
    };

    let prompt = format!(
        "You process dictated speech-to-text input that has already been cleaned of filler words.\n\
        Style: {style_guide}{dict}{user_rules}\n\n\
        Guiding rule: Do NOT add false information, summarize away key details, or add preamble/quotes.\n\
        Output ONLY the final enhanced text.\n\nText:\n{text}"
    );

    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "temperature": 0.2, "topP": 0.9 }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent?key={}",
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
        .candidates
        .into_iter()
        .next()
        .and_then(|c| c.content.parts.into_iter().next())
        .map(|p| p.text)
        .unwrap_or_default();
    let out = out.trim().to_string();
    if out.is_empty() {
        Err("empty AI response".into())
    } else {
        Ok(out)
    }
}

#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
}
#[derive(Deserialize)]
struct Candidate {
    content: Content,
}
#[derive(Deserialize)]
struct Content {
    #[serde(default)]
    parts: Vec<Part>,
}
#[derive(Deserialize)]
struct Part {
    #[serde(default)]
    text: String,
}
