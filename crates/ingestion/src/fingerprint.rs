//! Exact critical-token fingerprints for conservative chunk deduplication.
//!
//! Near-duplicate merging is allowed only when the normalized text is very
//! similar **and** every number, unit, formula token, and negation is identical.
//! A missing tokenizer feature therefore causes an extra chunk, never a
//! silently merged counterexample.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NEAR_DUPLICATE_THRESHOLD_MILLIS: u32 = 850;
const MINIMUM_NEAR_DUPLICATE_TOKENS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticalTokenKind {
    Number,
    Unit,
    Formula,
    Negation,
    Indentation,
}

impl CriticalTokenKind {
    fn label(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Unit => "unit",
            Self::Formula => "formula",
            Self::Negation => "negation",
            Self::Indentation => "indentation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriticalToken {
    pub kind: CriticalTokenKind,
    pub canonical: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkFingerprint {
    pub normalized_text: String,
    pub text_sha256: String,
    pub critical_sha256: String,
    pub critical_tokens: Vec<CriticalToken>,
    pub word_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupReason {
    ExactText,
    NearText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum DedupDecision {
    Distinct,
    Duplicate {
        reason: DedupReason,
        similarity_millis: u32,
    },
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Normalize presentation differences without erasing critical content.
/// Superscript digits become `^n` because a VLM may emit either form.
pub fn normalize_preserving_case(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\u{00a0}' | '\u{2007}' | '\u{202f}' | '\u{2009}' => normalized.push(' '),
            '\u{2212}' | '\u{2013}' | '\u{2014}' | '\u{2010}' | '\u{2011}' => normalized.push('-'),
            '\u{2018}' | '\u{2019}' | '\u{2032}' => normalized.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{2033}' => normalized.push('"'),
            '\u{00b2}' => normalized.push_str("^2"),
            '\u{00b3}' => normalized.push_str("^3"),
            '\u{00b9}' => normalized.push_str("^1"),
            '\u{2070}' => normalized.push_str("^0"),
            '\u{2074}' => normalized.push_str("^4"),
            '\u{2075}' => normalized.push_str("^5"),
            '\u{2076}' => normalized.push_str("^6"),
            '\u{2077}' => normalized.push_str("^7"),
            '\u{2078}' => normalized.push_str("^8"),
            '\u{2079}' => normalized.push_str("^9"),
            character if character.is_control() => normalized.push(' '),
            character => normalized.push(character),
        }
    }
    let expanded = expand_negation_contractions(&normalized);
    formula_view(&expanded)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase form used for text similarity and exact-text hashing. Critical
/// tokens are extracted from the case-preserving form so `mg` and `Mg` differ.
pub fn normalize_text(text: &str) -> String {
    normalize_preserving_case(text).to_lowercase()
}

fn expand_negation_contractions(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, output: &mut String| {
        if word.is_empty() {
            return;
        }
        let lower = word.to_lowercase();
        let replacement = match lower.as_str() {
            "don't" => Some("do not"),
            "doesn't" => Some("does not"),
            "didn't" => Some("did not"),
            "isn't" => Some("is not"),
            "aren't" => Some("are not"),
            "wasn't" => Some("was not"),
            "weren't" => Some("were not"),
            "can't" | "cannot" => Some("can not"),
            "won't" => Some("will not"),
            "wouldn't" => Some("would not"),
            "shouldn't" => Some("should not"),
            "couldn't" => Some("could not"),
            "haven't" => Some("have not"),
            "hasn't" => Some("has not"),
            "hadn't" => Some("had not"),
            "mustn't" => Some("must not"),
            _ => None,
        };
        output.push_str(replacement.unwrap_or(word));
        word.clear();
    };
    for character in text.chars() {
        if character.is_alphanumeric() || character == '\'' {
            word.push(character);
        } else {
            flush(&mut word, &mut output);
            output.push(character);
        }
    }
    flush(&mut word, &mut output);
    output
}

fn is_boundary(previous: Option<u8>) -> bool {
    previous.is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
}

fn parse_number(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    // Thousands separators: only consume commas followed by exactly three digits.
    while bytes.get(index) == Some(&b',') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        let mut digits = 0;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) && digits < 3 {
            index += 1;
            digits += 1;
        }
        if digits != 3 {
            break;
        }
    }
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut exponent = index + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let digits_start = exponent;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent += 1;
        }
        if exponent > digits_start {
            index = exponent;
        }
    }
    if bytes.get(index) == Some(&b'%') {
        index += 1;
    }
    // Compact fractions such as 3/4 are a single numeric token.
    if bytes.get(index) == Some(&b'/') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        let mut fraction = index + 1;
        while bytes.get(fraction).is_some_and(u8::is_ascii_digit) {
            fraction += 1;
        }
        index = fraction;
    }
    Some(index)
}

fn unit_after(bytes: &[u8], mut index: usize) -> Option<String> {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let start = index;
    while let Some(byte) = bytes.get(index) {
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'%' | b'\'' | b'"' | b'/' | b'*' | b'^' | b'-' | b'_' | b'~'
            )
            || *byte >= 0x80;
        if !allowed {
            break;
        }
        index += 1;
    }
    if index == start {
        return None;
    }
    let token = std::str::from_utf8(&bytes[start..index]).ok()?;
    if token
        .chars()
        .all(|character| character.is_ascii_digit() || matches!(character, '.' | ',' | '%'))
    {
        return None;
    }
    if !token
        .chars()
        .any(|character| character.is_alphabetic() || matches!(character, '%' | '°' | 'µ' | 'μ'))
    {
        return None;
    }
    Some(token.to_owned())
}

fn formula_view(normalized: &str) -> String {
    let mut view = normalized.to_owned();
    for operator in [
        "=", "^", "≤", "≥", "≠", "≈", "±", "×", "÷", "√", "∑", "∫", "→", "↔", "∧", "∨", "¬",
    ] {
        view = view.replace(&format!(" {operator} "), operator);
        view = view.replace(&format!("{operator} "), operator);
        view = view.replace(&format!(" {operator}"), operator);
    }
    view = view.replace(" + ", "+").replace(" - ", "-");
    view
}

fn is_formula_token(token: &str) -> bool {
    const OPERATORS: &[char] = &[
        '=', '^', '+', '-', '×', '÷', '√', '∑', '∫', '≤', '≥', '≠', '≈', '±', '∞', 'π', '→', '↔',
        '∧', '∨', '¬',
    ];
    // A signed number is a number, not a formula; otherwise `-7` would create
    // both a Number and a Formula token.
    let bytes = token.as_bytes();
    if parse_number(bytes, 0) == Some(bytes.len()) {
        return false;
    }
    token
        .chars()
        .any(|character| OPERATORS.contains(&character))
        || (token.contains('(')
            && token.contains(')')
            && token
                .chars()
                .any(|character| character.is_ascii_alphabetic()))
}

fn trim_word(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']'
        )
    })
}

fn negation_word(word: &str) -> Option<&'static str> {
    const NEGATIONS: &[&str] = &[
        "not",
        "no",
        "never",
        "none",
        "neither",
        "nor",
        "without",
        "except",
        "excluding",
        "lacks",
        "lacking",
        "absent",
        "hardly",
        "barely",
        "scarcely",
    ];
    let lower = word.to_lowercase();
    NEGATIONS
        .iter()
        .copied()
        .find(|negation| *negation == lower)
}

/// Extract critical tokens left to right. The order matters: "42 then 24" and
/// "24 then 42" must not share a fingerprint. Duplicates are retained.
pub fn critical_tokens(text: &str) -> Vec<CriticalToken> {
    let normalized = normalize_preserving_case(text);
    let bytes = normalized.as_bytes();
    let mut tokens = Vec::new();
    // Normalization below collapses presentation whitespace. Protect leading
    // indentation separately: in Python it can change program meaning.
    for (line_index, line) in text.lines().enumerate() {
        let prefix = line
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .collect::<String>();
        if !prefix.is_empty() {
            tokens.push(CriticalToken {
                kind: CriticalTokenKind::Indentation,
                canonical: format!("{line_index}:{}", hex::encode(prefix.as_bytes())),
            });
        }
    }
    let mut index = 0;
    while index < bytes.len() {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            index += 1;
        }
        let raw = &normalized[start..index];
        let word = trim_word(raw);
        if word.is_empty() {
            continue;
        }
        if is_formula_token(word) {
            tokens.push(CriticalToken {
                kind: CriticalTokenKind::Formula,
                canonical: word.to_owned(),
            });
            continue;
        }
        let word_bytes = word.as_bytes();
        let word_offset = start + (word.as_ptr() as usize - raw.as_ptr() as usize);
        let mut cursor = 0;
        while cursor < word_bytes.len() {
            let previous = cursor
                .checked_sub(1)
                .and_then(|offset| word_bytes.get(offset).copied());
            if is_boundary(previous)
                && (word_bytes[cursor].is_ascii_digit()
                    || (matches!(word_bytes[cursor], b'+' | b'-')
                        && word_bytes.get(cursor + 1).is_some_and(u8::is_ascii_digit)))
                && let Some(end) = parse_number(word_bytes, cursor)
            {
                let literal = std::str::from_utf8(&word_bytes[cursor..end])
                    .unwrap_or_default()
                    .to_owned();
                tokens.push(CriticalToken {
                    kind: CriticalTokenKind::Number,
                    canonical: literal,
                });
                if end == word_bytes.len()
                    && let Some(unit) = unit_after(bytes, word_offset + end)
                {
                    tokens.push(CriticalToken {
                        kind: CriticalTokenKind::Unit,
                        canonical: unit,
                    });
                }
                cursor = end;
                continue;
            }
            cursor += 1;
        }
        if let Some(negation) = negation_word(word) {
            tokens.push(CriticalToken {
                kind: CriticalTokenKind::Negation,
                canonical: negation.to_owned(),
            });
        }
    }
    tokens
}

pub fn fingerprint(text: &str) -> ChunkFingerprint {
    let normalized_text = normalize_text(text);
    let critical_tokens = critical_tokens(text);
    let mut critical_hasher = Sha256::new();
    for token in &critical_tokens {
        critical_hasher.update(format!("{}:{}\n", token.kind.label(), token.canonical).as_bytes());
    }
    let word_count = normalized_text
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .count() as u32;
    ChunkFingerprint {
        text_sha256: sha256_hex(normalized_text.as_bytes()),
        critical_sha256: hex::encode(critical_hasher.finalize()),
        normalized_text,
        critical_tokens,
        word_count,
    }
}

fn unique_words(text: &str) -> HashSet<&str> {
    text.split_whitespace().collect()
}

fn jaccard_millis(left: &str, right: &str) -> u32 {
    let left = unique_words(left);
    let right = unique_words(right);
    if left.is_empty() && right.is_empty() {
        return 1000;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    ((intersection as u64 * 1000) / union.max(1) as u64) as u32
}

pub fn compare(existing: &ChunkFingerprint, candidate: &ChunkFingerprint) -> DedupDecision {
    if existing.text_sha256 == candidate.text_sha256 {
        if existing.critical_sha256 == candidate.critical_sha256 {
            return DedupDecision::Duplicate {
                reason: DedupReason::ExactText,
                similarity_millis: 1000,
            };
        }
        return DedupDecision::Distinct;
    }
    // Any difference in a number, unit, formula, or negation blocks a merge.
    if existing.critical_sha256 != candidate.critical_sha256 {
        return DedupDecision::Distinct;
    }
    if existing.word_count < MINIMUM_NEAR_DUPLICATE_TOKENS as u32
        || candidate.word_count < MINIMUM_NEAR_DUPLICATE_TOKENS as u32
    {
        return DedupDecision::Distinct;
    }
    let similarity = jaccard_millis(&existing.normalized_text, &candidate.normalized_text);
    if similarity >= NEAR_DUPLICATE_THRESHOLD_MILLIS {
        DedupDecision::Duplicate {
            reason: DedupReason::NearText,
            similarity_millis: similarity,
        }
    } else {
        DedupDecision::Distinct
    }
}

/// Split lines without cutting through a token or rewriting their whitespace.
/// Line separators stay in the page's verbatim source_text; every character
/// inside a nonempty line is retained, including code indentation and tabs.
pub fn chunk_text(text: &str, maximum_chars: usize) -> Vec<String> {
    let maximum_chars = maximum_chars.clamp(120, 4000);
    let mut chunks = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.chars().count() <= maximum_chars {
            chunks.push(line.to_owned());
            continue;
        }
        let mut rest = line;
        while rest.chars().count() > maximum_chars {
            let boundary = rest
                .char_indices()
                .enumerate()
                .take(maximum_chars)
                .filter(|(_, (_, c))| c.is_whitespace())
                .map(|(_, (i, c))| i + c.len_utf8())
                .last()
                .or_else(|| {
                    rest.char_indices()
                        .find(|(_, c)| c.is_whitespace())
                        .map(|(i, c)| i + c.len_utf8())
                });
            let Some(boundary) = boundary else { break };
            chunks.push(rest[..boundary].to_owned());
            rest = &rest[boundary..];
        }
        if !rest.is_empty() {
            chunks.push(rest.to_owned());
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicate(left: &str, right: &str) -> Option<DedupReason> {
        match compare(&fingerprint(left), &fingerprint(right)) {
            DedupDecision::Duplicate { reason, .. } => Some(reason),
            DedupDecision::Distinct => None,
        }
    }

    #[test]
    fn numbers_are_exact_and_block_merges() {
        assert_eq!(
            duplicate(
                "The reaction yield is 42 percent in this trial.",
                "The reaction yield is 24 percent in this trial."
            ),
            None
        );
        assert_eq!(
            duplicate(
                "The reaction yield is 42 percent in this trial.",
                "The reaction yield is 42 percent in this trial."
            ),
            Some(DedupReason::ExactText)
        );
        let tokens = critical_tokens("Values: 3.14, 1,000, 42%, 1e-3, 3/4 and -7.");
        let mut numbers: Vec<&str> = tokens
            .iter()
            .filter(|token| token.kind == CriticalTokenKind::Number)
            .map(|token| token.canonical.as_str())
            .collect();
        numbers.sort_unstable();
        assert_eq!(numbers, ["-7", "1,000", "1e-3", "3.14", "3/4", "42%"]);
        // Reordering the same numbers changes the fact and must not merge.
        assert_eq!(
            duplicate(
                "The reaction yield is 42%, not 24%, in the final trial.",
                "The reaction yield is 24%, not 42%, in the final trial."
            ),
            None
        );
    }

    #[test]
    fn units_are_exact_and_block_merges() {
        assert_eq!(
            duplicate(
                "The sample mass is 5 kg in the laboratory.",
                "The sample mass is 5 g in the laboratory."
            ),
            None
        );
        let tokens = critical_tokens("Acceleration is 9.81 m/s^2 and temperature is 20 °C.");
        let units: Vec<&str> = tokens
            .iter()
            .filter(|token| token.kind == CriticalTokenKind::Unit)
            .map(|token| token.canonical.as_str())
            .collect();
        assert!(units.contains(&"m/s^2"));
        assert!(units.contains(&"°C"));
    }

    #[test]
    fn formulas_are_exact_and_block_merges() {
        assert_eq!(
            duplicate(
                "The expression is x^2 for this exercise.",
                "The expression is x^3 for this exercise."
            ),
            None
        );
        assert_eq!(
            duplicate(
                "The mass-energy relation is E = mc^2.",
                "The mass-energy relation is E=mc^2."
            ),
            Some(DedupReason::ExactText)
        );
        // Superscript OCR output must canonicalize to the same formula.
        assert_eq!(
            duplicate("The expression is x^2.", "The expression is x²."),
            Some(DedupReason::ExactText)
        );
    }

    #[test]
    fn negation_is_exact_and_blocks_merges() {
        assert_eq!(
            duplicate(
                "The net force on the object is zero.",
                "The net force on the object is not zero."
            ),
            None
        );
        assert_eq!(
            duplicate(
                "The net force on the object is not zero.",
                "The net force on the object isn't zero."
            ),
            Some(DedupReason::ExactText)
        );
        assert_eq!(
            duplicate(
                "The sample is never heated during this step.",
                "The sample is heated during this step."
            ),
            None
        );
    }

    #[test]
    fn near_duplicates_merge_only_with_identical_critical_tokens() {
        assert_eq!(
            duplicate(
                "The cell membrane controls what enters and leaves the cell.",
                "The cell membrane controls what enters and leaves a cell."
            ),
            Some(DedupReason::NearText)
        );
        assert_eq!(
            duplicate(
                "The cell membrane controls 3 what enters and leaves the cell.",
                "The cell membrane controls 4 what enters and leaves the cell."
            ),
            None
        );
    }

    #[test]
    fn chunking_never_splits_tokens_and_preserves_content() {
        let text = "First line.\nSecond line has 42% and E = mc^2.";
        let chunks = chunk_text(text, 120);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "First line.");
        assert_eq!(chunks[1], "Second line has 42% and E = mc^2.");
        let long = "word ".repeat(200);
        let chunks = chunk_text(&long, 120);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 120 + 5));
        assert_eq!(chunks.join(" ").split_whitespace().count(), 200);
        let code = "    return  x\t!= 5";
        assert_eq!(chunk_text(code, 120), [code]);
        let long_code = format!("    {}", "x\t  ".repeat(100));
        assert_eq!(chunk_text(&long_code, 120).concat(), long_code);
        assert_eq!(duplicate("    return x", "return x"), None);
        assert_eq!(duplicate("\treturn x", "    return x"), None);
    }
}
