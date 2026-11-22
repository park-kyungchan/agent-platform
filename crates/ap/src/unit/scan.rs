//! The scan of the added lines for a secret (`specs/ap/unit-run.md` R44): the five forms, the
//! admissions of a unit's `notes`, the reading of the diff between the base and the head one path at
//! a time, and the detail of a stop. No byte of a hit line, of its value or of its digest leaves
//! this module: a hit is a path, a line number and a form name.
use crate::host::{System, args, digest};
use crate::review::{self, DIFF_OPTIONS, GIT_SAFE};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The most bytes of the JSON text of a `secret_in_diff` detail (R22's cut never falls inside it).
const DETAIL: usize = 1000;
/// The most hits a detail names.
const NAMED: usize = 10;
/// The prefix of an admission line in a unit's `notes`.
const ADMITTED: &str = "Admitted line: ";
/// What the scan ends with when it stops the unit: the stop cause and its detail.
#[derive(Debug, PartialEq)]
pub struct Ended {
    pub cause: &'static str,
    pub detail: String,
}
/// One added line that holds a form and that no admission takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: String,
    pub line: u64,
    pub form: &'static str,
}

fn name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}
/// The length of the run of bytes of `bytes` from `from` that `class` accepts.
fn run(bytes: &[u8], from: usize, class: impl Fn(u8) -> bool) -> usize {
    bytes[from..].iter().take_while(|b| class(**b)).count()
}
/// Whether `bytes` holds the header of a private key from `at`.
fn private_key(bytes: &[u8], at: usize) -> bool {
    const BEGIN: &[u8] = b"-----BEGIN ";
    const KEY: &[u8] = b"PRIVATE KEY";
    if !bytes[at..].starts_with(BEGIN) {
        return false;
    }
    let from = at + BEGIN.len();
    let n = run(bytes, from, |b| {
        b.is_ascii_uppercase() || b.is_ascii_digit() || b == b' '
    });
    bytes[from..from + n].windows(KEY.len()).any(|w| w == KEY)
}
/// Whether the prefix `p` is at `at`, followed by a run of at least `min` bytes of `class`.
fn prefixed(bytes: &[u8], at: usize, p: &[u8], min: usize, class: fn(u8) -> bool) -> bool {
    bytes[at..].starts_with(p) && run(bytes, at + p.len(), class) >= min
}
fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn alnum_underscore(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn alnum_underscore_dash(b: u8) -> bool {
    name_byte(b)
}
fn alnum_dash(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}
fn github_token(bytes: &[u8], at: usize) -> bool {
    let rest = &bytes[at..];
    let classic = rest.len() > 3
        && rest.starts_with(b"gh")
        && b"opsur".contains(&rest[2])
        && rest[3] == b'_'
        && run(bytes, at + 4, alnum) >= 36;
    classic || prefixed(bytes, at, b"github_pat_", 22, alnum_underscore)
}
fn model_key(bytes: &[u8], at: usize) -> bool {
    prefixed(bytes, at, b"sk-", 32, alnum_underscore_dash)
        || prefixed(bytes, at, b"AIza", 35, alnum_underscore_dash)
}
fn tailnet_key(bytes: &[u8], at: usize) -> bool {
    prefixed(bytes, at, b"tskey-", 20, alnum_dash)
}
/// The words of a name that make a value a secret.
const NAMES: [&str; 9] = [
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "api-key",
    "private_key",
    "access_key",
];
fn value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"+/=_.~-".contains(&b)
}
fn blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}
fn quote(b: u8) -> bool {
    b == b'"' || b == b'\''
}
/// For each offset, the end of the run of value bytes from it and whether the run's rest from the
/// offset holds a digit and a letter, so that a long line is read once.
struct Values {
    end: Vec<usize>,
    digit: Vec<bool>,
    letter: Vec<bool>,
}
impl Values {
    fn of(bytes: &[u8]) -> Values {
        let n = bytes.len();
        let mut v = Values {
            end: vec![n; n + 1],
            digit: vec![false; n + 1],
            letter: vec![false; n + 1],
        };
        for i in (0..n).rev() {
            if value_byte(bytes[i]) {
                v.end[i] = v.end[i + 1];
                v.digit[i] = bytes[i].is_ascii_digit() || v.digit[i + 1];
                v.letter[i] = bytes[i].is_ascii_alphabetic() || v.letter[i + 1];
            } else {
                v.end[i] = i;
            }
        }
        v
    }
    /// Whether the run of value bytes that starts at `at` is a value of (e).
    fn secret(&self, at: usize) -> bool {
        self.end[at] - at >= 32 && self.digit[at] && self.letter[at]
    }
}
/// Whether the name `bytes[start..end]` holds a word of `NAMES` and is followed by a separator and
/// a value of (e).
fn assigned(bytes: &[u8], (start, end): (usize, usize), values: &Values) -> bool {
    let name = bytes[start..end].to_ascii_lowercase();
    if !NAMES
        .iter()
        .any(|n| name.windows(n.len()).any(|w| w == n.as_bytes()))
    {
        return false;
    }
    let mut at = end;
    if bytes.get(at).is_some_and(|b| quote(*b)) {
        at += 1;
    }
    at += run(bytes, at, blank);
    // `:=`, `=` and `:` are each tried: the value may begin with the `=` of a `:`.
    [&b":="[..], b"=", b":"].iter().any(|sep| {
        if !bytes[at..].starts_with(sep) {
            return false;
        }
        let mut v = at + sep.len();
        v += run(bytes, v, blank);
        if bytes.get(v).is_some_and(|b| quote(*b)) {
            v += 1;
        }
        values.secret(v)
    })
}
fn assigned_value(bytes: &[u8]) -> bool {
    let mut values = None;
    let mut i = 0;
    while i < bytes.len() {
        let n = run(bytes, i, |b| {
            b.is_ascii_alphanumeric() || b"_.-".contains(&b)
        });
        if n == 0 {
            i += 1;
            continue;
        }
        let values = values.get_or_insert_with(|| Values::of(bytes));
        if assigned(bytes, (i, i + n), values) {
            return true;
        }
        i += n;
    }
    false
}
/// The first form, in the order of the specification, that the line holds.
pub fn form(line: &[u8]) -> Option<&'static str> {
    let at_boundary = |i: usize| i == 0 || !name_byte(line[i - 1]);
    let any = |f: fn(&[u8], usize) -> bool| (0..line.len()).any(|i| f(line, i));
    let any_bounded =
        |f: fn(&[u8], usize) -> bool| (0..line.len()).any(|i| at_boundary(i) && f(line, i));
    if any(private_key) {
        Some("private_key")
    } else if any_bounded(github_token) {
        Some("github_token")
    } else if any_bounded(model_key) {
        Some("model_key")
    } else if any_bounded(tailnet_key) {
        Some("tailnet_key")
    } else if assigned_value(line) {
        Some("assigned_value")
    } else {
        None
    }
}

/// The admissions of `notes`: the pairs (path, digest) of its lines of the exact form
/// `Admitted line: <path> <64 lower-case hexadecimal digits>`. Any other line admits nothing.
pub fn admissions(notes: &str) -> HashSet<(String, String)> {
    let mut found = HashSet::new();
    for line in notes.split('\n') {
        let Some(rest) = line.strip_prefix(ADMITTED) else {
            continue;
        };
        let Some(space) = rest.rfind(' ') else {
            continue;
        };
        let (path, hex) = (&rest[..space], &rest[space + 1..]);
        let hex_ok = hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !path.is_empty() && hex_ok {
            found.insert((path.to_owned(), hex.to_owned()));
        }
    }
    found
}

/// The added lines of one path's `-U0` diff: their line numbers and bytes, from the first hunk
/// header on. A hunk ends when the counts of its header are used up, so the header lines of a
/// second section of the reply (a type change shows a deletion and an addition) are never read as
/// lines. `None` when a header is not of the form `@@ -a[,b] +c[,d] @@` or a hunk does not hold
/// the lines its header counts.
pub fn added(reply: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    let mut lines = vec![];
    // The line number of the next added line and the old and new lines the hunk still counts.
    let (mut next, mut old, mut new) = (0u64, 0u64, 0u64);
    for line in reply.split(|b| *b == b'\n') {
        if old == 0 && new == 0 {
            if line.starts_with(b"@@ ") {
                (next, old, new) = hunk(line)?;
            }
        } else if line.starts_with(b"\\") {
            continue;
        } else if let Some(text) = line.strip_prefix(b"+") {
            new = new.checked_sub(1)?;
            lines.push((next, text));
            next += 1;
        } else if line.starts_with(b"-") {
            old = old.checked_sub(1)?;
        } else {
            return None;
        }
    }
    (old == 0 && new == 0).then_some(lines)
}
/// The first new line number `c` and the counts of the old and new lines of a hunk header
/// `@@ -a[,b] +c[,d] @@` (a count that is left out is 1); the text after the second `@@` is not
/// read (it need not be UTF-8).
fn hunk(header: &[u8]) -> Option<(u64, u64, u64)> {
    let mut words = header.strip_prefix(b"@@ -")?.splitn(3, |b| *b == b' ');
    let (old, new, tail) = (words.next()?, words.next()?, words.next()?);
    let number = |t: &[u8]| {
        let digits = !t.is_empty() && t.iter().all(u8::is_ascii_digit);
        digits.then(|| std::str::from_utf8(t).ok()?.parse::<u64>().ok())?
    };
    let pair = |t: &[u8]| {
        let mut parts = t.split(|b| *b == b',');
        let first = number(parts.next()?)?;
        let count = parts.next().map_or(Some(1), number)?;
        parts.next().is_none().then_some((first, count))
    };
    let (_, removed) = pair(old)?;
    let (start, added) = pair(new.strip_prefix(b"+")?)?;
    tail.starts_with(b"@@").then_some((start, removed, added))
}

/// The detail of a stop on `hits`: the first of them that keep the JSON text within 1,000 bytes.
pub fn detail(hits: &[Hit], count: usize) -> String {
    let mut lines: Vec<Value> = vec![];
    for hit in hits.iter().take(NAMED) {
        let one = json!({"path": hit.path, "line": hit.line, "form": hit.form});
        lines.push(one);
        if json!({"lines": lines, "count": count}).to_string().len() > DETAIL {
            lines.pop();
            break;
        }
    }
    json!({"lines": lines, "count": count}).to_string()
}

/// R44: reads the lines the head adds, one path at a time, and ends with the stop that names the
/// first failed reading, else the hits no admission of `notes` takes.
pub fn scan(
    s: &mut impl System,
    worktree: &str,
    base: &str,
    head: &str,
    notes: &str,
) -> Result<(), Ended> {
    let git_failed = |words: &[&str]| Ended {
        cause: "git_failed",
        detail: words.join(" "),
    };
    let mut words = vec!["diff"];
    words.extend(
        DIFF_OPTIONS
            .into_iter()
            .chain(["--diff-filter=d", base, head]),
    );
    let list = review::git(s, worktree, &words).map_err(|_| git_failed(&words))?;
    let admitted = admissions(notes);
    let (mut named, mut count) = (vec![], 0);
    for path in list.split_terminator('\0') {
        let mut argv = args(&["--no-replace-objects", "--literal-pathspecs"]);
        argv.push(format!("--attr-source={base}"));
        argv.extend(args(&[
            "-c",
            "core.attributesFile=/dev/null",
            "-C",
            worktree,
        ]));
        argv.extend(args(&GIT_SAFE));
        let diff = [
            "diff",
            "--no-renames",
            "--ignore-submodules=none",
            "--no-relative",
            "-U0",
            "--inter-hunk-context=0",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--diff-filter=d",
            base,
            head,
            "--",
            path,
        ];
        argv.extend(args(&diff));
        let reply = match s.call("git", &argv, &Value::Null, 30_000) {
            Ok(reply) if reply.success => reply,
            Ok(_) => return Err(git_failed(&diff)),
            Err(_) => {
                return Err(Ended {
                    cause: "diff_unread",
                    detail: json!({"path": path}).to_string(),
                });
            }
        };
        let lines = added(&reply.stdout).ok_or_else(|| git_failed(&diff))?;
        for (number, text) in lines {
            let Some(form) = form(text) else { continue };
            if admitted.contains(&(path.to_owned(), digest(text))) {
                continue;
            }
            count += 1;
            if named.len() < NAMED {
                named.push(Hit {
                    path: path.to_owned(),
                    line: number,
                    form,
                });
            }
        }
    }
    match count {
        0 => Ok(()),
        _ => Err(Ended {
            cause: "secret_in_diff",
            detail: detail(&named, count),
        }),
    }
}
