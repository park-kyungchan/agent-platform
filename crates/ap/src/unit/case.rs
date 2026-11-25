//! The case call of `ap unit` (README "One case"): a past unit again, in a copy of its base, judged
//! by its accepted tests. The stage is `ap run`'s code; the record, its claim and the bounded
//! evaluator are this module's.
use super::regular;
use crate::host::{Failure, LIMIT, System, args, digest, error, fail, require, text};
use crate::run::{stamp, write};
use serde_json::{Value, json};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
    time::SystemTime,
};

const MEMBERS: &str = "case clone worktree page out acceptance implementer at acceptance_seconds";
const CASE: &str = "id repository pull_request base head tests commands page_sha256";
const FILES: &str = ".unit.json .implementer.md .implementer.prompt.md .accept.log .candidate.diff";
/// A new repository at `$2` with one commit: the tree of `$3` with the paths `$5…` as they are at
/// `$4`. Only the objects of that tree go from the clone `$1` to the copy, which names no remote.
const COPY: &str = r#"set -euo pipefail
export GIT_NO_REPLACE_OBJECTS=1
clone=$1 wt=$2 at=$3 head=$4; shift 4
g() { git -C "$wt" -c core.hooksPath=/dev/null -c user.name=ap-case -c user.email=ap-case@localhost "$@"; }
tree=$(git -C "$clone" rev-parse --verify -q "$at^{tree}")
git init -q "$wt"
{ git -C "$clone" rev-list --objects "$tree"; for p; do git -C "$clone" rev-parse --verify -q "$head:$p"; done; } |
  git -C "$clone" pack-objects -q --stdout | g unpack-objects -q
g read-tree "$tree"
for p; do
  entry=$(git -C "$clone" ls-tree "$head" -- "$p"); mode=${entry%% *}; blob=${entry#* blob }
  g update-index --add --cacheinfo "$mode,${blob%%$'\t'*},$p"
done
g checkout-index -a -f
commit=$(g commit-tree -m "case at $at" "$(g write-tree)")
g update-ref HEAD "$commit"
echo "$commit""#;
/// The candidate's difference from the copy's commit: new files too, ignored files not.
const DIFF: &str = r#"set -euo pipefail -o noclobber
export GIT_NO_REPLACE_OBJECTS=1
cd "$1"; git add -A; git diff --cached --binary "$2" > "$3""#;

const STAGE: &str = "account model effort prompt permission_mode timeout_seconds next_accounts env resume runtime compact_window";
/// The files `ap run` writes beside a stage's answer.
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
type End = Result<&'static str, Stop>;
/// Ends the step with `cause` (and `detail`) unless `condition` holds.
macro_rules! ensure {
    ($condition:expr => $cause:expr, $detail:expr) => {
        if !$condition {
            return Err(stop($cause, $detail));
        }
    };
}

fn absolute(p: &str) -> bool {
    let path = Path::new(p);
    path.is_absolute() && !p.contains('\0') && !path.components().any(|c| c == Component::ParentDir)
}
fn path<'a>(r: &'a Value, key: &str, field: &str) -> Result<&'a str, Failure> {
    let p = r[key].as_str().filter(|p| absolute(p));
    p.ok_or_else(|| error("invalid_input", field))
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
/// The `ap run` request of the implementer stage, the same when it is admitted and when it starts.
fn stage_request(r: &Value, prompt: &str) -> Value {
    let (mut req, out) = (
        r["implementer"].clone(),
        r["out"].as_str().unwrap_or_default(),
    );
    (req["cwd"], req["prompt"]) = (r["worktree"].clone(), json!(prompt));
    req["out"] = json!(format!("{out}.implementer.md"));
    req["role"] = json!("implementer");
    // An author stage commits under its own name, as in `ap unit run` (`specs/ap/run-object.md` R5).
    req["git_author"] = json!(true);
    // Only a stage on Claude Code has a default mode: a stage on another runtime states its own or is refused.
    let claude = matches!(req["runtime"].as_str(), None | Some("claude"));
    if req.get("permission_mode").is_none() && claude {
        req["permission_mode"] = json!("auto");
    }
    req
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
        match self.rec["steps"].as_array_mut() {
            Some(steps) => steps.push(step),
            None => self.rec["steps"] = json!([step]),
        }
        self.save();
    }
    fn step(&mut self, name: &str, f: impl FnOnce(&mut Self) -> Step) -> Result<(), Stop> {
        self.begin(name);
        let detail = f(self)?;
        self.end("passed", detail);
        Ok(())
    }
}
fn unchanged(rec: &Value, path: &str) -> Result<(), Stop> {
    let now = regular(path, "digest_differs", "file").map(|b| digest(&b));
    ensure!(now.ok().as_deref() == rec["sha256"].as_str() => "digest_differs", json!({"path": path}));
    Ok(())
}
/// One `ap run` stage; its record's path goes into the record before its state is judged.
fn stage_run(s: &mut impl System, u: &mut Unit, which: &str, r: Value) -> Step {
    // The record's path is on disk before the stage starts: a killed call names it.
    u.rec["stages"][which] = json!(format!("{}.run.json", text(&r, "out")?));
    u.save();
    let refused = |f: Failure| stop("stage_refused", json!({"code": f.code, "field": f.field}));
    let (rec, status) = crate::run::execute(s, &r).map_err(refused)?;
    let state = rec["state"].as_str().unwrap_or("failed");
    ensure!(state == "ok" => state, Value::Null);
    // `ok` with an exit that is not 0: the stage's final record could not be written.
    ensure!(status == 0 => "stage_record_unwritten", Value::Null);
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
fn create(path: &str, bytes: &[u8]) -> Result<(), Stop> {
    let mut options = std::fs::OpenOptions::new();
    let created = options.write(true).create_new(true).open(path);
    let written = created.and_then(|mut f| f.write_all(bytes));
    written.map_err(|_| stop("io_error", json!({"path": path})))
}
/// The claim, the steps and the end of a case call.
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

fn hex(v: &Value, n: usize) -> bool {
    let ok = |s: &str| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    v.as_str().is_some_and(ok)
}
/// A nonempty array whose every item is a nonempty string that `each` admits.
fn strings(v: &Value, each: fn(&str) -> bool) -> bool {
    let item = |w: &Value| {
        w.as_str()
            .is_some_and(|w| !w.is_empty() && !w.contains('\0') && each(w))
    };
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.iter().all(item))
}
fn relative(p: &str) -> bool {
    let normal = |c: Component| matches!(c, Component::Normal(_));
    Path::new(p).components().all(normal)
}
fn command(v: &Value) -> bool {
    strings(v, |_| true)
}
fn texts(v: &Value) -> impl Iterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}
/// The case file: exactly its members, each well formed, named after its id.
fn read_case(file: &str) -> Result<(Value, String), Failure> {
    let bytes = regular(file, "case_unreadable", "case")?;
    let c: Value = serde_json::from_slice(&bytes).map_err(|_| error("case_unreadable", "case"))?;
    let o = c
        .as_object()
        .ok_or_else(|| error("case_unreadable", "case"))?;
    let id = c["id"].as_str().unwrap_or_default();
    let id_char =
        |(i, b): (usize, u8)| b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && b == b'-');
    let id_ok = (1..=64).contains(&id.len()) && id.bytes().enumerate().all(id_char);
    let named = Path::new(file).file_name().and_then(|n| n.to_str()) == Some(&format!("{id}.json"));
    let repo = c["repository"].as_str().and_then(|r| r.split_once('/'));
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    let ok = o.len() == 8
        && CASE.split(' ').all(|k| o.contains_key(k))
        && id_ok
        && named
        && repo.is_some_and(|(a, b)| part(a) && part(b))
        && c["pull_request"].as_u64().is_some_and(|n| n > 0)
        && hex(&c["base"], 40)
        && hex(&c["head"], 40)
        && hex(&c["page_sha256"], 64)
        && strings(&c["tests"], relative)
        && c["commands"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(command));
    require!(ok => "case_unreadable", "case");
    Ok((c, digest(&bytes)))
}
/// The block of facts appended to the request's prompt.
fn facts(r: &Value, c: &Value) -> String {
    let tests: String = texts(&c["tests"]).map(|t| format!("- {t}\n")).collect();
    let commands = c["commands"].as_array().into_iter().flatten();
    let commands: String = commands.map(|v| format!("{v}\n")).collect();
    let (page, wt) = (&r["page"], &r["worktree"]);
    format!(
        "\n## Facts of this case (ap unit)\n\nPage: {}\nCopy: {}\nTest files (restored before your work is judged):\n{tests}Commands, in order, each an argument array run in the copy:\n{commands}The copy has no remote.\n",
        page.as_str().unwrap_or_default(),
        wt.as_str().unwrap_or_default()
    )
}
/// The checks before any effect; the answer is the case and its digest.
fn request(s: &mut impl System, r: &Value) -> Result<(Value, String), Failure> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.split(' ').any(|m| m == *k)) {
        return Err(error("invalid_input", k));
    }
    let p = |k: &str| path(r, k, k);
    let (file, clone, worktree) = (p("case")?, p("clone")?, p("worktree")?);
    let (page, out, acceptance) = (p("page")?, p("out")?, p("acceptance")?);
    let staged = r.get("implementer").is_some();
    let at = r.get("at").map_or(Some("base"), Value::as_str);
    // No stage ever works in a copy that holds the head.
    require!(at == Some("base") || (at == Some("head") && !staged) => "invalid_input", "at");
    seconds(r)?;
    if staged {
        stage(r, "implementer")?;
        require!(r["implementer"].get("resume").is_none() => "invalid_input", "implementer.resume");
        // A stage that is asked for is admitted or refused, never dropped.
        require!(r["implementer"]["prompt"].is_string() => "invalid_input", "implementer.prompt");
    }
    let (c, sha) = read_case(file)?;
    let root = s.work_root();
    for (k, p) in [("page", page), ("out", out)] {
        require!(Path::new(p).starts_with(&root) => "outside_work_root", k);
    }
    require!(!Path::new(acceptance).starts_with(worktree) => "inside_worktree", "acceptance");
    let bytes = regular(page, "page_unreadable", "page")?;
    require!(c["page_sha256"] == digest(&bytes).as_str() => "case_page_differs", "page");
    if let Some(lead) = r["implementer"]["prompt"].as_str() {
        let field = |f: Failure| fail(f.code, &format!("implementer.{}", f.field), f.detail);
        crate::run::admit_stage(s, &stage_request(r, lead)).map_err(field)?;
        let size = std::fs::metadata(lead).map_or(LIMIT, |m| m.len());
        require!(size + (facts(r, &c).len() as u64) < LIMIT => "prompt_unreadable", "implementer.prompt");
    }
    executable(acceptance)?;
    let has = |s: &mut _, what: &str| crate::review::git(s, clone, &["cat-file", "-t", what]);
    let (base, head) = (
        c["base"].as_str().unwrap_or_default(),
        c["head"].as_str().unwrap_or_default(),
    );
    for (k, commit) in [("case.base", base), ("case.head", head)] {
        require!(has(s, &format!("{commit}^{{commit}}")).is_ok() => "case_commit_missing", k);
    }
    // A judging file is a regular file at `head`: a link's target would be the candidate's.
    for t in texts(&c["tests"]) {
        let entry = crate::review::git(s, clone, &["ls-tree", head, "--", t]).unwrap_or_default();
        let regular = ["100644 blob ", "100755 blob "]
            .iter()
            .any(|m| entry.starts_with(m));
        require!(regular && entry.trim_end().ends_with(&format!("\t{t}")) => "case_commit_missing", "case.tests");
    }
    for suffix in FILES.split(' ') {
        for extra in RUN_FILES {
            let file = format!("{out}{suffix}{extra}");
            require!(Path::new(&file).symlink_metadata().is_err() => "out_exists", "out");
        }
    }
    require!(Path::new(worktree).symlink_metadata().is_err() => "worktree_exists", "worktree");
    Ok((c, sha))
}

/// The copy is still the directory this call made: a link put there (by a process of the candidate)
/// would send the evaluator, the diff and the removal to another place.
fn still_a_directory(wt: &str) -> Result<(), Stop> {
    match std::fs::symlink_metadata(wt) {
        Ok(m) if m.is_dir() => Ok(()),
        _ => Err(stop("copy_replaced", Value::Null)),
    }
}
fn steps(s: &mut impl System, r: &Value, c: &Value, u: &mut Unit) -> End {
    let f = |k: &str| r[k].as_str().unwrap_or_default();
    let (clone, wt, out, acceptance) = (f("clone"), f("worktree"), f("out"), f("acceptance"));
    let at = r["at"].as_str().unwrap_or("base");
    let mut commit = String::new();
    u.step("worktree", |_| {
        let (from, head) = (
            c[at].as_str().unwrap_or_default(),
            c["head"].as_str().unwrap_or_default(),
        );
        let mut words = vec!["-c", COPY, "bash", clone, wt, from, head];
        words.extend(texts(&c["tests"]));
        let reply = s.call("bash", &args(&words), &Value::Null, 1_800_000);
        let copied = reply.ok().filter(|r| r.success).map(|r| r.stdout);
        let copied = copied.map(|b| String::from_utf8_lossy(&b).trim().to_owned());
        commit = copied
            .filter(|c| c.len() == 40)
            .ok_or_else(|| stop("copy_failed", Value::Null))?;
        Ok(json!({"commit": commit}))
    })?;
    let mut failed = None;
    if let Some(lead) = r["implementer"]["prompt"].as_str() {
        u.begin("implementer");
        unchanged(&u.rec["prompt"], lead)?;
        let mut bytes = regular(lead, "digest_differs", "implementer.prompt")?;
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend(facts(r, c).as_bytes());
        let prompt = format!("{out}.implementer.prompt.md");
        create(&prompt, &bytes)?;
        u.rec["files"]["report"] = json!(format!("{out}.implementer.md"));
        let req = stage_request(r, &prompt);
        match stage_run(s, u, "implementer", req) {
            Ok(detail) => u.end("passed", detail),
            Err(Stop { cause, .. }) if cause == "timeout" => {
                failed = Some(json!({"stage": "timeout"}));
                u.end("failed", failed.clone().unwrap_or_default());
            }
            Err(stopped) => return Err(stopped),
        }
    }
    if failed.is_none() {
        u.begin("acceptance");
        still_a_directory(wt)?;
        unchanged(&u.rec["acceptance"], acceptance)?;
        // The case file names the judging files and the commands: the stage cannot change it.
        let file = r["case"].as_str().unwrap_or_default();
        unchanged(&u.rec["case"], file)?;
        let log = format!("{out}.accept.log");
        u.rec["files"]["acceptance_output"] = json!(log);
        let exit = accept(s, wt, acceptance, &log, seconds(r)?, &[file, clone]);
        let detail = json!({"exit": exit});
        if exit == Some(0) {
            u.end("passed", detail);
        } else if exit == Some(1) || exit == Some(124) {
            u.end("failed", detail.clone());
            failed = Some(detail);
        } else {
            return Err(stop("acceptance_failed", detail));
        }
    }
    u.begin("cleanup");
    still_a_directory(wt)?;
    let diff = format!("{out}.candidate.diff");
    let a = args(&["-c", DIFF, "bash", wt, &commit, &diff]);
    let written = s
        .call("bash", &a, &Value::Null, 600_000)
        .is_ok_and(|r| r.success);
    u.rec["files"]["candidate_diff"] = json!(diff);
    let removed = std::fs::remove_dir_all(wt).is_ok();
    let detail = json!({"diff": written, "copy": if removed { "removed" } else { "failed" }});
    let result = if written && removed {
        "passed"
    } else {
        "failed"
    };
    u.end(result, detail);
    u.rec["detail"] = failed.clone().unwrap_or_default();
    Ok(if failed.is_some() { "failed" } else { "passed" })
}

/// One case call. A refusal (exit 2) comes before any effect and writes no file.
pub(super) fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let (c, sha) = request(s, r)?;
    let file =
        |p: &str| regular(p, "io_error", "file").map(|b| json!({"path": p, "sha256": digest(&b)}));
    let lead = r["implementer"]["prompt"].as_str();
    // The bound in effect: the stage's own, or `ap run`'s default of the implementer (its first role).
    let bound = r["implementer"]["timeout_seconds"].as_u64();
    let stage_seconds = lead.map(|_| bound.unwrap_or(crate::run::ROLES[0].1));
    let rec = json!({"mode": "case", "repository": c["repository"], "clone": r["clone"], "worktree": r["worktree"],
        "branch": null, "task_key": null, "base": c["base"], "page": file(r["page"].as_str().unwrap_or_default())?,
        "write_set": null, "acceptance": file(r["acceptance"].as_str().unwrap_or_default())?,
        "verifier_prompt": null, "verdict_file": null, "merge": null, "pull_request": null, "head": null,
        "review": null, "merge_commit": null, "stages": {"implementer": null, "verifier": null},
        "files": {"gate": null, "acceptance_output": null, "report": null, "candidate_diff": null},
        "case": {"path": r["case"], "sha256": sha, "id": c["id"]}, "at": r["at"].as_str().unwrap_or("base"),
        "prompt": lead.map(file).transpose()?, "stage_seconds": stage_seconds, "state": "running",
        "step": null, "stopped_at": null, "cause": null, "detail": null, "steps": [], "start": now(), "end": null});
    claimed(r, rec, |u| steps(s, r, &c, u))
}
