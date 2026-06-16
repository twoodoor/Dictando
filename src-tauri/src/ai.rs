//! Opt-in AI enhancement layer (Phase 4) — Gemini.
//!
//! When enabled in settings, raw transcription is passed through Gemini to fix
//! punctuation/capitalization, remove filler words, and resolve spoken
//! self-corrections — without changing meaning. With the toggle off this code
//! never runs, so transcription stays fully offline.

use serde::Deserialize;

const MODEL: &str = "gemini-2.0-flash";

/// Clean up dictated `text`. Returns the enhanced text, or an error (callers
/// fall back to the raw text on failure).
pub fn enhance(text: &str, api_key: &str, custom_words: &[String]) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("no Gemini API key".into());
    }

    let dict = if custom_words.is_empty() {
        String::new()
    } else {
        format!(
            " Preserve the exact spelling/casing of these terms if they occur: {}.",
            custom_words.join(", ")
        )
    };

    let prompt = format!(
        "You clean up dictated speech-to-text. Rewrite the text below with correct \
punctuation and capitalization, remove filler words (um, uh, like, you know), and \
resolve spoken self-corrections (e.g. \"Tuesday, no wait, Wednesday\" becomes \
\"Wednesday\"). Do NOT add content, summarize, translate, or change the meaning or \
voice. Output ONLY the cleaned text with no preamble or quotes.{dict}\n\nText:\n{text}"
    );

    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "temperature": 0.2, "topP": 0.9 }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent?key={api_key}"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
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
