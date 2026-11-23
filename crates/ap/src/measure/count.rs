//! The counts of R7 (`specs/ap/measure.md`): what one tool call reads and writes, and the refusals one tool result
//! shows. Only counts leave this file: no command, input or text.
use serde_json::Value;
use std::collections::BTreeMap;

/// The reads and writes of one call or one day.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counts {
    pub file_reads: u64,
    pub file_writes: u64,
    pub store_reads: u64,
    pub store_writes: u64,
}
impl Counts {
    pub fn add(&mut self, o: Counts) {
        self.file_reads += o.file_reads;
        self.file_writes += o.file_writes;
        self.store_reads += o.store_reads;
        self.store_writes += o.store_writes;
    }
}

const STORE_READS: [&str; 7] = [
    "ont_describe",
    "ont_get",
    "ont_search",
    "ont_log",
    "ont_fn",
    "ont_action_list",
    "ont_action_validate",
];

/// What one `tool_use` block of the tool `name` with the input `input` counts. `REPL` and `Poll`, the carriers of a
/// code-mode script, count nothing: the calls of the script are lines of their own.
pub fn of_tool(name: &str, input: &Value) -> Counts {
    let mut c = Counts::default();
    match name {
        "Read" | "Grep" | "Glob" => c.file_reads = 1,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => c.file_writes = 1,
        "Bash" => return of_bash(input["command"].as_str().unwrap_or_default()),
        _ => match name.strip_prefix("mcp__ont__") {
            Some("ont_action_apply") => c.store_writes = 1,
            Some(tool) if STORE_READS.contains(&tool) => c.store_reads = 1,
            _ => {}
        },
    }
    c
}

/// The parts of a shell command: split at `&&`, `||`, `;`, `|` and line breaks outside quotes.
fn parts(command: &str) -> Vec<String> {
    let (mut out, mut cur) = (vec![], String::new());
    let (mut single, mut double) = (false, false);
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !single => {
                cur.push(ch);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            '\'' if !double => {
                single = !single;
                cur.push(ch);
            }
            '"' if !single => {
                double = !double;
                cur.push(ch);
            }
            '\n' => {
                // A line break ends a part even inside an open quote: a heredoc's line is read as a part.
                (single, double) = (false, false);
                out.push(std::mem::take(&mut cur));
            }
            ';' | '|' if !single && !double => {
                if ch == '|' && chars.peek() == Some(&'|') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '&' if !single && !double && chars.peek() == Some(&'&') => {
                chars.next();
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

fn assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Whether a part redirects (`>` or `>>`, outside quotes) to a path other than `/dev/null`; a target `&<n>` is a
/// descriptor, no path.
fn redirects(part: &str) -> bool {
    let b = part.as_bytes();
    let (mut single, mut double, mut i) = (false, false, 0);
    while i < b.len() {
        match b[i] {
            b'\\' if !single => i += 1,
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'>' if !single && !double => {
                let mut j = i + 1;
                if b.get(j) == Some(&b'>') {
                    j += 1;
                }
                let rest = part[j..].trim_start();
                let target: String = rest
                    .chars()
                    .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '|' | '&' | ')'))
                    .filter(|c| !matches!(c, '"' | '\''))
                    .collect();
                if !rest.starts_with('&') && !target.is_empty() && target != "/dev/null" {
                    return true;
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// The words after the leading `cd <dir>`, `set`, `export`, `time` and assignments.
fn command_words(part: &str) -> Vec<&str> {
    let mut words: Vec<&str> = part.split_whitespace().collect();
    loop {
        match words.first() {
            Some(&"cd") => {
                words.drain(..words.len().min(2));
            }
            Some(&("set" | "export" | "time")) => {
                words.remove(0);
            }
            Some(w) if assignment(w) => {
                words.remove(0);
            }
            _ => return words,
        }
    }
}

/// Skips a program's options with a value before its verb (`git -C <dir>`, `ont --store <path>`).
fn verb_after<'a>(rest: &[&'a str], with_value: &[&str]) -> (Option<&'a str>, Option<&'a str>) {
    let mut i = 0;
    while let Some(w) = rest.get(i) {
        if with_value.contains(w) {
            i += 2;
        } else if with_value.iter().any(|o| w.starts_with(&format!("{o}="))) {
            i += 1;
        } else {
            break;
        }
    }
    (rest.get(i).copied(), rest.get(i + 1).copied())
}

const AP_WRITES: [&str; 7] = [
    "request", "intent", "deliver", "run", "api", "resource", "measure",
];
const UNIT_WRITES: [&str; 9] = [
    "add", "amend", "ready", "drop", "review", "approve", "reject", "run", "write",
];
const ONT_READS: [&str; 7] = [
    "search",
    "get",
    "aggregate",
    "describe",
    "log",
    "around",
    "fn",
];
const GIT_WRITES: [&str; 4] = ["add", "commit", "mv", "rm"];
const GIT_READS: [&str; 10] = [
    "show",
    "log",
    "diff",
    "grep",
    "ls-tree",
    "status",
    "rev-parse",
    "cat-file",
    "ls-files",
    "blame",
];
const WRITERS: [&str; 8] = [
    "tee", "mv", "cp", "rm", "mkdir", "install", "chmod", "touch",
];
const READERS: [&str; 15] = [
    "cat",
    "sed",
    "grep",
    "rg",
    "head",
    "tail",
    "wc",
    "ls",
    "find",
    "awk",
    "jq",
    "diff",
    "sha256sum",
    "stat",
    "du",
];

/// What one `Bash` command counts (R7): each part once by its first words, and one file write more for a part that
/// is not `ap` or `ont` and redirects to a path.
pub fn of_bash(command: &str) -> Counts {
    let mut c = Counts::default();
    for part in parts(command) {
        let words = command_words(&part);
        let Some((first, rest)) = words.split_first() else {
            continue;
        };
        let program = first.rsplit('/').next().unwrap_or(first);
        match program {
            "ap" => {
                let (verb, sub) = (rest.first().copied(), rest.get(1).copied());
                match verb {
                    Some(v) if AP_WRITES.contains(&v) => c.store_writes += 1,
                    Some("unit") if sub.is_some_and(|s| UNIT_WRITES.contains(&s)) => {
                        c.store_writes += 1;
                    }
                    Some("status") => c.store_reads += 1,
                    _ => {}
                }
            }
            "ont" => match verb_after(rest, &["--store", "--now", "--client"]).0 {
                Some("action") => c.store_writes += 1,
                Some(v) if ONT_READS.contains(&v) => c.store_reads += 1,
                _ => {}
            },
            _ => {
                match program {
                    "git" => match verb_after(rest, &["-C", "-c"]).0 {
                        Some(v) if GIT_WRITES.contains(&v) => c.file_writes += 1,
                        Some(v) if GIT_READS.contains(&v) => c.file_reads += 1,
                        _ => {}
                    },
                    "sed"
                        if rest
                            .iter()
                            .any(|w| w.starts_with("-i") || w.starts_with("--in-place")) =>
                    {
                        c.file_writes += 1;
                    }
                    p if WRITERS.contains(&p) => c.file_writes += 1,
                    p if READERS.contains(&p) => c.file_reads += 1,
                    _ => {}
                }
                if redirects(&part) {
                    c.file_writes += 1;
                }
            }
        }
    }
    c
}

/// The objects of a JSON value that are refusals: `{"error":{...}}` with a string `code` (of `ap`) and an object with
/// a string `code` and `"status":"refused"` (of `ont`), as `(code, field)`.
fn collect(v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(o) => {
            let field = |x: &Value| x["field"].as_str().unwrap_or_default().to_owned();
            if let Some(e) = o.get("error").filter(|e| e.is_object())
                && let Some(code) = e["code"].as_str()
            {
                out.push((code.to_owned(), field(e)));
            }
            if let Some(code) = o.get("code").and_then(Value::as_str)
                && o.get("status").and_then(Value::as_str) == Some("refused")
            {
                out.push((code.to_owned(), field(v)));
            }
            o.values().for_each(|x| collect(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        _ => {}
    }
}

fn found_in(text: &str) -> BTreeMap<(String, String), u64> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(at) = text[i..].find('{') {
        let from = i + at;
        let opens = text[from + 1..].trim_start();
        if opens.starts_with('"') {
            let mut stream = serde_json::Deserializer::from_str(&text[from..]).into_iter::<Value>();
            if let Some(Ok(v)) = stream.next() {
                collect(&v, &mut out);
                i = from + stream.byte_offset().max(1);
                continue;
            }
        }
        i = from + 1;
    }
    let mut counted = BTreeMap::new();
    for k in out {
        *counted.entry(k).or_insert(0) += 1;
    }
    counted
}

/// The refusals one tool result's text shows (R7), by code and field: as the text is, or with its escaped quotes
/// read as quotes (a result that carries another's as a JSON string), whichever shows more of each.
pub fn refusals(text: &str) -> BTreeMap<(String, String), u64> {
    let mut plain = found_in(text);
    if text.contains("\\\"") {
        for (k, n) in found_in(&text.replace("\\\\", "\\").replace("\\\"", "\"")) {
            let e = plain.entry(k).or_insert(0);
            *e = (*e).max(n);
        }
    }
    plain
}
