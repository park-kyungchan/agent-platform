//! `ap unit`: the unit object and its runs (`object`, `drive`, `page_review`, `page_write`), and with a
//! request on standard input the case call (README "One case") or the refusal of a request of the unit
//! object (`specs/ap/unit.md` R1). Any other form is refused as an argument list that reaches no verb
//! (`specs/ap/verbs.md` R4).
use crate::host::{Failure, Host, LIMIT, System, error, fail, require};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{Read, Write},
    process::ExitCode,
};

mod case;
pub mod drive;
pub mod object;
pub mod page_review;
pub mod page_write;
pub mod render;
pub mod routing;
pub mod scan;

/// The refusal of an argument list that reaches no verb (`specs/ap/verbs.md` R4), byte for byte.
const UNKNOWN: &[u8] = b"{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}\n";

/// The bytes of the regular file at `path`, at most `LIMIT`; else `code` with `field`.
pub(crate) fn regular(path: &str, code: &'static str, field: &str) -> Result<Vec<u8>, Failure> {
    let bad = || error(code, field);
    let f = std::fs::File::open(path).map_err(|_| bad())?;
    let meta = f.metadata().map_err(|_| bad())?;
    require!(meta.is_file() && meta.len() <= LIMIT => code, field);
    let mut bytes = vec![];
    f.take(LIMIT).read_to_end(&mut bytes).map_err(|_| bad())?;
    Ok(bytes)
}
/// The head line of a verdict: exactly one run of 40 hexadecimal characters, and it is the head.
fn names_head(line: Option<&str>, head: &str) -> bool {
    let mut heads = line
        .unwrap_or_default()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    heads.next() == Some(head) && heads.next().is_none()
}
/// The most bytes of a `gh` reply that a detail keeps: its end, where `gh` says why.
const REPLY: usize = 200;
/// What replaces a credential in a reply.
const REDACTED: &str = "<redacted>";
/// The text of a failed `gh` call as a detail may hold it: every token, password of a URL, `…TOKEN=`
/// value and value after `token` or `Bearer` replaced by `<redacted>`, then its last 200 bytes cut
/// at a character boundary. The one redaction of every `gh` reply that reaches a detail.
pub(crate) fn redact(bytes: &[u8]) -> String {
    let mut t = String::from_utf8_lossy(bytes).into_owned();
    for find in [
        gh_token,
        github_pat,
        url_password,
        token_variable,
        token_value,
    ] {
        t = replaced(&t, find);
    }
    let mut from = t.len().saturating_sub(REPLY);
    while !t.is_char_boundary(from) {
        from += 1;
    }
    t[from..].to_owned()
}
/// A matcher of the redaction: at byte `i`, the start and the end of the part a match replaces.
type Matcher = fn(&[u8], usize) -> Option<(usize, usize)>;
/// One pass of `find` over `t`: at each character, `find(bytes, i)` gives the start and the end of
/// the part a match starting at `i` replaces, the text before that start kept.
fn replaced(t: &str, find: Matcher) -> String {
    let b = t.as_bytes();
    let (mut out, mut i) = (String::with_capacity(t.len()), 0);
    while i < b.len() {
        if let Some((from, to)) = find(b, i) {
            out.push_str(&t[i..from]);
            out.push_str(REDACTED);
            i = to;
            continue;
        }
        let next = (i + 1..=b.len())
            .find(|j| t.is_char_boundary(*j))
            .unwrap_or(b.len());
        out.push_str(&t[i..next]);
        i = next;
    }
    out
}
/// `[[:space:]]` of a POSIX class: no part of a character of more than one byte.
fn space(c: u8) -> bool {
    b" \t\n\x0b\x0c\r".contains(&c)
}
/// The end of the run from `i` of bytes that `keep` takes.
fn run_of(b: &[u8], i: usize, keep: impl Fn(u8) -> bool) -> usize {
    (i..b.len()).find(|j| !keep(b[*j])).unwrap_or(b.len())
}
/// The end of a run of at least one byte that `keep` takes, from `i`.
fn some(b: &[u8], i: usize, keep: impl Fn(u8) -> bool) -> Option<usize> {
    Some(run_of(b, i, keep)).filter(|end| *end > i)
}
/// `gh[opsur]_[A-Za-z0-9]+`.
fn gh_token(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let r = b.get(i..)?;
    let head = r.starts_with(b"gh") && r.get(2).is_some_and(|c| b"opsur".contains(c));
    let end = some(b, i + 4, |c| c.is_ascii_alphanumeric())
        .filter(|_| head && r.get(3) == Some(&b'_'))?;
    Some((i, end))
}
/// `github_pat_[A-Za-z0-9_]+`.
fn github_pat(b: &[u8], i: usize) -> Option<(usize, usize)> {
    const P: &[u8] = b"github_pat_";
    let at = b.get(i..)?.starts_with(P).then_some(i + P.len())?;
    let end = some(b, at, |c| c.is_ascii_alphanumeric() || c == b'_')?;
    Some((i, end))
}
/// `https?://[^:/@[:space:]]+:[^@[:space:]]+@`: a URL that carries a user and a password.
fn url_password(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let r = b.get(i..)?;
    let at = match () {
        _ if r.starts_with(b"https://") => i + 8,
        _ if r.starts_with(b"http://") => i + 7,
        _ => return None,
    };
    let user = some(b, at, |c| !space(c) && !b":/@".contains(&c))?;
    let pass = some(b, user + 1, |c| !space(c) && c != b'@').filter(|_| b[user] == b':')?;
    (b.get(pass) == Some(&b'@')).then_some((i, pass + 1))
}
/// `[A-Z_]*TOKEN=[^[:space:]]+`: `GH_TOKEN=…`, `GITHUB_TOKEN=…`.
fn token_variable(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let name = run_of(b, i, |c| c.is_ascii_uppercase() || c == b'_');
    let named = name >= i + 5 && &b[name - 5..name] == b"TOKEN" && b.get(name) == Some(&b'=');
    let end = some(b, name + 1, |c| !space(c)).filter(|_| named)?;
    Some((i, end))
}
/// The run of non-space characters after `token` or `Bearer` (any case) and a run of spaces, `:`,
/// `=` or `"`: `Authorization: Bearer …`, `"token": "…"`; the word and the run before it stay.
fn token_value(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let r = b.get(i..)?;
    let word = ["token", "bearer"]
        .iter()
        .find(|w| r.len() >= w.len() && r[..w.len()].eq_ignore_ascii_case(w.as_bytes()))?;
    let value = some(b, i + word.len(), |c| b" :=\"".contains(&c))?;
    let end = some(b, value, |c| !space(c))?;
    Some((value, end))
}

/// One request of `ap unit < request.json`: a request with `op` is refused before any other check
/// of it, a request with `case` is a case call, any other is `usage` (field `args`). A refusal (exit
/// 2) comes before any effect and writes no file.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    // A request of the unit object sent without its word: its op is the first argument (R1 of
    // `specs/ap/unit.md`), before any other check, a case call's included.
    if r.get("op").is_some() {
        let said = "ap unit <op> < request.json: the op is the first argument";
        return Err(fail("usage", "op", json!(said)));
    }
    if r.get("case").is_some() {
        return case::execute(s, r);
    }
    Err(error("usage", "args"))
}
/// The refusal of `specs/ap/verbs.md` R4 on stderr, exit 2 (4 when it cannot be written).
fn unknown() -> ExitCode {
    ExitCode::from(if std::io::stderr().write_all(UNKNOWN).is_ok() {
        2
    } else {
        4
    })
}
/// No argument: one JSON request on stdin, the answer on stdout and a refusal on stderr in the form of
/// `ap run`; a request that is no JSON, or one with neither `op` nor `case`, is refused as R4 says.
fn on_stdin() -> ExitCode {
    let mut bytes = vec![];
    let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
    let result = match read {
        Err(_) => Err(fail("io_error", "stdin", Value::Null)),
        Ok(_) if bytes.len() as u64 > LIMIT => Err(error("input_too_large", "stdin")),
        Ok(_) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(r) => execute(&mut Host, &r),
            Err(_) => Err(error("usage", "args")),
        },
    };
    match result {
        Ok((v, status)) => {
            if writeln!(std::io::stdout(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(status)
        }
        Err(e) if e.code == "usage" && e.field == "args" => unknown(),
        Err(e) => {
            let v = json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}});
            if writeln!(std::io::stderr(), "{v}").is_err() {
                return ExitCode::from(4);
            }
            ExitCode::from(2)
        }
    }
}
/// No argument: a request on stdin (`on_stdin`). `add`, `amend`, `ready`, `drop`, `defer`, `undefer`,
/// `used` or `page` first: the unit object (`specs/ap/unit.md`); `review` first: a review of a page or a
/// plan (`specs/ap/unit-review.md`); `run`, `approve` or `reject` first: a unit driven, or the Lead's
/// word (`specs/ap/unit-run.md`); `write` first: the writer of a unit's page (`specs/ap/unit-write.md`).
/// Any other form is refused as `specs/ap/verbs.md` R4 says.
pub fn cli(argv: &[OsString]) -> ExitCode {
    if argv.is_empty() {
        return on_stdin();
    }
    if argv.first().is_some_and(|w| w == "review") {
        return page_review::cli(&argv[1..]);
    }
    if argv.first().is_some_and(|w| w == "run") {
        return drive::cli(&argv[1..]);
    }
    if argv.first().is_some_and(|w| w == "write") {
        return page_write::cli(&argv[1..]);
    }
    if let Some(op) = argv.first().and_then(|w| w.to_str())
        && (op == "approve" || op == "reject")
    {
        return drive::decide_cli(op, &argv[1..]);
    }
    if argv
        .first()
        .is_some_and(|w| object::WORDS.iter().any(|o| w == o))
    {
        return object::cli(argv);
    }
    unknown()
}

#[cfg(test)]
mod tests {
    use super::redact;

    /// Every pattern of the redaction, each replaced whole (or, after `token` and `Bearer`, its
    /// value), and the cut to the last 200 bytes at a character boundary.
    #[test]
    fn a_gh_reply_is_redacted_then_cut() {
        for (said, want) in [
            ("bad ghp_Ab12 end", "bad <redacted> end"),
            (
                "x gho_a ghs_b ghu_c ghr_d",
                "x <redacted> <redacted> <redacted> <redacted>",
            ),
            ("ghx_a gh_b", "ghx_a gh_b"),
            ("p=github_pat_11AB_cd!", "p=<redacted>!"),
            ("at https://me:pw@github.com/x", "at <redacted>github.com/x"),
            ("http://me:p:w@h", "<redacted>h"),
            (
                "https://github.com/x and a@b",
                "https://github.com/x and a@b",
            ),
            ("GH_TOKEN=abc next", "<redacted> next"),
            ("aGITHUB_TOKEN=abc", "a<redacted>"),
            ("TOKEN= x", "TOKEN= <redacted>"),
            (
                "Authorization: Bearer abc.def x",
                "Authorization: Bearer <redacted> x",
            ),
            ("{\"token\": \"abc\"}", "{\"token\": \"<redacted>"),
            ("Token=abc", "Token=<redacted>"),
            ("tokens are fine", "tokens are fine"),
            ("no secret here", "no secret here"),
        ] {
            assert_eq!(redact(said.as_bytes()), want, "{said}");
        }
        let long = format!("{}{}", "é".repeat(150), "x");
        let cut = redact(long.as_bytes());
        assert!(
            cut.len() <= 200 && cut.len() >= 199 && cut.ends_with("éx"),
            "{cut}"
        );
        assert_eq!(redact(b"\xff ok"), "\u{fffd} ok");
    }
}
