//! `ap unit run <key>`: one unit of the store driven from `ready`, `stopped` or a cut run to
//! `accepted` or `stopped`, its repairs and the updates of its branch inside, then a `plain` or
//! approved unit merged and its worktree removed (`specs/ap/unit-run.md`); `ap unit approve|reject`,
//! the Lead's word on an accepted unit. The stages are `ap run`'s code, the gate is `ap gate`'s, the
//! acceptance checks run as `ap unit ready` runs them, and the review of a head is recorded as
//! `ap unit review` records one.
use super::object::{self, branch_ok, clone_of};
use super::page_review::{self, key_ok, stage_unit, unit_ended};
use super::render::{self, Role};
use super::scan;
use crate::host::{Failure, Host, System, args, digest, error, fail, new_session_id};
use crate::review::{self, DIFF_OPTIONS, GIT_SAFE};
use crate::store::work::{self, Action, Id, ObjectType, actions, objects, properties, values};
use crate::store::{self, Actor, Applied, at};
use serde_json::{Map, Value, json};
use std::{
    ffi::OsString,
    fs::File,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, UNIX_EPOCH},
};

const FIXER: &str = include_str!("frames/fixer.md");
/// The line that names a verifier round's description file, after `## The change` (R19).
const DESCRIPTION: &str = "Description of the pull request: ";
const DESCRIPTION_TAIL: &str =
    " (its body as the run read it before this round; a CLAIM to check, not a fact)";
/// The form of a stage token: `github_pat_` and 1 to 255 of these (R41).
const PAT: &str = "github_pat_";
const SECOND_LOOK: &str = include_str!("frames/second-look.md");
const CONTINUE: &str = include_str!("frames/continue.md");
const UNITS: &str = "/var/lib/agent-platform/units";
const REVIEWS: &str = "/var/lib/agent-platform/reviews";
/// The bound of each stage by role, in seconds (R3).
const TIMEOUTS: [(&str, u64); 3] = [
    ("implementer", 10_800),
    ("verifier", 7_200),
    ("fixer", 7_200),
];
/// The most bytes of a stop's detail (R22).
const DETAIL: usize = 1000;
/// The most updates of a branch in one call (R26).
const UPDATES: u32 = 5;
/// The author and committer of the update's merge (R26).
const MERGER: [&str; 4] = [
    "GIT_AUTHOR_NAME=ap unit run",
    "GIT_AUTHOR_EMAIL=ap-unit@agents.invalid",
    "GIT_COMMITTER_NAME=ap unit run",
    "GIT_COMMITTER_EMAIL=ap-unit@agents.invalid",
];

/// A refusal (R25): exit 2 before the run applied anything, else 3.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub status: u8,
}
/// What ends the loop early: a stop the store records, or a refusal or failure that ends the call (R22).
enum Halt {
    Stop(String, String),
    /// A stop of a failed `gh` call: the cause, the detail and the call's reply, redacted (R22, R25).
    Said(String, String, String),
    Fail(Failure),
}
impl From<Failure> for Halt {
    fn from(f: Failure) -> Self {
        Self::Fail(f)
    }
}
type Step<T = ()> = Result<T, Halt>;
/// The paths a unit of class `plain` may not change (R36), every name lower-case: exactly these
/// whole paths.
pub(super) const PROTECTED_EXACT: [&str; 2] = ["check", "review.md"];
/// The last component, in any directory: Cargo, rustfmt and rustup read the configuration files
/// below the root too, and a build script or a manifest of any package runs or steers a build that
/// delivery makes as root.
pub(super) const PROTECTED_NAMES: [&str; 9] = [
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    "build.rs",
    "cargo.toml",
    "cargo.lock",
];
/// The last two components, in any directory: Cargo's configuration below the root.
pub(super) const PROTECTED_CONFIGS: [&str; 2] = [".cargo/config.toml", ".cargo/config"];
/// The first component: the path itself or anything under it.
pub(super) const PROTECTED_TREES: [&str; 12] = [
    "ci", ".github", "specs", "scripts", ".cargo", "deploy", "etc", "content", "github",
    "ontology", "engine", "host",
];
/// Whether the first component of `path` is one of the protected trees, compared without case: the
/// path itself and everything under it is protected (R36).
pub(super) fn protected_tree(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path).to_lowercase();
    PROTECTED_TREES.contains(&first.as_str())
}
/// A path a unit of class `plain` may not change (R36): `path` is repository-relative, and every
/// name is compared without case (`CI/x`, `.GitHub/…`).
pub(super) fn protected(path: &str) -> bool {
    let path = path.to_lowercase();
    let last = path.rsplit('/').next().unwrap_or(&path);
    let config = PROTECTED_CONFIGS
        .iter()
        .any(|c| path == *c || path.ends_with(&format!("/{c}")));
    PROTECTED_EXACT.contains(&path.as_str())
        || PROTECTED_NAMES.contains(&last)
        || protected_tree(&path)
        || config
}
/// The table of the declared skips of `./check` in a unit's repository (`specs/ap/check.md` R23).
pub(super) const SKIPS: &str = "ci/skips.json";
/// One row of `ci/skips.json` as the run reads it: its id, whether its `runs_on` is not `ci`, and its
/// covers (`specs/ap/unit-run.md` R49).
pub(super) type SkipRow = (String, bool, Vec<String>);
/// The rows of a text of `ci/skips.json`, or `None` when the text is not of its form: exactly
/// `{"rows": [...]}`, each row exactly `id` (lower-case letters, digits and hyphens, unique), `file`
/// (an exact path of the write-set grammar), `covers` (1 to 20 patterns of that grammar), `runs_on`
/// (`vps-root`, `vps` or `ci`) and `how` (one line, not blank) (`specs/ap/check.md` R23).
pub(super) fn skip_rows(text: &str) -> Option<Vec<SkipRow>> {
    let table: Value = serde_json::from_str(text).ok()?;
    let table = table.as_object().filter(|m| m.len() == 1)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = vec![];
    for row in table.get("rows")?.as_array()? {
        let r = row.as_object().filter(|r| r.len() == 5)?;
        let id = r.get("id")?.as_str()?;
        let id_ok = !id.is_empty()
            && (id.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok || !seen.insert(id) {
            return None;
        }
        let file = r.get("file")?.as_str()?;
        if !review::write_set_pattern(file).grammar || file.ends_with("/**") {
            return None;
        }
        let covers = r.get("covers")?.as_array()?;
        if !(1..=20).contains(&covers.len()) {
            return None;
        }
        let covers = (covers.iter())
            .map(|c| c.as_str().filter(|c| review::write_set_pattern(c).grammar))
            .map(|c| c.map(str::to_owned))
            .collect::<Option<Vec<String>>>()?;
        let runs_on = r.get("runs_on")?.as_str()?;
        if !["vps-root", "vps", "ci"].contains(&runs_on) {
            return None;
        }
        let how = r.get("how")?.as_str()?;
        if how.trim().is_empty() || how.chars().any(char::is_control) {
            return None;
        }
        rows.push((id.to_owned(), runs_on != "ci", covers));
    }
    Some(rows)
}
/// The changed paths that meet the covers of a row whose `runs_on` is not `ci`, the rows of the base
/// and of the head taken together: an id of both counts when either is not `ci`, with the covers of
/// both (R49, R50). `None` when the table of either is not of its form.
pub(super) fn skip_hits<'p>(
    at_base: Option<Vec<SkipRow>>,
    at_head: Option<Vec<SkipRow>>,
    paths: &[&'p str],
) -> Option<Vec<&'p str>> {
    let rows: Vec<SkipRow> = at_base?.into_iter().chain(at_head?).collect();
    let host = |id: &str| rows.iter().any(|(i, h, _)| i == id && *h);
    let covers: Vec<String> = (rows.iter())
        .filter(|(id, _, _)| host(id))
        .flat_map(|(_, _, c)| c.iter().cloned())
        .collect();
    let outside = review::outside(&covers, paths);
    Some(
        paths
            .iter()
            .filter(|p| !outside.contains(p))
            .copied()
            .collect(),
    )
}
/// `gh pr merge` with `words`: `Err` with the redacted reply when it failed (no reply: empty).
fn merge_call(s: &mut impl System, words: &str) -> Result<(), String> {
    match s.call("gh", &args(&w(words)), &Value::Null, 120_000) {
        Ok(r) if r.success => Ok(()),
        Ok(r) => Err(crate::unit::redact(&r.bytes)),
        Err(_) => Err(String::new()),
    }
}
/// Whether Git's message says a ref was locked by another process: two fetches into one clone at
/// once (two units of one repository), the one cause of a failed fetch that a second try removes.
pub(crate) fn racing(message: &[u8]) -> bool {
    message.windows(15).any(|w| w == b"cannot lock ref")
}
/// Whether the redacted reply of a failed `gh` call names a transient failure of the service (R9).
fn transient(reply: &str) -> bool {
    const SAYS: [&str; 9] = [
        "i/o timeout",
        "connection reset",
        "tls handshake timeout",
        "http 502",
        "http 503",
        "http 504",
        "http 500",
        "bad credentials",
        "could not resolve host",
    ];
    let reply = reply.to_lowercase();
    SAYS.iter().any(|t| reply.contains(t))
}
/// A stop that keeps the redacted reply of the failed `gh` call that caused it.
fn said<T>(cause: &str, detail: impl ToString, reply: String) -> Step<T> {
    Err(Halt::Said(cause.to_owned(), detail.to_string(), reply))
}
fn stop<T>(cause: &str, detail: impl ToString) -> Step<T> {
    Err(Halt::Stop(cause.to_owned(), detail.to_string()))
}
fn failed<T>(code: &'static str, field: &str, detail: Value) -> Step<T> {
    Err(Halt::Fail(fail(code, field, detail)))
}
/// A stage `ap run` refused (R11).
fn refused(f: Failure) -> Halt {
    let detail = json!({"code": f.code, "field": f.field});
    Halt::Stop("stage_refused".into(), detail.to_string())
}
/// What the read of R40 found.
#[derive(Debug, PartialEq)]
enum Links {
    Clear,
    /// The symbolic links found, in the order of their raw bytes: every one, and the count.
    Found(Vec<PathBuf>),
    /// The first entry the walk could not read, or `<A>` for the bound.
    Unreadable(PathBuf),
}
/// The most entries below `<A>` that R40 reads.
const ENTRIES: usize = 10_000;
/// Whether `e` says the entry is not there (R40).
fn absent(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}
/// The read of R40: the components of the branch's ref and reflog below `<G>/refs/heads` and
/// `<G>/logs/refs/heads`, then `<A>` (when it exists) and every entry below it, each by `lstat`,
/// no link followed or entered, no recursion, no program and no Git call.
fn read_links(g: &str, admin: Option<&str>, branch: &str) -> Links {
    let mut links: Vec<PathBuf> = vec![];
    for root in [format!("{g}/refs/heads"), format!("{g}/logs/refs/heads")] {
        let mut at = PathBuf::from(root);
        for part in branch.split('/') {
            at.push(part);
            match at.symlink_metadata() {
                Ok(m) if m.file_type().is_symlink() => {
                    links.push(at.clone());
                    break;
                }
                Ok(m) if m.is_dir() => {}
                Ok(_) => break,
                Err(e) if absent(&e) => break,
                Err(_) => return Links::Unreadable(at),
            }
        }
    }
    if let Some(admin) = admin {
        let mut todo = vec![PathBuf::from(admin)];
        let mut seen = 0;
        while let Some(dir) = todo.pop() {
            let listing = match std::fs::read_dir(&dir) {
                Ok(l) => l,
                Err(e) if absent(&e) => continue,
                Err(_) => return Links::Unreadable(dir),
            };
            for entry in listing {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) if absent(&e) => continue,
                    Err(_) => return Links::Unreadable(dir),
                };
                seen += 1;
                if seen > ENTRIES {
                    return Links::Unreadable(PathBuf::from(admin));
                }
                let path = entry.path();
                match path.symlink_metadata() {
                    Ok(m) if m.file_type().is_symlink() => links.push(path),
                    Ok(m) if m.is_dir() => todo.push(path),
                    Ok(_) => {}
                    Err(e) if absent(&e) => {}
                    Err(_) => return Links::Unreadable(path),
                }
            }
        }
    }
    links.sort_by(|a, b| a.as_os_str().as_bytes().cmp(b.as_os_str().as_bytes()));
    match links.is_empty() {
        true => Links::Clear,
        false => Links::Found(links),
    }
}
/// A path as UTF-8, U+FFFD in place of a byte sequence that is not (R40).
fn shown(p: &Path) -> String {
    String::from_utf8_lossy(p.as_os_str().as_bytes()).into_owned()
}
/// The detail of `git_dir_link` (R40): the paths that keep the JSON within R22's bound, and the count.
fn link_detail(links: &[PathBuf]) -> String {
    let all: Vec<String> = links.iter().map(|p| shown(p)).collect();
    let mut kept = all.len();
    loop {
        let paths = Value::from(all[..kept].to_vec());
        let text = format!("{{\"paths\":{paths},\"count\":{}}}", all.len());
        if text.len() <= DETAIL || kept == 0 {
            return text;
        }
        kept -= 1;
    }
}
/// The first `max` bytes of `t`, cut back to a character boundary.
fn cut(t: &str, max: usize) -> String {
    let mut end = t.len().min(max);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}
/// `body` byte for byte, but each match of `gh[opsur]_[A-Za-z0-9]+` or `github_pat_[A-Za-z0-9_]+`
/// (R22's first two patterns) replaced by `<redacted>`, and nothing cut (R19).
fn scrubbed(body: &str) -> String {
    let b = body.as_bytes();
    let run = |from: usize, keep: fn(u8) -> bool| {
        let end = (from..b.len()).find(|j| !keep(b[*j])).unwrap_or(b.len());
        (end > from).then_some(end)
    };
    let (mut out, mut kept, mut i) = (String::with_capacity(body.len()), 0, 0);
    while i < b.len() {
        let r = &b[i..];
        let login = r.len() > 4 && r.starts_with(b"gh") && b"opsur".contains(&r[2]) && r[3] == b'_';
        let end = match () {
            _ if r.starts_with(PAT.as_bytes()) => {
                run(i + PAT.len(), |c| c.is_ascii_alphanumeric() || c == b'_')
            }
            _ if login => run(i + 4, |c| c.is_ascii_alphanumeric()),
            _ => None,
        };
        match end {
            // A match begins and ends on ASCII bytes, so on character boundaries.
            Some(end) => {
                out.push_str(&body[kept..i]);
                out.push_str("<redacted>");
                (kept, i) = (end, end);
            }
            None => i += 1,
        }
    }
    out.push_str(&body[kept..]);
    out
}
fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v[k].as_str().filter(|t| !t.is_empty())
}
/// The session a record names: 1 to 200 bytes.
fn session(rec: &Value) -> Option<&str> {
    text(rec, "session").filter(|t| t.len() <= 200)
}
fn hex40(h: &str) -> bool {
    h.len() == 40
        && h.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
/// The key of a unit as `ap unit add` makes one (R2).
pub(crate) fn key_form(key: &str) -> bool {
    let byte = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-';
    let first = key.starts_with(|c: char| c.is_ascii_lowercase());
    (1..=59).contains(&key.len()) && first && key.bytes().all(byte)
}
/// A directory, and not a link.
fn real_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}
fn json_file(path: &str) -> Option<Value> {
    let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    v.is_object().then_some(v)
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// Words without spaces (a validated repository, branch and number), separated by one space.
fn w(words: &str) -> Vec<&str> {
    words.split(' ').collect()
}

/// The refusal an `ont action validate` answers, in the form of the store's refusal of an apply.
fn not_valid(checked: &Value) -> Option<Value> {
    let criteria = checked["result"]["criteria"].as_array().into_iter();
    let no: Vec<&Value> = criteria
        .flatten()
        .filter(|c| c["passed"] == false)
        .map(|c| &c["code"])
        .collect();
    let detail = json!({"status": "refused", "code": "submission_criteria", "failed": no});
    (checked["result"]["valid"] != true).then_some(detail)
}
/// The arguments after `run`: `<key> [--store <absolute path>]` (R1).
pub fn arguments(argv: &[OsString]) -> Result<(String, Option<String>), Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.as_deref() {
        Some([key]) if !key.starts_with('-') => Ok(((*key).to_owned(), None)),
        Some([key, "--store", p]) if !key.starts_with('-') && p.starts_with('/') => {
            Ok(((*key).to_owned(), Some((*p).to_owned())))
        }
        _ => Err(error("usage", "args")),
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = arguments(argv)
        .map_err(|f| refusal(f, 2, false))
        .and_then(|(key, store)| execute(&mut Host, store.as_deref(), &key));
    let key = argv.first().and_then(|k| k.to_str());
    let (written, status) = match result {
        Ok((v, status)) => (writeln!(std::io::stdout(), "{v}"), status),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail, "unit": key}});
            (writeln!(std::io::stderr(), "{v}"), e.status)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// Before the run a refusal of the store keeps the store's code (R6); after its first apply it is
/// `store_refused` with the store's refusal as `detail` (R22).
fn refusal(f: Failure, status: u8, running: bool) -> Box<Refusal> {
    let code = if running {
        f.code.to_owned()
    } else {
        store::code(&f)
    };
    let (field, detail) = (f.field, f.detail);
    Box::new(Refusal {
        code,
        field,
        detail,
        status,
    })
}
/// One call: the answer and its exit, or the refusal.
pub fn execute(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
) -> Result<(Value, u8), Box<Refusal>> {
    let (mut run, _lock) = prepare(s, store, key).map_err(|f| refusal(f, 2, false))?;
    let state = run.u["state"].as_str().unwrap_or_default().to_owned();
    let ended = state == "merged" || state == "dropped";
    let begun = match state.as_str() {
        // The Lead's rejection is undone by an amendment or a drop, never by a run (R6).
        "stopped" if run.u["stop_cause"] == "lead_rejected" => {
            let detail = run.u["stop_detail"].as_str().unwrap_or_default().to_owned();
            let answered = run.answer(s, Some(("lead_rejected", &detail)), &Outcome::default());
            return answered.map_err(|f| refusal(f, 3, true));
        }
        "ready" | "stopped" => run.begin(s),
        "running" => run.held(s),
        "accepted" | "merged" | "dropped" => Ok(()),
        other => Err(fail("unit_not_runnable", "unit", json!(other))),
    };
    begun.map_err(|f| refusal(f, 2, false))?;
    let driven = run.leftovers(s).and_then(|()| match ended {
        true => Ok(Outcome {
            cleanup: Some(run.cleanup(s)),
            ..Outcome::default()
        }),
        false => run.drive(s),
    });
    let driven = driven.map_err(|h| match h {
        Halt::Said(cause, detail, reply) => (Halt::Stop(cause, detail), Some(reply)),
        other => (other, None),
    });
    let answered = match driven {
        Ok(out) => run.answer(s, None, &out),
        // A unit that is not running is not stopped: the answer names the refusal, and the reply
        // of a failed `gh` call that caused it as `detail` (R22, R25, R30).
        Err((Halt::Stop(cause, _), reply)) if run.u["state"] != "running" => {
            let out = Outcome {
                refused: Some(cause),
                detail: reply,
                ..Outcome::default()
            };
            run.answer(s, None, &out)
        }
        Err((Halt::Stop(cause, detail), _)) => {
            let detail = cut(&detail, DETAIL);
            let stop = actions::StopUnit {
                cause: cause.clone(),
                detail: Some(detail.clone()),
                unit: Id::new(key),
            };
            let stopped = run.apply(s, &stop);
            stopped.and_then(|_| run.answer(s, Some((&cause, &detail)), &Outcome::default()))
        }
        Err((Halt::Fail(f), _)) => Err(f),
        Err((Halt::Said(..), _)) => unreachable!("a stop with a reply was taken apart above"),
    };
    let status = if run.applied || ended { 3 } else { 2 };
    answered.map_err(|f| refusal(f, status, true))
}
/// How a verifier's review closes: the verdict, the session that closes it (the recovery session when none), the
/// report with its SHA-256 and the account of the stage.
struct Closing {
    verdict: &'static str,
    session: Option<String>,
    report: Option<String>,
    sha: Option<String>,
    account: Option<String>,
}
impl Closing {
    fn abandoned(session: Option<String>) -> Self {
        Closing {
            verdict: "abandoned",
            session,
            report: None,
            sha: None,
            account: None,
        }
    }
}
/// How a call that did not stop ended: the merge, the cleanup, the refusal that left the unit as
/// it is (R25).
#[derive(Default)]
struct Outcome {
    merged_commit: Option<String>,
    cleanup: Option<Value>,
    refused: Option<String>,
    /// The redacted reply of the failed `gh` call that caused `refused`.
    detail: Option<String>,
}

/// An author record under `<directory>/<key>/`: `<n>.implementer.md.run.json` or `<n>.fixer.md.run.json`.
struct Author {
    role: String,
    path: String,
    rec: Value,
}
/// The worktree as read: clean, its HEAD, on the unit's branch, its commits beyond the base.
struct Tree {
    clean: bool,
    head: String,
    on_branch: bool,
    ahead: String,
    changed: usize,
    /// `<G>` and `<A>` of this read, for the reads of R40 before the pass's merges.
    dirs: (String, String),
}
impl Tree {
    fn shown(&self) -> String {
        let (changed, commits, on_branch) = (self.changed, &self.ahead, self.on_branch);
        json!({"changed": changed, "commits": commits, "on_branch": on_branch, "head": self.head})
            .to_string()
    }
}
/// The reviews of kind `packet` of the unit, by `started_at`, and what counts of them (R18).
struct Verdicts {
    list: Vec<(String, Value)>,
    amended: String,
}
impl Verdicts {
    fn counted(&self) -> impl Iterator<Item = &(String, Value)> {
        let counts = |v: &Value| {
            let verdict = v["verdict"] == "accept" || v["verdict"] == "repair";
            v["status"] == "closed" && verdict && self.later(v)
        };
        self.list.iter().filter(move |(_, v)| counts(v))
    }
    fn later(&self, v: &Value) -> bool {
        v["started_at"]
            .as_str()
            .is_some_and(|t| t > self.amended.as_str())
    }
    /// A carried review is a verdict but not a round: the counts leave it out (R18, R37).
    fn rounds(&self) -> usize {
        let judged = |(_, v): &&(String, Value)| v["carried_from"].is_null();
        self.counted().filter(judged).count()
    }
    fn repairs(&self) -> usize {
        self.counted()
            .filter(|(_, v)| v["verdict"] == "repair")
            .count()
    }
    fn newest(&self) -> Option<&(String, Value)> {
        self.counted().last()
    }
    fn of(&self, head: &str) -> Option<&(String, Value)> {
        self.counted().filter(|(_, v)| v["head"] == head).last()
    }
}

/// What this call did so far: its request ids, the steps it takes once, the head its own fixer moved
/// from (R12, R13, R19).
#[derive(Default)]
struct Done {
    count: u32,
    settled: bool,
    turned: bool,
    moved: bool,
    own: Option<Own>,
    checks: u32,
    updates: u32,
    /// The last accept this call carried to an updated head: `{review, from, file}` (R25, R37).
    carried: Option<Value>,
    /// The credential of each author stage this call admitted (R25, R41).
    credentials: Vec<Value>,
    /// The answer file of the newest fixer stage of this call (R19).
    fixer_answer: Option<String>,
    /// The head whose checks passed in this call and the `guarded` recorded for it (R46, R47).
    guarded: Option<(String, bool)>,
}
/// The heads this call's own fixer and updates moved, one after the other, from the head `from`
/// (R19).
struct Own {
    from: String,
    to: String,
    repair: bool,
    update: bool,
}
/// One call's unit and its places.
struct Run {
    store: Option<String>,
    key: String,
    session: String,
    prefix: String,
    u: Value,
    repo: String,
    branch: String,
    clone: String,
    home: PathBuf,
    wt: String,
    reviews: PathBuf,
    task: String,
    implementer: Value,
    verifier: Value,
    timeouts: Map<String, Value>,
    /// The file of the repository's stage token, when `units.github.stage_tokens` names it (R41).
    token_file: Option<String>,
    /// `units.approval` of the policy (R46 to R48).
    approval: Option<Approval>,
    base: String,
    applied: bool,
    done: Done,
}
/// The member `units` (R3): the directory and the timeouts by role.
pub(crate) fn units(policy: &Value) -> Result<(String, Map<String, Value>), Failure> {
    let mut timeouts: Map<String, Value> = TIMEOUTS
        .iter()
        .map(|(k, t)| ((*k).to_owned(), json!(t)))
        .collect();
    let Some(m) = policy.get("units") else {
        return Ok((UNITS.to_owned(), timeouts));
    };
    let m = m
        .as_object()
        .ok_or_else(|| invalid("units", "is not an object"))?;
    let mut directory = UNITS.to_owned();
    for (k, v) in m {
        match (k.as_str(), v) {
            ("directory", Value::String(d))
                if d.starts_with('/')
                    && (2..=400).contains(&d.len())
                    && d.bytes().all(|c| (b'!'..=b'~').contains(&c)) =>
            {
                directory = d.clone();
            }
            ("github", g) => {
                github_member(g)?;
            }
            ("approval", a) => {
                approval_member(a)?;
            }
            ("timeouts", Value::Object(t)) => {
                for (name, n) in t {
                    let seconds = n.as_u64().filter(|n| (1..=86_400).contains(n));
                    if !timeouts.contains_key(name) || seconds.is_none() {
                        let member = format!("units.timeouts.{name}");
                        return Err(invalid(&member, "is not a timeout of 1 to 86,400 seconds"));
                    }
                    timeouts.insert(name.clone(), n.clone());
                }
            }
            _ => {
                return Err(invalid(
                    &format!("units.{k}"),
                    "is not a member of this form",
                ));
            }
        }
    }
    Ok((directory, timeouts))
}
/// The member `units.github` (R3): the Lead's App, and the file of each repository's stage token.
#[derive(Default)]
pub(crate) struct Github {
    /// The App's id and the path of its private key (R42).
    pub lead_app: Option<(u64, String)>,
    /// The path of the stage token's file by repository (R41).
    pub stage_tokens: Map<String, Value>,
}
/// A path of the form `units.directory` has (R3).
fn path_form(d: &str) -> bool {
    d.starts_with('/')
        && (2..=400).contains(&d.len())
        && d.bytes().all(|c| (b'!'..=b'~').contains(&c))
}
/// `<owner>/<name>` of `specs/ap/unit.md` R9.
fn repository_form(t: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    matches!(t.split_once('/'), Some((o, n)) if part(o) && part(n) && n != "." && n != "..")
}
/// `units.github.lead_app`: exactly `{"id": <1 to 4,294,967,295>, "key": <path>}` (R3, R42).
pub(crate) fn lead_app(v: &Value) -> Result<(u64, String), Failure> {
    let m = v.as_object().filter(|m| m.len() == 2);
    let id = m.and_then(|m| m.get("id")?.as_u64());
    let id = id.filter(|n| (1..=u64::from(u32::MAX)).contains(n));
    let key = m.and_then(|m| m.get("key")?.as_str().filter(|k| path_form(k)));
    let why = "is not {\"id\": an integer of 1 to 4,294,967,295, \"key\": a path}";
    id.zip(key.map(str::to_owned))
        .ok_or_else(|| invalid("units.github.lead_app", why))
}
/// The member `units.github` read by its form alone: no file it names is opened (R3).
fn github_member(g: &Value) -> Result<Github, Failure> {
    let g = g
        .as_object()
        .ok_or_else(|| invalid("units.github", "is not an object"))?;
    let mut out = Github::default();
    for (k, v) in g {
        match k.as_str() {
            "lead_app" => out.lead_app = Some(lead_app(v)?),
            "stage_tokens" => {
                let why = "is not 1 to 32 repositories <owner>/<name>, each with a path";
                let m = v.as_object().filter(|m| (1..=32).contains(&m.len()));
                let ok = m.is_some_and(|m| {
                    m.iter()
                        .all(|(r, p)| repository_form(r) && p.as_str().is_some_and(path_form))
                });
                match (m, ok) {
                    (Some(m), true) => out.stage_tokens = m.clone(),
                    _ => return Err(invalid("units.github.stage_tokens", why)),
                }
            }
            other => {
                let member = format!("units.github.{other}");
                return Err(invalid(&member, "is not a member of this form"));
            }
        }
    }
    Ok(out)
}
/// The member `units.approval` (`specs/ap/policy.md` R20; R46 to R48): who approves an accepted
/// head of a `contract` unit, the paths that leave it to the Lead, and how often a merge so approved
/// opens a finding.
#[derive(Clone, Debug)]
pub(crate) struct Approval {
    /// `contract` is `verifier`: the run approves a `contract` unit's unguarded head (R47).
    pub by_verifier: bool,
    pub guarded: Vec<String>,
    pub sample: u64,
}
/// `units.approval` read by its form: exactly `contract`, `guarded` and `sample`.
fn approval_member(v: &Value) -> Result<Approval, Failure> {
    let m = v
        .as_object()
        .ok_or_else(|| invalid("units.approval", "is not an object"))?;
    for k in m.keys() {
        if !["contract", "guarded", "sample"].contains(&k.as_str()) {
            let member = format!("units.approval.{k}");
            return Err(invalid(&member, "is not a member of this form"));
        }
    }
    let by_verifier = match m.get("contract").and_then(Value::as_str) {
        Some("verifier") => true,
        Some("lead") => false,
        _ => {
            return Err(invalid(
                "units.approval.contract",
                "is not lead or verifier",
            ));
        }
    };
    let list = m.get("guarded").and_then(Value::as_array);
    let list = list.filter(|l| (1..=60).contains(&l.len()));
    let patterns: Option<Vec<String>> = list.and_then(|l| {
        l.iter()
            .map(|p| p.as_str().filter(|t| review::write_set_pattern(t).grammar))
            .map(|p| p.map(str::to_owned))
            .collect()
    });
    let Some(guarded) = patterns else {
        let why = "is not 1 to 60 patterns of the write set";
        return Err(invalid("units.approval.guarded", why));
    };
    let sample = m.get("sample").and_then(Value::as_u64);
    let Some(sample) = sample.filter(|n| (2..=100).contains(n)) else {
        let why = "is not an integer of 2 to 100";
        return Err(invalid("units.approval.sample", why));
    };
    Ok(Approval {
        by_verifier,
        guarded,
        sample,
    })
}
/// `units.approval` of the policy, or none (R46 to R48).
pub(crate) fn approval(policy: &Value) -> Result<Option<Approval>, Failure> {
    match policy.get("units").and_then(|u| u.get("approval")) {
        None => Ok(None),
        Some(a) => approval_member(a).map(Some),
    }
}
/// `units.github` of the policy, or none (R3, R41).
pub(crate) fn github(policy: &Value) -> Result<Github, Failure> {
    match policy.get("units").and_then(|u| u.get("github")) {
        None => Ok(Github::default()),
        Some(g) => github_member(g),
    }
}
/// A tier as members of an `ap run` request, its accounts by `specs/ap/policy.md` R6 (R4).
fn tier(u: &Value, which: &str, role: &str, policy: &Value) -> Result<Value, Failure> {
    let t = &u[which];
    let (runtime, model) = (text(t, "runtime").unwrap_or_default(), text(t, "model"));
    // Claude and `agy` only (R2): any other word stays out.
    if !matches!(runtime, "claude" | "agy") {
        return Err(fail("tier_not_supported", which, json!(runtime)));
    }
    let model = model.unwrap_or_default();
    let keys = [
        format!("{runtime}/{role}"),
        format!("{runtime}/{model}"),
        runtime.into(),
    ];
    let accounts = &policy["accounts"];
    let key = keys.iter().find(|k| accounts.get(k.as_str()).is_some());
    let key = key.ok_or_else(|| error("account_not_mapped", which))?;
    let list = accounts[key.as_str()]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let paths: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
    if paths.is_empty() || paths.len() > 5 || paths.len() != list.len() {
        return Err(invalid(&format!("accounts.{key}"), "is not 1 to 5 paths"));
    }
    // An `agy` stage has one account and never `auto` (`specs/ap/run.md` R47); a Claude stage's
    // other accounts follow it.
    let agy = runtime == "agy";
    let mode = if agy { "bypassPermissions" } else { "auto" };
    let mut req = json!({"runtime": runtime, "model": model, "effort": t["effort"],
        "account": paths[0], "permission_mode": mode});
    if paths.len() > 1 && !agy {
        req["next_accounts"] = json!(paths[1..]);
    }
    Ok(req)
}
/// The policy file as one JSON object, or null.
fn policy_of(s: &impl System) -> Value {
    let file = s
        .policy_file()
        .and_then(|p| s.read(&p.to_string_lossy()).ok());
    file.and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
/// The directories and the lock of the unit `key`, of a checked form (R3, R5, R35): the units
/// directory is resolved once; `<key>/wt` is appended to it unresolved (R8).
fn locked(directory: &str, key: &str) -> Result<(PathBuf, File), Failure> {
    let io = |what: &str| fail("io_error", "directory", json!(what));
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    dirs.create(directory)
        .map_err(|_| io("the units directory cannot be made"))?;
    let root = std::fs::canonicalize(directory).map_err(|_| io("the units directory"))?;
    let home = root.join(key);
    match dirs.recursive(false).create(&home) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(io("the unit's")),
        _ if !real_dir(&home) => return Err(fail("worktree_failed", "directory", json!(home))),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).mode(0o600);
    options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    let lock = options
        .open(home.join("lock"))
        .map_err(|_| io("the lock"))?;
    let held = rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive);
    held.map_err(|_| error("unit_run_active", "unit"))?;
    Ok((home, lock))
}
/// What the checks of a unit read before any action give (R2 to R4): its places and its tiers.
pub(crate) struct Checked {
    repo: String,
    branch: String,
    clone: String,
    reviews: Map<String, Value>,
    task: String,
    directory: String,
    timeouts: Map<String, Value>,
    implementer: Value,
    verifier: Value,
    github: Github,
    approval: Option<Approval>,
}
/// The checks `ap unit run` makes on the unit `u` of the key `key` before any action (R2 to R4): the
/// key, the repository, the branch, the policy's entries and the tiers. The dispatch pass
/// (`specs/ap/work.md` R10) calls the same function, so a unit the run refuses is not started.
pub(crate) fn checked(s: &mut impl System, key: &str, u: &Value) -> Result<Checked, Failure> {
    if !key_form(key) {
        return Err(error("invalid_input", "unit"));
    }
    let repo = text(u, "repository").ok_or_else(|| error("no_repository", "unit"))?;
    let branch = text(u, "branch").map_or_else(|| format!("unit/{key}"), str::to_owned);
    // `git check-ref-format` only for a name of R10's form, which never begins with `-`.
    let format = args(&["check-ref-format", "--branch", &branch]);
    let named = branch_ok(&branch)
        && (s.call("git", &format, &Value::Null, 10_000)).is_ok_and(|r| r.success);
    if !named {
        return Err(fail("invalid_branch", "branch", json!(branch)));
    }
    let clone = clone_of(s, repo)?;
    // `clone_of` read the file as one JSON object.
    let policy = policy_of(s);
    let reviews = page_review::policy(s)?;
    let task = reviews.get("task").and_then(Value::as_str);
    let task = task
        .ok_or_else(|| error("invalid_input", "task"))?
        .to_owned();
    let (directory, timeouts) = units(&policy)?;
    let github = github(&policy)?;
    let approval = approval(&policy)?;
    let implementer = tier(u, "implementer", "implementer", &policy)?;
    let verifier = tier(u, "verifier", "reviewer", &policy)?;
    Ok(Checked {
        github,
        approval,
        repo: repo.to_owned(),
        branch,
        clone,
        reviews,
        task,
        directory,
        timeouts,
        implementer,
        verifier,
    })
}
/// The checks before any effect (R1 to R4), then the directories and the lock (R3, R5).
fn prepare(s: &mut impl System, store: Option<&str>, key: &str) -> Result<(Run, File), Failure> {
    if object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    if !key_form(key) {
        return Err(error("invalid_input", "unit"));
    }
    let u = at(store::get_of::<objects::Unit>(s, store, key), "unit")?;
    let u = u.ok_or_else(|| error("unknown_unit", "unit"))?;
    let c = checked(s, key, &u)?;
    // The status needs the Lead's App; the run alone refuses its absence, before the lock (R41).
    if c.github.lead_app.is_none() {
        return Err(error("github_not_mapped", "lead_app"));
    }
    let token_file = object::member_of(&c.github.stage_tokens, &c.repo).map(|(_, v)| v);
    let token_file = token_file.and_then(Value::as_str).map(str::to_owned);
    let (home, lock) = locked(&c.directory, key)?;
    let drawn = new_session_id(s)?.replace('-', "");
    let dir = c.reviews.get("directory").and_then(Value::as_str);
    let run = Run {
        store: store.map(str::to_owned),
        key: key.to_owned(),
        session: format!("u-{key}"),
        prefix: format!("ur-{}", &drawn[..16]),
        repo: c.repo,
        branch: c.branch,
        clone: c.clone,
        wt: home.join("wt").to_string_lossy().into_owned(),
        home,
        reviews: PathBuf::from(dir.unwrap_or(REVIEWS)),
        task: c.task,
        implementer: c.implementer,
        verifier: c.verifier,
        timeouts: c.timeouts,
        token_file,
        approval: c.approval,
        u,
        base: String::new(),
        applied: false,
        done: Done::default(),
    };
    Ok((run, lock))
}

impl Run {
    fn store(&self) -> Option<&str> {
        self.store.as_deref()
    }
    /// One apply as the client `ap`, the agent `ap-unit`, the session `u-<key>` and the role `dispatch` (R1).
    fn apply<A: Action>(&mut self, s: &mut impl System, action: &A) -> Result<Applied, Failure> {
        let rid = self.rid();
        self.apply_as(s, rid, action)
    }
    fn apply_as<A: Action>(
        &mut self,
        s: &mut impl System,
        rid: String,
        action: &A,
    ) -> Result<Applied, Failure> {
        let actor = Actor {
            agent: "ap-unit",
            session: &self.session,
            role: "dispatch",
        };
        let applied = store::apply_action(s, self.store.as_deref(), &actor.call(rid), action);
        self.applied |= applied.is_ok();
        at(applied, A::NAME)
    }
    /// The `intent` of an action of an objective unit, copied from the unit's head; the head is the store's, so
    /// what `Id::from_value` does not read ends the form before the action (`name`), nothing applied.
    fn intent(&self, name: &str) -> Result<Option<Id<objects::Intent>>, Failure> {
        match self.u["lane"] == "objective" {
            true => store::id_of(&self.u["intent"], name),
            false => Ok(None),
        }
    }
    /// A new request id `ur-<16 hex>-<counter>` (R1).
    fn rid(&mut self) -> String {
        self.done.count += 1;
        format!("{}-{}", self.prefix, self.done.count)
    }
    fn reread(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let u = at(
            store::get_of::<objects::Unit>(s, self.store(), &self.key),
            "unit",
        )?;
        self.u = u.ok_or_else(|| error("store_answer_unreadable", "unit"))?;
        Ok(())
    }
    /// `start_unit` from `ready`, `resume_unit` from `stopped` (R6).
    fn begin(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let unit = Id::new(&self.key);
        if self.u["state"] == "ready" {
            let intent = self.intent(actions::StartUnit::NAME)?;
            self.apply(s, &actions::StartUnit { intent, unit })?;
        } else {
            let intent = self.intent(actions::ResumeUnit::NAME)?;
            self.apply(s, &actions::ResumeUnit { intent, unit })?;
        }
        self.u["state"] = json!("running");
        Ok(())
    }
    /// The take-over of a cut run applies nothing: an active hold whose subject is the unit or its intent
    /// refuses it (R6).
    fn held(&mut self, s: &mut impl System) -> Result<(), Failure> {
        let active =
            json!({"eq": [properties::Hold::Status.name(), values::HoldStatus::Active.as_str()]});
        let holds = at(
            store::search_of::<objects::Hold>(s, self.store(), vec![active]),
            "holds",
        )?;
        let intent = self.u["intent"]["native_key"].as_str();
        let named = |f: &Value| {
            let subject = &f[properties::Hold::Subject.name()];
            let key = subject["native_key"].as_str();
            match subject["object_type"].as_str() {
                Some(objects::Unit::NAME) => key == Some(self.key.as_str()),
                Some(objects::Intent::NAME) => key.is_some() && key == intent,
                _ => false,
            }
        };
        match holds.iter().find(|(_, f)| named(f)) {
            Some((key, _)) => Err(fail("held", "unit", json!(key))),
            None => Ok(()),
        }
    }

    /// The author records by `<n>` (R3).
    fn authors(&self) -> Vec<Author> {
        let mut list = vec![];
        for e in std::fs::read_dir(&self.home)
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((n, rest)) = name.split_once('.') else {
                continue;
            };
            let role = rest
                .strip_suffix(".md.run.json")
                .filter(|r| ["implementer", "fixer"].contains(r));
            let path = self.home.join(&name).to_string_lossy().into_owned();
            if let (Ok(n), Some(role), Some(rec)) = (n.parse::<u64>(), role, json_file(&path)) {
                list.push((
                    n,
                    Author {
                        role: role.to_owned(),
                        path,
                        rec,
                    },
                ));
            }
        }
        list.sort_by_key(|(n, _)| *n);
        list.into_iter().map(|(_, a)| a).collect()
    }
    /// The next stage's `<n>`: one more than any `<n>.` of the unit's directory.
    fn next(&self) -> u64 {
        let entries = std::fs::read_dir(&self.home)
            .into_iter()
            .flatten()
            .flatten();
        let numbers = entries.filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.split_once('.')
                .and_then(|(n, _)| n.parse::<u64>().ok())
        });
        numbers.max().unwrap_or(0) + 1
    }
    /// A new prompt file of mode 0600 under `<directory>/<key>/`.
    fn prompt(&self, name: &str, text: &str) -> Step<String> {
        let path = self.home.join(name);
        let mut options = std::fs::OpenOptions::new();
        let file = options.write(true).create_new(true).mode(0o600).open(&path);
        let written = file.and_then(|mut f| f.write_all(text.as_bytes()));
        written.map_err(|_| Halt::Stop("io_error".into(), name.into()))?;
        Ok(path.to_string_lossy().into_owned())
    }
    /// A stage's end state and the first bytes of its answer.
    fn answered(out: &str, rec: &Value) -> String {
        let answer = std::fs::read(out).unwrap_or_default();
        let state = rec["state"].as_str().unwrap_or("failed");
        cut(
            &format!("{state}: {}", String::from_utf8_lossy(&answer)),
            DETAIL,
        )
    }
    /// The `ap run` request of a stage of `tier` in the worktree (R4, R11, R19).
    fn stage(&self, tier: &Value, role: &str, timeout: &str, (prompt, out): (&str, &str)) -> Value {
        let mut req = tier.clone();
        (req["role"], req["cwd"], req["prompt"]) = (json!(role), json!(self.wt), json!(prompt));
        (req["out"], req["unit"]) = (json!(out), json!(self.key));
        req["timeout_seconds"] = self.timeouts[timeout].clone();
        if let Some(store) = &self.store {
            req["store"] = json!(store);
        }
        req
    }

    /// The reviews of kind `packet` of the unit (R18).
    fn verdicts(&mut self, s: &mut impl System) -> Step<Verdicts> {
        let args = json!({"unit": self.key});
        let keys = at(
            store::view_keys(s, self.store(), "reviews_of_unit", args),
            "reviews",
        )?;
        let mut list = vec![];
        for k in keys {
            let v = at(
                store::get_of::<objects::Review>(s, self.store(), &k),
                "reviews",
            )?;
            if let Some(v) = v.filter(|v| v["kind"] == "packet") {
                list.push((k, v));
            }
        }
        let started = |v: &Value| v["started_at"].as_str().map(str::to_owned);
        list.sort_by_key(|(k, v)| (started(v), v["round"].as_u64(), k.clone()));
        let amended = self.u["amended_at"].as_str().unwrap_or_default().to_owned();
        Ok(Verdicts { list, amended })
    }
    /// A review's report path, by its key (checked) and never by a member of the store (R3).
    fn report(&self, key: &str) -> Option<String> {
        let path = self.reviews.join(format!("{key}.md"));
        key_ok(key).then(|| path.to_string_lossy().into_owned())
    }
    /// `close_unit_review`; a session the store refuses as a writer closes `abandoned` with the
    /// session `u-<key>:recovery` (R7). True when the review closed as asked.
    fn close(&mut self, s: &mut impl System, review: &str, e: Closing) -> Step<bool> {
        let recovery = format!("{}:recovery", self.session);
        let session = e.session.unwrap_or_else(|| recovery.clone());
        let action = actions::CloseUnitReview {
            account: e.account,
            report: e.report.map(values::FilePath),
            report_sha256: e.sha.map(values::Sha256Hex),
            review: Id::new(review),
            session: Some(session.clone()),
            unit: Some(Id::new(&self.key)),
            verdict: values::ReviewVerdict(e.verdict.to_owned()),
        };
        match self.apply(s, &action) {
            Ok(_) => Ok(true),
            Err(f) if store::code(&f) == "writer_cannot_review" && session != recovery => self
                .close(s, review, Closing::abandoned(Some(recovery)))
                .map(|_| false),
            Err(f) => Err(f.into()),
        }
    }

    /// The leftovers of a cut run, before the loop (R7).
    fn leftovers(&mut self, s: &mut impl System) -> Step {
        for a in self.authors() {
            self.end_stage(s, &a.path, &a.rec)?;
        }
        let v = self.verdicts(s)?;
        for (k, _) in v.list.iter().filter(|(_, r)| r["status"] == "running") {
            let record = self.report(k).map(|out| format!("{out}.run.json"));
            let rec = record.as_deref().and_then(json_file);
            if let (Some(path), Some(rec)) = (&record, &rec) {
                self.end_stage(s, path, rec)?;
            }
            let session = rec.as_ref().and_then(session);
            self.close(s, k, Closing::abandoned(session.map(str::to_owned)))?;
        }
        // No stage of the unit lives now: every scratch directory goes, the one a killed `ap run`
        // left (its record still `running`) among them; no link is followed (R7).
        let scratch = self.home.join("scratch");
        if real_dir(&scratch) {
            for e in std::fs::read_dir(&scratch).into_iter().flatten().flatten() {
                let p = e.path();
                let _ = match p.symlink_metadata().is_ok_and(|m| m.is_dir()) {
                    true => std::fs::remove_dir_all(&p),
                    false => std::fs::remove_file(&p),
                };
            }
        }
        Ok(())
    }
    /// A stage of a cut run: its unit stopped and read as ended, its run closed (R7).
    fn end_stage(&mut self, s: &mut impl System, path: &str, rec: &Value) -> Step {
        let running = rec["state"] == "running";
        if let Some(name) = rec["unit"].as_str().filter(|_| running)
            && !(stage_unit(name) && unit_ended(s, name, true))
        {
            return failed("run_still_active", "record", json!(name));
        }
        let closed = ["applied", "already_applied"].map(|c| rec["run_object"]["close"] == c);
        let run = rec["run_object"]["key"]
            .as_str()
            .filter(|_| closed == [false, false]);
        let Some(run) = run else { return Ok(()) };
        let found = store::get_of::<objects::Run>(s, self.store(), run)
            .ok()
            .flatten();
        if !found.is_some_and(|r| r["record"] == path && r["status"] == "running") {
            return Ok(());
        }
        let state = if running {
            "killed"
        } else {
            rec["state"].as_str().unwrap_or("failed")
        };
        let action = actions::CloseRun {
            cache_read_tokens: None,
            cache_write_tokens: None,
            input_tokens: None,
            limit: None,
            output_tokens: None,
            record: None,
            run: Id::new(run),
            served_by: None,
            session: session(rec).map(str::to_owned),
            settled_usd_micros: None,
            state: state.to_owned(),
            stream: None,
        };
        let out = path.strip_suffix(".run.json").unwrap_or(path);
        let rid = format!("run-{}-close", &digest(out.as_bytes())[..32]);
        // The stage's own request id; the store's answer is the run's, and a later call tries again.
        let _ = store::close_run_action(s, self.store(), rid, &action);
        Ok(())
    }

    fn git(&self, s: &mut impl System, dir: &str, words: &[&str]) -> Step<String> {
        let out = review::git(s, dir, words);
        let out = out.map_err(|_| Halt::Stop("git_failed".into(), words.join(" ")))?;
        Ok(out.trim_end().to_owned())
    }
    fn git_ok(&self, s: &mut impl System, dir: &str, words: &[&str]) -> bool {
        review::git(s, dir, words).is_ok()
    }
    /// `git fetch origin <what>` in the clone within 120 seconds (R8, R12), no hook run (R39).
    fn fetch(&self, s: &mut impl System, what: &str) -> Step {
        let mut words = vec!["--no-replace-objects", "-C", &self.clone];
        words.extend(GIT_SAFE);
        words.extend(["fetch", "--quiet", "origin", what]);
        let mut fetched = s.call("git", &args(&words), &Value::Null, 120_000);
        // A fetch of `main` that raced another one on the same clone is tried once more (R8).
        if what == "main"
            && fetched
                .as_ref()
                .is_ok_and(|r| !r.success && racing(&r.bytes))
        {
            s.pause(Duration::from_secs(1));
            fetched = s.call("git", &args(&words), &Value::Null, 120_000);
        }
        match fetched.is_ok_and(|r| r.success) {
            true => Ok(()),
            false => stop("fetch_failed", what),
        }
    }
    fn gh(&self, s: &mut impl System, words: &str) -> Step<Value> {
        // A transient failure of the hosting service is tried again, twice at most, after 10 and
        // then 30 seconds (R9); a retry adds nothing to the store.
        let mut reply = s.call("gh", &args(&w(words)), &Value::Null, 60_000);
        for pause in [10, 30] {
            let transient = match &reply {
                Ok(r) if r.success => false,
                Ok(r) => transient(&crate::unit::redact(&r.bytes)),
                Err(_) => true,
            };
            if !transient {
                break;
            }
            s.pause(Duration::from_secs(pause));
            reply = s.call("gh", &args(&w(words)), &Value::Null, 60_000);
        }
        // A call that ran and failed keeps its reply, redacted, in the detail (R22, R25).
        if let Some(r) = reply.as_ref().ok().filter(|r| !r.success) {
            let reply = crate::unit::redact(&r.bytes);
            let detail = json!({"call": &words[..7], "reply": reply});
            return said("github_unreadable", detail, reply);
        }
        let v = reply.ok();
        let v = v.and_then(|r| serde_json::from_slice::<Value>(&r.stdout).ok());
        v.map_or_else(|| stop("github_unreadable", &words[..7]), Ok)
    }
    /// The open pull request of the branch against `main` from the repository itself (R9).
    fn list(&self, s: &mut impl System) -> Step<Option<(u64, String)>> {
        let fields = "number,headRefOid,headRefName,baseRefName,isCrossRepository,headRepositoryOwner,headRepository";
        let (repo, branch) = (&self.repo, &self.branch);
        let words = format!(
            "pr list --repo {repo} --head {branch} --base main --state open --json {fields}"
        );
        let v = self.gh(s, &words)?;
        let own = |p: &&Value| p["baseRefName"] == "main" && self.of_branch(p);
        let list: Vec<&Value> = v.as_array().into_iter().flatten().filter(own).collect();
        let [p] = list.as_slice() else {
            return match list.len() {
                0 => Ok(None),
                n => stop("pull_request_ambiguous", json!({"count": n})),
            };
        };
        let n = p["number"].as_u64().filter(|n| *n > 0);
        let head = p["headRefOid"]
            .as_str()
            .filter(|h| hex40(h))
            .map(str::to_owned);
        n.zip(head)
            .map_or_else(|| stop("github_unreadable", "pr list"), |p| Ok(Some(p)))
    }
    /// A pull request whose head branch is the unit's branch of the repository itself (R9, R10).
    fn of_branch(&self, p: &Value) -> bool {
        let (owner, name) = self.repo.split_once('/').unwrap_or_default();
        let same = |a: &Value, b: &str| a.as_str().is_some_and(|a| a.eq_ignore_ascii_case(b));
        p["headRefName"] == self.branch.as_str()
            && p["isCrossRepository"] == false
            && same(&p["headRepositoryOwner"]["login"], owner)
            && same(&p["headRepository"]["name"], name)
    }
    /// Pull request `n` as the hosting service shows it: its state and head (checked), its base, its
    /// head branch and repository, its merge commit.
    fn pull(&self, s: &mut impl System, n: u64) -> Step<Value> {
        let fields = "state,baseRefName,headRefOid,headRefName,isCrossRepository,headRepositoryOwner,headRepository,mergeCommit";
        let v = self.gh(
            s,
            &format!("pr view {n} --repo {} --json {fields}", self.repo),
        )?;
        let head = v["headRefOid"].as_str().is_some_and(hex40);
        match (v["state"].is_string(), head) {
            (true, true) => Ok(v),
            _ => stop("github_unreadable", "pr view"),
        }
    }
    /// The state and the head of pull request `n`.
    fn view(&self, s: &mut impl System, n: u64) -> Step<(String, String)> {
        let v = self.pull(s, n)?;
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok((text("state"), text("headRefOid")))
    }
    /// The read of the hosting service at the start of a pass (R9).
    fn read(&mut self, s: &mut impl System) -> Step<Option<(u64, String)>> {
        if let Some(n) = self.u["pull_request"].as_u64() {
            let (state, head) = self.view(s, n)?;
            if state != "OPEN" {
                return stop(
                    "pull_request_not_open",
                    json!({"pull_request": n, "state": state}),
                );
            }
            return Ok(Some((n, head)));
        }
        let found = self.list(s)?;
        if let (Some((n, _)), true) = (&found, self.authors().is_empty()) {
            return stop("branch_in_use", json!({"pull_request": n}));
        }
        Ok(found)
    }
    /// After an author stage, its local head seen as the pull request's head within 120 seconds
    /// (R20); without a pull request, until one shows or the bound passes.
    fn wait(&mut self, s: &mut impl System) -> Step {
        let local = self.git(s, &self.wt, &["rev-parse", "HEAD"])?;
        let deadline = s.now() + Duration::from_secs(120);
        loop {
            let seen = match self.u["pull_request"].as_u64() {
                Some(n) => self.view(s, n).ok().map(|(_, h)| h),
                None => self.list(s).ok().flatten().map(|(_, h)| h),
            };
            match seen {
                Some(h) if h == local => return Ok(()),
                None if s.now() >= deadline && self.u["pull_request"].is_null() => return Ok(()),
                _ if s.now() >= deadline => return stop("head_not_seen", json!({"head": local})),
                _ => s.pause(Duration::from_secs(5)),
            }
        }
    }

    /// The clone's worktree entry whose path is exactly `wt` (R8).
    fn entry(&self, s: &mut impl System) -> Step<Option<String>> {
        let list = self.git(s, &self.clone, &["worktree", "list", "--porcelain"])?;
        let first = format!("worktree {}", self.wt);
        let found = list
            .split("\n\n")
            .find(|e| e.lines().next() == Some(first.as_str()));
        Ok(found.map(str::to_owned))
    }
    /// No worktree: a new branch from the base, or the unit's own branch as it is (R10).
    fn make_worktree(&mut self, s: &mut impl System) -> Step {
        let (clone, wt, branch) = (&self.clone, &self.wt, &self.branch);
        let reference = format!("refs/heads/{branch}");
        let exists = self.git_ok(s, clone, &["rev-parse", "--verify", "--quiet", &reference]);
        let added = if exists {
            let entry = self.entry(s)?;
            let own = entry.is_some_and(|e| e.lines().any(|l| l == format!("branch {reference}")));
            // A pull request the unit holds makes the branch its own only as that one's head branch.
            let held = match self.u["pull_request"].as_u64().filter(|_| !own) {
                Some(n) => self.of_branch(&self.pull(s, n)?),
                None => own,
            };
            if !held {
                return stop("branch_in_use", json!({"branch": branch}));
            }
            self.no_links_to_come(s)?;
            self.git(s, clone, &["worktree", "add", "--quiet", wt, branch])
        } else {
            self.no_links_to_come(s)?;
            self.git(
                s,
                clone,
                &["worktree", "add", "--quiet", "-b", branch, wt, &self.base],
            )
        };
        added
            .map(|_| ())
            .or_else(|_| stop("worktree_failed", "git worktree add"))
    }
    /// R40's read before `git worktree add`: `<G>` by one Git call, the two branch paths alone.
    fn no_links_to_come(&self, s: &mut impl System) -> Step {
        let g = self.common_dir(s)?;
        self.no_links(&g, None)
    }
    /// The worktree as read, after its guards (R8).
    fn tree(&self, s: &mut impl System) -> Step<Tree> {
        if !real_dir(&self.home) || !real_dir(Path::new(&self.wt)) {
            return stop("worktree_failed", "not a directory");
        }
        let Some(entry) = self.entry(s)? else {
            return stop("worktree_failed", "no worktree of the clone at this path");
        };
        let dirs = self.git_dirs(s)?;
        self.no_links(&dirs.0, Some(&dirs.1))?;
        let on_branch = entry
            .lines()
            .any(|l| l == format!("branch refs/heads/{}", self.branch));
        let wt = self.wt.as_str();
        let status = self.git(
            s,
            wt,
            &[
                "status",
                "--porcelain",
                "--untracked-files=normal",
                "--ignore-submodules=all",
            ],
        )?;
        let head = self.git(s, wt, &["rev-parse", "HEAD"])?;
        let ahead = self.git(
            s,
            wt,
            &["rev-list", "--count", &format!("{}..HEAD", self.base)],
        )?;
        let changed = status.lines().count();
        Ok(Tree {
            clean: changed == 0,
            head,
            on_branch,
            ahead,
            changed,
            dirs,
        })
    }

    /// `<G>`: what `git rev-parse --path-format=absolute --git-common-dir` prints in the clone (R8).
    fn common_dir(&self, s: &mut impl System) -> Step<String> {
        let words = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
        self.git(s, &self.clone, &words)
    }
    /// The read of R40 for the unit's branch: a stop `git_dir_link` or `worktree_failed`.
    fn no_links(&self, g: &str, admin: Option<&str>) -> Step {
        match read_links(g, admin, &self.branch) {
            Links::Clear => Ok(()),
            Links::Found(links) => stop("git_dir_link", link_detail(&links)),
            Links::Unreadable(at) => stop(
                "worktree_failed",
                format!("git directory unreadable: {}", shown(&at)),
            ),
        }
    }
    /// The clone's Git directory `<G>` and the worktree's administrative directory `<A>`, after the
    /// guard that the worktree names its own Git directory and nothing else (R8).
    fn git_dirs(&self, s: &mut impl System) -> Step<(String, String)> {
        let g = self.common_dir(s)?;
        // The one line of a regular file that is not a link.
        let line = |p: &Path| -> Option<String> {
            p.symlink_metadata()
                .ok()
                .filter(|m| m.file_type().is_file())?;
            let text = std::fs::read_to_string(p).ok()?;
            let text = text.strip_suffix('\n').unwrap_or(&text);
            (!text.contains('\n')).then(|| text.to_owned())
        };
        let dotgit = Path::new(&self.wt).join(".git");
        let admin = line(&dotgit).and_then(|l| l.strip_prefix("gitdir: ").map(str::to_owned));
        let Some(admin) = admin else {
            return stop("worktree_failed", "git directory");
        };
        let name = admin.strip_prefix(&format!("{g}/worktrees/"));
        let one = name.is_some_and(|n| !n.is_empty() && !n.contains('/') && n != "." && n != "..");
        let at = Path::new(&admin);
        let back = dotgit.to_string_lossy();
        let named = one
            && at.symlink_metadata().is_ok_and(|m| m.is_dir())
            && line(&at.join("gitdir")).as_deref() == Some(&*back)
            && line(&at.join("commondir")).as_deref() == Some("../..");
        match named {
            true => Ok((g, admin)),
            false => stop("worktree_failed", "git directory"),
        }
    }
    /// The `limits` and the `env` of the stage `<n>` (R38): a scratch directory of its own, a Cargo
    /// home in the scratch directory, the state of `gh` shown only to an author with the host's login
    /// (`host`, R41); for an author also the worktree and what a commit, a merge, a fetch and a push
    /// write of the clone's Git directory, after the run made those paths and set the `origin/main`
    /// of the stage's own view to the base; then, read-only, the files its prompt names (`files`:
    /// the verifier's report for a fixer, the description and the author's answer for a verifier).
    fn limits(
        &self,
        s: &mut impl System,
        n: u64,
        author: bool,
        host: bool,
        files: &[String],
    ) -> Step<(Value, Value)> {
        let mut dirs = std::fs::DirBuilder::new();
        dirs.mode(0o700);
        let made = |dirs: &std::fs::DirBuilder, p: &Path| match dirs.create(p) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => Err(e),
            _ => Ok(()),
        };
        let scratch = self.home.join("scratch");
        made(&dirs, &scratch).map_err(|_| Halt::Stop("io_error".into(), "scratch".into()))?;
        let at = scratch.join(n.to_string()).to_string_lossy().into_owned();
        let env = json!({"CARGO_HOME": format!("{at}/cargo")});
        let mut limits = json!({"scratch": at});
        if author && host {
            limits["show"] = json!(["~/.config/gh"]);
        }
        if !author {
            if !files.is_empty() {
                limits["read_only"] = json!(files);
            }
            return Ok((limits, env));
        }
        let (g, admin) = self.git_dirs(s)?;
        let text = |p: &Path| p.to_string_lossy().into_owned();
        let (gp, branch) = (Path::new(&g), Path::new(&self.branch));
        let holder = |p: PathBuf| p.parent().map(Path::to_path_buf).unwrap_or(p);
        let refs = holder(gp.join("refs/heads").join(branch));
        let logs = holder(gp.join("logs/refs/heads").join(branch));
        let targets = [
            gp.join("refs/remotes/origin"),
            gp.join("logs/refs/remotes/origin"),
            gp.join("refs/tags"),
        ];
        let failed = |what: &str| Halt::Stop("worktree_failed".into(), what.into());
        for d in [&refs, &logs].into_iter().chain(&targets) {
            std::fs::create_dir_all(d).map_err(|_| failed("git directory"))?;
        }
        let remotes = self.home.join("remotes");
        let sources = ["refs", "logs", "tags"].map(|d| remotes.join(d));
        made(&dirs, &remotes).map_err(|_| failed("remotes"))?;
        if !real_dir(&remotes) {
            return stop("worktree_failed", "remotes");
        }
        for d in &sources {
            made(&dirs, d).map_err(|_| failed("remotes"))?;
        }
        let worktree_config = Path::new(&admin).join("config.worktree");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o644);
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        match options.open(&worktree_config) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
                return stop("worktree_failed", "git directory");
            }
            _ => {}
        }
        // `origin/main` of the stage's view is the base: a new file of the run's own, renamed over
        // the name a stage could have made a link or a directory (R38).
        let main = sources[0].join("main");
        if main.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            std::fs::remove_dir_all(&main).map_err(|_| failed("remotes"))?;
        }
        let new = remotes.join("main.new");
        let _ = std::fs::remove_file(&new);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        let written = options
            .open(&new)
            .and_then(|mut f| f.write_all(format!("{}\n", self.base).as_bytes()));
        written.map_err(|_| failed("remotes"))?;
        std::fs::rename(&new, &main).map_err(|_| failed("remotes"))?;
        let wt = Path::new(&self.wt);
        let a = Path::new(&admin);
        limits["write"] = json!([
            self.wt,
            text(&gp.join("objects")),
            admin,
            text(&refs),
            text(&logs)
        ]);
        let pairs: Vec<Value> = (sources.iter().zip(&targets))
            .map(|(from, to)| json!([text(from), text(to)]))
            .collect();
        limits["over"] = json!(pairs);
        let mut read_only = vec![
            text(&wt.join(".git")),
            text(&a.join("commondir")),
            text(&a.join("gitdir")),
            text(&worktree_config),
        ];
        read_only.extend(files.iter().cloned());
        limits["read_only"] = json!(read_only);
        Ok((limits, env))
    }
    fn page(&mut self, s: &mut impl System, role: Role) -> Step<String> {
        let u = self.u.clone();
        Ok(render::page_text(s, self.store(), &self.key, &u, Some(role))?.0)
    }
    /// One author stage of `role` with `git_author`, resuming `from` when given, reading `files`
    /// beside its worktree: its record and its answer's path (R11 to R13, R21, R38).
    fn author(
        &mut self,
        s: &mut impl System,
        role: &str,
        from: Option<&Author>,
        text: &str,
        files: &[String],
    ) -> Step<(Value, String)> {
        let n = self.next();
        // The credential: the stage token read anew, or the host's login; named in its file (R41).
        let token = self.stage_token(s)?;
        let credential = if token.is_some() { "token" } else { "host" };
        self.prompt(&format!("{n}.credential"), &format!("{credential}\n"))?;
        let (limits, mut env) = self.limits(s, n, true, token.is_none(), files)?;
        if let Some(t) = token {
            env["GH_TOKEN"] = json!(t);
        }
        let prompt = self.prompt(&format!("{n}.{role}.prompt.md"), text)?;
        let out = self
            .home
            .join(format!("{n}.{role}.md"))
            .to_string_lossy()
            .into_owned();
        let mut req = self.stage(&self.implementer, role, role, (&prompt, &out));
        req["git_author"] = json!(true);
        (req["limits"], req["env"]) = (limits, env);
        if let Some(a) = from {
            for k in ["model", "effort", "runtime"]
                .iter()
                .filter(|k| a.rec[**k].is_string())
            {
                req[*k] = a.rec[*k].clone();
            }
            req["resume"] = json!({"record": a.path});
        }
        let (rec, _) = crate::run::execute(s, &req).map_err(refused)?;
        let admitted = json!({"stage": n, "role": role, "credential": credential});
        self.done.credentials.push(admitted);
        if role == "fixer" {
            self.done.fixer_answer = Some(out.clone());
        }
        Ok((rec, out))
    }
    /// The stage token of the unit's repository, read anew through the port, or `None` for the host's
    /// login (R41): a file that cannot be read, or text of another form, stops the unit.
    fn stage_token(&self, s: &impl System) -> Step<Option<String>> {
        let Some(path) = &self.token_file else {
            return Ok(None);
        };
        let unreadable = |reason: &str| stop("stage_token_unreadable", json!({"reason": reason}));
        let Ok(bytes) = s.read(path) else {
            return unreadable("read");
        };
        let text = match bytes.strip_suffix(b"\n") {
            Some(t) => t.strip_suffix(b"\r").unwrap_or(t),
            None => &bytes[..],
        };
        let rest = text.strip_prefix(PAT.as_bytes());
        let ok = rest.is_some_and(|r| {
            (1..=255).contains(&r.len())
                && r.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        });
        match ok {
            true => Ok(Some(String::from_utf8_lossy(text).into_owned())),
            false => unreadable("form"),
        }
    }
    /// No pull request: the implementer, fresh or resumed (R11).
    fn implement(&mut self, s: &mut impl System, t: &Tree) -> Step {
        let authors = self.authors();
        let named = |a: &&Author| a.role == "implementer" && session(&a.rec).is_some();
        let from = authors.iter().rev().find(named);
        let mut text = self.page(s, Role::Implementer)?;
        match from {
            Some(_) => text.push_str(&format!("\n{CONTINUE}")),
            None if !t.clean || !t.on_branch || t.ahead != "0" => {
                return stop("worktree_dirty", t.shown());
            }
            None => {}
        }
        let (rec, out) = self.author(s, "implementer", from, &text, &[])?;
        self.wait(s)?;
        match self.list(s)? {
            None => stop("no_pull_request", Self::answered(&out, &rec)),
            Some(_) => Ok(()),
        }
    }
    /// A worktree a cut author stage left: its author settles it, once in a call (R12).
    fn settle(&mut self, s: &mut impl System, t: &Tree) -> Step {
        if self.done.settled {
            return stop("worktree_dirty", t.shown());
        }
        self.done.settled = true;
        let authors = self.authors();
        let v = self.verdicts(s)?;
        let newest = authors.last();
        if let (Some((k, r)), Some(a)) = (v.newest(), newest)
            && r["verdict"] == "repair"
            && a.role == "fixer"
            && a.rec["state"] != "ok"
        {
            let judged = r["head"].as_str().unwrap_or_default();
            return self.repair(s, k, judged, &v);
        }
        match newest.filter(|a| session(&a.rec).is_some()) {
            Some(a) => self.turn(s, a),
            None => stop("worktree_dirty", t.shown()),
        }
    }
    /// The settling stage: the session of `a` resumed under its role with `continue.md` (R12, R13).
    fn turn(&mut self, s: &mut impl System, a: &Author) -> Step {
        let text = format!("{}\n{CONTINUE}", self.page(s, Role::Implementer)?);
        self.author(s, &a.role, Some(a), &text, &[])?;
        self.wait(s)
    }
    /// The newest author record, when the author's one turn on an amended page is due (R13).
    fn amended_turn(&self) -> Option<Author> {
        let amended = self.u["amended_at"].as_str().unwrap_or_default();
        let authors = self.authors();
        let started = |a: &Author| a.rec["start"].as_str().unwrap_or_default().to_owned();
        let done = authors
            .iter()
            .any(|a| started(a).as_str() > amended && a.rec["state"] == "ok");
        let newest = authors.into_iter().last()?;
        let due = amended > started(&newest).as_str() && session(&newest.rec).is_some();
        (due && !done && !self.done.turned).then_some(newest)
    }
    /// `record_head` with the union of the unit's writers and every author session (R14), and `extra` (R20).
    fn record_head(
        &mut self,
        s: &mut impl System,
        n: u64,
        head: &str,
        extra: Option<&str>,
        guarded: Option<bool>,
    ) -> Step {
        let held = self.u["writers"].as_array().into_iter().flatten();
        let mut writers: Vec<String> = held.filter_map(|w| w.as_str().map(str::to_owned)).collect();
        let mut sessions = vec![];
        for a in self.authors() {
            let attempts = a.rec["attempts"].as_array().cloned().unwrap_or_default();
            let all = attempts.iter().chain([&a.rec]).filter_map(session);
            sessions.extend(all.map(str::to_owned));
        }
        for w in sessions.into_iter().chain(extra.map(str::to_owned)) {
            if !writers.contains(&w) {
                writers.push(w);
            }
        }
        let rounds = self.verdicts(s)?.rounds();
        let name = actions::RecordHead::NAME;
        let action = actions::RecordHead {
            delivered_lines: None,
            guarded,
            head: values::CommitOrDigest(head.to_owned()),
            pull_request: Some(store::integer(n, name)?),
            rounds: Some(store::integer(rounds, name)?),
            unit: Id::new(&self.key),
            writers,
        };
        self.apply(s, &action)?;
        Ok(self.reread(s)?)
    }

    fn clean_at(&self, s: &mut impl System, head: &str) -> Step {
        let t = self.tree(s)?;
        match t.clean && t.head == head && t.on_branch {
            true => Ok(()),
            false => stop("worktree_dirty", t.shown()),
        }
    }
    /// The deterministic checks on the head (R16); with `units.approval`, whether a changed path
    /// meets a guarded pattern (R46).
    fn checks(&mut self, s: &mut impl System, head: &str) -> Step<Option<bool>> {
        self.clean_at(s, head)?;
        let (wt, base) = (self.wt.as_str(), self.base.as_str());
        let mut words = vec!["diff"];
        words.extend(DIFF_OPTIONS.into_iter().chain([base, head]));
        let diff = self.git(s, wt, &words)?;
        let paths: Vec<&str> = diff.split_terminator('\0').collect();
        let set = self.u["write_set"].as_array().into_iter().flatten();
        let set: Vec<String> = set.filter_map(|p| p.as_str().map(str::to_owned)).collect();
        let by_policy = (self.approval.as_ref())
            .map(|a| review::outside(&a.guarded, &paths).len() != paths.len());
        let outside = review::outside(&set, &paths);
        if !outside.is_empty() {
            let shown: Vec<&&str> = outside.iter().take(10).collect();
            return stop(
                "path_outside_write_set",
                json!({"paths": shown, "count": outside.len()}),
            );
        }
        let plain = self.u["class"] == "plain";
        // R49, R50: the rows of `ci/skips.json` at the base and at the head, read from the clone.
        let skips = match plain || self.approval.is_some() {
            true => {
                let base = self.base.clone();
                let at_base = self.skip_rows_at(s, &base)?;
                let at_head = self.skip_rows_at(s, head)?;
                Some(skip_hits(at_base, at_head, &paths))
            }
            false => None,
        };
        if plain {
            let by_skip = |p: &str| matches!(&skips, Some(Some(h)) if h.contains(&p));
            let mut hit: Vec<&str> = (paths.iter().copied())
                .filter(|p| protected(p) || by_skip(p))
                .collect();
            if matches!(skips, Some(None)) && !hit.contains(&SKIPS) {
                hit.push(SKIPS);
            }
            if !hit.is_empty() {
                let shown: Vec<&&str> = hit.iter().take(10).collect();
                return stop(
                    "protected_path_changed",
                    json!({"paths": shown, "count": hit.len()}),
                );
            }
        }
        let by_skip = match &skips {
            Some(Some(hit)) => !hit.is_empty(),
            Some(None) => true,
            None => false,
        };
        let guarded = by_policy.map(|g| g || by_skip);
        // R44: the lines the head adds, before the acceptance checks, the gate and the verifier.
        let notes = self.u["notes"].as_str().unwrap_or_default();
        if let Err(e) = scan::scan(s, wt, base, head, notes) {
            return stop(e.cause, e.detail);
        }
        let merge_base = self.git(s, wt, &["merge-base", base, head])?;
        self.done.checks += 1;
        let list = self.u["acceptance"].as_array().cloned().unwrap_or_default();
        for (i, c) in list.iter().enumerate().map(|(i, c)| (i + 1, c)) {
            let name = format!("{}-{}-{i}.check.out", self.prefix, self.done.checks);
            let output = self.home.join(name).to_string_lossy().into_owned();
            let check = c["check"].as_str().unwrap_or_default();
            let status = object::run_check(s, &self.wt, (&merge_base, head), &output, check);
            let status = status.map_err(|f| Halt::Stop(f.code.into(), f.detail.to_string()))?;
            let cause = match status {
                Some(0) => continue,
                None => "acceptance_not_runnable",
                Some(124 | 137) => "acceptance_timeout",
                Some(_) => "acceptance_failed",
            };
            let bytes = std::fs::read(&output).unwrap_or_default();
            let mut from = bytes.len().saturating_sub(600);
            while from < bytes.len() && (bytes[from] & 0xC0) == 0x80 {
                from += 1;
            }
            let tail = String::from_utf8_lossy(&bytes[from..]);
            return stop(cause, json!({"index": i, "status": status, "output": tail}));
        }
        self.clean_at(s, head)?;
        Ok(guarded)
    }
    /// The rows of `ci/skips.json` at `commit` in the clone (R49): none without the file, `None` when
    /// it is not of its form (`specs/ap/check.md` R23) or is not a regular file there.
    fn skip_rows_at(&self, s: &mut impl System, commit: &str) -> Step<Option<Vec<SkipRow>>> {
        let clone = self.clone.as_str();
        let listed = self.git(s, clone, &["ls-tree", "-z", commit, "--", SKIPS])?;
        if listed.is_empty() {
            return Ok(Some(vec![]));
        }
        if !listed.starts_with("100644 blob ") && !listed.starts_with("100755 blob ") {
            return Ok(None);
        }
        let shown = review::git(s, clone, &["show", &format!("{commit}:{SKIPS}")]);
        Ok(shown.ok().and_then(|text| skip_rows(&text)))
    }
    /// The required check on the hosting service, without the automatic reviewer (R17).
    fn gate(&mut self, s: &mut impl System, number: u64, head: &str) -> Step {
        let req = json!({"repository": self.repo, "pull_request": number, "head": head});
        let gated = crate::gate::execute(s, &req);
        let (a, _) = gated.map_err(|f| Halt::Stop("github_unreadable".into(), f.code.into()))?;
        let end = a["end"].as_str().unwrap_or("github_unreadable");
        if end != "passed" {
            return stop(
                end,
                json!({"head": a["head"], "failed_check": a["failed_check"]}),
            );
        }
        let threads = a["threads"].as_array();
        if !threads.is_some_and(Vec::is_empty) {
            let ids: Vec<&Value> = threads.into_iter().flatten().map(|t| &t["id"]).collect();
            return stop("unresolved_threads", json!({"threads": ids}));
        }
        Ok(())
    }

    /// This call's own fixer or update moved the head `from` to `to` (R19).
    fn moved(&mut self, from: &str, to: &str, repair: bool) {
        let own = match self.done.own.take() {
            Some(o) if o.to == from => o,
            _ => Own {
                from: from.to_owned(),
                to: String::new(),
                repair: false,
                update: false,
            },
        };
        self.done.own = Some(Own {
            to: to.to_owned(),
            repair: own.repair || repair,
            update: own.update || !repair,
            ..own
        });
    }
    /// The record of the newest verdict's session, the head it judged and what moved it, when the
    /// round on `head` resumes it (R19): only when this call's own fixer and updates moved the judged
    /// head to `head`, and no move of another since.
    fn second_look(
        &self,
        v: &Verdicts,
        head: &str,
    ) -> Option<(String, String, &'static str, bool)> {
        let since = v.list.iter().rfind(|(_, r)| v.later(r));
        let abandoned = since.is_some_and(|(_, r)| r["verdict"] == "abandoned");
        let (k, r) = v.newest()?;
        let (judged, session) = (r["head"].as_str()?, r["session"].as_str()?);
        let writers = self.u["writers"].as_array();
        let writer = writers.is_some_and(|w| w.iter().any(|w| w == session));
        let own = self
            .done
            .own
            .as_ref()
            .filter(|o| o.from == judged && o.to == head);
        let what = match own.map(|o| (o.repair, o.update)) {
            Some((true, true)) => "the author's repair and a merge of origin/main",
            Some((true, false)) => "the author's repair",
            _ => "a merge of origin/main",
        };
        let record = format!("{}.run.json", self.report(k)?);
        let fit = own.is_some() && !self.done.moved && !abandoned && !writer;
        // A session that ran on the PC lives there: a second look starts fresh (`specs/ap/unit-run.md` R45).
        let fit = fit && json_file(&record).is_some_and(|r| r["host"] != "pc");
        let repaired = own.is_some_and(|o| o.repair);
        fit.then(|| (record, judged.to_owned(), what, repaired))
    }
    /// The body of pull request `number`, read with the host's login and written to
    /// `<directory>/<key>/<n>.description.md` with every login and stage token redacted (R19): its path.
    fn description(&self, s: &mut impl System, number: u64, n: u64) -> Step<String> {
        let v = self.gh(
            s,
            &format!("pr view {number} --repo {} --json body", self.repo),
        )?;
        let Some(body) = v["body"].as_str() else {
            return stop("github_unreadable", json!({"call": "pr view"}));
        };
        self.prompt(&format!("{n}.description.md"), &scrubbed(body))
    }
    /// One validate naming the review's key, the admission of the stage, the start (R19).
    fn start_review(
        &mut self,
        s: &mut impl System,
        (call, action): (&work::Call, &actions::StartUnitReview),
        req: &mut Value,
    ) -> Step<String> {
        let checked = at(
            store::validate_action(s, self.store(), call, action),
            "start",
        )?;
        if let Some(detail) = not_valid(&checked) {
            return failed(store::REFUSED, "start", detail);
        }
        let edits = checked["result"]["edits"].as_array().into_iter().flatten();
        let mut made =
            edits.filter(|e| e["change"] == "initial" && e["id"]["object_type"] == "review");
        let key = made.next().and_then(|e| e["id"]["native_key"].as_str());
        let key = key.filter(|k| key_ok(k)).map(str::to_owned);
        let review = key.ok_or_else(|| error("store_answer_unreadable", "start"))?;
        let out = self.report(&review).unwrap_or_default();
        if Path::new(&out).symlink_metadata().is_ok() {
            return failed("request_id_reused", "start", json!(review));
        }
        req["out"] = json!(out);
        let mut dirs = std::fs::DirBuilder::new();
        let made = dirs.recursive(true).mode(0o700).create(&self.reviews);
        made.map_err(|_| Halt::Stop("io_error".into(), "the reviews directory".into()))?;
        crate::run::admit_stage(s, req).map_err(refused)?;
        // The store compares strictly: a review in the second of the amendment would not count.
        let amended = self.u["amended_at"].as_str().unwrap_or_default().to_owned();
        self.past(s, &amended)?;
        let started = at(store::apply_action(s, self.store(), call, action), "start")?;
        self.applied = true;
        let key = started
            .made("review")
            .and_then(|i| i["native_key"].as_str());
        if started.outcome != "applied" || key != Some(review.as_str()) {
            return failed("request_id_reused", "start", json!(review));
        }
        Ok(review)
    }
    /// Whether the look of a fresh session asks for the PC (`specs/ap/unit-run.md` R45): the policy
    /// has `hosts.pc.stages`, the look is a Claude one, the files it reads are each a regular file of at most 1 MiB, and
    /// `capacity` (read through `store::function`, as the role `dispatch` that `ap work` sets) names the machine of the host `pc` free while the
    /// host `local` carries a one-minute load of at least its CPUs. A read that fails, answers no
    /// object or lacks a member gives no `host`: no stop, wait or write of the store comes of it.
    fn pc_free(&self, s: &mut impl System, req: &Value, files: &[String]) -> bool {
        let stages = crate::work::policy(s)
            .ok()
            .flatten()
            .and_then(|v| crate::check::hosts(&v).ok().flatten())
            .and_then(|pc| pc.stages);
        let small =
            |p: &String| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= 1 << 20);
        let claude = req["runtime"].is_null() || req["runtime"] == "claude";
        if stages.is_none() || !claude || !files.iter().all(small) {
            return false;
        }
        let Ok(a) = store::function(s, self.store(), "capacity", &json!({})) else {
            return false;
        };
        let row = |name: &str| {
            a["hosts"]
                .as_array()?
                .iter()
                .find(|h| h["host"] == name)
                .cloned()
        };
        let free = (|| {
            let machine = row("pc")?["machine"].as_str()?.to_owned();
            let named = a["can_start"].as_array()?.iter().any(|m| *m == machine);
            let local = row("local")?;
            let (load, cpus) = (local["load_x100"].as_u64()?, local["cpus"].as_u64()?);
            Some(named && load >= cpus.checked_mul(100)?)
        })();
        free == Some(true)
    }
    /// A verifier round on `head`: its verdict and its review (R19, R20).
    fn verify(
        &mut self,
        s: &mut impl System,
        number: u64,
        head: &str,
        v: &Verdicts,
    ) -> Step<(String, String)> {
        let resumed = self.second_look(v, head);
        let n = self.next();
        // The description as the run read it with the host's login, before the round (R19).
        let description = self.description(s, number, n)?;
        let mut text = self.page(s, Role::Verifier)?;
        text.push_str(&format!("{DESCRIPTION}{description}{DESCRIPTION_TAIL}\n"));
        // The files the prompt names, read-only in the stage's view (R38).
        let mut files = vec![description];
        if let Some((_, judged, what, repaired)) = &resumed {
            let mut round = format!("Head judged before: {judged}\nWhat moved it: {what}\n");
            if *repaired {
                let answer = self.done.fixer_answer.as_deref().filter(|p| {
                    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
                });
                if let Some(a) = answer {
                    files.push(a.to_owned());
                }
                let answer = answer.unwrap_or("none");
                round.push_str(&format!("Answer of the author: {answer}\n"));
            }
            text.push_str(&format!("\n{SECOND_LOOK}\n## The round\n\n{round}"));
        }
        let prompt = self.prompt(&format!("{n}.verifier.prompt.md"), &text)?;
        let t = &self.verifier;
        // The tier is the policy's: a model or an effort that is no text ends the form before the start.
        let name = actions::StartUnitReview::NAME;
        let word = |k: &str| t[k].as_str().map(str::to_owned);
        let word = |k: &str| word(k).ok_or_else(|| error("store_answer_unreadable", name));
        let action = actions::StartUnitReview {
            base: Some(values::CommitOrDigest(self.base.clone())),
            effort: word("effort")?,
            head: values::CommitOrDigest(head.to_owned()),
            intent: None,
            kind: values::ReviewKind::Packet,
            model: word("model")?,
            prompt: values::FilePath(prompt.clone()),
            prompt_sha256: values::Sha256Hex(digest(text.as_bytes())),
            pull_request: Some(store::integer(number, name)?),
            repository: Some(self.repo.clone()),
            round: Some(store::integer(v.rounds() + 1, name)?),
            runs: None,
            task: Id::new(&self.task),
            unit: Some(Id::new(&self.key)),
        };
        let rid = self.rid();
        let actor = Actor {
            agent: "ap-unit",
            session: &self.session,
            role: "dispatch",
        };
        let call = actor.call(rid);
        let mut req = self.stage(&self.verifier, "reviewer", "verifier", (&prompt, ""));
        (req["ont_role"], req["expect"]) = (json!("verifier"), json!({"first_line": "Verdict: *"}));
        (req["limits"], req["env"]) = self.limits(s, n, false, false, &files)?;
        if let Some((record, ..)) = &resumed {
            req["resume"] = json!({"record": record});
        } else if self.pc_free(s, &req, &files) {
            req["host"] = json!("pc");
        }
        let review = self.start_review(s, (&call, &action), &mut req)?;
        let out = req["out"].as_str().unwrap_or_default().to_owned();
        let rec = match crate::run::execute(s, &req) {
            Ok((rec, _)) => rec,
            Err(f) => {
                self.close(s, &review, Closing::abandoned(None))?;
                return Err(refused(f));
            }
        };
        let session = session(&rec).map(str::to_owned);
        let abandon = || Closing::abandoned(session.clone());
        // A verifier that wrote or pushed becomes a writer before the unit stops; a read that fails
        // closes the review first (R20).
        let read = self
            .tree(s)
            .and_then(|t| self.view(s, number).map(|v| (t, v)));
        let (t, (state, now)) = match read {
            Ok(read) => read,
            Err(h) => {
                self.close(s, &review, abandon())?;
                return Err(h);
            }
        };
        if !t.clean || t.head != head || now != head || !t.on_branch || state != "OPEN" {
            self.close(s, &review, abandon())?;
            self.record_head(s, number, &now, session.as_deref(), None)?;
            let detail = json!({"review": review, "head": now, "worktree": t.shown()});
            return stop("verifier_wrote", detail);
        }
        let bytes = std::fs::read(&out).unwrap_or_default();
        let sha = digest(&bytes);
        let named = rec["answer"]["path"] == *out && rec["answer"]["sha256"] == *sha;
        let report = String::from_utf8_lossy(&bytes);
        let mut lines = report.split('\n').map(|l| l.trim_end_matches('\r'));
        let verdict = match lines.next() {
            Some("Verdict: accept") => Some("accept"),
            Some("Verdict: repair") => Some("repair"),
            _ => None,
        };
        let head_line = super::names_head(lines.next(), head);
        let Some(verdict) = verdict.filter(|_| rec["state"] == "ok" && named && head_line) else {
            self.close(s, &review, abandon())?;
            return stop(
                "verifier_failed",
                json!({"review": review, "state": rec["state"]}),
            );
        };
        let account = rec["account"]
            .as_str()
            .and_then(|a| Path::new(a).file_name()?.to_str());
        let closing = Closing {
            verdict,
            session: session.clone(),
            report: Some(out.clone()),
            sha: Some(sha),
            account: account.filter(|a| a.len() <= 40).map(str::to_owned),
        };
        if !self.close(s, &review, closing)? {
            let detail = json!({"review": review, "cause": "writer_cannot_review"});
            return stop("verifier_failed", detail);
        }
        // The report as ONE comment, with the host's login; its failure changes nothing (R43).
        let n = number.to_string();
        let words = [
            "pr",
            "comment",
            &n,
            "--repo",
            &self.repo,
            "--body-file",
            &out,
        ];
        let _ = s.call("gh", &args(&words), &Value::Null, 60_000);
        // The moves of this call are counted again from this verdict.
        self.done.own = None;
        Ok((verdict.to_owned(), review))
    }
    /// The repair step (R21).
    fn repair(&mut self, s: &mut impl System, review: &str, judged: &str, v: &Verdicts) -> Step {
        if v.repairs() >= 3 {
            return stop("repair_limit", json!({"repairs": v.repairs()}));
        }
        let report = self.report(review).unwrap_or_default();
        let authors = self.authors();
        let Some(from) = authors.last().filter(|a| session(&a.rec).is_some()) else {
            return stop("stage_refused", json!({"code": "no_author_session"}));
        };
        let page = self.page(s, Role::Implementer)?;
        let round = format!("Head judged: {judged}\nVerifier report: {report}\n");
        let text = format!("{page}\n{FIXER}\n## The round\n\n{round}");
        let (rec, out) = self.author(s, "fixer", Some(from), &text, &[report])?;
        let now = self.git(s, &self.wt, &["rev-parse", "HEAD"])?;
        if now == judged {
            return stop("repair_refused", Self::answered(&out, &rec));
        }
        self.moved(judged, &now, true);
        self.wait(s)
    }

    /// The accept of the head the run's own updates moved from, carried to `head` without a verifier
    /// (R37): the review `carry_review` made, or `None` when a condition fails or the store refuses the
    /// carry, and the head goes to a verifier round (R19).
    fn carry(&mut self, s: &mut impl System, head: &str, v: &Verdicts) -> Step<Option<String>> {
        // (a) The newest verdict is `accept`, by a review that started after the amendment.
        let Some((from, r)) = v.newest().filter(|(_, r)| r["verdict"] == "accept") else {
            return Ok(None);
        };
        let (from, started) = (from.clone(), r["started_at"].as_str().unwrap_or_default());
        let Some(judged) = r["head"].as_str().filter(|h| hex40(h) && *h != head) else {
            return Ok(None);
        };
        let (judged, started) = (judged.to_owned(), started.to_owned());
        // (b) Only the run's own merges from the accepted head to this one; (d) each without a conflict.
        let Some(bases) = self.updates_since(s, head, &judged) else {
            return Ok(None);
        };
        let (wt, base) = (self.wt.clone(), self.base.clone());
        let before = review::git(s, &wt, &["merge-base", &judged, &base]).ok();
        let Some(before) = before.map(|b| b.trim_end().to_owned()) else {
            return Ok(None);
        };
        let same = self.patch(s, &before, &judged);
        if same.is_none() || same != self.patch(s, &base, head) {
            return Ok(None);
        }
        // (c) No path the other side changed meets the write set.
        let first = review::git(s, &wt, &["merge-base", &judged, &bases[0]]).ok();
        let Some(first) = first.map(|b| b.trim_end().to_owned()) else {
            return Ok(None);
        };
        let mut words = vec!["diff"];
        words.extend(
            DIFF_OPTIONS
                .into_iter()
                .chain([first.as_str(), base.as_str()]),
        );
        let Ok(diff) = review::git(s, &wt, &words) else {
            return Ok(None);
        };
        let theirs: Vec<&str> = diff.split_terminator('\0').collect();
        let set = self.u["write_set"].as_array().into_iter().flatten();
        let set: Vec<String> = set.filter_map(|p| p.as_str().map(str::to_owned)).collect();
        if review::outside(&set, &theirs).len() != theirs.len() {
            return Ok(None);
        }
        // The store compares by the second: the carried review starts after the one it carries.
        self.past(s, &started)?;
        let action = actions::CarryReview {
            head: values::CommitOrDigest(head.to_owned()),
            review: Id::new(&from),
            unit: Id::new(&self.key),
        };
        let applied = match self.apply(s, &action) {
            Ok(a) => a,
            Err(f) if f.code == store::REFUSED => return Ok(None),
            Err(f) => return Err(f.into()),
        };
        let made = applied
            .made("review")
            .and_then(|i| i["native_key"].as_str());
        let made = made.filter(|k| key_ok(k)).map(str::to_owned);
        let review =
            made.ok_or_else(|| error("store_answer_unreadable", actions::CarryReview::NAME))?;
        let mut text = format!(
            "# The accept of {judged} carried to {head}\n\nUnit: {key}\nAccepted head: {judged}\nIts review: {from}\nHead now: {head}\nCarried review: {review}\n\nUpdates, the base each merged, oldest first:\n",
            key = self.key
        );
        for b in &bases {
            text.push_str(&format!("- {b}\n"));
        }
        text.push_str(&format!(
            "\nPaths the other side changed (git diff --name-only {first} {base}):\n"
        ));
        for path in &theirs {
            text.push_str(&format!("- {path}\n"));
        }
        text.push_str(&format!(
            "\nNone of these paths meets the unit's write set.\nEvery update merged without a conflict.\nThe unit's patch is unchanged: git diff {before} {judged} and git diff {base} {head} are byte for byte the same.\nR16 on {head}: passed (the write set, the protected paths, the acceptance checks, the worktree clean).\nR17 on {head}: passed (the required check, no unresolved review thread).\n"
        ));
        let file = self.prompt(&format!("{}.update.md", self.next()), &text)?;
        self.done.carried = Some(json!({"review": review, "from": from, "file": file}));
        Ok(Some(review))
    }
    /// The bases the run's own merges took on the first-parent chain from `head` back to `judged`,
    /// oldest first: each a merge with the message of R26, the previous head as its first parent and as
    /// its second an ancestor of the present base (R26 to R28, R37). `None` when another commit is on it.
    fn updates_since(&self, s: &mut impl System, head: &str, judged: &str) -> Option<Vec<String>> {
        let mut bases = vec![];
        let mut at = head.to_owned();
        while at != judged {
            let parents = review::git(s, &self.wt, &["rev-list", "--parents", "-n", "1", &at]);
            let parents = parents.ok()?;
            let [_, first, second] = parents.split_whitespace().collect::<Vec<_>>()[..] else {
                return None;
            };
            let message = review::git(s, &self.wt, &["log", "-1", "--format=%B", &at]).ok()?;
            let ancestor = ["merge-base", "--is-ancestor", second, &self.base];
            if message.trim_end() != self.merge_message() || !self.git_ok(s, &self.wt, &ancestor) {
                return None;
            }
            bases.push(second.to_owned());
            at = first.to_owned();
            if bases.len() > 100 {
                return None;
            }
        }
        bases.reverse();
        (!bases.is_empty()).then_some(bases)
    }
    /// The bytes of `git diff <from> <to>`, every option that could vary it fixed (R37).
    fn patch(&self, s: &mut impl System, from: &str, to: &str) -> Option<Vec<u8>> {
        let words = [
            "--no-replace-objects",
            "-C",
            &self.wt,
            GIT_SAFE[0],
            GIT_SAFE[1],
            GIT_SAFE[2],
            GIT_SAFE[3],
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--binary",
            "--full-index",
            from,
            to,
        ];
        let out = s.call("git", &args(&words), &Value::Null, 120_000).ok()?;
        out.success.then_some(out.stdout)
    }
    /// One second passes when the store's clock reads `floor` or earlier: the store writes and compares
    /// times by the second (R19, R37).
    fn past(&mut self, s: &mut impl System, floor: &str) -> Step {
        let read = at(
            store::head_now_of::<objects::Unit>(s, self.store(), &self.key),
            "unit",
        )?;
        let now = read.and_then(|(_, _, n)| n);
        let read_at = now.map(|n| crate::run::stamp(UNIX_EPOCH + Duration::from_secs(n)));
        if read_at.is_none_or(|t| t.as_str() <= floor) {
            s.pause(Duration::from_secs(1));
        }
        Ok(())
    }

    /// One base commit: `refs/remotes/origin/main` after a fetch (R8, R30).
    fn take_base(&mut self, s: &mut impl System) -> Step {
        self.fetch(s, "main")?;
        let main = ["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"];
        self.base = self.git(s, &self.clone, &main)?;
        if !hex40(&self.base) {
            return stop("fetch_failed", "refs/remotes/origin/main");
        }
        Ok(())
    }
    fn contains_base(&self, s: &mut impl System, head: &str) -> bool {
        let words = ["merge-base", "--is-ancestor", &self.base, head];
        self.git_ok(s, &self.clone, &words)
    }
    /// The loop: each pass reads, then takes the first step that applies (R8 to R21, R26 to R30).
    fn drive(&mut self, s: &mut impl System) -> Step<Outcome> {
        loop {
            if self.u["state"] == "accepted" {
                match self.land(s)? {
                    Some(out) => return Ok(out),
                    None => continue,
                }
            }
            self.take_base(s)?;
            let pr = self.read(s)?;
            if Path::new(&self.wt).symlink_metadata().is_err() {
                self.make_worktree(s)?;
                continue;
            }
            let t = self.tree(s)?;
            let Some((number, head)) = pr else {
                self.implement(s, &t)?;
                continue;
            };
            let pending = self.own_merge(s, &t, &head);
            if pending.is_none() && (t.head != head || !t.clean || !t.on_branch) {
                let forward = t.clean && t.on_branch && {
                    self.fetch(s, &format!("refs/heads/{}", self.branch))?;
                    self.git_ok(
                        s,
                        &self.wt,
                        &["merge-base", "--is-ancestor", &t.head, &head],
                    )
                };
                if forward {
                    self.no_links(&t.dirs.0, Some(&t.dirs.1))?;
                    self.git(s, &self.wt, &["merge", "--ff-only", "--quiet", &head])?;
                    self.done.moved = true;
                } else {
                    self.settle(s, &t)?;
                }
                continue;
            }
            if let Some(a) = self.amended_turn().filter(|_| pending.is_none()) {
                self.done.turned = true;
                self.turn(s, &a)?;
                continue;
            }
            if self.u["head"] != *head || self.u["pull_request"] != number {
                self.record_head(s, number, &head, None, None)?;
            }
            // A head judged `repair` goes to its author before any update (R29).
            let v = self.verdicts(s)?;
            if let Some((k, r)) = v.of(&head).filter(|_| pending.is_none())
                && r["verdict"] == "repair"
            {
                let k = k.clone();
                self.repair(s, &k, &head, &v)?;
                continue;
            }
            if !self.contains_base(s, &head) {
                self.update(s, number, &head, pending, &t)?;
                continue;
            }
            // R46: the head's checks passed; with `units.approval`, its `guarded` is recorded.
            if let Some(guarded) = self.checks(s, &head)? {
                self.record_head(s, number, &head, None, Some(guarded))?;
                self.done.guarded = Some((head.clone(), guarded));
            }
            self.gate(s, number, &head)?;
            let v = self.verdicts(s)?;
            let (verdict, review) = match v.of(&head) {
                Some((k, r)) => (
                    r["verdict"].as_str().unwrap_or_default().to_owned(),
                    k.clone(),
                ),
                None => match self.carry(s, &head, &v)? {
                    Some(review) => ("accept".to_owned(), review),
                    None => self.verify(s, number, &head, &v)?,
                },
            };
            if verdict == "accept" {
                let accepted = actions::AcceptUnit {
                    head: values::CommitOrDigest(head.to_string()),
                    unit: Id::new(&self.key),
                };
                self.apply(s, &accepted)?;
                self.reread(s)?;
                // A `plain` unit goes on to its merge in this call (R18, R30), and so does a
                // `contract` unit when the policy's verifier approves it (R47).
                if self.u["class"] != "plain" && !self.by_verifier() {
                    return Ok(Outcome::default());
                }
                continue;
            }
            let v = self.verdicts(s)?;
            self.repair(s, &review, &head, &v)?;
        }
    }

    /// The message of the update's own merges (R26, R27).
    fn merge_message(&self) -> String {
        format!("Merge origin/main into {} (ap unit run)", self.branch)
    }
    /// A clean worktree on the branch whose HEAD is the update's own merge of `head` with the base or
    /// an ancestor of it, made by a call cut before its push (R27).
    fn own_merge(&self, s: &mut impl System, t: &Tree, head: &str) -> Option<String> {
        if !t.clean || !t.on_branch || t.head == head {
            return None;
        }
        let parents = review::git(s, &self.wt, &["rev-list", "--parents", "-n", "1", &t.head]);
        let parents = parents.ok()?;
        let [_, first, second] = parents.split_whitespace().collect::<Vec<_>>()[..] else {
            return None;
        };
        let message = review::git(s, &self.wt, &["log", "-1", "--format=%B", &t.head]).ok()?;
        let ours = first == head && message.trim_end() == self.merge_message();
        let ancestor = ["merge-base", "--is-ancestor", second, &self.base];
        (ours && self.git_ok(s, &self.wt, &ancestor)).then(|| t.head.clone())
    }
    /// The update of a head behind the base: the base merged into the branch in the worktree (or the
    /// run's own unpushed merge taken), the writers recorded, the push, the wait (R26 to R28).
    fn update(
        &mut self,
        s: &mut impl System,
        number: u64,
        head: &str,
        pending: Option<String>,
        t: &Tree,
    ) -> Step {
        if self.done.updates >= UPDATES {
            return stop("update_limit", json!({"updates": self.done.updates}));
        }
        let merged = match pending {
            Some(m) => m,
            None => self.merge_base(s, t)?,
        };
        let word = format!("{}:update", self.session);
        let writers = self.u["writers"].as_array();
        let known = writers.is_some_and(|w| w.iter().any(|w| *w == *word));
        if self.u["head"] != *head || !known {
            self.record_head(s, number, head, Some(&word), None)?;
        }
        let to = format!("HEAD:refs/heads/{}", self.branch);
        let push = [
            "--no-replace-objects",
            "-C",
            &self.wt,
            GIT_SAFE[0],
            GIT_SAFE[1],
            GIT_SAFE[2],
            GIT_SAFE[3],
            "push",
            "--quiet",
            "origin",
            &to,
        ];
        let pushed = s.call("git", &args(&push), &Value::Null, 120_000);
        if !pushed.is_ok_and(|r| r.success) {
            return stop("push_failed", json!({"head": merged}));
        }
        self.done.updates += 1;
        self.moved(head, &merged, false);
        self.wait(s)
    }
    /// The base merged into the worktree's HEAD by the run's own author; a conflict is aborted (R26).
    fn merge_base(&self, s: &mut impl System, t: &Tree) -> Step<String> {
        self.no_links(&t.dirs.0, Some(&t.dirs.1))?;
        let message = self.merge_message();
        let mut words = MERGER.to_vec();
        words.extend(["git", "--no-replace-objects", "-C", &self.wt]);
        words.extend(GIT_SAFE);
        words.extend(["merge", "--no-ff"]);
        words.extend(["--no-edit", "--quiet", "-m", &message, &self.base]);
        let merged = s.call("env", &args(&words), &Value::Null, 120_000);
        if merged.is_ok_and(|r| r.success) {
            return self.git(s, &self.wt, &["rev-parse", "HEAD"]);
        }
        let paths = [
            "diff",
            "--ignore-submodules=all",
            "--name-only",
            "--diff-filter=U",
        ];
        let paths = review::git(s, &self.wt, &paths).unwrap_or_default();
        self.git(s, &self.wt, &["merge", "--abort"])?;
        let paths: Vec<&str> = paths.lines().take(10).collect();
        stop("merge_conflict", json!({"base": self.base, "paths": paths}))
    }

    /// An accepted unit: the pull request read against a new base, then the merge, the reopen or the
    /// wait for the Lead's approval (R30). `None` when the unit runs again.
    fn land(&mut self, s: &mut impl System) -> Step<Option<Outcome>> {
        self.take_base(s)?;
        let head = self.u["head"].as_str().unwrap_or_default().to_owned();
        let Some(n) = self.u["pull_request"].as_u64() else {
            return stop("pull_request_not_open", json!({"pull_request": null}));
        };
        let p = self.pull(s, n)?;
        let (state, now) = (p["state"].as_str().unwrap_or_default(), &p["headRefOid"]);
        // Only a pull request against `main` is merged or recorded merged; one moved off it is not.
        if p["baseRefName"] != "main" {
            let detail = json!({"pull_request": n, "state": state, "base": p["baseRefName"]});
            return stop("pull_request_not_open", detail);
        }
        match state {
            "MERGED" if *now == *head => {
                let commit = p["mergeCommit"]["oid"].as_str().filter(|c| hex40(c));
                let Some(commit) = commit else {
                    return stop("github_unreadable", "mergeCommit");
                };
                let rid = self.rid();
                let action = self.merge_action(&head, commit)?;
                self.record_merge(s, rid, action).map(Some)
            }
            "OPEN" if *now != *head => self.reopen(s, "head_moved").map(|()| None),
            "OPEN" if !self.contains_base(s, &head) => self.reopen(s, "main_moved").map(|()| None),
            "OPEN" if self.u["class"] == "plain" || self.u["approved_head"] == *head => {
                self.merged(s, n, &head).map(Some)
            }
            "OPEN" if self.unguarded(&head) => match self.approve_by_policy(s, &head)? {
                true => self.merged(s, n, &head).map(Some),
                false => Ok(Some(Outcome::default())),
            },
            "OPEN" => Ok(Some(Outcome::default())),
            _ => stop(
                "pull_request_not_open",
                json!({"pull_request": n, "state": state, "head": now}),
            ),
        }
    }
    /// A `contract` unit whose accepted head the policy's verifier approves (R47).
    fn by_verifier(&self) -> bool {
        self.u["class"] == "contract" && self.approval.as_ref().is_some_and(|a| a.by_verifier)
    }
    /// This call ran the checks of R16 on `head` and recorded `guarded` false for it (R46, R47): a
    /// stored `guarded` another call recorded never approves.
    fn unguarded(&self, head: &str) -> bool {
        self.by_verifier()
            && self
                .done
                .guarded
                .as_ref()
                .is_some_and(|(h, g)| h == head && !*g)
    }
    /// `approve_unit_by_policy` on the head: true when applied, false when the store refuses it, and
    /// the unit then waits for the Lead's approve (R47).
    fn approve_by_policy(&mut self, s: &mut impl System, head: &str) -> Step<bool> {
        let action = actions::ApproveUnitByPolicy {
            head: values::CommitOrDigest(head.to_owned()),
            unit: Id::new(&self.key),
        };
        match self.apply(s, &action) {
            Err(f) if f.code == store::REFUSED => Ok(false),
            other => {
                other?;
                self.reread(s)?;
                Ok(true)
            }
        }
    }
    /// The merge of R31, then, for a unit the policy approved, the sample of R48.
    fn merged(&mut self, s: &mut impl System, n: u64, head: &str) -> Step<Outcome> {
        let out = self.merge(s, n, head)?;
        if out.cleanup.is_some() && self.u["approved_by"] == "policy" {
            self.sample(s, n);
        }
        Ok(out)
    }
    /// R48: after a merge the policy approved, the units so approved that are `merged` or `done`
    /// are counted; every `sample`-th opens a finding for the Lead's reading. Nothing of it changes
    /// the call's answer or exit.
    fn sample(&mut self, s: &mut impl System, n: u64) {
        let Some(every) = self.approval.as_ref().map(|a| a.sample) else {
            return;
        };
        let all = vec![
            json!({"eq": ["approved_by", "policy"]}),
            json!({"in": ["state", ["merged", "done"]]}),
        ];
        let Ok(found) = store::search_of::<objects::Unit>(s, self.store(), all) else {
            return;
        };
        let count = found.len() as u64;
        if count == 0 || !count.is_multiple_of(every) {
            return;
        }
        let action = actions::OpenFinding {
            detector: "ap-unit".to_owned(),
            evidence: None,
            finding: Id::new(format!("policy-sample-{}", self.key)),
            kind: values::FindingKind::Note,
            severity: values::FindingSeverity::Act,
            subject: store::subject_of("unit", &self.key),
            summary: format!(
                "sample: read the merged diff of {}, pull request {n}",
                self.key
            ),
        };
        let _ = self.apply(s, &action);
    }
    /// `reopen_unit` with `cause`; the store's refusal ends the call with the unit `accepted` (R30).
    fn reopen(&mut self, s: &mut impl System, cause: &str) -> Step {
        let action = actions::ReopenUnit {
            cause: cause.to_owned(),
            intent: self.intent(actions::ReopenUnit::NAME)?,
            unit: Id::new(&self.key),
        };
        match self.apply(s, &action) {
            Err(f) if f.code == store::REFUSED => stop(&store::code(&f), f.detail),
            other => {
                other?;
                Ok(self.reread(s)?)
            }
        }
    }
    fn merge_action(&self, head: &str, commit: &str) -> Result<actions::MergeUnit, Failure> {
        Ok(actions::MergeUnit {
            head: values::CommitOrDigest(head.to_owned()),
            intent: self.intent(actions::MergeUnit::NAME)?,
            merged_commit: values::CommitOrDigest(commit.to_owned()),
            unit: Id::new(&self.key),
        })
    }
    /// The merge: `merge_unit` validated, the status `lead-review`, the merge with the head pinned,
    /// read back, then `merge_unit` applied with the merge commit (R31).
    fn merge(&mut self, s: &mut impl System, n: u64, head: &str) -> Step<Outcome> {
        let rid = self.rid();
        let action = self.merge_action(head, &"0".repeat(40))?;
        let actor = Actor {
            agent: "ap-unit",
            session: &self.session,
            role: "dispatch",
        };
        let call = actor.call(rid.clone());
        let validated = store::validate_action(s, self.store(), &call, &action);
        let checked = at(validated, actions::MergeUnit::NAME)?;
        if let Some(detail) = not_valid(&checked) {
            let code = detail["failed"][0]
                .as_str()
                .unwrap_or(store::REFUSED)
                .to_owned();
            return stop(&code, detail);
        }
        let v = self.verdicts(s)?;
        let review = v.of(head).map(|(k, _)| k.clone()).unwrap_or_default();
        let target = json!({"repository": self.repo, "head": head});
        let posted = review::post(s, &target, "success", &format!("review {review} accept"));
        if let Err(f) = posted {
            let reply = f.detail.as_str().unwrap_or_default().to_owned();
            return said("status_failed", json!({"head": head}), reply);
        }
        let words = format!(
            "pr merge {n} --repo {} --merge --match-head-commit {head}",
            self.repo
        );
        let mut called = merge_call(s, &words);
        // The read decides, also after a call that failed: it may have merged.
        let mut read = self.pull(s, n).ok();
        // A merge refused for a status GitHub had not yet taken: once more, when the head's
        // `lead-review` reads `success`, after a pause (R31).
        let open = read.as_ref().is_some_and(|p| p["state"] != "MERGED");
        if called.is_err()
            && open
            && review::lead_review(s, &self.repo, head).as_deref() == Some("success")
        {
            s.pause(Duration::from_secs(5));
            called = merge_call(s, &words);
            read = self.pull(s, n).ok();
        }
        let merged = read.filter(|p| {
            p["state"] == "MERGED" && p["headRefOid"] == head && p["baseRefName"] == "main"
        });
        let commit = merged.and_then(|p| p["mergeCommit"]["oid"].as_str().map(str::to_owned));
        let Some(commit) = commit.filter(|c| hex40(c)) else {
            let Err(reply) = called else {
                return stop("merge_unconfirmed", json!({"head": head}));
            };
            let _ = review::post(s, &target, "failure", "merge failed");
            return said("merge_failed", json!({"head": head}), reply);
        };
        let action = self.merge_action(head, &commit)?;
        self.record_merge(s, rid, action)
    }
    /// `merge_unit` applied, then the cleanup; the store's refusal leaves the unit as it reads (R30,
    /// R31, R35).
    fn record_merge(
        &mut self,
        s: &mut impl System,
        rid: String,
        action: actions::MergeUnit,
    ) -> Step<Outcome> {
        let mut out = Outcome {
            merged_commit: Some(action.merged_commit.0.clone()),
            ..Outcome::default()
        };
        match self.apply_as(s, rid, &action) {
            Err(f) if f.code == store::REFUSED => out.refused = Some(store::code(&f)),
            Err(f) => return Err(f.into()),
            Ok(_) => {
                self.reread(s)?;
                out.cleanup = Some(self.cleanup(s));
            }
        }
        self.reread(s)?;
        Ok(out)
    }
    /// The cleanup of a merged or dropped unit: the one worktree, the one branch, and for a dropped
    /// unit its pull request named when it is open (R32).
    fn cleanup(&mut self, s: &mut impl System) -> Value {
        let dropped = self.u["state"] == "dropped";
        let mut c = json!({});
        let wt = Path::new(&self.wt);
        let worktree: Result<&str, String> = if wt.symlink_metadata().is_err() {
            Ok("nothing")
        } else if !real_dir(&self.home) || !real_dir(wt) {
            Err("not a directory".to_owned())
        } else {
            match self.entry(s) {
                // A worktree that does not name its own Git directory is not removed (R8).
                Ok(Some(_)) => match self.git_dirs(s) {
                    Err(_) => Err("git directory".to_owned()),
                    // Nor is one whose Git directory holds a link or cannot be read (R40).
                    Ok((g, admin)) => match read_links(&g, Some(&admin), &self.branch) {
                        Links::Found(links) => {
                            Err(format!("link in the git directory: {}", shown(&links[0])))
                        }
                        Links::Unreadable(at) => {
                            Err(format!("git directory unreadable: {}", shown(&at)))
                        }
                        Links::Clear => {
                            let mut words = vec!["worktree", "remove"];
                            words.extend(dropped.then_some("--force"));
                            words.push(&self.wt);
                            match self.git_ok(s, &self.clone, &words) {
                                true => Ok("removed"),
                                false => {
                                    Err("git worktree remove refused: the worktree is not clean"
                                        .to_owned())
                                }
                            }
                        }
                    },
                },
                Ok(None) => Err("no worktree of the clone at this path".to_owned()),
                Err(_) => Err("git worktree list failed".to_owned()),
            }
        };
        match worktree {
            Ok(w) => c["worktree"] = json!(w),
            Err(why) => {
                (c["worktree"], c["worktree_reason"]) = (json!("worktree_kept"), json!(why))
            }
        }
        let made = !self.u["pull_request"].is_null() || !self.authors().is_empty();
        let branch = if made {
            self.drop_branch(s)
        } else {
            Ok("nothing")
        };
        match branch {
            Ok(b) => c["branch"] = json!(b),
            Err(why) => (c["branch"], c["branch_reason"]) = (json!("kept"), json!(why)),
        }
        if let Some(n) = self.u["pull_request"].as_u64().filter(|_| dropped)
            && self.view(s, n).is_ok_and(|(state, _)| state == "OPEN")
        {
            c["pull_request_open"] = json!(n);
        }
        // The stages' scratch directories and the remote-tracking refs of their views: only a
        // directory that is not a link, and no link inside it followed (R32).
        for d in ["scratch", "remotes"].map(|d| self.home.join(d)) {
            if real_dir(&d) {
                let _ = std::fs::remove_dir_all(&d);
            }
        }
        c
    }
    /// The unit's branch deleted only when a fetch now shows its tip in `main` (R32).
    fn drop_branch(&mut self, s: &mut impl System) -> Result<&'static str, &'static str> {
        let (clone, branch) = (self.clone.clone(), self.branch.clone());
        let reference = format!("refs/heads/{branch}");
        let tip = review::git(s, &clone, &["rev-parse", "--verify", "--quiet", &reference]);
        let Ok(tip) = tip.map(|t| t.trim().to_owned()) else {
            return Ok("nothing");
        };
        if self.fetch(s, "main").is_err() {
            return Err("fetch_failed");
        }
        let merged = [
            "merge-base",
            "--is-ancestor",
            &tip,
            "refs/remotes/origin/main",
        ];
        if !self.git_ok(s, &clone, &merged) {
            return Err("not merged into main");
        }
        let deleted = self.git_ok(s, &clone, &["branch", "-d", &branch])
            || self.git_ok(s, &clone, &["branch", "-D", &branch]);
        deleted
            .then_some("removed")
            .ok_or("git branch -D refused: the branch is checked out")
    }

    /// The answer (R25).
    fn answer(
        &mut self,
        s: &mut impl System,
        stopped: Option<(&str, &str)>,
        out: &Outcome,
    ) -> Result<(Value, u8), Failure> {
        self.reread(s)?;
        let v = match self.verdicts(s) {
            Ok(v) => v,
            Err(Halt::Fail(f)) => return Err(f),
            Err(Halt::Stop(..) | Halt::Said(..)) => {
                return Err(error("store_answer_unreadable", "reviews"));
            }
        };
        let one = |(k, r): &(String, Value)| json!({"key": k, "round": r["round"], "head": r["head"], "verdict": r["verdict"]});
        let u = &self.u;
        let merged = out
            .merged_commit
            .as_ref()
            .map_or(u["merged_commit"].clone(), |c| json!(c));
        let mut a = json!({"unit": self.key, "state": u["state"], "head": u["head"],
            "pull_request": u["pull_request"], "rounds": v.rounds(), "repairs": v.repairs(),
            "reviews": v.list.iter().map(one).collect::<Vec<_>>(), "updates": self.done.updates,
            "merged_commit": merged});
        if let Some(c) = &self.done.carried {
            a["carried"] = c.clone();
        }
        if !self.done.credentials.is_empty() {
            a["credentials"] = json!(self.done.credentials);
        }
        if let Some(c) = &out.cleanup {
            a["cleanup"] = c.clone();
        }
        if let Some((cause, detail)) = stopped {
            (a["stop_cause"], a["stop_detail"]) = (json!(cause), json!(detail));
            return Ok((a, 3));
        }
        if let Some(code) = &out.refused {
            a["refused"] = json!(code);
            if let Some(reply) = &out.detail {
                a["detail"] = json!(reply);
            }
            return Ok((a, 3));
        }
        if u["state"] == "accepted" {
            let approve = u["class"] == "contract" || u["class"] == "effect";
            a["waits_for"] = json!(if approve { "approve" } else { "merge" });
        }
        Ok((a, 0))
    }
}

/// The arguments after `approve` or `reject`: none, or `--store <absolute path>` (R33).
fn store_argument(argv: &[OsString]) -> Result<Option<String>, Failure> {
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    match words.as_deref() {
        Some([]) => Ok(None),
        Some(["--store", p]) if p.starts_with('/') => Ok(Some((*p).to_owned())),
        _ => Err(error("usage", "args")),
    }
}
/// `ap unit approve|reject [--store <path>] < request.json`: the answer, or the refusal on standard
/// error with `after` and `unit`, exit 2 (R33, R34).
pub fn decide_cli(op: &str, argv: &[OsString]) -> ExitCode {
    let mut after = None;
    let mut unit = None;
    let result = store_argument(argv).and_then(|store| {
        let r = object::request()?;
        unit = r["unit"].as_str().map(str::to_owned);
        decide(&mut Host, store.as_deref(), op, &r, &mut after)
    });
    let (written, status) = match result {
        Ok(v) => (writeln!(std::io::stdout(), "{v}"), 0),
        Err(f) => {
            let code = store::code(&f);
            let v = json!({"error": {"code": code, "field": f.field, "detail": f.detail,
                "after": after, "unit": unit}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// One `approve` or `reject` (R33 to R35): every check before the lock, the lock, the Lead read, the
/// applies. `after` is the suffix of the last action applied.
pub fn decide(
    s: &mut impl System,
    store: Option<&str>,
    op: &str,
    r: &Value,
    after: &mut Option<String>,
) -> Result<Value, Failure> {
    let approve = op == "approve";
    let members = match approve {
        true => "request_id agent session unit head reason",
        false => "request_id agent session unit reason",
    };
    if !r.is_object() {
        return Err(store::bad("stdin"));
    }
    store::only(r, members, "")?;
    let rid = store::shaped(r, "request_id", 44, "._-")?;
    let actor = Actor {
        agent: store::shaped(r, "agent", 64, "._:-")?,
        session: store::shaped(r, "session", 64, "._:-")?,
        role: "lead",
    };
    let key = r["unit"].as_str().filter(|k| key_form(k));
    let key = key.ok_or_else(|| store::bad("unit"))?;
    let limit = if approve { 600 } else { DETAIL };
    let reason = r["reason"].as_str().ok_or_else(|| store::bad("reason"))?;
    if reason.len() > limit {
        let detail = json!({"bytes": reason.len(), "limit": limit});
        return Err(fail("invalid_input", "reason", detail));
    }
    let head = r["head"].as_str().filter(|h| hex40(h));
    if approve && head.is_none() {
        return Err(store::bad("head"));
    }
    // `reject` sends no role: the engine would give it the role of the environment.
    if !approve && object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    let (directory, _) = units(&policy_of(s))?;
    let (_, _lock) = locked(&directory, key)?;
    let u = at(store::get_of::<objects::Unit>(s, store, key), "unit")?;
    let u = u.ok_or_else(|| error("unknown_unit", "unit"))?;
    let intent = u["intent"]["native_key"].as_str();
    let intent = intent.ok_or_else(|| error("no_intent", "unit"))?;
    let read = at(
        store::head_of::<objects::Intent>(s, store, intent),
        "intent",
    )?;
    let (revision, i) = read.ok_or_else(|| error("unknown_intent", "intent"))?;
    if i["lead_session"].as_str() != Some(actor.session) {
        return Err(fail("not_the_lead", "session", i["lead_session"].clone()));
    }
    if approve {
        let action = actions::ApproveUnit {
            head: values::CommitOrDigest(head.unwrap_or_default().to_owned()),
            reason: reason.to_owned(),
            unit: Id::new(key),
        };
        // The Lead's apply carries the intent's revision as `expected`.
        let call = work::Call {
            expected: vec![(u["intent"].clone(), revision)],
            ..actor.call(format!("{rid}-approve"))
        };
        let applied = at(store::apply_action(s, store, &call, &action), "approve")?;
        *after = Some("approve".into());
        let back = at(store::get_of::<objects::Unit>(s, store, key), "unit")?.unwrap_or_default();
        return Ok(
            json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
            "approved_head": back["approved_head"]}),
        );
    }
    // No action clears an approval: an approved head is not rejected (R34).
    if u["approved_head"].is_string() && u["approved_head"] == u["head"] {
        return Err(fail("approved", "unit", u["head"].clone()));
    }
    let intent = match u["lane"] == "objective" {
        true => store::id_of(&u["intent"], actions::ReopenUnit::NAME)?,
        false => None,
    };
    let reopen = actions::ReopenUnit {
        cause: "rejected by the Lead".to_owned(),
        intent,
        unit: Id::new(key),
    };
    let reopened = store::apply_action(s, store, &actor.call(format!("{rid}-reopen")), &reopen);
    at(reopened, "reopen")?;
    *after = Some("reopen".into());
    let stop = actions::StopUnit {
        cause: "lead_rejected".to_owned(),
        detail: Some(reason.to_owned()),
        unit: Id::new(key),
    };
    let stopped = store::apply_action(s, store, &actor.call(format!("{rid}-stop")), &stop);
    let applied = at(stopped, "stop")?;
    *after = Some("stop".into());
    let mut posted = Value::Null;
    if let (Some(_), Some(head), Some(repo)) = (
        u["pull_request"].as_u64(),
        u["head"].as_str(),
        u["repository"].as_str(),
    ) {
        let target = json!({"repository": repo, "head": head});
        let sent = review::post(s, &target, "failure", "rejected by the Lead");
        posted = json!(if sent.is_ok() {
            "failure"
        } else {
            "not_posted"
        });
    }
    let back = at(store::get_of::<objects::Unit>(s, store, key), "unit")?.unwrap_or_default();
    Ok(
        json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
        "lead_review": posted}),
    )
}

#[cfg(test)]
mod tests {
    use super::{PROTECTED_CONFIGS, PROTECTED_EXACT, PROTECTED_NAMES, PROTECTED_TREES, protected};

    /// Every protected path of R36, and the neighbours that are not (`specs/ap/unit-run.md` R36).
    #[test]
    fn r36_the_protected_paths_are_pinned() {
        // The lists themselves, so that a path added or taken away fails here without this change.
        assert_eq!(PROTECTED_EXACT, ["check", "review.md"]);
        assert_eq!(
            PROTECTED_NAMES,
            [
                "clippy.toml",
                ".clippy.toml",
                "rustfmt.toml",
                ".rustfmt.toml",
                "rust-toolchain",
                "rust-toolchain.toml",
                "build.rs",
                "cargo.toml",
                "cargo.lock",
            ]
        );
        assert_eq!(PROTECTED_CONFIGS, [".cargo/config.toml", ".cargo/config"]);
        assert_eq!(
            PROTECTED_TREES,
            [
                "ci", ".github", "specs", "scripts", ".cargo", "deploy", "etc", "content",
                "github", "ontology", "engine", "host",
            ]
        );
        let held = [
            "host/etc/agent-platform/policy.json",
            "Host/scripts/install-etc.sh",
            "host",
            "content/SHA256SUMS",
            "Content/AGENTS.md",
            "content/skills/lead/SKILL.md",
            "github/apply",
            "github/ruleset-main.json",
            "ontology",
            "ontology/migrate",
            "ontology/json.py",
            "ontology/work.schema.json",
            "Ontology/README.md",
            "build.rs",
            "crates/ap/build.rs",
            "crates/x/Build.rs",
            "Cargo.toml",
            "crates/ap/Cargo.toml",
            "crates/x/CARGO.TOML",
            "Cargo.lock",
            "sub/Cargo.lock",
            "check",
            "REVIEW.md",
            "review.md",
            "rust-toolchain",
            "rust-toolchain.toml",
            "crates/x/rust-toolchain.toml",
            "sub/rust-toolchain",
            "ontology/rehearsal.sh",
            "ontology/notes.md",
            "clippy.toml",
            ".clippy.toml",
            "crates/x/clippy.toml",
            "rustfmt.toml",
            ".rustfmt.toml",
            "crates/x/.rustfmt.toml",
            "ci",
            "ci/x",
            "CI/x",
            ".github/workflows/rust.yml",
            ".GitHub/workflows/rust.yml",
            "specs/ap/unit.md",
            "scripts/",
            "scripts/install-ap.sh",
            ".cargo",
            ".cargo/config.toml",
            "crates/x/.cargo/config.toml",
            "crates/x/.cargo/config",
            "crates/x/.Cargo/Config.toml",
            "deploy",
            "deploy/ap-deliver.timer",
            "Deploy/x",
            "etc",
            "etc/agent-platform/policy.json",
        ];
        for path in held {
            assert!(protected(path), "{path}");
        }
        let free = [
            "checks/x",
            "docs/specs/x",
            "crates/ci/x",
            "crates/x/deploy/x",
            "src/etc/x",
            "REVIEW.md.bak",
            "crates/x/cargo/config.toml",
            "crates/x/.cargo/other.toml",
            "rust-toolchain.txt",
            "deployment/x",
            "etcetera",
            "contents/x",
            "github.md",
            "githubs/x",
            "ontologies/x",
            "hosts/x",
            "crates/x/host/x",
            "docs/content/x",
            "crates/ap/src/main.rs",
            "crates/x/build.rs.bak",
            "crates/x/cargo.lock.txt",
            "crates/x/mybuild.rs",
        ];
        for path in free {
            assert!(!protected(path), "{path}");
        }
    }
}
