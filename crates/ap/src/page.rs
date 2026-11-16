//! The page's block and `ap accept` (README "The page's block"). A check reads Git and files only.
use crate::host::{Failure, Host, System, args, error, fail, require};
use crate::review::{self, DIFF_OPTIONS};
use serde_json::{Map, Value, json};
use std::{ffi::OsString, io::Write, process::ExitCode};

const MEMBERS: &str =
    "unit write_set checks repository branch item tier merge implementer verifier";
const KINDS: [&str; 4] = ["unchanged", "adds", "holds", "lines"];
pub(crate) enum Hold {
    Text(String),
    Exact(String),
}
#[derive(Default)]
pub(crate) struct Checks {
    unchanged: Vec<String>,
    adds: Vec<(String, Vec<String>)>,
    holds: Vec<(String, Hold)>,
    lines: Vec<(String, u64, u64)>,
}
pub(crate) struct Block {
    pub write_set: Vec<String>,
    pub checks: Checks,
    /// Every member of the block as parsed: `ap unit <page>` composes its request from them.
    pub members: Map<String, Value>,
}

pub(crate) fn invalid(reason: &str, member: Option<&str>) -> Failure {
    let detail = json!({"reason": reason, "member": member});
    fail("page_block_invalid", "page", detail)
}
pub(crate) fn shape(member: &str) -> Failure {
    invalid("shape", Some(member))
}
fn pattern(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    (!s.is_empty() && !s.contains(',') && s.trim() == s).then(|| s.to_owned())
}
fn word(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    ((1..=2000).contains(&s.len()) && !s.contains('\n')).then(|| s.to_owned())
}
/// A string of 1 to 40 characters; the `unit` member also only `A-Z`, `a-z`, `0-9` and `-`.
fn small(v: &Value, strict: bool) -> bool {
    let s = v.as_str().unwrap_or_default();
    let alnum = |c: char| c.is_ascii_alphanumeric() || c == '-';
    (1..=40).contains(&s.chars().count()) && (!strict || s.chars().all(alnum))
}
fn branch(b: &str) -> bool {
    let safe = |c: u8| c.is_ascii_alphanumeric() || b"._/-".contains(&c);
    (1..=100).contains(&b.len())
        && !b.starts_with(['-', '/'])
        && !b.contains("..")
        && b.bytes().all(safe)
}
fn repository(v: &Value) -> bool {
    let ok = |c: char| c.is_ascii_alphanumeric() || "._-".contains(c);
    let parts: Vec<_> = v.as_str().unwrap_or_default().split('/').collect();
    parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(ok))
}
/// A stage of the block: `model` and `effort`, optional `runtime`, `permission_mode`, `timeout_seconds`.
fn stage(v: &Value) -> bool {
    let known: Vec<_> = "model effort runtime permission_mode timeout_seconds"
        .split(' ')
        .collect();
    let text = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .is_some_and(|t| !t.is_empty())
    };
    let option = |k: &str| v.get(k).is_none() || text(k);
    let seconds = v
        .get("timeout_seconds")
        .is_none_or(|n| n.as_u64() > Some(0));
    let (ok, optional) = (
        text("model") && text("effort") && seconds,
        option("runtime"),
    );
    entry(v, &known).is_some() && ok && optional && option("permission_mode")
}
fn entry<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    v.as_object()
        .filter(|o| o.keys().all(|k| keys.contains(&k.as_str())))
}
fn add(v: &Value) -> Option<(String, Vec<String>)> {
    let o = entry(v, &["path", "words"])?;
    let words: Option<Vec<_>> = o.get("words")?.as_array()?.iter().map(word).collect();
    Some((pattern(o.get("path")?)?, words.filter(|w| !w.is_empty())?))
}
fn hold(v: &Value) -> Option<(String, Hold)> {
    let o = entry(v, &["path", "text", "exact"])?;
    let path = o.get("path")?.as_str()?;
    let part = |c: &str| !c.is_empty() && c != "." && c != "..";
    if !path.split('/').all(part) || path.contains('\0') {
        return None;
    }
    let exact = |x: &Value| x.as_str().filter(|x| x.len() <= 65_536).map(str::to_owned);
    let hold = match (o.get("text"), o.get("exact")) {
        (Some(t), None) => Hold::Text(word(t)?),
        (None, Some(x)) => Hold::Exact(exact(x)?),
        _ => return None,
    };
    Some((path.to_owned(), hold))
}
fn line(v: &Value) -> Option<(String, u64, u64)> {
    let o = entry(v, &["path", "max", "min"])?;
    let max = o.get("max")?.as_u64()?;
    let min = o.get("min").map_or(Some(0), Value::as_u64)?;
    (min <= max).then_some((pattern(o.get("path")?)?, min, max))
}
fn checks(v: &Value) -> Result<Checks, Failure> {
    let o = v.as_object().ok_or_else(|| shape("checks"))?;
    if let Some(k) = o.keys().find(|k| !KINDS.contains(&k.as_str())) {
        return Err(invalid("unknown_member", Some(&format!("checks.{k}"))));
    }
    let list = |k: &str| match o.get(k) {
        None => Some(&[][..]),
        Some(v) => v.as_array().filter(|a| a.len() <= 40).map(Vec::as_slice),
    };
    let mut c = Checks::default();
    for k in KINDS {
        let bad = || shape(&format!("checks.{k}"));
        for v in list(k).ok_or_else(bad)? {
            let ok = match k {
                "unchanged" => pattern(v).map(|p| c.unchanged.push(p)),
                "adds" => add(v).map(|a| c.adds.push(a)),
                "holds" => hold(v).map(|h| c.holds.push(h)),
                _ => line(v).map(|l| c.lines.push(l)),
            };
            ok.ok_or_else(bad)?;
        }
    }
    Ok(c)
}
/// The members of every object of a parsed value, and the colons outside strings of its text: a
/// member that appears twice leaves the parsed value (the last wins) with fewer than the text has.
fn members(v: &Value) -> usize {
    match v {
        Value::Object(o) => o.len() + o.values().map(members).sum::<usize>(),
        Value::Array(a) => a.iter().map(members).sum(),
        _ => 0,
    }
}
fn colons(text: &str) -> usize {
    let (mut n, mut inside, mut escaped) = (0, false, false);
    for c in text.chars() {
        match (inside, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (true, false, '"') => inside = false,
            (false, _, '"') => inside = true,
            (false, _, ':') => n += 1,
            _ => {}
        }
    }
    n
}
/// The page's block: none when it has no line "```unit"; else the block, or why it is refused.
pub(crate) fn block(text: &str) -> Result<Option<Block>, Failure> {
    let lines: Vec<&str> = text.split('\n').collect();
    let opens: Vec<_> = (0..lines.len())
        .filter(|i| lines[*i] == "```unit")
        .collect();
    if opens.len() > 1 {
        return Err(invalid("two_blocks", None));
    }
    let Some(open) = opens.first().map(|i| i + 1) else {
        return Ok(None);
    };
    let len = lines[open..].iter().position(|l| *l == "```");
    let body = lines[open..open + len.ok_or_else(|| invalid("unclosed", None))?].join("\n");
    let value: Value = serde_json::from_str(&body).map_err(|_| invalid("not_an_object", None))?;
    let o = value
        .as_object()
        .filter(|_| members(&value) == colons(&body));
    let o = o.ok_or_else(|| invalid("not_an_object", None))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.split(' ').any(|m| m == *k)) {
        return Err(invalid("unknown_member", Some(k)));
    }
    let (mut write_set, mut checks_of) = (vec![], Checks::default());
    for k in MEMBERS.split(' ') {
        let (v, bad) = (o.get(k), || shape(k));
        let ok = match (k, v) {
            ("unit" | "write_set", None) => false,
            (_, None) => true,
            ("unit", Some(v)) => small(v, true),
            ("write_set", Some(v)) => {
                let all = v.as_array().filter(|a| (1..=40).contains(&a.len()));
                write_set = all
                    .and_then(|a| a.iter().map(pattern).collect())
                    .unwrap_or_default();
                !write_set.is_empty()
            }
            ("checks", Some(v)) => {
                checks_of = checks(v)?;
                true
            }
            ("repository", Some(v)) => repository(v),
            ("branch", Some(v)) => v.as_str().is_some_and(branch),
            ("item", Some(v)) => v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 60),
            ("tier", Some(v)) => small(v, false),
            ("implementer" | "verifier", Some(v)) => stage(v),
            (_, Some(v)) => v.is_boolean(),
        };
        if !ok {
            return Err(bad());
        }
    }
    let twice = text.lines().any(|l| l.starts_with("Write set:"));
    require!(!twice => "write_set_twice", "page");
    Ok(Some(Block {
        write_set,
        checks: checks_of,
        members: o.clone(),
    }))
}
/// The lines of the failed checks as `ap accept` prints them.
pub(crate) fn report(failed: &[String]) -> String {
    match failed.is_empty() {
        true => "accept: ok\n".to_owned(),
        false => failed
            .iter()
            .map(|l| format!("accept: FAILED: {l}\n"))
            .collect(),
    }
}

fn git(s: &mut impl System, dir: &str, words: &[&str]) -> Result<Vec<u8>, Failure> {
    let mut argv = args(&["--no-replace-objects", "-C", dir]);
    argv.extend(args(words));
    let reply = s.call("git", &argv, &Value::Null, 30_000)?;
    require!(reply.success => "git_failed", "git");
    Ok(reply.stdout)
}
fn gits(s: &mut impl System, dir: &str, words: &[&str]) -> Result<String, Failure> {
    git(s, dir, words).map(|b| String::from_utf8_lossy(&b).into_owned())
}
/// `git diff` of the change from `base` to `HEAD` with the options that select no output of the
/// step `write_set`'s list, then `extra`, and `tail` after the revisions.
fn diff(
    s: &mut impl System,
    dir: &str,
    base: &str,
    extra: &[&str],
    tail: &[&str],
) -> Result<String, Failure> {
    let mut words = vec!["diff"];
    words.extend(&DIFF_OPTIONS[..3]);
    words.extend(extra);
    words.extend([base, "HEAD"]);
    words.extend(tail);
    gits(s, dir, &words)
}
/// The change from the merge base of `origin/main` and `HEAD` to `HEAD`: its base and changed paths.
pub(crate) struct Change {
    dir: String,
    base: String,
    pub paths: Vec<String>,
}
pub(crate) fn change(s: &mut impl System, dir: &str) -> Result<Change, Failure> {
    let base = gits(s, dir, &["merge-base", "origin/main", "HEAD"])?
        .trim()
        .to_owned();
    let names = diff(s, dir, &base, &DIFF_OPTIONS[3..], &[])?;
    let paths = names.split_terminator('\0').map(str::to_owned).collect();
    Ok(Change {
        dir: dir.to_owned(),
        base,
        paths,
    })
}
/// The regular file `path` of the commit `HEAD`, as bytes; none when it is not one.
fn blob(s: &mut impl System, dir: &str, path: &str) -> Option<Vec<u8>> {
    let tree = gits(
        s,
        dir,
        &["ls-tree", "-z", "--full-tree", "HEAD", "--", path],
    )
    .ok()?;
    let entries = tree
        .split_terminator('\0')
        .filter_map(|e| e.split_once('\t'));
    let (meta, _) = entries.into_iter().find(|(_, name)| *name == path)?;
    let meta: Vec<_> = meta.split(' ').collect();
    let regular = meta.len() == 3 && meta[1] == "blob" && ["100644", "100755"].contains(&meta[0]);
    git(
        s,
        dir,
        &["cat-file", "blob", meta.get(2).filter(|_| regular)?],
    )
    .ok()
}
/// The changed lines of the diff of one path, `+` or `-` first: those of its hunks, `-U0`.
fn hunks(s: &mut impl System, c: &Change, path: &str) -> Result<Vec<String>, Failure> {
    let extra = ["--no-color", "--no-ext-diff", "--no-textconv", "-U0"];
    let patch = diff(
        s,
        &c.dir,
        &c.base,
        &extra,
        &["--", &format!(":(literal){path}")],
    )?;
    // A header (`diff --git`, up to the first `@@`) is no changed line: a typechange has two.
    let (mut lines, mut hunk) = (vec![], false);
    for l in patch.split('\n') {
        hunk = !l.starts_with("diff --git ") && (hunk || l.starts_with("@@"));
        if hunk && l.starts_with(['+', '-']) {
            lines.push(l.to_owned());
        }
    }
    Ok(lines)
}
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}
/// The checks of the block that failed, one line each (`<kind>: <what>`), in the block's order.
pub(crate) fn failed(s: &mut impl System, c: &Change, k: &Checks) -> Result<Vec<String>, Failure> {
    let mut out = vec![];
    let covered =
        |p: &String, x: &&String| review::outside(std::slice::from_ref(p), &[x]).is_empty();
    let matching =
        |p: &String| -> Vec<&String> { c.paths.iter().filter(|x| covered(p, x)).collect() };
    for p in &k.unchanged {
        if let Some(first) = matching(p).first() {
            out.push(format!("unchanged: {p:?}: {first:?} changed"));
        }
    }
    for (p, words) in &k.adds {
        let mut lines = vec![];
        for path in matching(p) {
            lines.extend(hunks(s, c, path)?);
        }
        let has = |w: &&String| {
            lines
                .iter()
                .any(|l| l.strip_prefix('+').is_some_and(|t| t.contains(w.as_str())))
        };
        for w in words.iter().filter(|w| !has(w)) {
            out.push(format!("adds: {p:?}: no added line contains {w:?}"));
        }
    }
    for (path, hold) in &k.holds {
        let what = match (blob(s, &c.dir, path), hold) {
            (None, _) => "is not a regular file of HEAD",
            (Some(b), Hold::Text(t)) if !contains(&b, t.as_bytes()) => "lacks the text",
            (Some(b), Hold::Exact(x)) if b != x.as_bytes() => "differs from the exact bytes",
            _ => continue,
        };
        out.push(format!("holds: {path:?}: {what}"));
    }
    for (p, min, max) in &k.lines {
        let mut n = 0;
        for path in matching(p) {
            n += hunks(s, c, path)?.len() as u64;
        }
        if n > *max {
            out.push(format!("lines: {p:?}: {n} changed lines, above max {max}"));
        } else if n < *min {
            out.push(format!("lines: {p:?}: {n} changed lines, below min {min}"));
        }
    }
    Ok(out)
}

/// `ap accept <page>` in the current directory: the lines of the failed checks, none when all hold.
pub fn execute(s: &mut impl System, argv: &[OsString]) -> Result<Vec<String>, Failure> {
    let page = argv.first().and_then(|p| p.to_str());
    let page = page.filter(|p| argv.len() == 1 && std::path::Path::new(p).is_absolute());
    let page = crate::unit::regular(
        page.ok_or_else(|| error("usage", "args"))?,
        "page_unreadable",
        "page",
    )?;
    let text = String::from_utf8_lossy(&page);
    let block = block(&text)?.ok_or_else(|| invalid("no_block", None))?;
    let cwd = s.cwd().ok().and_then(|c| c.to_str().map(str::to_owned));
    let top = gits(
        s,
        &cwd.unwrap_or_default(),
        &["rev-parse", "--show-toplevel"],
    );
    let top = top.map_err(|_| error("not_a_worktree", "cwd"))?;
    let dir = top.trim();
    let known = git(
        s,
        dir,
        &["rev-parse", "--verify", "-q", "origin/main^{commit}"],
    );
    known.map_err(|_| error("origin_main_unknown", "cwd"))?;
    let mut out = vec![];
    if !git(s, dir, &["status", "--porcelain"])?.is_empty() {
        out.push("clean: the worktree has changes".to_owned());
    }
    let c = change(s, dir)?;
    let paths: Vec<&str> = c.paths.iter().map(String::as_str).collect();
    let outside = review::outside(&block.write_set, &paths);
    if !outside.is_empty() {
        let first: Vec<_> = outside.iter().take(10).collect();
        out.push(format!(
            "write_set: {} paths outside the write set: {first:?}",
            outside.len()
        ));
    }
    out.extend(failed(s, &c, &block.checks)?);
    Ok(out)
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    match execute(&mut Host, argv) {
        Ok(lines) => match std::io::stdout().write_all(report(&lines).as_bytes()) {
            Ok(()) => ExitCode::from(u8::from(!lines.is_empty())),
            Err(_) => ExitCode::from(4),
        },
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}});
            ExitCode::from(if writeln!(std::io::stderr(), "{v}").is_ok() {
                2
            } else {
                4
            })
        }
    }
}
