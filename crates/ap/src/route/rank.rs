//! The match of the function `route` (`specs/ap/route.md` R19 to R21): the words of a question against the
//! heading, path and text of each section and the name and purpose of each component.
use std::collections::{BTreeMap, BTreeSet};

/// Common English words, which match nothing.
const COMMON: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "an", "and", "any", "are", "as", "at", "be",
    "been", "before", "both", "but", "by", "can", "do", "does", "done", "each", "for", "from",
    "get", "got", "had", "has", "have", "he", "her", "his", "how", "i", "if", "in", "into", "is",
    "it", "its", "made", "make", "may", "more", "must", "no", "not", "of", "on", "once", "one",
    "only", "or", "other", "our", "out", "over", "said", "say", "says", "she", "should", "so",
    "some", "such", "than", "that", "the", "their", "them", "then", "there", "these", "they",
    "this", "those", "through", "to", "under", "up", "upon", "us", "was", "we", "were", "what",
    "when", "where", "which", "while", "who", "whom", "why", "will", "with", "would", "you",
    "your", "yours",
];
/// The characters kept inside a word beside letters and digits.
const INSIDE: [char; 5] = ['-', '_', '.', '/', ':'];
/// The weight of a word found in the heading (or a component's name), in the path, in the text.
const HEADING: f64 = 3.0;
const PATH: f64 = 2.0;
const TEXT: f64 = 1.0;

/// The words of a text (R19): lower case, split at every character that is not a letter, a digit or one of
/// `-_./:`, those five trimmed from both ends; a word of one character and a common English word dropped.
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || INSIDE.contains(&c)))
        .map(|w| w.trim_matches(INSIDE))
        .filter(|w| w.chars().count() > 1 && !COMMON.contains(w))
        .map(str::to_owned)
        .collect()
}

/// The form a word is compared in (R19): a word of ASCII letters only loses one ending `ing`, `ed`, `es` or
/// `s` when at least three letters remain; any other word is compared as it is.
pub fn stem(word: &str) -> String {
    if word.bytes().all(|b| b.is_ascii_lowercase()) {
        for end in ["ing", "ed", "es", "s"] {
            if let Some(rest) = word.strip_suffix(end)
                && rest.len() >= 3
            {
                return rest.to_owned();
            }
        }
    }
    word.to_owned()
}

fn stems(text: &str) -> BTreeSet<String> {
    words(text).iter().map(|w| stem(w)).collect()
}

/// What the match reads of one answer: the heading (a component's name), path and text (a component's
/// purpose; it has no path to match).
pub struct Matched<'a> {
    pub heading: &'a str,
    pub path: &'a str,
    pub text: &'a str,
}

/// Each candidate's score and the words of `question` it matched (R20), in the candidates' order; a
/// candidate that matched no word has the score 0. A word weighs its field (heading 3, path 2, text 1,
/// the highest field it is in) times ln(1 + N / n), N the candidates and n those that hold the word.
pub fn score(candidates: &[Matched], question: &str) -> Vec<(f64, Vec<String>)> {
    let fields: Vec<[BTreeSet<String>; 3]> = candidates
        .iter()
        .map(|c| [stems(c.heading), stems(c.path), stems(c.text)])
        .collect();
    let mut holding: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &fields {
        let all: BTreeSet<&str> = f.iter().flatten().map(String::as_str).collect();
        for w in all {
            *holding.entry(w).or_default() += 1;
        }
    }
    let total = candidates.len() as f64;
    let mut asked: Vec<(String, String)> = vec![];
    for w in words(question) {
        let s = stem(&w);
        if !asked.iter().any(|(_, t)| *t == s) {
            asked.push((w, s));
        }
    }
    fields
        .iter()
        .map(|[heading, path, text]| {
            let mut sum = 0.0;
            let mut matched = vec![];
            for (word, s) in &asked {
                let weight = if heading.contains(s) {
                    HEADING
                } else if path.contains(s) {
                    PATH
                } else if text.contains(s) {
                    TEXT
                } else {
                    continue;
                };
                let n = holding.get(s.as_str()).copied().unwrap_or(1) as f64;
                sum += weight * (1.0 + total / n).ln();
                matched.push(word.clone());
            }
            (sum, matched)
        })
        .collect()
}
