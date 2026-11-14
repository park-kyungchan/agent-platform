//! One unit of work from the page to a judged head: implementer, the reading of the pull request,
//! the gate, the reviewer, then rounds of fixer, gate and judgment. One record `<out>/unit.json`,
//! replaced at every step; the step a continuation takes is kept in it (`next`). No store call.
use crate::run::{stamp, write};
use crate::work::{Failure, LIMIT, System, args, digest, error, fail, require, text};
use serde_json::{Value, json};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{ffi::OsString, io::Read, io::Write, path::Path, process::ExitCode, time::Duration};

const VERDICT: &str = include_str!("../unit/verdict.schema.json");
const FIX: &str = include_str!("../unit/fix.schema.json");
const IMPLEMENTER: &str = include_str!("../unit/implementer.md");
const REVIEWER: &str = include_str!("../unit/reviewer.md");
const FIXER: &str = include_str!("../unit/fixer.md");
const JUDGE: &str = include_str!("../unit/judge.md");
const CONTINUE: &str = include_str!("../unit/continue.md");
const MEMBERS: [&str; 9] = [
    "repository",
    "cwd",
    "out",
    "implementer",
    "reviewer",
    "bound_minutes",
    "gate",
    "continue",
    "findings",
];
const ROLE: [&str; 6] = [
    "account",
    "model",
    "effort",
    "prompt",
    "permission_mode",
    "timeout_seconds",
];
/// The closed list of ends and their exit codes.
const ENDS: [(&str, u8); 12] = [
    ("accepted", 0),
    ("blocking_left", 20),
    ("blocking_refused", 21),
    ("check_failed", 22),
    ("no_pull_request", 23),
    ("merged_or_closed", 24),
    ("head_moved", 25),
    ("stage_failed", 26),
    ("usage_limit", 27),
    ("bound", 28),
    ("github_unreadable", 29),
    ("stopped", 30),
];
/// The variables the chain's unit gets from its starter (by name: systemd copies the values).
const PASS: [&str; 14] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "TERM",
    "TMPDIR",
    "TZ",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_CONFIG_DIR",
    "GH_HOST",
];
const BY_BRANCH: &str = "query($owner:String!,$name:String!,$branch:String!){repository(owner:$owner,name:$name){pullRequests(headRefName:$branch,states:OPEN,first:20){nodes{number state headRefOid baseRefName headRepository{nameWithOwner}}}}}";
const BY_NUMBER: &str = "query($owner:String!,$name:String!,$n:Int!){repository(owner:$owner,name:$name){pullRequest(number:$n){number state headRefOid baseRefName}}}";

/// An end of the chain: its name and what it came from.
type End = (&'static str, Value);

struct Req {
    repository: String,
    cwd: String,
    out: String,
    implementer: Value,
    reviewer: Value,
    bound: u64,
    gate: Value,
    cont: Option<String>,
    findings: Option<String>,
}

fn path(r: &Value, key: &str) -> Result<String, Failure> {
    let p = text(r, key)?;
    let plain = !p.contains(|c: char| c.is_whitespace() || c == '\0' || c == '"' || c == '\'');
    require!(Path::new(p).is_absolute() && plain => "invalid_input", key);
    Ok(p.to_owned())
}
fn role(r: &Value, key: &str) -> Result<Value, Failure> {
    let o = r[key]
        .as_object()
        .ok_or_else(|| error("invalid_input", key))?;
    require!(o.keys().all(|k| ROLE.contains(&k.as_str())) => "invalid_input", key);
    let v = &r[key];
    path(v, "account")
        .and(path(v, "prompt"))
        .map_err(|_| error("invalid_input", key))?;
    text(v, "model")
        .and(text(v, "effort"))
        .map_err(|_| error("invalid_input", key))?;
    Ok(v.clone())
}
fn parse(r: &Value) -> Result<Req, Failure> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(error("invalid_input", k));
    }
    let repository = text(r, "repository")?;
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with(['-', '.'])
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    let parts: Vec<&str> = repository.split('/').collect();
    require!(parts.len() == 2 && parts.iter().all(|p| part(p)) => "invalid_input", "repository");
    let bound = match r.get("bound_minutes") {
        None => 480,
        Some(v) => v
            .as_u64()
            .filter(|m| (10..=1440).contains(m))
            .ok_or_else(|| error("invalid_input", "bound_minutes"))?,
    };
    let gate = r.get("gate").cloned().unwrap_or(json!({}));
    let gate_ok = gate.as_object().is_some_and(|g| {
        g.iter().all(|(k, v)| match k.as_str() {
            "autoreview" => {
                v == false || v.as_str().is_some_and(|l| !l.is_empty() && l.len() <= 100)
            }
            "check_minutes" | "autoreview_minutes" => {
                v.as_u64().is_some_and(|m| (1..=1440).contains(&m))
            }
            _ => false,
        })
    });
    require!(gate_ok => "invalid_input", "gate");
    let out = path(r, "out")?;
    let cont = r.get("continue").map(|_| path(r, "continue")).transpose()?;
    require!(cont.as_deref().is_none_or(|c| *c == format!("{out}/unit.json")) => "invalid_input", "continue");
    let findings = r.get("findings").map(|_| path(r, "findings")).transpose()?;
    require!(findings.is_none() || cont.is_some() => "invalid_input", "findings");
    Ok(Req {
        repository: repository.to_owned(),
        cwd: path(r, "cwd")?,
        out,
        implementer: role(r, "implementer")?,
        reviewer: role(r, "reviewer")?,
        bound: bound * 60,
        gate,
        cont,
        findings,
    })
}
fn push(list: &mut Value, item: Value) {
    if let Some(l) = list.as_array_mut() {
        l.push(item);
    }
}
fn read_json(path: &str) -> Option<Value> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
fn save_json(path: &str, v: &Value) -> bool {
    let mut bytes = serde_json::to_vec_pretty(v).unwrap_or_default();
    bytes.push(b'\n');
    write(path, &bytes).is_ok()
}
fn code(end: &str) -> u8 {
    ENDS.iter().find(|(e, _)| *e == end).map_or(3, |(_, c)| *c)
}
fn git(s: &mut impl System, cwd: &str, a: &[&str]) -> Option<String> {
    let mut all = args(&["-C", cwd, "-c", "core.quotepath=off"]);
    all.extend(args(a));
    let r = s
        .call("git", &all, &Value::Null, 60_000)
        .ok()
        .filter(|r| r.success)?;
    String::from_utf8(r.stdout).ok()
}
/// The Lead's findings: an array of at most 20 objects of the verdict's blocking form, ids `L<n>`.
fn lead_findings(path: &str) -> Option<Vec<Value>> {
    let v = read_json(path)?;
    let schema: Value = serde_json::from_str(VERDICT).ok()?;
    let mut item = schema["properties"]["blocking"]["items"].clone();
    item["properties"]["id"]["pattern"] = json!("^L[0-9]{1,2}$");
    let list = v.as_array().filter(|l| l.len() <= 20)?;
    let ids: std::collections::BTreeSet<_> = list.iter().map(|f| f["id"].as_str()).collect();
    let ok = ids.len() == list.len() && list.iter().all(|f| crate::schema::valid(&item, f));
    ok.then(|| {
        list.iter()
            .map(|f| json!({"kind": "lead", "finding": f, "id": f["id"]}))
            .collect()
    })
}

/// `ap unit`: checks the request, writes the record and starts the chain's unit.
pub fn start(s: &mut impl System, r: &Value, exe: &str) -> Result<(Value, u8), Failure> {
    let q = parse(r)?;
    let cwd = s
        .directory(&q.cwd)
        .map_err(|_| error("cwd_missing", "cwd"))?;
    let out = s
        .directory(&q.out)
        .map_err(|_| error("invalid_input", "out"))?;
    require!(!out.starts_with(&cwd) => "out_in_worktree", "out");
    let branch = git(s, &q.cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let branch = branch
        .map(|b| b.trim().to_owned())
        .filter(|b| !b.is_empty());
    let branch = branch.ok_or_else(|| error("branch_missing", "cwd"))?;
    for (key, v) in [("implementer", &q.implementer), ("reviewer", &q.reviewer)] {
        let p = s.read(v["prompt"].as_str().unwrap_or(""));
        require!(p.is_ok_and(|b| !b.is_empty() && b.len() as u64 <= LIMIT) => "prompt_unreadable", key);
    }
    require!(s.units() => "systemd_absent", "host");
    let unit = format!("ap-unit-{}.service", &digest(q.cwd.as_bytes())[..16]);
    let active = s.call(
        "systemctl",
        &args(&["is-active", &unit]),
        &Value::Null,
        10_000,
    );
    require!(!active.is_ok_and(|a| a.stdout == b"active\n") => "chain_active", "cwd");
    let file = format!("{}/unit.json", q.out);
    let before = std::fs::read(&file).ok();
    let now = stamp(SystemTime::now());
    let record = match &q.cont {
        None => {
            require!(before.is_none() => "out_exists", "out");
            json!({"repository": q.repository, "cwd": q.cwd, "branch": branch, "unit": unit,
                "state": "running", "exit": null, "step": null, "next": {"step": "implement"},
                "end_from": null, "pull_request": null, "base": null, "heads": [], "stages": [],
                "gates": [], "verdicts": [], "dispositions": [], "rounds_used": 0,
                "notes_round_used": false, "left": null, "autoreview": null, "tokens": null,
                "start": now, "finish": null, "continued": []})
        }
        Some(c) => {
            let mut rec = read_json(c).ok_or_else(|| error("record_missing", "continue"))?;
            require!(rec["cwd"] == q.cwd.as_str() && rec["repository"] == q.repository.as_str() => "record_differs", "continue");
            require!(rec["state"] != "merged_or_closed" => "chain_cannot_continue", "continue");
            let prev = rec["state"].as_str().unwrap_or("").to_owned();
            let n = rec["continued"].as_array().map_or(0, Vec::len) + 1;
            if let Some(f) = &q.findings {
                let mut items =
                    lead_findings(f).ok_or_else(|| error("findings_unreadable", "findings"))?;
                if rec["next"]["step"] == "fix" {
                    let left = rec["next"]["items"].as_str().and_then(read_json);
                    items.extend(left.and_then(|l| l.as_array().cloned()).unwrap_or_default());
                }
                let path = format!("{}/continue-{n}-items.json", q.out);
                require!(save_json(&path, &json!(items)) => "invalid_input", "out");
                rec["next"] = json!({"step": "fix", "items": path, "counts": false});
            } else if prev == "accepted" {
                rec["next"] = json!({"step": "read", "expect": null, "then": {"step": "gate"}});
            } else if rec["next"]["step"] == "fix" {
                // The first round a continuation starts is the Lead's: it uses none of the two.
                rec["next"]["counts"] = json!(false);
            }
            push(&mut rec["continued"], json!(now));
            for (k, v) in [
                ("state", json!("running")),
                ("exit", Value::Null),
                ("step", Value::Null),
                ("end_from", Value::Null),
                ("finish", Value::Null),
                ("unit", json!(unit)),
                ("branch", json!(branch)),
            ] {
                rec[k] = v;
            }
            rec
        }
    };
    let saved = save_json(&format!("{}/unit.request.json", q.out), r) && save_json(&file, &record);
    require!(saved => "invalid_input", "out");
    let mut a = args(&[
        "--quiet",
        "--collect",
        &format!("--unit={unit}"),
        "--slice=agents.slice",
        "-p",
        &format!("RuntimeMaxSec={}s", q.bound + 300),
        "-p",
        "TimeoutStopSec=30s",
        "-p",
        &format!("ExecStopPost={exe} unit finish {}", q.out),
    ]);
    let env = s.environment();
    a.extend(
        PASS.iter()
            .filter(|k| env.iter().any(|(n, _)| n == *k))
            .map(|k| format!("--setenv={k}")),
    );
    a.extend(args(&["--", exe, "unit", "chain", &q.out, &unit]));
    let launched = s
        .call("systemd-run", &a, &Value::Null, 60_000)
        .is_ok_and(|r| r.success);
    let answer = json!({"record": file, "unit": unit, "started": launched});
    if !launched {
        // The record as it was: removed for a fresh request.
        let _ = match before {
            Some(b) => write(&file, &b).map_err(|_| ()),
            None => std::fs::remove_file(&file).map_err(|_| ()),
        };
        return Ok((answer, 3));
    }
    Ok((answer, 0))
}

struct Chain<'a, S: System> {
    s: &'a mut S,
    q: Req,
    rec: Value,
    file: String,
    unit: String,
    t0: Instant,
    wall0: u64,
    deadline: u64,
}
impl<S: System> Chain<'_, S> {
    fn wall(&self) -> u64 {
        self.wall0 + self.s.now().duration_since(self.t0).as_secs()
    }
    fn save(&mut self) {
        let sum = |k: &str| {
            self.rec["stages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|st| st["tokens"][k].as_u64())
                .sum::<u64>()
        };
        self.rec["tokens"] = json!({"input": sum("input"), "output": sum("output"),
            "cache_read": sum("cache_read"), "cache_creation": sum("cache_creation")});
        let _ = save_json(&self.file, &self.rec);
    }
    fn last_stage(&self, pick: &dyn Fn(&Value) -> bool) -> Option<Value> {
        self.rec["stages"]
            .as_array()
            .and_then(|l| l.iter().rev().find(|st| pick(st)).cloned())
    }
    /// Runs one stage, or resumes the latest stage of this step when it is not `ok`; sleeps to a
    /// reset inside the bound. Ok: the stage's index and its structured answer (null without one).
    fn stage(
        &mut self,
        step: &str,
        role: &str,
        prompt: String,
        again: &str,
        resume: Option<Value>,
        schema: Option<&str>,
    ) -> Result<(usize, Value), End> {
        loop {
            let last = self.last_stage(&|_| true);
            let redo = last
                .as_ref()
                .filter(|l| l["step"] == step && l["state"] != "ok");
            let (prompt, resume) = match redo {
                Some(l) => (format!("{CONTINUE}{again}"), Some(l["record"].clone())),
                None => (prompt.clone(), resume.clone()),
            };
            let left = self.deadline.saturating_sub(self.wall());
            if left < 180 {
                return Err(("bound", json!({"step": step})));
            }
            let n = self.rec["stages"].as_array().map_or(0, Vec::len) + 1;
            let (out, file) = (
                format!("{}/{n}-{role}.md", self.q.out),
                format!("{}/{n}-{role}.prompt.md", self.q.out),
            );
            let set = if step == "verdict" {
                &self.q.reviewer
            } else {
                &self.q.implementer
            };
            let default = match role {
                "implementer" => 5_400,
                "fixer" => 3_600,
                _ => 3_300,
            };
            let timeout = set["timeout_seconds"]
                .as_u64()
                .unwrap_or(default)
                .min(left - 120);
            let mut req = json!({"role": if role == "judge" { "reviewer" } else { role },
                "account": set["account"], "model": set["model"], "effort": set["effort"],
                "permission_mode": set.get("permission_mode").unwrap_or(&json!("auto")),
                "cwd": self.q.cwd, "prompt": file, "out": out, "timeout_seconds": timeout, "bind": self.unit});
            if let Some(r) = resume {
                req["resume"] = json!({"record": r});
            }
            if let Some(sc) = schema {
                req["schema"] = json!(format!("{}/{sc}", self.q.out));
            }
            if write(&file, prompt.as_bytes()).is_err() {
                return Err(("stage_failed", json!({"stage": n, "refused": "io_error"})));
            }
            push(
                &mut self.rec["stages"],
                json!({"step": step, "role": role,
                "record": format!("{out}.run.json"), "state": "running", "turns": null, "tokens": null, "head": null}),
            );
            self.rec["step"] = json!(step);
            self.save();
            let i = n - 1;
            let run = match crate::run::execute(self.s, &req) {
                Ok((run, _)) => run,
                Err(f) => {
                    self.rec["stages"][i]["state"] = json!("failed");
                    self.rec["stages"][i]["refused"] = json!(f.code);
                    return Err(("stage_failed", json!({"stage": n, "refused": f.code})));
                }
            };
            for k in ["state", "turns", "tokens"] {
                self.rec["stages"][i][k] = run[k].clone();
            }
            match run["state"].as_str() {
                Some("ok") => {
                    let answer = schema.and_then(|_| read_json(&out)).unwrap_or(Value::Null);
                    return Ok((n, answer));
                }
                Some("cut") => match run["resets_at"].as_u64() {
                    Some(t) if t + 60 <= self.deadline => {
                        self.rec["step"] = json!("sleep");
                        self.save();
                        let wait = (t + 60).saturating_sub(self.wall());
                        self.s.pause(Duration::from_secs(wait));
                    }
                    t => return Err(("usage_limit", json!({"stage": n, "resets_at": t}))),
                },
                _ => return Err(("stage_failed", json!({"stage": n, "state": run["state"]}))),
            }
        }
    }
    /// A stage whose answer the chain does not take: its state is `no_result`.
    fn no_result(&mut self, n: usize, why: &str) -> End {
        self.rec["stages"][n - 1]["state"] = json!("no_result");
        self.rec["stages"][n - 1]["unmet"] = json!(why);
        (
            "stage_failed",
            json!({"stage": n, "state": "no_result", "unmet": why}),
        )
    }
    fn graphql(&mut self, query: &str, var: &str) -> Option<Value> {
        let (owner, name) = self.q.repository.split_once('/')?;
        let flag = if var.starts_with("n=") { "-F" } else { "-f" };
        let a = args(&[
            "api",
            "graphql",
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
            flag,
            var,
            "-f",
            &format!("query={query}"),
        ]);
        for i in 0..3 {
            if i > 0 {
                self.s.pause(Duration::from_secs(30));
            }
            let reply = self.s.call("gh", &a, &Value::Null, 60_000);
            let v: Option<Value> = reply
                .ok()
                .filter(|r| r.success)
                .and_then(|r| serde_json::from_slice(&r.stdout).ok());
            if let Some(repo) = v
                .as_ref()
                .and_then(|v| v.pointer("/data/repository"))
                .filter(|r| r.is_object())
            {
                return Some(repo.clone());
            }
        }
        None
    }
    /// The reading of the pull request after a stage: open, and at the head the stage named.
    fn read(&mut self, expect: &Value) -> Result<(), End> {
        let from = json!({"reading": "pull_request"});
        let pr = match self.rec["pull_request"].as_u64() {
            Some(n) => self
                .graphql(BY_NUMBER, &format!("n={n}"))
                .map(|r| r["pullRequest"].clone()),
            None => {
                let branch = format!("branch={}", self.rec["branch"].as_str().unwrap_or(""));
                let nodes = self
                    .graphql(BY_BRANCH, &branch)
                    .map(|r| r["pullRequests"]["nodes"].clone());
                match nodes {
                    None => None,
                    Some(l) => {
                        let mine = |p: &&Value| {
                            p["headRepository"]["nameWithOwner"]
                                .as_str()
                                .is_some_and(|n| n.eq_ignore_ascii_case(&self.q.repository))
                        };
                        let pr = l.as_array().and_then(|l| l.iter().find(mine).cloned());
                        Some(pr.ok_or(("no_pull_request", from.clone()))?)
                    }
                }
            }
        };
        let pr = pr
            .filter(|p| p.is_object())
            .ok_or(("github_unreadable", from.clone()))?;
        if pr["state"] != "OPEN" {
            return Err((
                "merged_or_closed",
                json!({"reading": "pull_request", "state": pr["state"]}),
            ));
        }
        let head = pr["headRefOid"].clone();
        self.rec["pull_request"] = pr["number"].clone();
        self.rec["base"] = pr["baseRefName"].clone();
        if self.rec["heads"].as_array().and_then(|h| h.last()) != Some(&head) {
            push(&mut self.rec["heads"], head.clone());
        }
        if !expect.is_null() && *expect != head {
            return Err((
                "head_moved",
                json!({"reading": "pull_request", "expected": expect, "head_now": head}),
            ));
        }
        Ok(())
    }
    fn gate(&mut self) -> Result<Value, End> {
        let left = self.deadline.saturating_sub(self.wall()) / 60;
        if left < 1 {
            return Err(("bound", json!({"step": "gate"})));
        }
        let head = self.rec["heads"]
            .as_array()
            .and_then(|h| h.last())
            .cloned()
            .unwrap_or_default();
        let mut req = json!({"repository": self.q.repository, "pull_request": self.rec["pull_request"], "head": head});
        if let Some(l) = self.q.gate.get("autoreview") {
            req["autoreview"] = l.clone();
        }
        let mut clipped = false;
        for (k, d) in [("check_minutes", 90), ("autoreview_minutes", 40)] {
            let m = self.q.gate[k].as_u64().unwrap_or(d);
            clipped |= m > left;
            req[k] = json!(m.min(left));
        }
        self.rec["step"] = json!("gate");
        self.save();
        let n = self.rec["gates"].as_array().map_or(0, Vec::len) + 1;
        let from = json!({"gate": n});
        let (a, _) =
            crate::gate::execute(self.s, &req).map_err(|_| ("github_unreadable", from.clone()))?;
        push(&mut self.rec["gates"], json!({"head": head, "answer": a}));
        self.rec["autoreview"] = a["autoreview"]["state"].clone();
        match a["end"].as_str().unwrap_or("") {
            "passed" => Ok(json!({"step": "verdict"})),
            "check_failed" => {
                let c = &a["failed_check"];
                let items = json!([{"id": "check", "kind": "check", "name": c["name"], "state": c["state"], "run": c["run"]}]);
                Ok(json!({"step": "fix", "items": self.items(&items)?, "counts": true}))
            }
            // A check still pending when the chain's own bound ended the gate: the bound's end.
            "check_timeout" if clipped => Err(("bound", from)),
            "check_timeout" => Err(("check_failed", from)),
            "merged" | "closed" => Err(("merged_or_closed", from)),
            "head_differs" | "head_moved" => Err(("head_moved", from)),
            _ => Err(("github_unreadable", from)),
        }
    }
    fn items(&mut self, items: &Value) -> Result<String, End> {
        let count = |k: &str| self.rec[k].as_array().map_or(0, Vec::len);
        let p = format!(
            "{}/items-{}-{}-{}.json",
            self.q.out,
            count("stages"),
            count("gates"),
            count("continued")
        );
        save_json(&p, items)
            .then_some(p)
            .ok_or(("stage_failed", json!({"items": "io_error"})))
    }
    fn fill(&self, template: &str, more: &[(&str, String)]) -> String {
        let gate = self.rec["gates"]
            .as_array()
            .and_then(|g| g.last())
            .cloned()
            .unwrap_or_default();
        let a = &gate["answer"];
        let checks: Vec<Value> = a["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| json!({"name": c["name"], "state": c["state"]}))
            .collect();
        let mut t = template.to_owned();
        let facts = [
            ("repository", self.q.repository.clone()),
            ("cwd", self.q.cwd.clone()),
            (
                "branch",
                self.rec["branch"].as_str().unwrap_or("").to_owned(),
            ),
            ("pull_request", self.rec["pull_request"].to_string()),
            ("head", gate["head"].as_str().unwrap_or("").to_owned()),
            ("base", self.rec["base"].as_str().unwrap_or("").to_owned()),
            ("checks", json!(checks).to_string()),
            (
                "autoreview",
                format!(
                    "{} (unresolved findings: {})",
                    a["autoreview"]["state"], a["autoreview"]["has_unresolved_findings"]
                ),
            ),
            ("threads", a["threads"].to_string()),
        ];
        for (k, v) in facts.iter().chain(
            more.iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>()
                .iter(),
        ) {
            t = t.replace(&format!("{{{k}}}"), v);
        }
        t
    }
    /// The program's list of test files the diff `from..to` changed or deleted.
    fn tests(&mut self, from: &str, to: &str) -> Value {
        let Some(diff) = git(
            self.s,
            &self.q.cwd,
            &["diff", "--no-renames", "--no-color", "-U0", from, to],
        ) else {
            return json!("unreadable");
        };
        let (mut list, mut file, mut header) = (Vec::<String>::new(), None::<String>, false);
        let named = |p: &str| {
            let name = p.rsplit('/').next().unwrap_or(p);
            p.split('/')
                .rev()
                .skip(1)
                .any(|d| ["tests", "test", "fixtures"].contains(&d))
                || name.starts_with("test_")
                || name.starts_with("test-")
                || ["_test.", ".test.", "_tests."]
                    .iter()
                    .any(|m| name.contains(m))
        };
        for line in diff.lines() {
            if line.starts_with("diff --git ") {
                (header, file) = (true, None);
            } else if line.starts_with("@@") {
                header = false;
            } else if header && let Some(p) = line.strip_prefix("--- a/") {
                file = Some(p.to_owned());
                if named(p) && !list.iter().any(|f| f == p) {
                    list.push(p.to_owned());
                }
            } else if !header
                && line.starts_with('-')
                && (line.contains("#[test]") || line.contains("assert"))
                && let Some(f) = file.as_ref().filter(|f| !list.contains(f))
            {
                list.push(f.clone());
            }
        }
        json!(list)
    }
    fn verdict(&mut self) -> Result<Value, End> {
        let gate = self.rec["gates"]
            .as_array()
            .and_then(|g| g.last())
            .cloned()
            .unwrap_or_default();
        let head = gate["head"].as_str().unwrap_or("").to_owned();
        let judged = self.last_stage(&|st| st["step"] == "verdict" && st["state"] == "ok");
        let (n, v) = match &judged {
            None => {
                let lead =
                    std::fs::read_to_string(self.q.reviewer["prompt"].as_str().unwrap_or(""))
                        .unwrap_or_default();
                let prompt = format!("{lead}{}", self.fill(REVIEWER, &[]));
                self.stage(
                    "verdict",
                    "reviewer",
                    prompt,
                    "",
                    None,
                    Some("verdict.schema.json"),
                )?
            }
            Some(prev) => {
                let from = self.rec["verdicts"]
                    .as_array()
                    .and_then(|l| l.last())
                    .map_or(json!(""), |v| v["head"].clone());
                let from = from.as_str().unwrap_or("").to_owned();
                let since = prev["record"].clone();
                let n0 = self.rec["verdicts"]
                    .as_array()
                    .and_then(|l| l.last())
                    .and_then(|v| v["stage"].as_u64())
                    .unwrap_or(0);
                let disp: Vec<Value> = self.rec["dispositions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|d| d["stage"].as_u64() > Some(n0))
                    .cloned()
                    .collect();
                let reasons: Vec<Value> = disp
                    .iter()
                    .map(|d| {
                        let fix = read_json(&format!("{}/{}-fixer.md", self.q.out, d["stage"]));
                        let r = fix.and_then(|f| {
                            f["dispositions"]
                                .as_array()
                                .and_then(|l| l.iter().find(|x| x["id"] == d["id"]).cloned())
                        });
                        r.unwrap_or_else(|| d.clone())
                    })
                    .collect();
                let tests = self.tests(&from, &head);
                let prompt = self.fill(
                    JUDGE,
                    &[
                        ("dispositions", json!(reasons).to_string()),
                        ("from", from),
                        ("tests", tests.to_string()),
                    ],
                );
                self.stage(
                    "verdict",
                    "judge",
                    prompt,
                    "",
                    Some(since),
                    Some("verdict.schema.json"),
                )?
            }
        };
        let blocking = v["blocking"].as_array().cloned().unwrap_or_default();
        let notes = v["notes"].as_array().cloned().unwrap_or_default();
        if v["head"] != head.as_str() || (v["verdict"] == "accept") != blocking.is_empty() {
            return Err(self.no_result(n, "verdict"));
        }
        self.rec["stages"][n - 1]["head"] = json!(head);
        push(
            &mut self.rec["verdicts"],
            json!({"stage": n, "head": head,
            "verdict": v["verdict"], "blocking": blocking.len(), "notes": notes.len()}),
        );
        let threads: Vec<Value> = gate["answer"]["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["answered"] == false)
            .map(|t| json!({"id": t["id"], "kind": "thread", "path": t["path"], "line": t["line"]}))
            .collect();
        let mut items: Vec<Value> = blocking
            .iter()
            .map(|b| json!({"id": b["id"], "kind": "blocking", "finding": b}))
            .collect();
        items.extend(
            notes
                .iter()
                .map(|x| json!({"id": x["id"], "kind": "note", "note": x["text"]})),
        );
        items.extend(threads.iter().cloned());
        let first = self.rec["verdicts"].as_array().map_or(0, Vec::len) == 1;
        let then = if !blocking.is_empty() {
            json!({"step": "fix", "items": self.items(&json!(items))?, "counts": true})
        } else if !items.is_empty() && first && self.rec["notes_round_used"] == false {
            self.rec["notes_round_used"] = json!(true);
            json!({"step": "fix", "items": self.items(&json!(items))?, "counts": false})
        } else {
            let refused: Vec<Value> = self.rec["dispositions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["disposition"] == "refused")
                .map(|d| json!({"stage": d["stage"], "id": d["id"], "kind": d["kind"]}))
                .collect();
            let notes: Vec<Value> = notes
                .iter()
                .map(|x| json!({"stage": n, "id": x["id"]}))
                .collect();
            self.rec["left"] = json!({"notes": notes, "threads": threads, "refused": refused});
            json!({"step": "end", "end": "accepted"})
        };
        Ok(json!({"step": "read", "expect": head, "then": then}))
    }
    fn fix(&mut self, next: &Value) -> Result<Value, End> {
        let items = next["items"]
            .as_str()
            .and_then(read_json)
            .unwrap_or(json!([]));
        let list = items.as_array().cloned().unwrap_or_default();
        let blocking =
            |i: &Value| ["blocking", "lead", "check"].contains(&i["kind"].as_str().unwrap_or(""));
        if next["counts"] == true {
            if self.rec["rounds_used"].as_u64() >= Some(2) {
                let end = if list.iter().any(|i| i["kind"] != "check" && blocking(i)) {
                    "blocking_left"
                } else {
                    "check_failed"
                };
                return Err((end, json!({"rounds_used": 2})));
            }
            self.rec["rounds_used"] = json!(self.rec["rounds_used"].as_u64().unwrap_or(0) + 1);
            self.rec["next"]["counts"] = json!(false);
        }
        let line = format!("\nItems: {items}\n");
        let prompt = FIXER.replace("{items}", &items.to_string());
        let session = self
            .last_stage(&|st| st["role"] == "implementer" || st["role"] == "fixer")
            .map(|st| st["record"].clone());
        let (n, v) = self.stage(
            "fix",
            "fixer",
            prompt,
            &line,
            session,
            Some("fix.schema.json"),
        )?;
        let given: Vec<&Value> = v["dispositions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| &d["id"])
            .collect();
        let all = given.len() == list.len()
            && list
                .iter()
                .all(|i| given.iter().filter(|g| ***g == i["id"]).count() == 1);
        if !all {
            return Err(self.no_result(n, "dispositions"));
        }
        self.rec["stages"][n - 1]["head"] = v["head"].clone();
        let mut refused = false;
        for d in v["dispositions"].as_array().into_iter().flatten() {
            let item = list
                .iter()
                .find(|i| i["id"] == d["id"])
                .cloned()
                .unwrap_or_default();
            refused |= d["disposition"] == "refused" && blocking(&item);
            push(
                &mut self.rec["dispositions"],
                json!({"stage": n, "id": d["id"],
                "kind": item["kind"], "disposition": d["disposition"], "commit": d["commit"]}),
            );
        }
        let next = json!({"step": "read", "expect": v["head"], "then": {"step": "gate"}});
        if refused {
            self.rec["next"] = next;
            return Err(("blocking_refused", json!({"stage": n})));
        }
        Ok(next)
    }
    /// One step of `next`: the next step, or the end.
    fn step(&mut self, next: &Value) -> Result<Value, End> {
        if self.wall() >= self.deadline {
            return Err(("bound", json!({"step": next["step"]})));
        }
        match next["step"].as_str().unwrap_or("") {
            "implement" => {
                let lead =
                    std::fs::read_to_string(self.q.implementer["prompt"].as_str().unwrap_or(""))
                        .unwrap_or_default();
                let prompt = format!("{lead}{}", self.fill(IMPLEMENTER, &[]));
                self.stage("implement", "implementer", prompt, "", None, None)?;
                Ok(json!({"step": "read", "expect": null, "then": {"step": "gate"}}))
            }
            "read" => {
                self.rec["step"] = json!("read");
                self.read(&next["expect"])?;
                Ok(next["then"].clone())
            }
            "gate" => self.gate(),
            "verdict" => self.verdict(),
            "fix" => self.fix(next),
            "end" => Err((
                "accepted",
                json!({"stage": self.rec["verdicts"].as_array().and_then(|v| v.last()).map(|v| v["stage"].clone())}),
            )),
            _ => Err(("stage_failed", json!({"next": next}))),
        }
    }
}

/// `ap unit chain <out> <unit>`: the sequence, in the chain's own unit. Returns the final record.
pub fn chain(s: &mut impl System, out: &str, unit: &str) -> Value {
    let file = format!("{out}/unit.json");
    let (Some(r), Some(rec)) = (
        read_json(&format!("{out}/unit.request.json")),
        read_json(&file),
    ) else {
        return Value::Null;
    };
    let Ok(q) = parse(&r) else {
        return Value::Null;
    };
    let wall0 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let deadline = wall0 + q.bound;
    let t0 = s.now();
    let mut c = Chain {
        s,
        q,
        rec,
        file,
        unit: unit.to_owned(),
        t0,
        wall0,
        deadline,
    };
    for (name, body) in [("verdict.schema.json", VERDICT), ("fix.schema.json", FIX)] {
        let _ = write(&format!("{out}/{name}"), body.as_bytes());
    }
    c.save();
    let (end, from) = loop {
        let next = c.rec["next"].clone();
        match c.step(&next) {
            Ok(n) => {
                c.rec["next"] = n;
                c.save();
            }
            Err(e) => break e,
        }
    };
    match end {
        "accepted" | "merged_or_closed" => c.rec["next"] = Value::Null,
        // A head someone else pushed: a continuation gates and judges the head it reads.
        "head_moved" => {
            c.rec["next"] = json!({"step": "read", "expect": null, "then": {"step": "gate"}})
        }
        _ => (),
    }
    c.rec["state"] = json!(end);
    c.rec["exit"] = json!(code(end));
    c.rec["end_from"] = from;
    c.rec["step"] = Value::Null;
    c.rec["finish"] = json!(stamp(SystemTime::now()));
    c.save();
    c.rec
}
/// `ap unit finish <out>`: a record that still says `running` after the chain's process ended.
pub fn finish(out: &str, service_result: &str) -> Value {
    let file = format!("{out}/unit.json");
    let Some(mut rec) = read_json(&file) else {
        return Value::Null;
    };
    if rec["state"] == "running" {
        let end = if service_result == "timeout" {
            "bound"
        } else {
            "stopped"
        };
        rec["state"] = json!(end);
        rec["exit"] = json!(code(end));
        rec["end_from"] = json!({"unit": service_result});
        rec["finish"] = json!(stamp(SystemTime::now()));
        let _ = save_json(&file, &rec);
    }
    rec
}
/// `ap unit wait`: blocks until the record is final and the chain's unit has ended.
pub fn wait(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    require!(o.keys().all(|k| k == "record" || k == "timeout_seconds") => "invalid_input", "stdin");
    let file = text(r, "record")?.to_owned();
    let limit = match r.get("timeout_seconds") {
        None => 2_400,
        Some(v) => v
            .as_u64()
            .filter(|t| (1..=86_400).contains(t))
            .ok_or_else(|| error("invalid_input", "timeout_seconds"))?,
    };
    let t0 = s.now();
    loop {
        let rec = read_json(&file).ok_or_else(|| error("record_missing", "record"))?;
        let unit = rec["unit"].as_str().unwrap_or("").to_owned();
        if rec["state"] != "running" && crate::run::ended(s, &unit) {
            let end = rec["state"].as_str().unwrap_or("").to_owned();
            return Ok((rec, code(&end)));
        }
        if s.now().duration_since(t0).as_secs() >= limit {
            return Ok((rec, 8));
        }
        s.pause(Duration::from_secs(5));
    }
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let a: Vec<&str> = argv.iter().filter_map(|a| a.to_str()).collect();
    let print = |v: &Value, code: u8| -> ExitCode {
        let ok = writeln!(std::io::stdout(), "{v}").is_ok();
        ExitCode::from(if ok { code } else { 4 })
    };
    match a.as_slice() {
        ["chain", out, unit] => return print(&chain(&mut crate::work::Host, out, unit), 0),
        ["finish", out] => {
            return print(
                &finish(out, &std::env::var("SERVICE_RESULT").unwrap_or_default()),
                0,
            );
        }
        _ => (),
    }
    let result = (|| {
        require!(a.is_empty() || a == ["wait"] => "usage", "args");
        let mut bytes = vec![];
        std::io::stdin()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("io_error", "stdin"))?;
        require!(bytes.len() as u64 <= LIMIT => "input_too_large", "stdin");
        let r: Value =
            serde_json::from_slice(&bytes).map_err(|_| error("invalid_input", "stdin"))?;
        if a.is_empty() {
            let exe = std::env::current_exe().map_err(|_| error("io_error", "exe"))?;
            start(
                &mut crate::work::Host,
                &r,
                exe.to_str().ok_or_else(|| error("io_error", "exe"))?,
            )
        } else {
            wait(&mut crate::work::Host, &r)
        }
    })();
    match result {
        Ok((v, code)) => print(&v, code),
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}});
            let _ = writeln!(std::io::stderr(), "{v}");
            ExitCode::from(2)
        }
    }
}
