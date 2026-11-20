//! `ap unit add|amend|ready|drop|page`: a unit's page as an object of the store, made ready only after
//! its acceptance checks ended on the base as their kinds require (`specs/ap/unit.md`).
use super::render::{self, Role};
use crate::host::{Failure, Host, LIMIT, System, error, fail, id, new_session_id};
use crate::store::{self, Actor, at, bad, only, shaped, sized};
use serde_json::{Map, Value, json};
use std::{
    ffi::OsString,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
};

/// The first arguments that select this module; a page's path is absolute, so none is one.
pub const WORDS: [&str; 5] = ["add", "amend", "ready", "drop", "page"];
/// The bound of a whole action request: the engine takes 65,536 bytes.
const PAGE_LIMIT: usize = 60_000;
/// The texts of the page with the schema's bounds.
const TEXTS: [(&str, usize); 9] = [
    ("objective", 2000),
    ("interface", 20_000),
    ("invariants", 3000),
    ("not_built", 1500),
    ("notes", 3000),
    ("verifier_questions", 4000),
    ("report", 1000),
    ("repository", 200),
    ("branch", 200),
];
/// The page properties `amend` takes; `add` takes `lane`, `intent` and `origin` too.
const PAGE: &str = "class objective interface invariants acceptance not_built write_set read_first specs notes verifier_questions report repository branch implementer verifier estimate_lines depends_on";
/// Runs the command after the output file with both outputs into that file and standard input from
/// `/dev/null`, and prints its status as the last line on the standard output it was given. The
/// command does not hold that output: a background process of a check cannot keep the call open or
/// print a status of its own; an output file that cannot be opened prints no status.
const WRAPPER: &str =
    "o=$1; shift; exec 3>&1 >\"$o\" 2>&1 </dev/null || exit 0; \"$@\" 3>&-; echo \"$?\" >&3";
/// The bound of one check, and of the call that runs it.
const CHECK_SECONDS: &str = "300";
const CALL_MS: u64 = 330_000;
const FETCH_MS: u64 = 120_000;
const OUTPUT_TAIL: usize = 2000;
/// The states `systemctl is-active` answers while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];

/// A refusal: `after` the op's suffix once its action applied, `unit` the key the request names.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<String>,
    pub unit: Option<String>,
}
/// One call by its arguments: the op, `--store`, and for `page` the key, `--for` and `--digest`.
#[derive(Debug)]
pub struct Call {
    pub op: &'static str,
    pub store: Option<String>,
    pub key: Option<String>,
    pub role: Option<Role>,
    pub digest: bool,
}
/// The arguments after `unit` (R1): any other form is `usage`.
pub fn arguments(argv: &[OsString]) -> Result<Call, Failure> {
    let usage = || error("usage", "args");
    let words: Option<Vec<&str>> = argv.iter().map(|a| a.to_str()).collect();
    let words = words.ok_or_else(usage)?;
    let (first, rest) = words.split_first().ok_or_else(usage)?;
    let op = *WORDS.iter().find(|w| *w == first).ok_or_else(usage)?;
    let mut call = Call {
        op,
        store: None,
        key: None,
        role: None,
        digest: false,
    };
    let mut rest = rest.iter();
    if op == "page" {
        let key = rest
            .next()
            .filter(|k| (1..=512).contains(&k.len()) && !k.starts_with('-'));
        call.key = Some(key.ok_or_else(usage)?.to_string());
    }
    while let Some(arg) = rest.next() {
        match *arg {
            "--store" if call.store.is_none() => {
                let path = rest.next().filter(|p| p.starts_with('/'));
                call.store = Some(path.ok_or_else(usage)?.to_string());
            }
            "--for" if op == "page" && call.role.is_none() && !call.digest => {
                let role = rest.next().and_then(|r| Role::named(r));
                call.role = Some(role.ok_or_else(usage)?);
            }
            "--digest" if op == "page" && call.role.is_none() && !call.digest => call.digest = true,
            _ => return Err(usage()),
        }
    }
    Ok(call)
}
/// The call: what it prints on standard output, or the refusal with how far it went.
pub fn execute(s: &mut impl System, call: &Call, r: &Value) -> Result<String, Box<Refusal>> {
    let store = call.store.as_deref();
    let mut after = None;
    let (out, unit) = match call.op {
        "page" => {
            let key = call.key.as_deref().unwrap_or_default();
            let page = render::page(s, store, key, call.role, call.digest);
            (page, call.key.clone())
        }
        op => {
            let answer = write(s, store, op, r, &mut after).map(|v| format!("{v}\n"));
            (answer, r["unit"].as_str().map(str::to_owned))
        }
    };
    out.map_err(|f| {
        Box::new(Refusal {
            code: store::code(&f),
            field: f.field,
            detail: f.detail,
            after,
            unit,
        })
    })
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let refused = |f: Failure| {
        Box::new(Refusal {
            code: f.code.into(),
            field: f.field,
            detail: f.detail,
            after: None,
            unit: None,
        })
    };
    let result = arguments(argv).map_err(refused).and_then(|call| {
        let r = match call.op {
            "page" => Value::Null,
            _ => request().map_err(refused)?,
        };
        execute(&mut Host, &call, &r)
    });
    let (written, status) = match result {
        Ok(out) => (std::io::stdout().write_all(out.as_bytes()), 0),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail,
                "after": e.after, "unit": e.unit}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// The request of standard input: one JSON value of at most 1 MiB.
pub(super) fn request() -> Result<Value, Failure> {
    let mut bytes = vec![];
    let read = std::io::stdin().take(LIMIT + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > LIMIT {
        return Err(error("input_too_large", "stdin"));
    }
    serde_json::from_slice(&bytes).map_err(|_| bad("stdin"))
}
/// A string of at most `max` bytes, never cut (an empty one is the store's to refuse); none when
/// absent and not `required`.
fn bounded<'a>(
    r: &'a Value,
    k: &str,
    max: usize,
    required: bool,
) -> Result<Option<&'a str>, Failure> {
    match r.get(k) {
        None if !required => Ok(None),
        Some(Value::String(t)) if t.len() <= max => Ok(Some(t)),
        Some(Value::String(t)) => Err(fail(
            "invalid_input",
            k,
            json!({"bytes": t.len(), "limit": max}),
        )),
        _ => Err(bad(k)),
    }
}
/// The key of a new unit (R6).
fn new_key(r: &Value) -> Result<&str, Failure> {
    let ok = |t: &&str| {
        (1..=59).contains(&t.len())
            && t.starts_with(|c: char| c.is_ascii_lowercase())
            && t.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    };
    r["unit"].as_str().filter(ok).ok_or_else(|| bad("unit"))
}
fn one_of(r: &Value, k: &str, values: &[&str]) -> Result<Value, Failure> {
    let v = r[k].as_str().filter(|v| values.contains(v));
    v.map(|v| json!(v)).ok_or_else(|| bad(k))
}
/// An array of at most `max` strings of 1 to `bytes` bytes without a control character, each
/// admitted by `ok`.
fn entries(
    r: &Value,
    k: &str,
    max: usize,
    bytes: usize,
    ok: fn(&str) -> bool,
) -> Result<Value, Failure> {
    let list = r[k]
        .as_array()
        .filter(|l| l.len() <= max)
        .ok_or_else(|| bad(k))?;
    for (i, e) in list.iter().enumerate() {
        let e = e.as_str().filter(|e| {
            (1..=bytes).contains(&e.len()) && !e.chars().any(char::is_control) && ok(e)
        });
        e.ok_or_else(|| bad(&format!("{k}.{i}")))?;
    }
    Ok(json!(list))
}
/// `<owner>/<name>` (R9).
fn repository_ok(t: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    matches!(t.split_once('/'), Some((o, n)) if part(o) && part(n) && n != "." && n != "..")
}
/// A branch a unit's run may check out, push to and delete (R10).
pub fn branch_ok(b: &str) -> bool {
    let first = b.split('/').next().unwrap_or_default();
    (1..=100).contains(&b.len())
        && b.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c))
        && !b.starts_with(['-', '/'])
        && !b.contains("..")
        && !["origin", "refs", "heads", "remotes", "tags"].contains(&first)
        && b != "main"
        && b != "HEAD"
        && !b.ends_with(['/', '.'])
        && b.split('/')
            .all(|c| !c.is_empty() && !c.starts_with('.') && !c.ends_with(".lock"))
}
fn pattern_ok(p: &str) -> bool {
    !p.starts_with('/') && !p.split('/').any(|c| c == "..")
}
fn read_first_ok(e: &str) -> bool {
    let Some((path, range)) = e.rsplit_once(':') else {
        return true;
    };
    let numbers = range.split_once('-').and_then(|(a, b)| {
        let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
        (digits(a) && digits(b)).then(|| (a.parse::<u64>().ok(), b.parse::<u64>().ok()))
    });
    match numbers {
        None => true,
        Some((Some(from), Some(to))) => !path.is_empty() && 1 <= from && from <= to,
        Some(_) => false,
    }
}
fn spec_ok(e: &str) -> bool {
    match e.rsplit_once('#') {
        None => true,
        Some((path, r)) => {
            let n = r.strip_prefix('R').unwrap_or_default();
            !path.is_empty()
                && !path.contains('#')
                && !n.is_empty()
                && n.bytes().all(|c| c.is_ascii_digit())
        }
    }
}
/// A list property: its name, its most entries, the most bytes of one, and the form of one.
type List = (&'static str, usize, usize, fn(&str) -> bool);
/// The acceptance checks (R14).
fn acceptance(r: &Value) -> Result<Value, Failure> {
    let list = r["acceptance"].as_array().filter(|l| l.len() <= 20);
    let list = list.ok_or_else(|| bad("acceptance"))?;
    for (i, c) in list.iter().enumerate() {
        let at = format!("acceptance.{i}.");
        only(c, "check kind", &at)?;
        let check = sized(&c["check"], &format!("{at}check"), 600)?;
        if check.contains(['\n', '\r']) {
            return Err(bad(&format!("{at}check")));
        }
        one_of(c, "kind", &["fails_on_base", "invariant"])
            .map_err(|_| bad(&format!("{at}kind")))?;
    }
    Ok(json!(list))
}
/// A tier: exactly `runtime`, `model` and `effort`, three strings of 1 to 80 bytes.
fn tier(r: &Value, k: &str) -> Result<Value, Failure> {
    let t = &r[k];
    let at = format!("{k}.");
    only(t, "runtime model effort", &at)?;
    for m in ["runtime", "model", "effort"] {
        let ok = t[m].as_str().is_some_and(|v| (1..=80).contains(&v.len()));
        if !ok {
            return Err(bad(&format!("{at}{m}")));
        }
    }
    Ok(t.clone())
}
/// The parameters of `add_unit` or `amend_unit` from the request, every rule but the dependencies'
/// existence checked; the keys of `depends_on`.
fn page_parameters(r: &Value, op: &str, key: &str) -> Result<(Value, Vec<String>), Failure> {
    let mut p = Map::from_iter([("unit".to_owned(), id("unit", key))]);
    let add = op == "add";
    if add {
        p.insert("lane".into(), one_of(r, "lane", &["small", "objective"])?);
        for (k, kind) in [("intent", "intent"), ("origin", "request")] {
            if r.get(k).is_some() {
                p.insert(k.into(), id(kind, sized(&r[k], k, 512)?));
            }
        }
    }
    if add || r.get("class").is_some() {
        p.insert(
            "class".into(),
            one_of(r, "class", &["plain", "contract", "effect"])?,
        );
    }
    for (k, max) in TEXTS {
        if let Some(t) = bounded(r, k, max, add && k == "objective")? {
            p.insert(k.into(), json!(t));
        }
    }
    if p.get("repository")
        .is_some_and(|v| !repository_ok(v.as_str().unwrap_or_default()))
    {
        return Err(bad("repository"));
    }
    if p.get("branch")
        .is_some_and(|v| !branch_ok(v.as_str().unwrap_or_default()))
    {
        return Err(bad("branch"));
    }
    let lists: [List; 3] = [
        ("write_set", 60, 200, pattern_ok),
        ("read_first", 40, 300, read_first_ok),
        ("specs", 20, 300, spec_ok),
    ];
    for (k, max, bytes, ok) in lists {
        if r.get(k).is_some() {
            p.insert(k.into(), entries(r, k, max, bytes, ok)?);
        }
    }
    if r.get("acceptance").is_some() {
        p.insert("acceptance".into(), acceptance(r)?);
    }
    for k in ["implementer", "verifier"] {
        if r.get(k).is_some() {
            p.insert(k.into(), tier(r, k)?);
        }
    }
    if let Some(n) = r.get("estimate_lines") {
        let n = n.as_u64().filter(|n| *n <= i64::MAX as u64);
        p.insert(
            "estimate_lines".into(),
            json!(n.ok_or_else(|| bad("estimate_lines"))?),
        );
    }
    let mut depends = vec![];
    if r.get("depends_on").is_some() {
        let list = r["depends_on"].as_array().filter(|l| l.len() <= 20);
        for (i, d) in list.ok_or_else(|| bad("depends_on"))?.iter().enumerate() {
            depends.push(sized(d, &format!("depends_on.{i}"), 512)?.to_owned());
        }
        let ids: Vec<Value> = depends.iter().map(|d| id("unit", d)).collect();
        p.insert("depends_on".into(), json!(ids));
    }
    Ok((p.into(), depends))
}
/// The engine gives the role of `ONT_ROLE` to a request that carries none (R4).
pub(crate) fn lead_in_environment(s: &impl System) -> bool {
    s.environment()
        .iter()
        .any(|(k, v)| k == "ONT_ROLE" && v == "lead")
}
/// The members each writing op admits besides `request_id`, `agent` and `session`.
fn members(op: &str) -> String {
    match op {
        "add" => format!("unit lane intent origin {PAGE}"),
        "amend" => format!("unit {PAGE}"),
        "ready" => "unit".into(),
        _ => "unit reason".into(),
    }
}
/// A writing op (R2 to R28).
fn write(
    s: &mut impl System,
    store: Option<&str>,
    op: &str,
    r: &Value,
    after: &mut Option<String>,
) -> Result<Value, Failure> {
    if !r.is_object() {
        return Err(bad("stdin"));
    }
    if op == "amend"
        && let Some(k) = ["lane", "intent", "origin"]
            .into_iter()
            .find(|k| r.get(*k).is_some())
    {
        return Err(bad(k));
    }
    only(r, &format!("request_id agent session {}", members(op)), "")?;
    let rid = shaped(r, "request_id", 44, "._-")?;
    let actor = Actor {
        agent: shaped(r, "agent", 64, "._:-")?,
        session: shaped(r, "session", 64, "._:-")?,
    };
    let key = match op {
        "add" => new_key(r)?,
        _ => sized(&r["unit"], "unit", 512)?,
    };
    let rid = format!("{rid}-{op}");
    let (p, depends) = match op {
        "add" | "amend" => page_parameters(r, op, key)?,
        "drop" => {
            let reason = bounded(r, "reason", 600, true)?.unwrap_or_default();
            (json!({"unit": id("unit", key), "reason": reason}), vec![])
        }
        _ => (json!({"unit": id("unit", key)}), vec![]),
    };
    let action = format!("{}_unit", if op == "add" { "add" } else { op });
    let size = serde_json::to_vec(&store::body(&actor, &action, rid.clone(), p.clone()));
    let size = size.map_or(usize::MAX, |b| b.len());
    if size > PAGE_LIMIT {
        return Err(fail(
            "page_too_large",
            "page",
            json!({"bytes": size, "limit": PAGE_LIMIT}),
        ));
    }
    if lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    if op == "ready" {
        return ready(s, store, &actor, rid, key, after);
    }
    // `amend` and `drop` of a unit the store does not have: the actions would answer another refusal.
    if op != "add" && at(store::get(s, store, "unit", key), "unit")?.is_none() {
        return Err(error("unknown_unit", "unit"));
    }
    for d in &depends {
        if at(store::get(s, store, "unit", d), "depends_on")?.is_none() {
            return Err(fail("unknown_dependency", "depends_on", json!(d)));
        }
    }
    let apply = match op {
        "add" => store::add_unit,
        "amend" => store::amend_unit,
        _ => store::drop_unit,
    };
    let applied = match at(apply(s, store, &actor, rid, p), op) {
        Err(f) if op == "add" && store::code(&f) == "revision_conflict" => {
            return Err(fail("unit_exists", "unit", f.detail));
        }
        other => other?,
    };
    *after = Some(op.to_owned());
    let back = read_back(s, store, key)?;
    let mut answer = json!({"unit": key, "outcome": applied.outcome, "state": back["state"],
        "amended_at": back["amended_at"]});
    if op == "amend" {
        let outdated = back["class"] == "contract" || back["class"] == "effect";
        answer["page_review_outdated"] = json!(outdated);
    }
    Ok(answer)
}
fn read_back(s: &mut impl System, store: Option<&str>, key: &str) -> Result<Value, Failure> {
    let back = at(store::get(s, store, "unit", key), "unit")?;
    back.ok_or_else(|| error("store_answer_unreadable", "unit"))
}
/// `ready` (R20 to R28): the unit read once, its page review, its checks on the base, the apply.
fn ready(
    s: &mut impl System,
    store: Option<&str>,
    actor: &Actor,
    rid: String,
    key: &str,
    after: &mut Option<String>,
) -> Result<Value, Failure> {
    let read = at(store::head(s, store, "unit", key), "unit")?;
    let (revision, u) = read.ok_or_else(|| error("unknown_unit", "unit"))?;
    let mut p = json!({"unit": id("unit", key)});
    if u["lane"] == "objective" {
        p["intent"] = u["intent"].clone();
    }
    let (mut base, mut checks) = (Value::Null, vec![]);
    if u["state"] == "draft" {
        if u["class"] == "contract" || u["class"] == "effect" {
            page_reviewed(s, store, key, &u)?;
        }
        (base, checks) = on_base(s, &u)?;
    }
    let applied = at(
        store::ready_unit(s, store, actor, rid, p, &revision),
        "ready",
    )?;
    *after = Some("ready".into());
    let back = read_back(s, store, key)?;
    Ok(
        json!({"unit": key, "outcome": applied.outcome, "state": back["state"], "base": base,
        "checks": checks}),
    )
}
/// Some accepting review of kind `page`, started after the last amendment, names the page's digest (R21).
fn page_reviewed(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    u: &Value,
) -> Result<(), Failure> {
    let digest = render::page_digest(key, u);
    let keys = at(
        store::view_keys(s, store, "reviews_of_unit", json!({"unit": key})),
        "reviews",
    )?;
    let amended = u["amended_at"].as_str().unwrap_or_default();
    for k in keys {
        let review = at(store::get(s, store, "review", &k), "reviews")?;
        let Some(v) = review else { continue };
        let started = v["started_at"].as_str().unwrap_or_default();
        if v["kind"] == "page"
            && v["status"] == "closed"
            && v["verdict"] == "accept"
            && started > amended
            && v["head"] == digest.as_str()
        {
            return Ok(());
        }
    }
    Err(fail("page_not_reviewed", "unit", json!({"digest": digest})))
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// The clone of `repository` named by the host's policy file (R22).
pub fn clone_of(s: &impl System, repository: &str) -> Result<String, Failure> {
    let unmapped = || error("repository_not_mapped", "repository");
    let file = s.policy_file().ok_or_else(unmapped)?;
    let Some(path) = file.to_str() else {
        let missing =
            std::fs::metadata(&file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        return Err(if missing {
            unmapped()
        } else {
            invalid("file", "path is not valid UTF-8")
        });
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(unmapped()),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    let v = v
        .as_object()
        .ok_or_else(|| invalid("file", "is not one JSON object"))?;
    let Some(all) = v.get("repositories") else {
        return Err(unmapped());
    };
    let all = all
        .as_object()
        .ok_or_else(|| invalid("repositories", "is not an object"))?;
    let member = format!("repositories.{repository}");
    let Some(entry) = all.get(repository) else {
        return Err(unmapped());
    };
    let entry = entry
        .as_object()
        .ok_or_else(|| invalid(&member, "is not an object"))?;
    let Some(clone) = entry.get("clone") else {
        return Err(unmapped());
    };
    let absolute = |p: &&str| {
        Path::new(p).is_absolute() && !p.contains('\0') && !p.split('/').any(|c| c == "..")
    };
    let clone = clone.as_str().filter(absolute);
    let clone =
        clone.ok_or_else(|| invalid(&format!("{member}.clone"), "is not an absolute path"))?;
    Ok(clone.to_owned())
}
/// A new temporary directory of mode 0700, removed when dropped, whatever is in it.
struct Scratch(PathBuf);
impl Scratch {
    fn new(s: &impl System) -> Result<Self, Failure> {
        let name = new_session_id(s).map_err(|_| error("io_error", "temporary"))?;
        let path = std::env::temp_dir().join(format!("ap-ready-{name}"));
        let mut b = std::fs::DirBuilder::new();
        b.mode(0o700)
            .create(&path)
            .map_err(|_| error("io_error", "temporary"))?;
        Ok(Self(path))
    }
    fn at(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.0).is_err() {
            // A check may have taken the permissions of a directory it made: give them back, again.
            writable(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
/// Every directory below `dir` (not through a link) readable, writable and searchable by its owner.
pub(super) fn writable(dir: &Path) {
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            writable(&e.path());
        }
    }
}
fn git_step(s: &mut impl System, words: &[&str], step: &str) -> Result<String, Failure> {
    let a: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    let reply = s.call("git", &a, &Value::Null, 60_000);
    let reply = reply.ok().filter(|r| r.success);
    let reply = reply.ok_or_else(|| fail("git_failed", "clone", json!(step)))?;
    Ok(String::from_utf8_lossy(&reply.stdout).trim().to_owned())
}
/// `git fetch origin main` in the clone within its bound (R23): the commit `origin/main` then names.
pub(super) fn fetch(s: &mut impl System, clone: &str) -> Result<String, Failure> {
    let fetch = ["-C", clone, "fetch", "--quiet", "origin", "main"];
    let fetch: Vec<String> = fetch.iter().map(|w| (*w).to_owned()).collect();
    let fetched = s.call("git", &fetch, &Value::Null, FETCH_MS);
    if !fetched.is_ok_and(|r| r.success) {
        return Err(error("fetch_failed", "clone"));
    }
    let named = ["-C", clone, "rev-parse", "--verify", "origin/main^{commit}"];
    git_step(s, &named, "rev-parse")
}
/// A throwaway copy of the clone at `dir` (R23): shared objects, no remote, a detached checkout of
/// `base`. Nothing is added to the clone.
pub(super) fn copy(s: &mut impl System, clone: &str, dir: &str, base: &str) -> Result<(), Failure> {
    let copy = ["clone", "--quiet", "--shared", "--no-checkout", clone, dir];
    git_step(s, &copy, "clone")?;
    git_step(s, &["-C", dir, "remote", "remove", "origin"], "remote")?;
    let checkout = ["-C", dir, "checkout", "--quiet", "--detach", base];
    git_step(s, &checkout, "checkout").map(|_| ())
}
/// The acceptance checks of `u` on the base, in a throwaway copy (R23 to R27): the base and the checks.
fn on_base(s: &mut impl System, u: &Value) -> Result<(Value, Vec<Value>), Failure> {
    let clone = match u["repository"].as_str().filter(|r| !r.is_empty()) {
        Some(repository) => Some(clone_of(s, repository)?),
        None => None,
    };
    checks_on_base(s, u, clone.as_deref(), None)
}
/// The criteria of the store's `ready_unit` on the page itself, in the store's order, judged from the
/// unit as read (`ap unit review`, `specs/ap/unit-review.md` R23): the first that fails, with every
/// failed code in `detail`.
pub(super) fn complete(u: &Value) -> Result<(), Failure> {
    let absent = |k: &str| u.get(k).is_none_or(Value::is_null);
    let empty = |k: &str| absent(k) || u[k].as_array().is_some_and(Vec::is_empty);
    let mut failed = vec![];
    if empty("write_set") {
        failed.push("write_set_missing");
    }
    if empty("acceptance") {
        failed.push("acceptance_missing");
    }
    if absent("implementer") || absent("verifier") {
        failed.push("tiers_missing");
    }
    match failed.first() {
        Some(code) => Err(fail(code, "unit", json!({"failed": failed}))),
        None => Ok(()),
    }
}
/// The acceptance checks of `u` (R23 to R27): with `clone`, in a throwaway copy of it at `fetched` (the
/// commit `origin/main` named after a fetch the caller made), or at the commit a fetch made here names;
/// without, in an empty directory. The base and the checks.
pub(super) fn checks_on_base(
    s: &mut impl System,
    u: &Value,
    clone: Option<&str>,
    fetched: Option<&str>,
) -> Result<(Value, Vec<Value>), Failure> {
    let scratch = Scratch::new(s)?;
    let dir = scratch.at("base");
    let base = match clone {
        Some(clone) => {
            let base = match fetched {
                Some(commit) => commit.to_owned(),
                None => fetch(s, clone)?,
            };
            copy(s, clone, &dir, &base)?;
            Some(base)
        }
        None => {
            std::fs::create_dir(&dir).map_err(|_| error("io_error", "temporary"))?;
            None
        }
    };
    let list = u["acceptance"].as_array().cloned().unwrap_or_default();
    let mut checks = vec![];
    for (i, c) in list.iter().enumerate() {
        let (index, kind) = (i + 1, c["kind"].as_str().unwrap_or_default());
        let check = c["check"].as_str().unwrap_or_default();
        let output = scratch.at(&format!("check-{index}.out"));
        let started = s.now();
        let base = base.as_deref().unwrap_or_default();
        let status = run_check(s, &dir, (base, base), &output, check)?;
        let seconds = s.now().saturating_duration_since(started).as_secs();
        let code = match (status, kind) {
            (None, _) => Some("acceptance_not_runnable"),
            (Some(124 | 137), _) => Some("acceptance_timeout"),
            (Some(125..=127), "fails_on_base") => Some("acceptance_not_runnable"),
            (Some(n), "fails_on_base") if n >= 200 => Some("acceptance_not_runnable"),
            (Some(0), "fails_on_base") => Some("acceptance_passes_on_base"),
            (Some(0), _) | (Some(_), "fails_on_base") => None,
            (Some(_), _) => Some("invariant_fails_on_base"),
        };
        if let Some(code) = code {
            let detail = json!({"index": index, "kind": kind, "check": check, "status": status,
                "output": tail(&output)});
            return Err(fail(code, "acceptance", detail));
        }
        checks.push(json!({"index": index, "kind": kind, "status": status, "seconds": seconds}));
    }
    drop(scratch);
    Ok((json!(base), checks))
}
/// The last bytes of a check's output, cut forward to a character boundary: the file is opened and read
/// from its last `OUTPUT_TAIL` bytes by a seek, never whole (a check may write without bound).
fn tail(output: &str) -> String {
    let last = || -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(output)?;
        let size = file.metadata()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(OUTPUT_TAIL as u64)))?;
        let mut bytes = vec![];
        file.take(OUTPUT_TAIL as u64).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = last().unwrap_or_default();
    let mut from = 0;
    while from < bytes.len() && (bytes[from] & 0xC0) == 0x80 {
        from += 1;
    }
    String::from_utf8_lossy(&bytes[from..]).into_owned()
}
/// The command of one check after the wrapper (R24): no directory or environment of the caller's but
/// `PATH` and `HOME`.
pub fn check_command(
    s: &impl System,
    dir: &str,
    (base, head): (&str, &str),
    output: &str,
    check: &str,
) -> Vec<String> {
    let env = s.environment();
    let var = |k: &str| {
        env.iter()
            .find(|(n, _)| n == k)
            .map_or("", |(_, v)| v.as_str())
    };
    let words = [
        "-c",
        WRAPPER,
        "sh",
        output,
        "env",
        "-i",
        "-C",
        dir,
        &format!("PATH={}", var("PATH")),
        &format!("HOME={}", var("HOME")),
        &format!("AP_BASE={base}"),
        &format!("AP_HEAD={head}"),
        "timeout",
        "-k",
        "10",
        CHECK_SECONDS,
        "bash",
        "-c",
        check,
    ];
    words.iter().map(|w| (*w).to_owned()).collect()
}
/// One check through the port, inside a transient unit where units are used (R24, R25); its status,
/// when the wrapper printed one. `ap unit run` gives the merge base and the head (`AP_BASE`, `AP_HEAD`).
pub(super) fn run_check(
    s: &mut impl System,
    dir: &str,
    commits: (&str, &str),
    output: &str,
    check: &str,
) -> Result<Option<i64>, Failure> {
    let command = check_command(s, dir, commits, output, check);
    let (reply, unit) = if s.units() {
        let id = new_session_id(s).map_err(|_| error("io_error", "unit"))?;
        let unit = format!("ap-check-{id}");
        let mut argv: Vec<String> = [
            "--wait",
            "--pipe",
            "--collect",
            "--quiet",
            "--expand-environment=no",
            "--unit",
            &unit,
            "--slice",
            "agents.slice",
            "-p",
            "RuntimeMaxSec=320s",
            "-p",
            "TimeoutStopSec=10s",
            "--",
            "sh",
        ]
        .iter()
        .map(|w| (*w).to_owned())
        .collect();
        argv.extend(command);
        (
            s.call("systemd-run", &argv, &Value::Null, CALL_MS),
            Some(unit),
        )
    } else {
        (s.call("sh", &command, &Value::Null, CALL_MS), None)
    };
    if let Some(unit) = unit
        && !stopped(s, &unit)
    {
        return Err(fail("check_not_stopped", "acceptance", json!(unit)));
    }
    let printed = reply
        .ok()
        .map(|r| String::from_utf8_lossy(&r.stdout).into_owned());
    let last = printed
        .as_deref()
        .and_then(|p| p.trim_end_matches('\n').rsplit('\n').next());
    Ok(last.and_then(|l| l.trim().parse().ok()))
}
/// Stops a check's unit and reads it as ended: `systemctl is-active` answers no live state.
fn stopped(s: &mut impl System, unit: &str) -> bool {
    let words = |w: &[&str]| w.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
    let _ = s.call("systemctl", &words(&["stop", unit]), &Value::Null, 60_000);
    let reply = s.call(
        "systemctl",
        &words(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let state = reply.map(|r| String::from_utf8_lossy(&r.stdout).trim().to_owned());
    state.is_ok_and(|st| !st.is_empty() && !ALIVE.contains(&st.as_str()))
}
