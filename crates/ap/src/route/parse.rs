//! The sections of one Markdown file (`specs/ap/route.md` R9 to R12): one per ATX heading outside fenced code,
//! one for the text above the first heading, or one for a whole file without a heading; their keys and bounds.
use serde_json::json;

/// At most this many bytes of a section's words are kept (R11).
pub const TEXT_BYTES: usize = 4000;
/// At most this many bytes of a heading path are kept (R11).
pub const HEADING_BYTES: usize = 1000;
/// Parent headings are joined by this in a heading path.
pub const SEPARATOR: &str = " > ";

/// One section of a file: its heading path, its lines (from 1, both included) and its ordinal among the
/// sections of the file with an equal heading path (from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: String,
    pub first_line: usize,
    pub last_line: usize,
    pub ordinal: usize,
}

/// The lines of a text: split at `\n`, a `\r` before it dropped; a final newline ends the last line.
pub fn lines(text: &str) -> Vec<&str> {
    let mut all: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if text.ends_with('\n') || text.is_empty() {
        all.pop();
    }
    all
}

/// Leading spaces of a line, when there are at most three, and the rest.
fn indented(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(' ');
    (line.len() - rest.len() <= 3).then_some(rest)
}

/// A fence opening or closing: its character, its length and what follows it.
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let rest = indented(line)?;
    let c = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = rest.chars().take_while(|x| *x == c).count();
    (n >= 3).then(|| (c, n, &rest[n..]))
}

/// An ATX heading: its level and its text without the closing sequence.
fn heading(line: &str) -> Option<(usize, String)> {
    let rest = indented(line)?;
    let level = rest.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &rest[level..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let mut text = after.trim_matches([' ', '\t']);
    let closed = text.trim_end_matches('#');
    if closed.is_empty() || closed.ends_with(' ') || closed.ends_with('\t') {
        text = closed.trim_end_matches([' ', '\t']);
    }
    Some((level, clean(text)))
}

/// Control characters made spaces.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The longest start of `text` of at most `max` bytes that ends at a character boundary.
pub fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The words of lines: each run of white space and control characters one space, at most `TEXT_BYTES`.
pub fn text_of(lines: &[&str]) -> String {
    let joined = clean(&lines.join(" "));
    let words: Vec<&str> = joined.split_whitespace().collect();
    cut(&words.join(" "), TEXT_BYTES).to_owned()
}

/// The sections of a file whose lines are `lines` and whose name (its last path component) is `name` (R9, R10).
pub fn sections(lines: &[&str], name: &str) -> Vec<Section> {
    let mut heads = vec![];
    let mut open: Option<(char, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((c, n)) = open {
            if let Some((d, m, rest)) = fence(line)
                && d == c
                && m >= n
                && rest.trim().is_empty()
            {
                open = None;
            }
            continue;
        }
        if let Some((c, n, info)) = fence(line)
            && !(c == '`' && info.contains('`'))
        {
            open = Some((c, n));
            continue;
        }
        if let Some((level, text)) = heading(line) {
            heads.push((i, level, text));
        }
    }
    let blank = |from: usize, to: usize| lines[from..to].iter().all(|l| l.trim().is_empty());
    let mut found: Vec<(String, usize, usize)> = vec![];
    let first = heads.first().map_or(lines.len(), |h| h.0);
    if first > 0 && !blank(0, first) {
        found.push((clean(name), 1, first));
    }
    let mut stack: Vec<(usize, String)> = vec![];
    for (k, (i, level, text)) in heads.iter().enumerate() {
        while stack.last().is_some_and(|(l, _)| l >= level) {
            stack.pop();
        }
        stack.push((*level, text.clone()));
        let end = heads[k + 1..]
            .iter()
            .find(|(_, l, _)| l <= level)
            .map_or(lines.len(), |h| h.0);
        let path: Vec<&str> = stack.iter().map(|(_, t)| t.as_str()).collect();
        found.push((path.join(SEPARATOR), i + 1, end));
    }
    let mut out: Vec<Section> = vec![];
    for (heading, first_line, last_line) in found {
        let heading = cut(&heading, HEADING_BYTES).to_owned();
        let ordinal = 1 + out.iter().filter(|s| s.heading == heading).count();
        out.push(Section {
            heading,
            first_line,
            last_line,
            ordinal,
        });
    }
    out
}

/// A section's key (R12): the hex SHA-256 of the JSON array of its origin (`owner/name`, or `instructions`),
/// path, heading path and ordinal.
pub fn key(origin: &str, path: &str, heading: &str, ordinal: usize) -> String {
    crate::host::digest(
        json!([origin, path, heading, ordinal])
            .to_string()
            .as_bytes(),
    )
}
