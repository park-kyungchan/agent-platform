//! `ap unit`: one unit of work once from its page to the merge (README "One unit"). The stages are
//! `ap run`'s code, the gate is `ap gate`'s, the review and the status are `ap review`'s.
use crate::review::{self, DIFF_OPTIONS};
use crate::run::{stamp, write};
use crate::work::{
    Failure, LIMIT, System, args, digest, error, fail, id, ont, parse, require, text,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
    process::ExitCode,
    time::SystemTime,
};

const MEMBERS: &str = "repository clone worktree branch task_key page out acceptance implementer verifier acceptance_seconds gate store merge pull_request verdict";
const STAGE: &str =
    "account model effort prompt permission_mode timeout_seconds next_accounts env resume";
/// The files of one call, by suffix of `out`: a stage's answer also has `ap run`'s three files.
const FILES: &str =
    ".unit.json .implementer.md .verifier.md .verifier.prompt.md .gate.json .accept.log";
const RUN_FILES: [&str; 4] = ["", ".run.json", ".stream.jsonl", ".err"];
/// The acceptance executable runs in the worktree under `timeout`, with the arguments after the
/// fourth; its exit is printed last.
const ACCEPT: &str = "cd \"$1\" || exit 0; a=$2 o=$3 t=$4; shift 4; timeout -k 10 \"$t\" \"$a\" \"$@\" > \"$o\" 2>&1 < /dev/null; echo \"$?\"";

/// The first step that did not pass.
struct Stop {
    cause: String,
    detail: Value,
}
fn stop(cause: &str, detail: Value) -> Stop {
    let cause = cause.to_owned();
    Stop { cause, detail }
}
impl From<Failure> for Stop {
    fn from(f: Failure) -> Self {
        stop(f.code, f.detail)
    }
}
type Step = Result<Value, Stop>;
/// Ends the step with `cause` (and `detail`) unless `condition` holds.
macro_rules! ensure {
    ($condition:expr => $cause:expr) => {
        ensure!($condition => $cause, Value::Null)
    };
    ($condition:expr => $cause:expr, $detail:expr) => {
        if !$condition {
            return Err(stop($cause, $detail));
        }
    };
}
mod case;

fn absolute(p: &str) -> bool {
    let path = Path::new(p);
    path.is_absolute() && !p.contains('\0') && !path.components().any(|c| c == Component::ParentDir)
}
fn path<'a>(r: &'a Value, key: &str, field: &str) -> Result<&'a str, Failure> {
    let p = r[key].as_str().filter(|p| absolute(p));
    p.ok_or_else(|| error("invalid_input", field))
}
fn regular(path: &str, code: &'static str, field: &str) -> Result<Vec<u8>, Failure> {
    let bad = || error(code, field);
    let f = std::fs::File::open(path).map_err(|_| bad())?;
    let meta = f.metadata().map_err(|_| bad())?;
    require!(meta.is_file() && meta.len() <= LIMIT => code, field);
    let mut bytes = vec![];
    f.take(LIMIT).read_to_end(&mut bytes).map_err(|_| bad())?;
    Ok(bytes)
}
fn stage(r: &Value, key: &str) -> Result<(), Failure> {
    let o = r[key]
        .as_object()
        .ok_or_else(|| error("invalid_input", key))?;
    let field = |k: &str| format!("{key}.{k}");
    if let Some(k) = o.keys().find(|k| !STAGE.split(' ').any(|m| m == *k)) {
        return Err(error("invalid_input", &field(k)));
    }
    Ok(())
}
/// `acceptance_seconds`: 1 to 86400, default 2700.
fn seconds(r: &Value) -> Result<u64, Failure> {
    let Some(v) = r.get("acceptance_seconds") else {
        return Ok(2700);
    };
    let n = v.as_u64().filter(|n| (1..=86_400).contains(n));
    n.ok_or_else(|| error("invalid_input", "acceptance_seconds"))
}
fn executable(acceptance: &str) -> Result<(), Failure> {
    let executable = std::fs::metadata(acceptance)
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    require!(executable => "acceptance_unreadable", "acceptance");
    regular(acceptance, "acceptance_unreadable", "acceptance").map(|_| ())
}
fn record(path: &str) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
/// The block of facts appended to the verifier's prompt.
fn facts(r: &Value, number: u64, head: &str, base: &str) -> String {
    let f = |k: &str| r[k].as_str().unwrap_or_default();
    let (repo, wt, out) = (f("repository"), f("worktree"), f("out"));
    format!(
        "\n## Facts of this unit (ap unit)\n\nRepository: {repo}\nPull request: {number}\nHead: {head}\nBase: {base}\nDiff: git -C {wt} diff {base} {head}\nGate answer: {out}.gate.json\nAcceptance output: {out}.accept.log\n"
    )
}
/// The `ap run` request of a stage, the same when it is admitted and when it starts.
fn stage_request(r: &Value, which: &str, prompt: &str) -> Value {
    let (mut req, out) = (r[which].clone(), r["out"].as_str().unwrap_or_default());
    (req["cwd"], req["prompt"]) = (r["worktree"].clone(), json!(prompt));
    req["out"] = json!(format!("{out}.{which}.md"));
    req["role"] = json!(if which == "verifier" {
        "reviewer"
    } else {
        which
    });
    if which == "verifier" {
        req["expect"] = json!({"first_line": "Verdict: *"});
    }
    if req.get("permission_mode").is_none() {
        req["permission_mode"] = json!("auto");
    }
    req
}
/// The checks before any effect; the answer is the page's write set.
fn request(s: &mut impl System, r: &Value) -> Result<Vec<String>, Failure> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.split(' ').any(|m| m == *k)) {
        return Err(error("invalid_input", k));
    }
    let p = |k: &str| path(r, k, k);
    p("clone")?;
    let (worktree, page, out, acceptance) =
        (p("worktree")?, p("page")?, p("out")?, p("acceptance")?);
    let branch = text(r, "branch")?;
    let safe = |c: u8| c.is_ascii_alphanumeric() || b"._/-".contains(&c);
    let branch_ok =
        branch.len() <= 100 && !branch.starts_with(['-', '/']) && !branch.contains("..");
    require!(branch_ok && branch.bytes().all(safe) => "invalid_input", "branch");
    let key = text(r, "task_key")?;
    let key_char =
        |(i, c): (usize, u8)| c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && c == b'-');
    require!(key.len() <= 64 && key.bytes().enumerate().all(key_char) => "invalid_input", "task_key");
    require!(r.get("merge").is_none_or(Value::is_boolean) => "invalid_input", "merge");
    seconds(r)?;
    if let Some(v) = r.get("pull_request") {
        require!(v.as_u64().is_some_and(|n| n > 0) => "invalid_input", "pull_request");
    }
    let verdict = match r.get("verdict") {
        None => None,
        Some(_) => {
            require!(r.get("pull_request").is_some() => "invalid_input", "verdict");
            Some(path(r, "verdict", "verdict")?)
        }
    };
    if let Some(store) = r.get("store") {
        require!(store.as_str().is_some_and(absolute) => "invalid_input", "store");
    }
    // Every call has a verdict of a model: a verifier stage, or the answer of an earlier one.
    require!(r.get("verifier").is_some() != verdict.is_some() => "invalid_input", "verifier");
    stage(r, "implementer")?;
    if r.get("verifier").is_some() {
        stage(r, "verifier")?;
    }
    let mut gate = r.get("gate").cloned().unwrap_or_else(|| json!({}));
    require!(gate.is_object() => "invalid_input", "gate");
    (gate["repository"], gate["pull_request"]) = (r["repository"].clone(), json!(1));
    crate::gate::check(&gate).map_err(|f| {
        let field = match f.field.as_str() {
            "repository" => f.field.clone(),
            other => format!("gate.{other}"),
        };
        fail(f.code, &field, f.detail)
    })?;
    let root = s.work_root();
    let v = verdict.unwrap_or(page);
    for (k, p) in [("page", page), ("out", out), ("verdict", v)] {
        require!(Path::new(p).starts_with(&root) => "outside_work_root", k);
    }
    let inside = |p: &str| Path::new(p).starts_with(worktree);
    require!(!inside(acceptance) => "inside_worktree", "acceptance");
    if let Some(prompt) = r["verifier"]["prompt"].as_str() {
        require!(!inside(prompt) => "inside_worktree", "verifier.prompt");
    }
    let bytes = regular(page, "page_unreadable", "page")?;
    let page_text = String::from_utf8_lossy(&bytes);
    let mut lines = page_text
        .lines()
        .filter_map(|l| l.strip_prefix("Write set:"));
    let parts = lines.next().unwrap_or_default().split(',').map(str::trim);
    let write_set: Vec<String> = parts.filter(|p| !p.is_empty()).map(String::from).collect();
    require!(lines.next().is_none() && !write_set.is_empty() => "write_set_missing", "page");
    // Every member `ap run` can judge before its stage's worktree exists, by its own code.
    for k in ["implementer", "verifier"]
        .into_iter()
        .filter(|k| r.get(*k).is_some())
    {
        let lead = r[k]["prompt"].as_str().unwrap_or_default();
        let field = |f: Failure| fail(f.code, &format!("{k}.{}", f.field), f.detail);
        crate::run::admit_stage(s, &stage_request(r, k, lead)).map_err(field)?;
    }
    // The stage refuses a prompt over the limit once the block of facts is appended: room for it.
    if let Some(lead) = r["verifier"]["prompt"].as_str() {
        let room = facts(r, u64::MAX, &"0".repeat(40), &"0".repeat(40)).len() as u64 + 1;
        let size = std::fs::metadata(lead).map_or(LIMIT, |m| m.len());
        require!(size + room <= LIMIT => "prompt_unreadable", "verifier.prompt");
    }
    executable(acceptance)?;
    // A verdict is taken only as the answer of a reviewer's stage: its `ap run` record lies beside.
    if let Some(v) = verdict {
        let bytes = regular(v, "verdict_unreadable", "verdict")?;
        let run = record(&format!("{v}.run.json")).unwrap_or_default();
        let prompt = run["prompt"]["path"].as_str().unwrap_or_default();
        let asked = regular(prompt, "verdict_unreadable", "verdict").map(|b| digest(&b));
        let ok = run["role"] == "reviewer" && run["state"] == "ok" && run["answer"]["path"] == v;
        let ok = ok
            && run["answer"]["sha256"] == digest(&bytes).as_str()
            && Path::new(prompt).starts_with(&root);
        require!(ok && asked.is_ok_and(|d| run["prompt"]["sha256"] == d.as_str()) => "verdict_unreadable", "verdict");
    }
    for suffix in FILES.split(' ') {
        for extra in RUN_FILES {
            let file = format!("{out}{suffix}{extra}");
            require!(Path::new(&file).symlink_metadata().is_err() => "out_exists", "out");
        }
    }
    if r.get("pull_request").is_none() {
        let exists = Path::new(worktree).symlink_metadata().is_ok();
        require!(!exists => "worktree_exists", "worktree");
    } else {
        require!(Path::new(worktree).is_dir() => "worktree_missing", "worktree");
    }
    Ok(write_set)
}

/// The record and its writes: replaced through a temporary name after every step.
struct Unit {
    path: String,
    rec: Value,
    clean: bool,
    started: String,
}
fn now() -> String {
    stamp(SystemTime::now())
}
impl Unit {
    fn save(&mut self) {
        let mut bytes = serde_json::to_vec_pretty(&self.rec).unwrap_or_default();
        bytes.push(b'\n');
        let saved = write(&self.path, &bytes).is_ok()
            || std::fs::File::create(&self.path).is_ok_and(|mut f| f.write_all(&bytes).is_ok());
        self.clean &= saved;
    }
    /// The step begun: named in `step`, on disk before it has an effect.
    fn begin(&mut self, name: &str) {
        self.rec["step"] = json!(name);
        self.started = now();
        self.save();
    }
    /// The step done: appended to `steps` with its result and times.
    fn end(&mut self, result: &str, detail: Value) {
        let name = std::mem::take(&mut self.rec["step"]);
        let step = json!({"name": name, "result": result, "start": self.started, "end": now(), "detail": detail});
        self.rec["steps"].as_array_mut().unwrap().push(step);
        self.save();
    }
    fn step(&mut self, name: &str, f: impl FnOnce(&mut Self) -> Step) -> Result<(), Stop> {
        self.begin(name);
        let detail = f(self)?;
        self.end("passed", detail);
        Ok(())
    }
    fn skip(&mut self, name: &str) {
        self.begin(name);
        self.end("skipped", Value::Null);
    }
}

fn git(s: &mut impl System, dir: &str, words: &[&str]) -> Result<String, Stop> {
    let answer = review::git(s, dir, words).map_err(|_| stop("git_failed", Value::Null))?;
    Ok(answer.trim_end().to_owned())
}
fn ok(s: &mut impl System, dir: &str, words: &[&str]) -> bool {
    review::git(s, dir, words).is_ok()
}
/// The worktree is clean and its `HEAD` is the head the gate passed.
fn at_head(s: &mut impl System, worktree: &str, head: &str) -> Result<(), Stop> {
    let porcelain = ["status", "--porcelain", "--untracked-files=normal"];
    let status = git(s, worktree, &porcelain)?;
    ensure!(status.is_empty() => "worktree_dirty");
    let local = git(s, worktree, &["rev-parse", "HEAD"])?;
    ensure!(local == head => "head_differs", json!({"head": local}));
    Ok(())
}
fn unchanged(rec: &Value, path: &str) -> Result<(), Stop> {
    let now = regular(path, "digest_differs", "file").map(|b| digest(&b));
    ensure!(now.ok().as_deref() == rec["sha256"].as_str() => "digest_differs", json!({"path": path}));
    Ok(())
}
/// One `ap run` stage; its record's path goes into the unit record before its state is judged.
fn stage_run(s: &mut impl System, u: &mut Unit, which: &str, r: Value) -> Step {
    // The record's path is on disk before the stage starts: a killed call names it.
    u.rec["stages"][which] = json!(format!("{}.run.json", text(&r, "out")?));
    u.save();
    let refused = |f: Failure| stop("stage_refused", json!({"code": f.code, "field": f.field}));
    let (rec, _) = crate::run::execute(s, &r).map_err(refused)?;
    let state = rec["state"].as_str().unwrap_or("failed");
    ensure!(state == "ok" => state);
    Ok(json!({"session": rec["session"]}))
}
/// `acceptance` in `wt` with `extra` arguments under the bound; its exit, when one was read.
fn accept(
    s: &mut impl System,
    wt: &str,
    a: &str,
    log: &str,
    n: u64,
    extra: &[&str],
) -> Option<i64> {
    let mut a = args(&["-c", ACCEPT, "sh", wt, a, log, &n.to_string()]);
    a.extend(args(extra));
    let reply = s.call("sh", &a, &Value::Null, (n + 30) * 1000).ok();
    reply.and_then(|r| String::from_utf8_lossy(&r.stdout).trim().parse().ok())
}
/// A verdict report: `Verdict: accept`, then the head.
fn verdict(bytes: &[u8], head: &str) -> Result<(), Stop> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let first = lines.next().unwrap_or_default();
    ensure!(first != "Verdict: repair" => "verdict_repair");
    ensure!(first == "Verdict: accept" => "verdict_unreadable");
    ensure!(lines.next().map(str::trim) == Some(head) => "verdict_head_differs");
    Ok(())
}
fn create(path: &str, bytes: &[u8]) -> Result<(), Stop> {
    let mut options = std::fs::OpenOptions::new();
    let created = options.write(true).create_new(true).open(path);
    let written = created.and_then(|mut f| f.write_all(bytes));
    written.map_err(|_| stop("io_error", json!({"path": path})))
}
fn gh(s: &mut impl System, words: &[&str]) -> Result<Value, Stop> {
    let reply = s.call("gh", &args(words), &Value::Null, 60_000);
    let value = reply
        .ok()
        .filter(|r| r.success)
        .and_then(|r| serde_json::from_slice(&r.stdout).ok());
    value.ok_or_else(|| stop("github_unreadable", Value::Null))
}

/// The closed review of kind `packet` that accepted this head of this repository for this task by
/// this report, when the store holds one.
fn reviewed(
    s: &mut impl System,
    r: &Value,
    key: &str,
    repo: &str,
    head: &str,
    sha: &str,
) -> Result<Option<Value>, Stop> {
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let filter = [
        ("kind", json!("packet")),
        ("repository", json!(repo)),
        ("head", json!(head)),
        ("status", json!("closed")),
        ("verdict", json!("accept")),
        ("report_sha256", json!(sha)),
    ];
    let mut all: Vec<_> = filter.into_iter().map(|(k, v)| eq(k, v)).collect();
    all.push(eq("task", id("task", key)));
    let q = json!({"object_type": "review", "where": {"all": all}, "limit": 1});
    let v = parse(ont(s, r, &["search"], &q)?, "store_answer_unreadable")?;
    ensure!(v["status"] == "ok" && v["result"]["basis"]["qualified"] == true => "store_answer_unreadable");
    Ok(v["result"]["objects"][0].get("id").cloned())
}
/// Words separated by single spaces (names without spaces: validated before).
fn w(words: &str) -> Vec<&str> {
    words.split(' ').collect()
}
type End = Result<&'static str, Stop>;
fn steps(s: &mut impl System, r: &Value, write_set: &[String], u: &mut Unit) -> End {
    let f = |k: &str| r[k].as_str().unwrap_or_default();
    let (repo, clone, wt) = (f("repository"), f("clone"), f("worktree"));
    let (branch, out, acceptance) = (f("branch"), f("out"), f("acceptance"));
    let (mut number, mut pr_head) = (r["pull_request"].as_u64().unwrap_or(0), None);
    if number > 0 {
        u.rec["pull_request"] = json!(number);
        for name in ["worktree", "implementer", "pull_request"] {
            u.skip(name);
        }
    } else {
        u.step("worktree", |u| {
            ensure!(ok(s, clone, &["fetch", "origin", "main"]) => "fetch_failed");
            let mut add = w("worktree add --track -b");
            add.extend([branch, wt, "origin/main"]);
            let base = git(s, clone, &add).and_then(|_| git(s, wt, &["rev-parse", "HEAD"]));
            u.rec["base"] = json!(base.map_err(|_| stop("worktree_failed", Value::Null))?);
            Ok(Value::Null)
        })?;
        u.step("implementer", |u| {
            let lead = text(&r["implementer"], "prompt")?;
            stage_run(s, u, "implementer", stage_request(r, "implementer", lead))
        })?;
        u.step("pull_request", |u| {
            let fields = "number,headRefOid,headRefName,baseRefName";
            let list = format!(
                "pr list --repo {repo} --head {branch} --base main --state open --json {fields}"
            );
            let list = gh(s, &w(&list))?;
            let mine = |p: &&Value| p["headRefName"] == branch && p["baseRefName"] == "main";
            let list: Vec<_> = list.as_array().into_iter().flatten().filter(mine).collect();
            ensure!(!list.is_empty() => "no_pull_request");
            ensure!(list.len() == 1 => "pull_request_ambiguous", json!({"count": list.len()}));
            number = list[0]["number"].as_u64().unwrap_or(0);
            ensure!(number > 0 => "github_unreadable");
            pr_head = list[0]["headRefOid"].as_str().map(str::to_owned);
            u.rec["pull_request"] = json!(number);
            Ok(Value::Null)
        })?;
    }
    u.step("gate", |u| {
        let mut req = r.get("gate").cloned().unwrap_or_else(|| json!({}));
        (req["repository"], req["pull_request"]) = (json!(repo), json!(number));
        if let Some(h) = &pr_head {
            req["head"] = json!(h);
        }
        let (answer, _) = crate::gate::execute(s, &req)?;
        let file = format!("{out}.gate.json");
        let bytes = serde_json::to_vec_pretty(&answer).unwrap_or_default();
        create(&file, &bytes)?;
        u.rec["files"]["gate"] = json!(file);
        ensure!(answer["end"] == "passed" => answer["end"].as_str().unwrap_or("github_unreadable"));
        u.rec["head"] = answer["head"].clone();
        ensure!(answer["autoreview"]["has_unresolved_findings"] != true => "unresolved_findings");
        let threads = &answer["threads"];
        ensure!(threads.as_array().is_some_and(Vec::is_empty) => "unresolved_threads", json!({"threads": threads}));
        // The status `lead-review` the gate lists and does not judge: a revoked acceptance is seen.
        let mut checks = answer["checks"].as_array().into_iter().flatten();
        let lead = checks.find(|c| c["judged"] == false).map(|c| c["state"].clone());
        Ok(json!({"lead_review": lead}))
    })?;
    let head = u.rec["head"].as_str().unwrap_or_default().to_owned();
    let log = format!("{out}.accept.log");
    u.step("acceptance", |u| {
        unchanged(&u.rec["acceptance"], acceptance)?;
        at_head(s, wt, &head)?;
        u.rec["files"]["acceptance_output"] = json!(log);
        let exit = accept(s, wt, acceptance, &log, seconds(r)?, &[]);
        ensure!(exit != Some(124) => "acceptance_timeout", json!({"exit": 124}));
        ensure!(exit == Some(0) => "acceptance_failed", json!({"exit": exit}));
        Ok(json!({"exit": 0}))
    })?;
    u.step("write_set", |u| {
        // The rulesets require an up-to-date branch: one behind `main` would not merge.
        ensure!(ok(s, clone, &["fetch", "origin", "main"]) => "fetch_failed");
        let base = git(s, wt, &["rev-parse", "origin/main"])?;
        let ancestor = ok(s, wt, &["merge-base", "--is-ancestor", &base, &head]);
        ensure!(ancestor => "behind_main", json!(base));
        u.rec["base"] = json!(base);
        let mut words = vec!["diff"];
        words.extend(DIFF_OPTIONS);
        words.extend([base.as_str(), head.as_str()]);
        let diff = git(s, wt, &words)?;
        let paths: Vec<_> = diff.split_terminator('\0').collect();
        let outside = review::outside(write_set, &paths);
        let detail =
            json!({"paths": outside.iter().take(10).collect::<Vec<_>>(), "count": outside.len()});
        ensure!(outside.is_empty() => "path_outside_write_set", detail);
        Ok(json!({"paths": paths.len()}))
    })?;
    let base = u.rec["base"].as_str().unwrap_or_default().to_owned();
    let report = match r["verdict"].as_str() {
        Some(file) => {
            u.begin("verifier");
            unchanged(&u.rec["verdict_file"], file)?;
            verdict(&regular(file, "verdict_unreadable", "verdict")?, &head)?;
            u.rec["stages"]["verifier"] = json!(format!("{file}.run.json"));
            u.end("skipped", Value::Null);
            file.to_owned()
        }
        None => {
            let report = format!("{out}.verifier.md");
            u.step("verifier", |u| {
                let lead = text(&r["verifier"], "prompt")?;
                unchanged(&u.rec["verifier_prompt"], lead)?;
                at_head(s, wt, &head)?;
                let mut bytes = regular(lead, "digest_differs", "verifier.prompt")?;
                if !bytes.ends_with(b"\n") {
                    bytes.push(b'\n');
                }
                bytes.extend(facts(r, number, &head, &base).as_bytes());
                let prompt = format!("{out}.verifier.prompt.md");
                create(&prompt, &bytes)?;
                let session = stage_run(s, u, "verifier", stage_request(r, "verifier", &prompt))?;
                u.rec["files"]["report"] = json!(report);
                verdict(&regular(&report, "verdict_unreadable", "report")?, &head)?;
                Ok(session)
            })?;
            report
        }
    };
    u.rec["files"]["report"] = json!(report);
    u.step("review", |u| {
        let store = |mut v: Value| {
            if let Some(store) = r.get("store") {
                v["store"] = store.clone();
            }
            v
        };
        let key = text(r, "task_key")?;
        let sha = digest(&regular(&report, "review_report_unreadable", "report")?);
        // One accepted head has one review object: a closed one that accepted this report is used.
        if let Some(id) = reviewed(s, &store(json!({})), key, repo, &head, &sha)? {
            u.rec["review"] = id;
            return Ok(json!({"reused": true}));
        }
        // The reviewer is the stage that wrote the report: its record names model, effort, account,
        // session and prompt.
        let path = u.rec["stages"]["verifier"].as_str().unwrap_or_default();
        let run = record(path).ok_or_else(|| stop("verdict_unreadable", json!({"path": path})))?;
        let start = store(
            json!({"task_dir": s.work_root().join(key), "kind": "packet", "head": head,
            "base": base, "repository": repo, "pull_request": number, "model": run["model"],
            "effort": run["effort"], "prompt": run["prompt"]["path"]}),
        );
        let started = review::start(s, &start)?;
        u.rec["review"] = started["review"].clone();
        u.save();
        let close = store(json!({"review": started["review"], "report": report,
            "account": run["account"], "session": run["session"]}));
        ensure!(review::close(s, &close)?["verdict"] == "accept" => "verdict_repair");
        Ok(json!({"reused": false}))
    })?;
    if r["merge"] == false {
        return Ok("accepted");
    }
    u.step("status", |u| {
        let p = review::pr(s, repo, &json!(number))
            .map_err(|_| stop("github_unreadable", Value::Null))?;
        let open =
            p["state"] == "OPEN" && p["baseRefName"] == "main" && p["headRefOid"] == head.as_str();
        ensure!(open => "head_differs", json!({"head": p["headRefOid"], "state": p["state"]}));
        let key = text(&u.rec["review"], "native_key")?.to_owned();
        let target = json!({"repository": repo, "head": head});
        let posted = review::post(s, &target, "success", &format!("review {key} accept"));
        posted.map_err(|_| stop("status_failed", Value::Null))?;
        Ok(Value::Null)
    })?;
    u.step("merge", |u| {
        let n = number.to_string();
        let merge = format!("pr merge {n} --repo {repo} --merge --match-head-commit {head}");
        let merge = args(&w(&merge));
        let called = s
            .call("gh", &merge, &Value::Null, 120_000)
            .is_ok_and(|r| r.success);
        // The read decides, also after a call that failed: it may have merged.
        let view = format!("pr view {n} --repo {repo} --json state,mergeCommit,headRefOid");
        let p = gh(s, &w(&view)).unwrap_or(Value::Null);
        let commit = p["mergeCommit"]["oid"].as_str().filter(|c| c.len() == 40);
        let merged = commit.is_some() && p["state"] == "MERGED" && p["headRefOid"] == head.as_str();
        let cause = if called {
            "merge_unconfirmed"
        } else {
            "merge_failed"
        };
        ensure!(merged => cause, json!({"state": p["state"]}));
        u.rec["merge_commit"] = json!(commit);
        Ok(json!({"call": called}))
    })?;
    u.begin("cleanup");
    let fetched = ok(s, clone, &["fetch", "origin", "main"]);
    let removed = ok(s, clone, &["worktree", "remove", wt]);
    let deleted = removed && ok(s, clone, &["branch", "-d", branch]);
    let kept = if removed { "failed" } else { "kept" };
    let detail = json!({"fetch": fetched, "worktree": if removed {"removed"} else {"failed"}, "branch": if deleted {"deleted"} else {kept}});
    u.end(if deleted { "passed" } else { "failed" }, detail);
    Ok("merged")
}

/// One `ap unit` request. A refusal (exit 2) comes before any effect and writes no file.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    if r.get("case").is_some() {
        return case::execute(s, r);
    }
    let write_set = request(s, r)?;
    let file = |p: Option<&str>| match p {
        Some(p) => regular(p, "io_error", "file").map(|b| json!({"path": p, "sha256": digest(&b)})),
        None => Ok(Value::Null),
    };
    let verifier_prompt = r["verifier"]["prompt"].as_str();
    let rec = json!({"mode": "unit", "repository": r["repository"], "clone": r["clone"], "worktree": r["worktree"],
        "branch": r["branch"], "task_key": r["task_key"], "base": null, "page": file(r["page"].as_str())?,
        "write_set": write_set, "acceptance": file(r["acceptance"].as_str())?,
        "verifier_prompt": file(verifier_prompt)?, "verdict_file": file(r["verdict"].as_str())?,
        "merge": r["merge"] != false, "pull_request": null, "head": null,
        "review": null, "merge_commit": null, "stages": {"implementer": null, "verifier": null},
        "files": {"gate": null, "acceptance_output": null, "report": null}, "state": "running",
        "step": null, "stopped_at": null, "cause": null, "detail": null, "steps": [], "start": now(), "end": null});
    claimed(r, rec, |u| steps(s, r, &write_set, u))
}
/// The claim, the steps and the end, of a unit call and a case call alike.
fn claimed(
    r: &Value,
    rec: Value,
    steps: impl FnOnce(&mut Unit) -> End,
) -> Result<(Value, u8), Failure> {
    let path = format!("{}.unit.json", text(r, "out")?);
    // The claim: created exclusively before any effect.
    let mut options = std::fs::OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => error("out_exists", "out"),
            _ => error("out_unwritable", "out"),
        })?;
    let (clean, started) = (true, String::new());
    let mut u = Unit {
        path,
        rec,
        clean,
        started,
    };
    u.save();
    let state = match steps(&mut u) {
        Ok(state) => state,
        Err(Stop { cause, detail }) => {
            u.rec["stopped_at"] = u.rec["step"].clone();
            u.end("stopped", detail.clone());
            (u.rec["cause"], u.rec["detail"]) = (json!(cause), detail);
            "stopped"
        }
    };
    (u.rec["state"], u.rec["end"]) = (json!(state), json!(now()));
    u.save();
    let status = match state {
        _ if !u.clean => 5,
        "stopped" => 3,
        "failed" => 4,
        _ => 0,
    };
    Ok((u.rec, status))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    crate::run::cli_with(argv, execute)
}
