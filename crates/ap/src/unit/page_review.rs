//! `ap unit review`: a unit's page (kind `page`) or an intent's plan (kind `plan`) reviewed by a fresh
//! session as one stage of `ap run`, the review recorded with `start_unit_review` and
//! `close_unit_review` (`specs/ap/unit-review.md`).
use super::object::{self, clone_of};
use super::render::{self, Role};
use crate::host::{Failure, Host, System, digest, error, fail, id};
use crate::store::{self, Actor, Found, at, bad, only, shaped, sized};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::ExitCode,
};

const PLAN_REVIEWER: &str = include_str!("frames/plan-reviewer.md");
const REPAIR_LOOK: &str = include_str!("frames/repair-look.md");
const EARLIER: &str =
    "## Earlier reports (claims of earlier reviewers; check them, do not trust them)";
const CYCLE: &str = "These units depend on each other in a cycle:";
const MEMBERS: &str =
    "request_id agent session unit intent task reviewer resume earlier timeout_seconds";
const REVIEWER: [&str; 7] = [
    "runtime",
    "model",
    "effort",
    "account",
    "next_accounts",
    "permission_mode",
    "compact_window",
];
const DIRECTORY: &str = "/var/lib/agent-platform/reviews";
const TIMEOUT: u64 = 7200;
/// The states `systemctl is-active` answers while something of a unit may live.
const ALIVE: [&str; 5] = [
    "active",
    "activating",
    "deactivating",
    "reloading",
    "refreshing",
];

/// A refusal (R1): `after` the last action this call applied, `reviews` the list as last read.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub field: String,
    pub detail: Value,
    pub after: Option<&'static str>,
    pub unit: Option<String>,
    pub intent: Option<String>,
    pub reviews: Value,
}
/// What a call did so far: what it applied, the list it read, the prompt file it wrote.
#[derive(Default)]
struct Trace {
    after: Option<&'static str>,
    reviews: Value,
    prompt: Option<PathBuf>,
    /// The start was sent and gave no answer: it may have been applied, the prompt stays.
    sent: bool,
}
/// The subject of one call and where its files are.
struct Look<'a> {
    store: Option<&'a str>,
    rid: &'a str,
    actor: Actor<'a>,
    page: bool,
    key: &'a str,
    dir: PathBuf,
}
impl Look<'_> {
    fn kind(&self) -> &'static str {
        if self.page { "page" } else { "plan" }
    }
    fn member(&self) -> &'static str {
        if self.page { "unit" } else { "intent" }
    }
    /// A review of this subject and kind.
    fn of(&self, v: &Value) -> bool {
        v["kind"] == self.kind() && v[self.member()]["native_key"] == self.key
    }
    fn work(&self, k: &str) -> PathBuf {
        self.dir.join("work").join(k)
    }
    /// The scratch directory of the look of review `k` under `limits` (R13).
    fn scratch(&self, k: &str) -> PathBuf {
        self.dir.join("scratch").join(k)
    }
    fn out(&self, k: &str) -> String {
        self.dir
            .join(format!("{k}.md"))
            .to_string_lossy()
            .into_owned()
    }
}
/// How a review closes (R14, R19).
struct Ending {
    verdict: &'static str,
    cause: Option<String>,
    report: Option<String>,
    sha: Option<String>,
    session: String,
    account: Option<String>,
}

/// A key the verb takes (R2): 1 to 80 ASCII letters, digits and `-`.
pub(super) fn key_ok(k: &str) -> bool {
    (1..=80).contains(&k.len()) && k.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn key<'a>(v: &'a Value, field: &str) -> Result<&'a str, Failure> {
    v.as_str().filter(|k| key_ok(k)).ok_or_else(|| bad(field))
}
/// The arguments after `review`: none, or `--store <absolute path>` (R1).
pub fn arguments(argv: &[OsString]) -> Result<Option<String>, Failure> {
    let usage = || error("usage", "args");
    match argv {
        [] => Ok(None),
        [flag, path] if flag == "--store" => {
            let path = path.to_str().filter(|p| p.starts_with('/'));
            Ok(Some(path.ok_or_else(usage)?.to_owned()))
        }
        _ => Err(usage()),
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let refused = |f: Failure| {
        Box::new(Refusal {
            code: f.code.into(),
            field: f.field,
            detail: f.detail,
            after: None,
            unit: None,
            intent: None,
            reviews: Value::Null,
        })
    };
    let read = arguments(argv).and_then(|store| Ok((store, object::request()?)));
    let result = read
        .map_err(refused)
        .and_then(|(store, r)| execute(&mut Host, store.as_deref(), &r));
    let (written, status) = match result {
        Ok((v, status)) => (writeln!(std::io::stdout(), "{v}"), status),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail,
                "after": e.after, "unit": e.unit, "intent": e.intent, "reviews": e.reviews}});
            (writeln!(std::io::stderr(), "{v}"), 2)
        }
    };
    ExitCode::from(if written.is_ok() { status } else { 4 })
}
/// One call: the answer and its exit (0 `accept` or `repair`, 3 `abandoned`), or the refusal.
pub fn execute(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
) -> Result<(Value, u8), Box<Refusal>> {
    let mut trace = Trace::default();
    let result = call(s, store, r, &mut trace);
    result.map_err(|f| {
        // A call refused before its start removes the prompt it wrote (R5).
        if let (None, false, Some(p)) = (trace.after, trace.sent, &trace.prompt) {
            let _ = std::fs::remove_file(p);
        }
        let named = |k: &str| r[k].as_str().map(str::to_owned);
        Box::new(Refusal {
            code: store::code(&f),
            field: f.field,
            detail: f.detail,
            after: trace.after,
            unit: named("unit"),
            intent: named("intent"),
            reviews: trace.reviews,
        })
    })
}
fn call(
    s: &mut impl System,
    store: Option<&str>,
    r: &Value,
    trace: &mut Trace,
) -> Result<(Value, u8), Failure> {
    if !r.is_object() {
        return Err(bad("stdin"));
    }
    let ending = ["abandon", "finish"]
        .into_iter()
        .find(|k| r.get(*k).is_some());
    only(
        r,
        match ending {
            Some("abandon") => "request_id agent session unit intent abandon discard",
            Some(_) => "request_id agent session unit intent finish",
            None => MEMBERS,
        },
        "",
    )?;
    let rid = shaped(r, "request_id", 44, "._-")?;
    let actor = Actor {
        agent: shaped(r, "agent", 64, "._:-")?,
        session: shaped(r, "session", 64, "._:-")?,
    };
    let (page, subject) = match (r.get("unit"), r.get("intent")) {
        (Some(u), None) => (true, key(u, "unit")?),
        (None, Some(i)) => (false, key(i, "intent")?),
        _ => return Err(bad("intent")),
    };
    let mut new = None;
    if let Some(which) = ending {
        key(&r[which], which)?;
        if !r.get("discard").is_none_or(Value::is_boolean) {
            return Err(bad("discard"));
        }
    } else {
        new = Some(request(r)?);
    }
    if object::lead_in_environment(s) {
        return Err(error("role_in_environment", "ONT_ROLE"));
    }
    let reviews = policy(s)?;
    let dir = reviews.get("directory").and_then(Value::as_str);
    let look = Look {
        store,
        rid,
        actor,
        page,
        key: subject,
        dir: PathBuf::from(dir.unwrap_or(DIRECTORY)),
    };
    match (ending, new) {
        (Some(which), _) => {
            let k = r[which].as_str().unwrap_or_default();
            ended(s, &look, which, k, r["discard"] == true, trace)
        }
        (None, Some(new)) => look_at(s, &look, new, &reviews, trace),
        (None, None) => Err(bad("stdin")),
    }
}
/// The members of a new look, checked before any call (R2 to R4, R8, R9).
struct New<'a> {
    task: Option<&'a str>,
    reviewer: Map<String, Value>,
    resume: Option<&'a str>,
    earlier: Vec<&'a str>,
    timeout: u64,
}
fn request(r: &Value) -> Result<New<'_>, Failure> {
    let task = r.get("task").map(|t| sized(t, "task", 512)).transpose()?;
    let reviewer = match r.get("reviewer") {
        None => Map::new(),
        Some(v) => {
            only(v, &REVIEWER.join(" "), "reviewer.")?;
            v.as_object().cloned().unwrap_or_default()
        }
    };
    let resume = r.get("resume").map(|v| key(v, "resume")).transpose()?;
    let earlier = match r.get("earlier") {
        None => vec![],
        Some(v) => {
            let list = v.as_array().filter(|l| (1..=5).contains(&l.len()));
            let list = list.ok_or_else(|| bad("earlier"))?;
            list.iter()
                .map(|k| key(k, "earlier"))
                .collect::<Result<_, _>>()?
        }
    };
    if resume.is_some() && !earlier.is_empty() {
        return Err(bad("earlier"));
    }
    if resume.is_some() && reviewer.contains_key("next_accounts") {
        return Err(bad("reviewer.next_accounts"));
    }
    let timeout = match r.get("timeout_seconds") {
        None => TIMEOUT,
        Some(v) => v
            .as_u64()
            .filter(|t| (1..=86_400).contains(t))
            .ok_or_else(|| bad("timeout_seconds"))?,
    };
    Ok(New {
        task,
        reviewer,
        resume,
        earlier,
        timeout,
    })
}
fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
/// The member `reviews` of the host's policy file, each member checked (R4, R5); empty without a
/// file or without the member.
pub(crate) fn policy(s: &impl System) -> Result<Map<String, Value>, Failure> {
    let Some(file) = s.policy_file() else {
        return Ok(Map::new());
    };
    let Some(path) = file.to_str() else {
        let missing =
            std::fs::metadata(&file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        return match missing {
            true => Ok(Map::new()),
            false => Err(invalid("file", "path is not valid UTF-8")),
        };
    };
    let bytes = match s.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(_) => return Err(invalid("file", "is unreadable")),
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| invalid("file", "is not JSON"))?;
    let v = v.as_object();
    let v = v.ok_or_else(|| invalid("file", "is not one JSON object"))?;
    let reviews = match v.get("reviews") {
        None => return Ok(Map::new()),
        Some(r) => r
            .as_object()
            .ok_or_else(|| invalid("reviews", "is not an object"))?,
    };
    for (k, v) in reviews {
        let word = v.as_str().filter(|w| !w.is_empty());
        let ok = match k.as_str() {
            "runtime" | "model" | "effort" | "permission_mode" | "task" => word.is_some(),
            "account" => word.is_some_and(|p| Path::new(p).is_absolute() && !p.contains('\0')),
            "compact_window" => v.is_u64(),
            "models" => v.as_array().is_some_and(|l| {
                (1..=64).contains(&l.len())
                    && l.iter().all(|m| m.as_str().is_some_and(|m| !m.is_empty()))
            }),
            "directory" => word.is_some_and(|d| {
                d.starts_with('/')
                    && (2..=400).contains(&d.len())
                    && d.bytes().all(|c| (b'!'..=b'~').contains(&c))
            }),
            _ => {
                return Err(invalid(
                    &format!("reviews.{k}"),
                    "is not a member of reviews",
                ));
            }
        };
        if !ok {
            return Err(invalid(&format!("reviews.{k}"), "has the wrong form"));
        }
    }
    Ok(reviews.clone())
}
/// The reviews of this subject and kind by `started_at` (R20).
fn reviews(s: &mut impl System, look: &Look) -> Result<Vec<Found>, Failure> {
    let mut list = vec![];
    if look.page {
        let args = json!({"unit": look.key});
        let keys = at(
            store::view_keys(s, look.store, "reviews_of_unit", args),
            "reviews",
        )?;
        for k in keys {
            if let Some(v) = at(store::get(s, look.store, "review", &k), "reviews")? {
                list.push((k, v));
            }
        }
    } else {
        let all = vec![json!({"eq": ["kind", "plan"]})];
        list = at(store::search(s, look.store, "review", all), "reviews")?;
    }
    list.retain(|(_, v)| look.of(v));
    let order = |(k, v): &Found| {
        let started = v["started_at"].as_str().map(str::to_owned);
        (started, v["round"].as_u64(), k.clone())
    };
    list.sort_by_key(order);
    Ok(list)
}
fn listing(list: &[Found]) -> Value {
    let one = |(k, v): &Found| {
        json!({"review": k, "head": v["head"], "round": v["round"], "status": v["status"],
            "verdict": v["verdict"]})
    };
    list.iter().map(one).collect()
}
/// The working directory `<directory>/work/<k>` that `cwd` is, compared as path components, for `k`
/// the key of a review of the list (R8).
fn work_of(look: &Look, cwd: &str, list: &[Found]) -> Option<PathBuf> {
    let keys = list.iter().map(|(k, _)| k).filter(|k| key_ok(k));
    keys.map(|k| look.work(k))
        .find(|p| p.components().eq(Path::new(cwd).components()))
}
/// Removes what is at `p` as written: a directory with everything in it, a link as a link (R22).
fn remove_tree(p: &Path) {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => {
            if std::fs::remove_dir_all(p).is_err() {
                object::writable(p);
                let _ = std::fs::remove_dir_all(p);
            }
        }
        Ok(_) => {
            let _ = std::fs::remove_file(p);
        }
        Err(_) => {}
    }
}
fn io(what: &str) -> Failure {
    fail("io_error", "directory", json!(what))
}
/// A JSON object read from `path`, when there is one.
fn json_file(path: &str) -> Option<Value> {
    let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    v.is_object().then_some(v)
}
/// A new look (R5 to R14, R20, R21, R23).
fn look_at(
    s: &mut impl System,
    look: &Look,
    new: New,
    reviews: &Map<String, Value>,
    trace: &mut Trace,
) -> Result<(Value, u8), Failure> {
    let task = new.task.or(reviews.get("task").and_then(Value::as_str));
    let task = task.ok_or_else(|| bad("task"))?;
    let mut stage = Map::new();
    for k in REVIEWER {
        if let Some(v) = new.reviewer.get(k).or(reviews.get(k)) {
            stage.insert(k.to_owned(), v.clone());
        }
    }
    let read = at(
        store::head(s, look.store, look.member(), look.key),
        look.member(),
    );
    let unknown = if look.page {
        "unknown_unit"
    } else {
        "unknown_intent"
    };
    let (revision, subject) = read?.ok_or_else(|| error(unknown, look.member()))?;
    let list = reviews_of(s, look, trace)?;
    let found = |k: &str| list.iter().find(|(key, _)| key == k).map(|(_, v)| v);
    let text_of = |v: &Value, k: &str| v[k].as_str().filter(|t| !t.is_empty()).map(str::to_owned);
    let mut resumed = None;
    if let Some(k) = new.resume {
        let no = |why: &str| fail("resume_not_possible", "resume", json!(why));
        let v = found(k).ok_or_else(|| no("no review of this subject and kind has this key"))?;
        if v["status"] != "closed" {
            return Err(no("the review is not closed"));
        }
        let session = text_of(v, "session").ok_or_else(|| no("the review names no session"))?;
        let report = text_of(v, "report").ok_or_else(|| no("the review has no report"))?;
        let record = format!("{report}.run.json");
        let rec = json_file(&record).ok_or_else(|| no("no run record beside the report"))?;
        let looks = list
            .iter()
            .filter(|(_, x)| x["session"] == session.as_str())
            .count();
        if looks >= 3 {
            return Err(fail(
                "resume_limit",
                "resume",
                json!({"session_looks": looks}),
            ));
        }
        let cwd = rec["cwd"].as_str().unwrap_or_default().to_owned();
        let wd = work_of(look, &cwd, &list);
        let wd = wd.ok_or_else(|| {
            no("the record's cwd is not a working directory of a review of this subject")
        })?;
        if let Some((running, _)) = list.iter().find(|(_, x)| x["status"] == "running") {
            return Err(no(&format!(
                "the review {running} of this subject is running"
            )));
        }
        for k in ["runtime", "model", "effort", "account"] {
            if let Some(v) = rec.get(k).filter(|v| v.is_string()) {
                stage.insert(k.to_owned(), v.clone());
            }
        }
        resumed = Some((record, cwd, wd));
    }
    let mut earlier = String::new();
    // The paths the stage reads outside its working directory (R13): a plan's clones, then the
    // earlier reports.
    let mut reports = vec![];
    for k in &new.earlier {
        let report = found(k)
            .filter(|v| v["status"] == "closed")
            .and_then(|v| text_of(v, "report"));
        let report = report.ok_or_else(|| fail("earlier_not_possible", "earlier", json!(k)))?;
        earlier.push_str(&format!("- {k}: {report}\n"));
        reports.push(report);
    }
    for k in ["model", "effort", "account", "permission_mode"] {
        if !stage.contains_key(k) {
            return Err(bad("reviewer"));
        }
    }
    let word = |k: &str| {
        stage[k]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| bad(&format!("reviewer.{k}")))
    };
    let (model, effort) = (word("model")?, word("effort")?);
    if let Some(models) = reviews.get("models").and_then(Value::as_array)
        && !models.iter().any(|m| m == model.as_str())
    {
        return Err(error("reviewer_model_not_allowed", "reviewer.model"));
    }
    // The text and its digest, from the one read of the subject (R6, R7).
    let repository = text_of(&subject, "repository").filter(|_| look.page);
    let mut read_only = vec![];
    let (mut text, head, copy) = if look.page {
        let clone = repository.as_deref().map(|r| clone_of(s, r)).transpose()?;
        if let Some(clone) = &clone {
            object::fetch(s, clone)?;
        }
        let (text, commit) =
            render::page_text(s, look.store, look.key, &subject, Some(Role::Reviewer))?;
        (
            text,
            render::page_digest(look.key, &subject),
            clone.zip(commit),
        )
    } else {
        let (part, rest, clones) = plan(s, look.store, look.key, &subject)?;
        read_only = clones;
        let head = digest(part.as_bytes());
        (format!("{part}\n{rest}\n{PLAN_REVIEWER}"), head, None)
    };
    // The checks `ap unit ready` runs on the base, at the commit the text names (R23).
    if look.page {
        object::complete(&subject)?;
        object::write_set_judged(&subject)?;
        let at = copy
            .as_ref()
            .map(|(clone, commit)| (clone.as_str(), commit.as_str()));
        object::checks_on_base(s, &subject, at.map(|a| a.0), at.map(|a| a.1))?;
    }
    if let Some((_, _, wd)) = &resumed {
        remove_tree(wd);
        text.push('\n');
        text.push_str(REPAIR_LOOK);
    }
    if !earlier.is_empty() {
        text.push_str(&format!("\n{EARLIER}\n\n{earlier}"));
    }
    // The pages of a page's dependencies: the stage reads no store (R6).
    if look.page {
        text.push_str(&dependencies(s, look.store, &subject)?);
    }
    text.push_str(&format!("\nDigest: {head}\n"));
    read_only.extend(reports);
    let round = list.len() + 1;
    let mut dirs = std::fs::DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    for sub in ["prompts", "work", "scratch"] {
        dirs.create(look.dir.join(sub))
            .map_err(|_| io("the reviews directory cannot be made"))?;
    }
    let name = format!("{}-{}-{}-{round}.md", look.kind(), look.key, &head[..16]);
    let prompt = look.dir.join("prompts").join(name);
    match std::fs::remove_file(&prompt) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(io("the prompt cannot be replaced"));
        }
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    let file = options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&prompt);
    trace.prompt = file.is_ok().then(|| prompt.clone());
    file.and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|_| io("the prompt cannot be written"))?;
    let prompt = prompt.to_string_lossy().into_owned();
    // ONE validate of the start names the review's key before anything is written (R10, R11).
    let mut p = json!({"task": id("task", task), "kind": look.kind(), "head": head, "model": model,
        "effort": effort, "prompt": prompt, "prompt_sha256": digest(text.as_bytes()),
        look.member(): id(look.member(), look.key), "round": round});
    if let Some(r) = &repository {
        p["repository"] = json!(r);
    }
    let rid = format!("{}-start", look.rid);
    let mut body = store::body(&look.actor, "start_unit_review", rid, p);
    if look.page {
        body["expected"] = json!([{"id": id("unit", look.key), "revision": revision}]);
    }
    let refused = |s: &mut _, f| started_refusal(s, look, &head, f);
    let v = match store::validate(s, look.store, &body) {
        Ok(v) => v,
        Err(f) => return Err(refused(s, f)),
    };
    if v["result"]["valid"] != true {
        let criteria = v["result"]["criteria"].as_array().into_iter().flatten();
        let failed: Vec<&Value> = criteria
            .filter(|c| c["passed"] == false)
            .map(|c| &c["code"])
            .collect();
        let detail = json!({"status": "refused", "code": "submission_criteria", "failed": failed});
        return Err(refused(s, fail(store::REFUSED, "start", detail)));
    }
    let made = |edits: &Value| -> Option<String> {
        let edits = edits.as_array()?.iter();
        let mut made =
            edits.filter(|e| e["change"] == "initial" && e["id"]["object_type"] == "review");
        made.next()?["id"]["native_key"]
            .as_str()
            .filter(|k| key_ok(k))
            .map(str::to_owned)
    };
    let review =
        made(&v["result"]["edits"]).ok_or_else(|| error("store_answer_unreadable", "start"))?;
    let out = look.out(&review);
    // The key of this client, agent and request id names a review already: the request id was used.
    if found(&review).is_some() || Path::new(&out).symlink_metadata().is_ok() {
        return Err(fail(
            "request_id_reused",
            "start",
            json!({"review": review}),
        ));
    }
    let (cwd, wd) = match &resumed {
        Some((_, cwd, wd)) => (cwd.clone(), wd.clone()),
        None => (
            look.work(&review).to_string_lossy().into_owned(),
            look.work(&review),
        ),
    };
    let mut req = Value::Object(stage);
    req["role"] = json!("reviewer");
    (req["cwd"], req["prompt"], req["out"]) = (json!(cwd), json!(prompt), json!(out));
    req["timeout_seconds"] = json!(new.timeout);
    // The report's second line names the digest; the close of R14 judges it exactly (R13).
    req["expect"] = json!({"first_line": "Verdict: *", "second_line": format!("*{head}*")});
    req[look.member()] = json!(look.key);
    if let Some(store) = look.store {
        req["store"] = json!(store);
    }
    if let Some((record, ..)) = &resumed {
        req["resume"] = json!({"record": record});
    }
    // A reviewer on the Claude CLI runs under `limits`: it writes only its home of the launch
    // (`specs/ap/run-limits.md` R14) and its own scratch directory, the working directory is
    // read-only to it, and it reads outside it only a plan's clones and the earlier reports (R13).
    if req.get("runtime").is_none_or(|r| r == "claude") {
        let scratch = look.scratch(&review).to_string_lossy().into_owned();
        req["limits"] = json!({ "scratch": scratch });
        if !read_only.is_empty() {
            req["limits"]["read_only"] = json!(read_only);
        }
    }
    crate::run::admit_stage(s, &req).map_err(|f| match REVIEWER.contains(&f.field.as_str()) {
        true => fail(f.code, &format!("reviewer.{}", f.field), f.detail),
        false => f,
    })?;
    trace.sent = true;
    let applied = match store::start_unit_review(s, look.store, &body) {
        Ok(a) => a,
        Err(f) => {
            trace.sent = f.code != store::REFUSED;
            return Err(refused(s, f));
        }
    };
    trace.sent = false;
    if applied.outcome != "applied" {
        return Err(fail("request_id_reused", "start", json!(applied.outcome)));
    }
    trace.after = Some("start");
    let key = applied
        .made("review")
        .and_then(|i| i["native_key"].as_str());
    if key != Some(review.as_str()) {
        return Err(error("store_answer_unreadable", "start"));
    }
    // The working directory, the stage, the directory removed whatever the end (R12, R13).
    let made = match &copy {
        Some((clone, commit)) => {
            let copied = object::copy(s, clone, &wd.to_string_lossy(), commit);
            let mode = std::fs::Permissions::from_mode(0o700);
            copied.map(|_| std::fs::set_permissions(&wd, mode).unwrap_or_default())
        }
        None => dirs
            .recursive(false)
            .create(&wd)
            .map_err(|_| io("the working directory cannot be made")),
    };
    let rec = match made {
        Ok(()) => crate::run::execute(s, &req).ok().map(|(rec, _)| rec),
        Err(_) => None,
    };
    remove_tree(&wd);
    let mut ending = judge(rec.as_ref(), &out, &head, look.actor.session);
    close(s, look, &review, &mut ending, "close", trace)?;
    answer(s, look, (&review, &head, json!(round)), &ending, trace)
}
/// The list read for `round` and the answer, kept for a refusal.
fn reviews_of(s: &mut impl System, look: &Look, trace: &mut Trace) -> Result<Vec<Found>, Failure> {
    let list = reviews(s, look)?;
    trace.reviews = listing(&list);
    Ok(list)
}
/// A refusal of the validate or the apply of the start, as the store gave it (R10, R11).
fn started_refusal(s: &mut impl System, look: &Look, head: &str, mut f: Failure) -> Failure {
    f.field = "start".into();
    if f.code == store::REFUSED
        && f.detail["code"] == "revision_conflict"
        && f.detail["id"]["object_type"] == "review"
    {
        return fail("request_id_reused", "start", f.detail);
    }
    if store::code(&f) == "duplicate_running_review" {
        let all = vec![
            json!({"eq": ["kind", look.kind()]}),
            json!({"eq": ["status", "running"]}),
        ];
        let found = store::search(s, look.store, "review", all).unwrap_or_default();
        let running = found
            .into_iter()
            .find(|(_, v)| look.of(v) && v["head"] == head);
        f.detail["running"] = json!(running.map(|(k, _)| k));
        f.detail["next"] = json!("close the running review with abandon or finish");
    }
    f
}
/// How the stage's report closes the review (R14): only a report the stage wrote, of the form, naming
/// `head`, closes `accept` or `repair`.
fn judge(rec: Option<&Value>, out: &str, head: &str, caller: &str) -> Ending {
    let session = rec
        .and_then(|r| r["session"].as_str())
        .filter(|t| !t.is_empty() && t.len() <= 200);
    let session = session.unwrap_or(caller).to_owned();
    let account = rec.and_then(|r| {
        Path::new(r["account"].as_str()?)
            .file_name()?
            .to_str()
            .map(str::to_owned)
    });
    let account = account.filter(|a| a.len() <= 40);
    let abandoned = |cause: &str| Ending {
        verdict: "abandoned",
        cause: Some(cause.to_owned()),
        report: None,
        sha: None,
        session: session.clone(),
        account: account.clone(),
    };
    let Some(rec) = rec else {
        return abandoned("stage_refused");
    };
    let state = rec["state"].as_str().unwrap_or("failed");
    if state != "ok" {
        return abandoned(state);
    }
    // Read once: what is judged is what is recorded.
    let bytes = crate::unit::regular(out, "report_changed", "report").ok();
    let sha = bytes.as_deref().map(digest);
    let named = rec["answer"]["path"] == out && rec["answer"]["sha256"].as_str() == sha.as_deref();
    let (Some(bytes), true) = (bytes, named) else {
        return abandoned("report_changed");
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let verdict = match lines.next() {
        Some("Verdict: accept") => "accept",
        Some("Verdict: repair") => "repair",
        _ => return abandoned("report_form"),
    };
    let hex = |w: &&str| w.len() == 64 && w.bytes().all(|b| b.is_ascii_hexdigit());
    let line = lines.next().unwrap_or_default();
    let mut runs = line.split(|c: char| !c.is_ascii_alphanumeric()).filter(hex);
    if runs.next() != Some(head) || runs.next().is_some() {
        return abandoned("report_form");
    }
    Ending {
        verdict,
        cause: None,
        report: Some(out.to_owned()),
        sha,
        session,
        account,
    }
}
/// `close_unit_review` with request id `<request_id>-<suffix>`; a session the store refuses as a
/// writer closes `abandoned` with the caller's session (R14, R19).
fn close(
    s: &mut impl System,
    look: &Look,
    review: &str,
    e: &mut Ending,
    suffix: &'static str,
    trace: &mut Trace,
) -> Result<(), Failure> {
    let params = |e: &Ending| {
        let mut p =
            json!({"review": id("review", review), "verdict": e.verdict, "session": e.session});
        let named = [
            ("report", &e.report),
            ("report_sha256", &e.sha),
            ("account", &e.account),
        ];
        for (k, v) in named {
            if let Some(v) = v {
                p[k] = json!(v);
            }
        }
        if look.page {
            p["unit"] = id("unit", look.key);
        }
        p
    };
    let rid = format!("{}-{suffix}", look.rid);
    let first = store::close_unit_review(s, look.store, &look.actor, rid.clone(), params(e));
    let caller = look.actor.session;
    let writer = |f: &Failure| store::code(f) == "writer_cannot_review";
    match first {
        Ok(_) => {}
        Err(f) if writer(&f) && e.session != caller => {
            e.verdict = "abandoned";
            (e.report, e.sha, e.session) = (None, None, caller.to_owned());
            if suffix == "close" {
                e.cause = Some("writer_cannot_review".into());
            }
            // A refused request id is not used: the same id carries the caller's session.
            let again = store::close_unit_review(s, look.store, &look.actor, rid, params(e));
            again.map_err(|f| match writer(&f) {
                true => another(review, f),
                false => named(f, suffix),
            })?;
        }
        Err(f) if writer(&f) => return Err(another(review, f)),
        Err(f) => return Err(named(f, suffix)),
    }
    trace.after = Some(suffix);
    Ok(())
}
/// A failure of the store named by its step, as `store::at` names it.
fn named(mut f: Failure, step: &str) -> Failure {
    if f.code == store::REFUSED || f.code.starts_with("store_answer") {
        f.field = step.into();
    }
    f
}
fn another(review: &str, f: Failure) -> Failure {
    let next =
        format!("this session wrote to the unit: another session must abandon the review {review}");
    fail(
        "writer_cannot_review",
        "session",
        json!({"store": f.detail, "next": next}),
    )
}
/// The answer (R20, R21): exit 0 for `accept` and `repair`, 3 for `abandoned`.
fn answer(
    s: &mut impl System,
    look: &Look,
    (review, head, round): (&str, &str, Value),
    e: &Ending,
    trace: &mut Trace,
) -> Result<(Value, u8), Failure> {
    let list = reviews_of(s, look, trace)?;
    let mut a = json!({"review": review, "kind": look.kind(), look.member(): look.key, "digest": head,
        "round": round, "verdict": e.verdict, "report": e.report, "session": e.session,
        "reviews": listing(&list)});
    if let Some(cause) = &e.cause {
        a["cause"] = json!(cause);
    }
    if look.page {
        // The page read again after the stage has another digest than the one reviewed (R21).
        let unit = at(store::get(s, look.store, "unit", look.key), "unit")?;
        let during = unit.is_some_and(|u| render::page_digest(look.key, &u) != head);
        a["page_amended_during_review"] = json!(during);
    }
    Ok((a, if e.verdict == "abandoned" { 3 } else { 0 }))
}
/// The name `ap run` gives a stage's unit: `ap-run-<36 of 0-9a-f->.scope` (R16).
pub(super) fn stage_unit(name: &str) -> bool {
    let id = name
        .strip_prefix("ap-run-")
        .and_then(|r| r.strip_suffix(".scope"));
    let digit = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-';
    id.is_some_and(|id| id.len() == 36 && id.bytes().all(digit))
}
/// Whether `systemctl is-active` answers a state in which nothing of the unit lives, after a stop
/// when `stop`.
pub(super) fn unit_ended(s: &mut impl System, unit: &str, stop: bool) -> bool {
    let words = |w: &[&str]| w.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
    if stop {
        let _ = s.call("systemctl", &words(&["stop", unit]), &Value::Null, 60_000);
    }
    let reply = s.call(
        "systemctl",
        &words(&["is-active", unit]),
        &Value::Null,
        10_000,
    );
    let state = reply.map(|r| String::from_utf8_lossy(&r.stdout).trim().to_owned());
    state.is_ok_and(|st| !st.is_empty() && !ALIVE.contains(&st.as_str()))
}
/// The record of a cut call's stage, as it is read now.
enum Record {
    None,
    Other,
    Json(Value),
}
/// `abandon` or `finish` of a review a cut call left `running` (R15 to R20).
fn ended(
    s: &mut impl System,
    look: &Look,
    which: &'static str,
    key: &str,
    discard: bool,
    trace: &mut Trace,
) -> Result<(Value, u8), Failure> {
    let v = at(store::get(s, look.store, "review", key), which)?;
    let v = v.filter(|v| v["status"] == "running" && look.of(v));
    let v = v.ok_or_else(|| error("review_not_running", which))?;
    let out = look.out(key);
    let record_path = format!("{out}.run.json");
    let record = match std::fs::read(&record_path) {
        Err(_) => Record::None,
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) if v.is_object() => Record::Json(v),
            _ => Record::Other,
        },
    };
    let rec = match &record {
        Record::Json(v) => Some(v),
        _ => None,
    };
    let why = match (&record, std::fs::symlink_metadata(&out).is_ok()) {
        (_, false) => Some("no report"),
        (Record::None, _) => Some("no run record"),
        (Record::Other, _) => Some("the run record is not JSON"),
        (Record::Json(r), _) if r["state"] != "ok" => Some("the run record's state is not ok"),
        _ => None,
    };
    match (which, why) {
        ("finish", Some(why)) => return Err(fail("finish_not_possible", "finish", json!(why))),
        ("abandon", None) if !discard => return Err(fail("report_exists", "abandon", json!(out))),
        _ => {}
    }
    let mut not_stopped = Value::Null;
    if let Some(name) = rec.and_then(|r| r["unit"].as_str()) {
        match stage_unit(name) {
            true if !unit_ended(s, name, which == "abandon") => {
                return Err(fail("run_still_active", which, json!(name)));
            }
            true => {}
            false => not_stopped = json!(name),
        }
    }
    let list = reviews_of(s, look, trace)?;
    let run_closed = close_run(s, look, rec, &record_path, &out);
    if run_closed == "applied" {
        trace.after = Some("close_run");
    }
    // The one working directory of that stage, before the close (R18).
    let wd = match rec {
        Some(r) => work_of(look, r["cwd"].as_str().unwrap_or_default(), &list),
        None => Some(look.work(key)),
    };
    if let Some(wd) = wd {
        remove_tree(&wd);
    }
    // The scratch directory a killed `ap run` left, by the review's own key (R18).
    remove_tree(&look.scratch(key));
    let head = v["head"].as_str().unwrap_or_default();
    let mut ending = match which {
        "finish" => judge(rec, &out, head, look.actor.session),
        _ => {
            let mut e = judge(None, &out, head, look.actor.session);
            let session = rec
                .and_then(|r| r["session"].as_str())
                .filter(|t| !t.is_empty() && t.len() <= 200);
            e.session = session.unwrap_or(look.actor.session).to_owned();
            e.cause = Some("abandon".into());
            e
        }
    };
    let suffix = if which == "finish" {
        "close"
    } else {
        "abandon"
    };
    close(s, look, key, &mut ending, suffix, trace)?;
    let (mut a, status) = answer(s, look, (key, head, v["round"].clone()), &ending, trace)?;
    (a["run_closed"], a["unit_not_stopped"]) = (run_closed, not_stopped);
    Ok((a, status))
}
/// `close_run` of the stage's own run, with the stage's own request id, when the store's run names this
/// record (R17): its outcome, the code of its refusal or failure, or null when it was not applied.
fn close_run(
    s: &mut impl System,
    look: &Look,
    rec: Option<&Value>,
    record: &str,
    out: &str,
) -> Value {
    let Some(rec) = rec else {
        return Value::Null;
    };
    let Some(run) = rec["run_object"]["key"].as_str() else {
        return Value::Null;
    };
    match store::get(s, look.store, "run", run) {
        Ok(Some(r)) if r["record"] == record => {}
        _ => return Value::Null,
    }
    let state = match rec["state"].as_str() {
        None | Some("running") => "killed",
        Some(state) => state,
    };
    let mut p = json!({"run": id("run", run), "state": state});
    if let Some(session) = rec["session"]
        .as_str()
        .filter(|t| !t.is_empty() && t.len() <= 200)
    {
        p["session"] = json!(session);
    }
    let rid = format!("run-{}-close", &digest(out.as_bytes())[..32]);
    match store::close_run(s, look.store, (&rid, "reviewer", &p)) {
        Ok(a) => json!(a.outcome),
        Err(f) => json!(store::code_of(&f)),
    }
}
/// A nonempty string property.
fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v[k].as_str().filter(|t| !t.is_empty())
}
fn ended_line(t: &str) -> String {
    match t.ends_with('\n') {
        true => t.to_owned(),
        false => format!("{t}\n"),
    }
}
/// The plan part and the rest (`## Repositories`, `## States now`) of the intent `key` read as `intent` (R7).
/// The section of a page look's dependencies, with the bytes of the writer's
/// (`specs/ap/unit-write.md` R5): each key of `depends_on` in its order with its state and its page
/// part, or `(not in the store)`; nothing for a unit without dependencies (R6).
fn dependencies(s: &mut impl System, store: Option<&str>, u: &Value) -> Result<String, Failure> {
    let deps: Vec<&str> = u["depends_on"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["native_key"].as_str().or(d.as_str()))
        .collect();
    if deps.is_empty() {
        return Ok(String::new());
    }
    let mut out = "\n## Pages of the dependencies\n".to_owned();
    for d in deps {
        match at(store::get(s, store, "unit", d), "depends_on")? {
            Some(dep) => {
                let state = dep["state"].as_str().unwrap_or_default();
                out.push_str(&format!("\n### {d}: {state}\n\n"));
                out.push_str(&render::page_part(d, &dep, false));
            }
            None => out.push_str(&format!("\n### {d}\n\n(not in the store)\n")),
        }
    }
    Ok(out)
}
/// The plan part, the rest of a plan's text, and the clones its `## Repositories` names, in its
/// order (R7, R13).
fn plan(
    s: &mut impl System,
    store: Option<&str>,
    key: &str,
    intent: &Value,
) -> Result<(String, String, Vec<String>), Failure> {
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let by = |o: &Found| {
        (
            o.1["added_at"].as_str().unwrap_or_default().to_owned(),
            o.0.clone(),
        )
    };
    let all = vec![eq("intent", id("intent", key))];
    let mut criteria = at(store::search(s, store, "criterion", all), "criteria")?;
    criteria.sort_by_key(by);
    // Both lanes: a unit of the lane `small` may name the intent too.
    let all = vec![json!({"in": ["lane", ["small", "objective"]]})];
    let mut units = at(store::search(s, store, "unit", all), "units")?;
    units.retain(|(_, u)| u["intent"]["native_key"] == key && u["state"] != "dropped");
    let deps = |u: &Value| -> Vec<String> {
        let d = u["depends_on"].as_array().into_iter().flatten();
        d.filter_map(|d| d["native_key"].as_str().map(str::to_owned))
            .collect()
    };
    // Each unit after those of the plan it depends on, else by key; the rest after the cycle line.
    let mut left: BTreeMap<String, (Vec<String>, Value)> =
        units.into_iter().map(|(k, u)| (k, (deps(&u), u))).collect();
    let mut placed: Vec<(String, Value)> = vec![];
    while let Some(k) = left
        .iter()
        .find(|(_, (d, _))| d.iter().all(|d| !left.contains_key(d)))
        .map(|(k, _)| k.clone())
    {
        let (_, u) = left.remove(&k).unwrap_or_default();
        placed.push((k, u));
    }
    let cycle = placed.len();
    placed.extend(left.into_iter().map(|(k, (_, u))| (k, u)));
    let mut part = format!("# Plan of intent {key}\n\n## The intent\n\n");
    for (label, k) in [
        ("Outcome", "outcome"),
        ("First user", "first_user"),
        ("Not included", "not_included"),
        ("Constraints", "constraints"),
    ] {
        if let Some(t) = text(intent, k) {
            part.push_str(&format!("{label}: {}", ended_line(t)));
        }
    }
    if !criteria.is_empty() {
        part.push_str("\nCriteria:\n");
    }
    for (_, c) in &criteria {
        let (statement, check) = (text(c, "statement"), text(c, "check"));
        let line = format!(
            "- {} — {}",
            statement.unwrap_or_default(),
            check.unwrap_or_default()
        );
        part.push_str(&ended_line(&line));
    }
    part.push_str("\n## Units\n");
    if placed.is_empty() {
        part.push_str("\nNo unit yet.\n");
    }
    for (i, (k, u)) in placed.iter().enumerate() {
        if i == cycle {
            part.push_str(&format!("\n{CYCLE}\n"));
        }
        let field = |n: &str| text(u, n).unwrap_or_default().to_owned();
        let repository = text(u, "repository").unwrap_or("no repository");
        part.push_str(&format!(
            "\n### {k}: {}, lane {}, {repository}\n\n",
            field("class"),
            field("lane")
        ));
        if let Some(o) = text(u, "objective") {
            part.push_str(&ended_line(o));
        }
        let list = |n: &str| {
            let l = u[n]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str);
            l.collect::<Vec<_>>().join(", ")
        };
        let tier = |n: &str| {
            let t = &u[n];
            let w = ["runtime", "model", "effort"].map(|m| t[m].as_str().unwrap_or_default());
            if t.is_object() {
                w.join(" ")
            } else {
                String::new()
            }
        };
        let estimate = u["estimate_lines"].as_u64().map(|n| format!("{n} lines"));
        for (label, value) in [
            ("Depends on", deps(u).join(", ")),
            ("Write set", list("write_set")),
            ("Specifications", list("specs")),
            ("Estimate", estimate.unwrap_or_default()),
            ("Implementer", tier("implementer")),
            ("Verifier", tier("verifier")),
        ] {
            if !value.is_empty() {
                part.push_str(&ended_line(&format!("{label}: {value}")));
            }
        }
    }
    let repositories: BTreeSet<&str> = placed
        .iter()
        .filter_map(|(_, u)| text(u, "repository"))
        .collect();
    let mut rest = "## Repositories\n\n".to_owned();
    if repositories.is_empty() {
        rest.push_str("No unit of the plan has a repository.\n");
    }
    let mut clones = vec![];
    for r in repositories {
        let clone = clone_of(s, r)?;
        let commit = object::fetch(s, &clone)?;
        rest.push_str(&format!("- {r}: clone {clone}, origin/main at {commit}\n"));
        clones.push(clone);
    }
    rest.push_str("\n## States now\n\n");
    for (k, u) in &placed {
        rest.push_str(&format!(
            "- unit {k}: {}\n",
            u["state"].as_str().unwrap_or_default()
        ));
    }
    for (k, c) in &criteria {
        rest.push_str(&format!(
            "- criterion {k}: {}\n",
            c["state"].as_str().unwrap_or_default()
        ));
    }
    Ok((part, rest, clones))
}
