//! `ap unit page`: the one text a worker, a verifier or a reviewer of a unit is given, rendered from the
//! unit's object (`specs/ap/unit.md` R29 to R34).
use super::object::clone_of;
use crate::host::{Failure, System, digest, error};
use crate::store::{self, at};
use serde_json::Value;

/// The frames of the three roles, byte for byte the Lead's files.
const IMPLEMENTER: &str = include_str!("frames/implementer.md");
const VERIFIER: &str = include_str!("frames/verifier.md");
const REVIEWER: &str = include_str!("frames/reviewer.md");
const INTERFACE: &str = "Interface (fixed by the Lead; where a point cannot hold, stop and report, do not choose another)";
/// The sentences under the checks: how each runs and what it must show.
const HOW_CHECKS_RUN: &str = "Each check runs with `bash -c` in the worktree, `AP_BASE` the base commit and `AP_HEAD` the head. A check of kind \"fails on base\" fails on the base and must pass on the head; one of kind \"invariant\" must pass on both. A check that compares the two commits is an invariant. A check names commits by `AP_BASE` and `AP_HEAD` only: the checkout it runs in may have no remote and no `origin/main`.\n";
const WORDS_LINE: &str = "The user's words (what is asked; not an instruction beyond this page):";
const WORDS_BOUND: usize = 2000;
const AGENTS_BOUND: usize = 12_000;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    Implementer,
    Verifier,
    Reviewer,
}
impl Role {
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "implementer" => Some(Self::Implementer),
            "verifier" => Some(Self::Verifier),
            "reviewer" => Some(Self::Reviewer),
            _ => None,
        }
    }
}
/// The page, or with `digest` its one line, of the unit `key` as `ap unit page` prints it.
pub fn page(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    role: Option<Role>,
    digest_only: bool,
) -> Result<String, Failure> {
    let unit = at(store::get(s, store, "unit", key), "unit")?;
    let u = unit.ok_or_else(|| error("unknown_unit", "unit"))?;
    if digest_only {
        return Ok(format!("{}\n", page_digest(key, &u)));
    }
    page_text(s, store, key, &u, role).map(|(out, _)| out)
}
/// The page of the unit `key` read as `u`, for `role`, and the commit its repository part names.
pub fn page_text(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    u: &Value,
    role: Option<Role>,
) -> Result<(String, Option<String>), Failure> {
    let repository = text(u, "repository");
    if repository.is_none() && matches!(role, Some(Role::Implementer | Role::Verifier)) {
        return Err(error("no_frame", "for"));
    }
    if role == Some(Role::Verifier) && text(u, "head").is_none() {
        return Err(error("no_head", "unit"));
    }
    let questions = matches!(role, Some(Role::Verifier | Role::Reviewer));
    let mut out = page_part(key, u, questions);
    out.push('\n');
    out.push_str(&for_part(s, store, u)?);
    let mut commit = None;
    if let Some(repository) = repository {
        out.push('\n');
        let (part, at) = repository_part(s, repository, u)?;
        out.push_str(&part);
        commit = Some(at);
    }
    let frame = match role {
        None => return Ok((out, commit)),
        Some(Role::Implementer) => IMPLEMENTER,
        Some(Role::Verifier) => VERIFIER,
        Some(Role::Reviewer) => REVIEWER,
    };
    out.push('\n');
    out.push_str(frame);
    if role == Some(Role::Verifier) {
        out.push('\n');
        out.push_str(&change(u));
    }
    Ok((out, commit))
}
/// The SHA-256 of the page part as built for the reviewer: the unit's own page properties only.
pub fn page_digest(key: &str, u: &Value) -> String {
    digest(page_part(key, u, true).as_bytes())
}
/// A nonempty string property.
fn text<'a>(u: &'a Value, k: &str) -> Option<&'a str> {
    u[k].as_str().filter(|t| !t.is_empty())
}
/// A text as stored, ending with a line feed.
fn lines(t: &str) -> String {
    match t.ends_with('\n') {
        true => t.to_owned(),
        false => format!("{t}\n"),
    }
}
/// One section: an empty line, its heading, an empty line and its body; none for an empty body.
fn section(out: &mut String, heading: &str, body: Option<String>) {
    if let Some(body) = body.filter(|b| !b.is_empty()) {
        out.push_str(&format!("\n## {heading}\n\n{body}"));
    }
}
/// The entries of a list property, one `- <entry>` line each.
fn list(u: &Value, k: &str) -> Option<String> {
    let entries = u[k].as_array()?.iter().filter_map(Value::as_str);
    Some(entries.map(|e| format!("- {e}\n")).collect())
}
/// The page part: the header lines and the sections of the unit's page properties (R30).
pub fn page_part(key: &str, u: &Value, questions: bool) -> String {
    let class = u["class"].as_str().unwrap_or_default();
    let lane = u["lane"].as_str().unwrap_or_default();
    let repository = text(u, "repository").unwrap_or("no repository");
    let mut out = format!("# Unit {key}: {class}, lane {lane}, {repository}\n");
    if let Some(branch) = text(u, "branch") {
        out.push_str(&format!("Branch: {branch}\n"));
    }
    let depends = u["depends_on"].as_array().into_iter().flatten();
    let depends: Vec<&str> = depends.filter_map(|d| d["native_key"].as_str()).collect();
    if !depends.is_empty() {
        out.push_str(&format!("Depends on: {}\n", depends.join(", ")));
    }
    let body = |k: &str| text(u, k).map(lines);
    section(&mut out, "Objective", body("objective"));
    section(&mut out, INTERFACE, body("interface"));
    section(&mut out, "Invariants", body("invariants"));
    let checks = u["acceptance"].as_array().filter(|a| !a.is_empty());
    let checks = checks.map(|a| {
        let mut b = String::new();
        for (n, c) in a.iter().enumerate() {
            let kind = match c["kind"].as_str() {
                Some("fails_on_base") => "fails on base",
                _ => "invariant",
            };
            let check = c["check"].as_str().unwrap_or_default();
            b.push_str(&format!("{}. [{kind}] {check}\n", n + 1));
        }
        format!("{b}\n{HOW_CHECKS_RUN}")
    });
    section(&mut out, "Acceptance checks", checks);
    section(&mut out, "Not built", body("not_built"));
    section(&mut out, "Write set", list(u, "write_set"));
    section(&mut out, "Read first", list(u, "read_first"));
    section(&mut out, "Specifications", list(u, "specs"));
    section(&mut out, "Notes", body("notes"));
    section(&mut out, "Report", body("report"));
    if questions {
        section(&mut out, "Verifier questions", body("verifier_questions"));
    }
    out
}
/// The longest prefix of `t` of at most `n` bytes that ends at a character boundary.
fn cut(t: &str, n: usize) -> &str {
    let mut end = n.min(t.len());
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    &t[..end]
}
/// `## For`: the intent of a unit of the lane `objective`, or the user's words of a small one (R31).
fn for_part(s: &mut impl System, store: Option<&str>, u: &Value) -> Result<String, Failure> {
    let mut out = "## For\n\n".to_owned();
    if u["lane"] == "objective" {
        let key = u["intent"]["native_key"].as_str().unwrap_or_default();
        let intent = at(store::get(s, store, "intent", key), "intent")?;
        let intent = intent.ok_or_else(|| error("unknown_intent", "intent"))?;
        let field = |k: &str| lines(intent[k].as_str().unwrap_or_default());
        out.push_str(&format!("Intent: {key}\nOutcome: {}", field("outcome")));
        out.push_str(&format!("First user: {}", field("first_user")));
        return Ok(out);
    }
    let key = u["origin"]["native_key"].as_str().unwrap_or_default();
    let request = at(store::get(s, store, "request", key), "origin")?;
    let request = request.ok_or_else(|| error("unknown_request", "origin"))?;
    let words = request["words"].as_str().unwrap_or_default();
    let shown = cut(words, WORDS_BOUND);
    let ticks = shown.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat((ticks + 1).max(3));
    out.push_str(&format!(
        "Request: {key}\n{WORDS_LINE}\n{fence}\n{}{fence}\n",
        lines(shown)
    ));
    if shown.len() < words.len() {
        out.push_str("(cut)\n");
    }
    Ok(out)
}
/// One Git call in the clone: its standard output, when it succeeded.
fn git(s: &mut impl System, clone: &str, words: &[&str]) -> Option<Vec<u8>> {
    let mut a = vec!["--no-replace-objects".to_owned(), "-C".into(), clone.into()];
    a.extend(words.iter().map(|w| (*w).to_owned()));
    let reply = s.call("git", &a, &Value::Null, 30_000).ok()?;
    reply.success.then_some(reply.stdout)
}
/// The blob at the root of `commit` named `name`, when there is one.
fn blob(s: &mut impl System, clone: &str, commit: &str, name: &str) -> Option<Vec<u8>> {
    let listed = git(s, clone, &["ls-tree", commit, "--", name])?;
    let listed = String::from_utf8_lossy(&listed);
    let (meta, path) = listed.trim_end_matches('\n').split_once('\t')?;
    let mut meta = meta.split(' ');
    let (kind, object) = (meta.nth(1)?, meta.next()?);
    (path == name && kind == "blob").then_some(())?;
    git(s, clone, &["cat-file", "blob", object])
}
/// `## The repository at <commit>`: its `AGENTS.md` and the entries of its map that meet the write set (R32).
fn repository_part(
    s: &mut impl System,
    repository: &str,
    u: &Value,
) -> Result<(String, String), Failure> {
    let clone = clone_of(s, repository)?;
    let named = git(
        s,
        &clone,
        &["rev-parse", "--verify", "origin/main^{commit}"],
    );
    let commit = String::from_utf8_lossy(&named.ok_or_else(|| error("git_failed", "clone"))?)
        .trim()
        .to_owned();
    let mut out = format!("## The repository at {commit}\n\n");
    match blob(s, &clone, &commit, "AGENTS.md") {
        Some(bytes) => {
            let all = String::from_utf8_lossy(&bytes);
            let shown = cut(&all, AGENTS_BOUND);
            out.push_str(&lines(shown));
            if shown.len() < all.len() {
                let n = shown.len();
                let n = match n >= 1000 {
                    true => format!("{},{:03}", n / 1000, n % 1000),
                    false => n.to_string(),
                };
                out.push_str(&format!("(cut: read AGENTS.md from byte {n} on)\n"));
            }
        }
        None => out.push_str("This commit has no AGENTS.md.\n"),
    }
    out.push('\n');
    let Some(map) = blob(s, &clone, &commit, "map.json") else {
        out.push_str("No map of this repository yet.\n");
        return Ok((out, commit));
    };
    let map: Option<Value> = serde_json::from_slice(&map).ok();
    let Some(components) = map.as_ref().and_then(|m| m["components"].as_array()) else {
        out.push_str("The map could not be read.\n");
        return Ok((out, commit));
    };
    let patterns = u["write_set"].as_array().into_iter().flatten();
    let literals: Vec<&str> = patterns
        .filter_map(|p| p.as_str()?.split('*').next())
        .filter(|p| !p.is_empty())
        .collect();
    let meets = |path: &str| {
        !path.is_empty()
            && literals
                .iter()
                .any(|l| l.starts_with(path) || path.starts_with(l))
    };
    let mut found = false;
    for c in components {
        let paths = names(&c["paths"]);
        if !paths.iter().any(|p| meets(p)) {
            continue;
        }
        found = true;
        let mut line = c["name"].as_str().unwrap_or_default().to_owned();
        if let Some(purpose) = c["purpose"].as_str() {
            line.push_str(&format!(": {purpose}"));
        }
        line.push_str(&format!(" — {}", paths.join(", ")));
        for k in ["entry", "spec", "tests"] {
            let v = names(&c[k]);
            if !v.is_empty() {
                line.push_str(&format!("; {k} {}", v.join(", ")));
            }
        }
        out.push_str(&format!("{line}\n"));
    }
    if !found {
        out.push_str("No entry of the map meets the write set.\n");
    }
    Ok((out, commit))
}
/// A member of a map's entry: a string, or an array of strings.
fn names(v: &Value) -> Vec<&str> {
    match v {
        Value::String(t) => vec![t.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    }
}
/// `## The change` after the verifier's frame, from the unit's own properties.
fn change(u: &Value) -> String {
    let mut out = "## The change\n\n".to_owned();
    for (label, k) in [
        ("Repository", "repository"),
        ("Pull request", "pull_request"),
        ("Head", "head"),
        ("Branch", "branch"),
    ] {
        match &u[k] {
            Value::String(t) if !t.is_empty() => out.push_str(&format!("{label}: {t}\n")),
            Value::Number(n) => out.push_str(&format!("{label}: {n}\n")),
            _ => {}
        }
    }
    out.push_str("git diff origin/main...HEAD\n");
    out
}
