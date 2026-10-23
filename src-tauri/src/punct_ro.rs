//! Rule-based punctuation for Romanian transcripts from models that output
//! bare lowercase text (SpeD ParakeetRo). Runs offline in microseconds.
//!
//! It handles what rules can do reliably:
//! - a capital at the start of each sentence, and a final "." or "?"
//! - "?" for questions, recognised from how they open ("de ce", "oare",
//!   "cine", "poți să"…) or from a closing tag ("…, nu-i așa?")
//! - commas before the adversative/conclusive conjunctions dar, însă, ci,
//!   iar, deci
//!
//! It can't find sentence breaks or proper nouns inside a long dictation;
//! that needs a neural punctuation model.

use crate::models::CatalogEntry;

/// Should the Romanian pass run on this model's output? Only for models that
/// leave out punctuation, when the dictation is Romanian: the language is set
/// to Romanian, or the model itself is Romanian-only.
pub fn applies(entry: Option<&CatalogEntry>, lang_code: Option<&str>) -> bool {
    let Some(entry) = entry else { return false };
    !entry.details.punctuation
        && (lang_code == Some("ro") || entry.supported_lang_codes == ["ro"])
}

/// Openers that make a sentence a question regardless of its length.
const STRONG_OPENERS: &[&[&str]] = &[
    &["de", "ce"],
    &["oare"],
    &["încotro"],
    &["a", "cui"],
    &["al", "cui"],
    &["ai", "cui"],
    &["ale", "cui"],
    &["cât", "costă"],
    &["ce", "mai", "faci"],
    &["ce", "mai", "faceți"],
    &["ce", "părere"],
    // Polite requests: "Poți să-mi trimiți…", "Ați putea…", "Vrei să vii…"
    &["poți", "să"],
    &["puteți", "să"],
    &["ai", "putea"],
    &["ați", "putea"],
    &["vrei", "să"],
    &["vreți", "să"],
    &["îmi", "poți"],
    &["îmi", "puteți"],
];

/// Interrogatives that also open statements ("Când ajung acasă te sun",
/// "Ce ai spus e adevărat"), so they count only in short sentences.
const WEAK_OPENERS: &[&str] = &[
    "ce", "cine", "unde", "când", "cum", "care", "cât", "câtă", "câți", "câte",
];
/// Longest sentence (in words) a weak opener may start and still be a question.
const WEAK_MAX_WORDS: usize = 5;

/// "Ce frumos", "Cât de bine": exclamations, not questions.
const EXCLAMATIVE_SECOND: &[&str] = &[
    "frumos", "frumoasă", "bine", "rău", "mult", "păcat", "noroc", "ciudat", "interesant",
    "tare", "minunat", "drăguț", "de",
];

/// Lead-ins skipped before looking at the opener ("Și unde ești").
const LEAD_INS: &[&str] = &["și", "dar", "deci", "bine", "păi", "atunci", "hai", "ok", "bun"];

/// Closing tags that make a statement a question; a comma goes before them.
const TAGS: &[&[&str]] = &[
    &["nu-i", "așa"],
    &["nu", "e", "așa"],
    &["nu", "este", "așa"],
    &["nu", "crezi"],
    &["nu", "credeți"],
    &["sau", "nu"],
    &["da", "sau", "nu"],
];

/// A comma goes before these conjunctions when they join two clauses.
const COMMA_BEFORE: &[&str] = &["dar", "însă", "ci", "iar", "deci"];
/// Before these words "dar" is the noun "gift" ("un dar", "acest dar").
const DAR_NOUN_AFTER: &[&str] = &[
    "un", "unui", "acest", "acel", "al", "fără", "ca", "de", "drept", "cu", "în", "cel",
    "mare", "frumos", "primit",
];

/// Lowercase, unify the old cedilla letters (ş ţ) with the comma ones (ș ț),
/// and drop surrounding punctuation.
fn norm(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
        .to_lowercase()
        .replace('ş', "ș")
        .replace('ţ', "ț")
}

fn starts_with(words: &[String], pattern: &[&str]) -> bool {
    // A pattern word also matches its clitic forms: "să" matches "să-mi".
    let matches = |p: &str, w: &str| w == p || (!p.contains('-') && w.split('-').next() == Some(p));
    words.len() >= pattern.len() && pattern.iter().zip(words).all(|(p, w)| matches(p, w))
}

fn ends_with(words: &[String], pattern: &[&str]) -> bool {
    words.len() > pattern.len() && starts_with(&words[words.len() - pattern.len()..], pattern)
}

fn is_question(words: &[String]) -> bool {
    if TAGS.iter().any(|t| ends_with(words, t)) {
        return true;
    }
    let mut i = 0;
    while i + 1 < words.len() && LEAD_INS.contains(&words[i].as_str()) {
        i += 1;
    }
    let w = &words[i..];
    if STRONG_OPENERS.iter().any(|p| starts_with(w, p)) {
        return true;
    }
    let Some(first) = w.first() else { return false };
    WEAK_OPENERS.contains(&first.as_str())
        && w.len() >= 2
        && w.len() <= WEAK_MAX_WORDS
        && !EXCLAMATIVE_SECOND.contains(&w[1].as_str())
}

/// Add commas before conjunctions and closing tags (in place, by token).
fn add_commas(tokens: &mut [String], words: &[String]) {
    let n = words.len();
    for i in 1..n {
        let prev_has_punct = tokens[i - 1].ends_with(|c: char| c.is_ascii_punctuation());
        if prev_has_punct {
            continue;
        }
        let w = words[i].as_str();
        let conj = COMMA_BEFORE.contains(&w)
            // A clause has to follow: "a venit iar" means "came again".
            && i + 1 < n
            && !(w == "dar" && DAR_NOUN_AFTER.contains(&words[i - 1].as_str()))
            // "Și deci…", "Bine dar…": the lead-in already sets it off.
            && !(i == 1 && LEAD_INS.contains(&words[0].as_str()));
        let tag = TAGS.iter().any(|t| n - i == t.len() && n > t.len() && starts_with(&words[i..], t));
        if conj || tag {
            tokens[i - 1].push(',');
        }
    }
}

/// Punctuate one sentence body (no terminator), returning it with `term`
/// replaced by "?" when it is a question.
fn sentence(body: &str, term: Option<char>, out: &mut String) {
    let mut tokens: Vec<String> = body.split_whitespace().map(str::to_string).collect();
    if tokens.is_empty() {
        out.extend(term);
        return;
    }
    let words: Vec<String> = tokens.iter().map(|t| norm(t)).collect();
    add_commas(&mut tokens, &words);

    // Capitalise the first letter.
    let first = &mut tokens[0];
    if let Some((idx, c)) = first.char_indices().find(|(_, c)| c.is_alphabetic()) {
        let upper: String = c.to_uppercase().collect();
        first.replace_range(idx..idx + c.len_utf8(), &upper);
    }

    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(&tokens.join(" "));
    let question = is_question(&words);
    match term {
        // Keep "!", "?" and "…" from the model; "." becomes "?" for questions.
        Some('.') | None if question => out.push('?'),
        Some(t) => out.push(t),
        None => out.push('.'),
    }
}

/// Punctuate a Romanian transcript.
pub fn punctuate(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(text.len() + 8);
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((i, ch)) = iter.next() {
        let ends = matches!(ch, '.' | '!' | '?')
            && iter.peek().map_or(true, |&(_, next)| next == ' ');
        if ends {
            sentence(&text[start..i], Some(ch), &mut out);
            start = i + ch.len_utf8();
        }
    }
    if start < text.len() {
        sentence(&text[start..], None, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> String {
        punctuate(s)
    }

    #[test]
    fn statements_get_capital_and_period() {
        assert_eq!(p("în cursul nopții au fost făcute copii"), "În cursul nopții au fost făcute copii.");
        assert_eq!(p("  ăsta e un test  "), "Ăsta e un test.");
        assert_eq!(p(""), "");
    }

    #[test]
    fn questions_from_strong_openers() {
        assert_eq!(p("de ce nu ai venit ieri la întâlnirea de la birou"), "De ce nu ai venit ieri la întâlnirea de la birou?");
        assert_eq!(p("oare va ploua mâine"), "Oare va ploua mâine?");
        assert_eq!(p("poți să-mi trimiți raportul până diseară"), "Poți să-mi trimiți raportul până diseară?");
        assert_eq!(p("și ce mai faci"), "Și ce mai faci?");
    }

    #[test]
    fn short_wh_sentences_are_questions() {
        assert_eq!(p("unde ești acum"), "Unde ești acum?");
        assert_eq!(p("cum te cheamă"), "Cum te cheamă?");
        assert_eq!(p("care e problema"), "Care e problema?");
        assert_eq!(p("când vine trenul"), "Când vine trenul?");
    }

    #[test]
    fn long_wh_clauses_and_exclamations_stay_statements() {
        assert_eq!(p("când ajung acasă o să te sun imediat"), "Când ajung acasă o să te sun imediat.");
        assert_eq!(p("ce ai spus ieri la ședință e adevărat"), "Ce ai spus ieri la ședință e adevărat.");
        assert_eq!(p("ce frumos e afară"), "Ce frumos e afară.");
        assert_eq!(p("cât de bine arată"), "Cât de bine arată.");
    }

    #[test]
    fn tags_get_comma_and_question_mark() {
        assert_eq!(p("vii mâine nu-i așa"), "Vii mâine, nu-i așa?");
        assert_eq!(p("mergem la mare sau nu"), "Mergem la mare, sau nu?");
    }

    #[test]
    fn commas_before_conjunctions() {
        assert_eq!(p("am vrut să vin dar am fost bolnav"), "Am vrut să vin, dar am fost bolnav.");
        assert_eq!(p("nu e roșu ci albastru"), "Nu e roșu, ci albastru.");
        assert_eq!(p("eu am plecat iar el a rămas"), "Eu am plecat, iar el a rămas.");
        assert_eq!(p("plouă deci rămânem acasă"), "Plouă, deci rămânem acasă.");
    }

    #[test]
    fn no_comma_where_it_does_not_belong() {
        // "dar" as the noun "gift", "iar" as "again", and after a lead-in.
        assert_eq!(p("am primit un dar frumos"), "Am primit un dar frumos.");
        assert_eq!(p("a venit iar"), "A venit iar.");
        assert_eq!(p("bine dar nu azi"), "Bine dar nu azi.");
        assert_eq!(p("am spus deja, dar nu m-a ascultat"), "Am spus deja, dar nu m-a ascultat.");
    }

    #[test]
    fn existing_punctuation_is_kept() {
        assert_eq!(p("Unde ești? Te aștept."), "Unde ești? Te aștept.");
        assert_eq!(p("unde ești. te aștept"), "Unde ești? Te aștept.");
        assert_eq!(p("ce surpriză!"), "Ce surpriză!");
    }

    #[test]
    fn cedilla_letters_are_recognised() {
        assert_eq!(p("vii mâine nu-i aşa"), "Vii mâine, nu-i aşa?");
    }

    #[test]
    fn applies_only_to_unpunctuated_romanian_models() {
        let ro = crate::models::catalog_entry("parakeet-tdt-ctc-110m-ro");
        let v3 = crate::models::catalog_entry("parakeet-tdt-0.6b-v3");
        let ru = crate::models::catalog_entry("giga-am-v3-int8");
        assert!(applies(ro, Some("ro")));
        assert!(applies(ro, None)); // Romanian-only model, auto-detect
        assert!(!applies(v3, Some("ro"))); // punctuates by itself
        assert!(!applies(ru, Some("ru"))); // unpunctuated, but not Romanian
        assert!(!applies(None, Some("ro")));
    }
}
