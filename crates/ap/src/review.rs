//! Store-owned reviews; a derived GitHub status is posted only after local checks.
use crate::work::{
    Failure, LIMIT, ROOT, System, apply_answer, args, codes, complete_id, digest, error, fail, id,
    json_cli, ont, parse, refused, require, text,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::{Component, Path},
    process::ExitCode,
};

/// The value of property `k` of an object of a search's answer.
pub(crate) fn field<'a>(o: &'a Value, k: &str) -> &'a Value {
    &o["head"]["fields"][k]["value"]
}
/// Every object of one search, over its pages; the answer must be `ok` and qualified. On the
/// engine a filter on an optional property answers `qualified: false` as soon as one scanned
/// object lacks it, so `all` names only properties every object of the type has. The second value
/// is the last answer, whose `schema` names the installed schema's version.
pub(crate) fn objects(
    s: &mut impl System,
    r: &Value,
    kind: &str,
    all: Vec<Value>,
) -> Result<(Vec<Value>, Value), Failure> {
    let mut q = json!({"object_type": kind, "where": {"all": all}, "limit": 200});
    let mut found = vec![];
    for _ in 0..20 {
        let v = parse(ont(s, r, &["search"], &q)?, "store_answer_unreadable")?;
        let ok = v["status"] == "ok" && v["result"]["basis"]["qualified"] == true;
        require!(ok && v["result"]["objects"].is_array() => "store_answer_unreadable", kind);
        found.extend(
            v["result"]["objects"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        q["continuation"] = v["result"]["continuation"].clone();
        if q["continuation"].is_null() {
            return Ok((found, v));
        }
    }
    Err(error("store_answer_unreadable", kind))
}
/// The closed reviews of kind `packet` of `head` that accepted, in `repository` (and of `task` when
/// given): the search names `kind`, `head`, `status` and the task, properties every review has;
/// `verdict` and `repository` are compared on the objects of the answer. `ap unit` and
/// `ap deliver` read an accepting review only here.
pub(crate) fn accepting(
    s: &mut impl System,
    r: &Value,
    repository: &str,
    head: &str,
    task: Option<Value>,
) -> Result<Vec<Value>, Failure> {
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let mut all = vec![
        eq("kind", json!("packet")),
        eq("head", json!(head)),
        eq("status", json!("closed")),
    ];
    all.extend(task.map(|t| eq("task", t)));
    let (found, _) = objects(s, r, "review", all)?;
    let accepts = |o: &Value| {
        field(o, "verdict") == "accept"
            && field(o, "repository") == repository
            && complete_id(&o["id"])
    };
    Ok(found.into_iter().filter(accepts).collect())
}
fn file(s: &impl System, path: &str, field: &str, code: &'static str) -> Result<Vec<u8>, Failure> {
    require!(Path::new(path).starts_with(s.work_root())
        && !Path::new(path).components().any(|c| matches!(c, Component::ParentDir)) => code, field);
    s.read(path).map_err(|_| error(code, field))
}
fn fields(
    s: &mut impl System,
    r: &Value,
    object: &Value,
    code: &'static str,
    field: &str,
) -> Result<Value, Failure> {
    let reply = ont(s, r, &["get"], &json!({"id":object})).map_err(|_| error(code, field))?;
    require!(reply.success => code, field);
    let v = parse(reply, code)?;
    require!(v["status"] == "ok" && v.get("code").is_none() && v.get("error").is_none() => code, field);
    let objects = v["result"]["objects"]
        .as_array()
        .ok_or_else(|| error(code, field))?;
    require!(objects.len() == 1 && objects[0]["id"] == *object => code, field);
    let head = text(&objects[0], "head").map_err(|_| error(code, field))?;
    let observations: Vec<_> = objects[0]["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["revision"] == head)
        .collect();
    require!(observations.len() == 1 && observations[0]["change"] != "retraction" && observations[0]["fields"].is_object() => code, field);
    Ok(Value::Object(
        observations[0]["fields"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    if v["kind"] == "value" {
                        v["value"].clone()
                    } else {
                        Value::Null
                    },
                )
            })
            .collect(),
    ))
}
fn apply(
    s: &mut impl System,
    r: &Value,
    action: &str,
    request: String,
    parameters: Value,
) -> Result<Value, Failure> {
    let body = json!({"action_type":action,"request_id":request,"parameters":parameters});
    let v = apply_answer(
        ont(s, r, &["action", "apply", "--client", "lead"], &body)?,
        "store_answer_unreadable",
    )?;
    if refused(&v) {
        return Err(fail("store_refused", "apply", codes(&v)));
    }
    require!(v["status"] == "ok" && v.get("error").is_none() => "store_answer_unreadable", "apply");
    Ok(v)
}
pub(crate) fn pr(s: &mut impl System, repository: &str, number: &Value) -> Result<Value, Failure> {
    let reply = s.call(
        "gh",
        &args(&[
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            repository,
            "--json",
            "state,baseRefName,headRefOid",
        ]),
        &Value::Null,
        30_000,
    )?;
    require!(reply.success => "review_head_differs", "pull_request");
    parse(reply, "review_head_differs")
}
pub(crate) fn start(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let attempt = r
        .get("attempt")
        .map(|v| {
            v.as_u64()
                .filter(|n| (2..=99).contains(n))
                .ok_or_else(|| error("invalid_input", "attempt"))
        })
        .transpose()?;
    let task_key = Path::new(text(r, "task_dir")?)
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or_else(|| error("invalid_input", "task_dir"))?;
    let task = id("task", task_key);
    let prompt = text(r, "prompt")?;
    let prompt_sha256 = digest(&file(s, prompt, "prompt", "review_prompt_unreadable")?);
    let kind = text(r, "kind")?;
    let head = text(r, "head")?;
    let mut parameters = json!({"task":task,"kind":kind,"head":head,"model":text(r,"model")?,"effort":text(r,"effort")?,"prompt":prompt,"prompt_sha256":prompt_sha256});
    for key in ["base", "repository", "pull_request", "runs"] {
        if let Some(v) = r.get(key) {
            parameters[key] = v.clone();
        }
    }
    if let Some(runs) = r.get("runs") {
        let runs = runs
            .as_array()
            .ok_or_else(|| error("invalid_input", "runs"))?;
        require!(runs.iter().enumerate().all(|(i,v)| complete_id(v) && v["object_type"] == "worker_run" && !runs[..i].contains(v)) => "invalid_input", "runs");
    }
    if let Some(number) = r.get("pull_request") {
        let repository = text(r, "repository")?;
        require!(number.as_u64().is_some_and(|n| n > 0) => "invalid_input", "pull_request");
        let p = pr(s, repository, number)?;
        if p["state"] != "OPEN" || p["headRefOid"] != head {
            return Err(fail("review_head_differs", "head", p["headRefOid"].clone()));
        }
    }
    for run in r["runs"].as_array().into_iter().flatten() {
        let f = fields(s, r, run, "review_run_unknown", "runs")?;
        require!(f["task"] == task => "review_run_of_other_task", "runs");
    }
    let mut input = format!("{task_key}\n{kind}\n{head}\n{prompt_sha256}");
    if let Some(attempt) = attempt {
        input.push_str(&format!("\n{attempt}"));
    }
    let request = format!("rv-start-{}", &digest(input.as_bytes())[..40]);
    let v = apply(s, r, "start_review", request, parameters)?;
    let edits = v["result"]["edits"]
        .as_array()
        .ok_or_else(|| error("store_answer_unreadable", "apply"))?;
    require!(edits.len() == 1 && edits[0]["change"] == "initial" && complete_id(&edits[0]["id"]) && edits[0]["id"]["object_type"] == "review" => "store_answer_unreadable", "apply");
    if v["result"]["outcome"] != "applied" {
        let review = &edits[0]["id"];
        let f = fields(s, r, review, "store_review_unreadable", "review")?;
        if f["status"] != "running" {
            return Err(fail("review_already_closed", "review", review.clone()));
        }
        return Ok(
            json!({"status":"started","outcome":"already_applied","review":review,"prompt_sha256":prompt_sha256}),
        );
    }
    Ok(
        json!({"status":"started","outcome":"applied","review":edits[0]["id"],"prompt_sha256":prompt_sha256}),
    )
}
pub(crate) fn close(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let mut parameters = json!({"review":r["review"]});
    let report_sha256;
    let verdict = if let Some(v) = r.get("verdict") {
        require!(*v == "abandoned" => "invalid_input", "verdict");
        require!(r.get("report").is_none() => "invalid_input", "report");
        report_sha256 = Value::Null;
        "abandoned"
    } else {
        let report = text(r, "report")?;
        let bytes = file(s, report, "report", "review_report_unreadable")?;
        require!(bytes.len() as u64 <= LIMIT => "review_report_unreadable", "report");
        let verdict = match bytes.split(|b| *b == b'\n').next() {
            Some(b"Verdict: accept") => "accept",
            Some(b"Verdict: repair") => "repair",
            _ => return Err(error("review_report_lacks_verdict", "report")),
        };
        report_sha256 = json!(digest(&bytes));
        parameters["report"] = json!(report);
        parameters["report_sha256"] = report_sha256.clone();
        verdict
    };
    parameters["verdict"] = json!(verdict);
    for key in ["account", "session"] {
        if let Some(v) = r.get(key) {
            text(r, key)?;
            parameters[key] = v.clone();
        }
    }
    let key = text(&r["review"], "native_key")?;
    let request = format!(
        "rv-close-{}",
        &digest(format!("{key}\n{}", report_sha256.as_str().unwrap_or("abandoned")).as_bytes())
            [..40]
    );
    apply(s, r, "close_review", request, parameters)?;
    Ok(json!({"status":"closed","verdict":verdict,"report_sha256":report_sha256}))
}
/// Git diff options shared with the real-Git path controls.
pub const DIFF_OPTIONS: [&str; 5] = [
    "--no-renames",
    "--ignore-submodules=none",
    "--no-relative",
    "--name-only",
    "-z",
];
pub(crate) fn git(
    s: &mut impl System,
    worktree: &str,
    command: &[&str],
) -> Result<String, Failure> {
    let mut argv = args(&["--no-replace-objects", "-C", worktree]);
    argv.extend(args(command));
    let reply = s.call("git", &argv, &Value::Null, 30_000)?;
    require!(reply.success => "process_failed", "git");
    String::from_utf8(reply.stdout).map_err(|_| error("process_failed", "git"))
}
pub(crate) fn post(
    s: &mut impl System,
    f: &Value,
    state: &str,
    description: &str,
) -> Result<(), Failure> {
    require!(description.len() <= 140 => "invalid_input", "description");
    let reply = s.call(
        "gh",
        &args(&[
            "api",
            "--method",
            "POST",
            &format!(
                "repos/{}/statuses/{}",
                text(f, "repository")?,
                text(f, "head")?
            ),
            "-f",
            &format!("state={state}"),
            "-f",
            "context=lead-review",
            "-f",
            &format!("description={description}"),
        ]),
        &Value::Null,
        30_000,
    )?;
    require!(reply.success => "process_failed", "gh");
    Ok(())
}
/// The paths no pattern covers: a pattern matches its exact path; a trailing `/**` matches
/// descendants, not the directory itself or another name with the same prefix.
pub(crate) fn outside<'a>(patterns: &[String], paths: &[&'a str]) -> Vec<&'a str> {
    paths
        .iter()
        .filter(|path| {
            !patterns.iter().any(|pattern| {
                pattern == **path
                    || pattern.strip_suffix("/**").is_some_and(|dir| {
                        path.strip_prefix(dir)
                            .is_some_and(|tail| tail.starts_with('/') && tail.len() > 1)
                    })
            })
        })
        .copied()
        .collect()
}
fn accept(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let f = fields(s, r, &r["review"], "store_review_unreadable", "review")?;
    for (key, valid) in [
        ("status", f["status"] == "closed"),
        ("verdict", f["verdict"] == "accept"),
        (
            "kind",
            matches!(f["kind"].as_str(), Some("packet" | "integrated")),
        ),
    ] {
        if !valid {
            return Err(fail("review_not_accepting", "review", json!(key)));
        }
    }
    for key in [
        "repository",
        "pull_request",
        "head",
        "base",
        "report",
        "report_sha256",
    ] {
        let valid = if key == "pull_request" {
            f[key].as_u64().is_some_and(|n| n > 0)
        } else {
            f[key].as_str().is_some_and(|s| !s.is_empty())
        };
        if !valid {
            return Err(fail("review_not_accepting", "review", json!(key)));
        }
    }
    let bytes = file(s, text(&f, "report")?, "report", "review_report_changed")?;
    require!(digest(&bytes) == f["report_sha256"] => "review_report_changed", "report");
    let runs = f["runs"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| error("review_runs_required", "runs"))?;
    let mut patterns = vec![];
    for run in runs {
        let key = run["native_key"].clone();
        let failure = || fail("review_run_lacks_write_set", "runs", key.clone());
        require!(complete_id(run) && run["object_type"] == "worker_run" => "invalid_input", "runs");
        let run = fields(s, r, run, "review_run_lacks_write_set", "runs").map_err(|_| failure())?;
        if run["task"] != f["task"] {
            return Err(fail("review_run_of_other_task", "runs", key));
        }
        let write_set = run["write_set"]
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(failure)?;
        for pattern in write_set {
            patterns.push(
                pattern
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(failure)?
                    .to_owned(),
            );
        }
    }
    let head = text(&f, "head")?;
    let p = pr(s, text(&f, "repository")?, &f["pull_request"])?;
    if p["state"] != "OPEN" || p["baseRefName"] != "main" || p["headRefOid"] != head {
        return Err(fail("review_head_differs", "head", p["headRefOid"].clone()));
    }
    let worktree = text(r, "worktree")?;
    git(s, worktree, &["fetch", "origin", "main"])?;
    let base = git(s, worktree, &["merge-base", "origin/main", head])?;
    if base.trim_end() != f["base"] {
        return Err(fail("review_base_differs", "base", json!(base.trim_end())));
    }
    // These options make the list independent of replace refs and the worktree's own diff configuration.
    let mut diff_argv = vec!["diff"];
    diff_argv.extend(DIFF_OPTIONS);
    diff_argv.extend([text(&f, "base")?, head]);
    let diff = git(s, worktree, &diff_argv)?;
    let paths: Vec<_> = diff.split_terminator('\0').collect();
    let outside = outside(&patterns, &paths);
    if !outside.is_empty() {
        return Err(fail(
            "review_path_outside_write_set",
            "paths",
            json!({"paths":outside.iter().take(10).collect::<Vec<_>>(),"count":outside.len()}),
        ));
    }
    let task = &f["task"];
    require!(complete_id(task) && task["object_type"] == "task" => "review_not_accepting", "task");
    let task_key = text(task, "native_key")?;
    require!(Path::new(task_key).components().count() == 1 && matches!(Path::new(task_key).components().next(),Some(Component::Normal(_))) => "review_not_accepting", "task");
    let cwd = s.cwd().map_err(|_| error("io_error", "cwd"))?;
    require!(!cwd.starts_with(Path::new(ROOT).join(task_key).join("wt")) => "review_accept_from_worktree", "cwd");
    post(
        s,
        &f,
        "success",
        &format!("review {} accept", text(&r["review"], "native_key")?),
    )?;
    Ok(json!({"status":"accepted","head":head,"paths":paths.len()}))
}
fn revoke(s: &mut impl System, r: &Value) -> Result<Value, Failure> {
    let reason = text(r, "reason")?;
    require!(reason.len() <= 100 && !reason.trim().is_empty() => "invalid_input", "reason");
    let f = fields(s, r, &r["review"], "store_review_unreadable", "review")?;
    for key in ["repository", "head"] {
        if text(&f, key).is_err() {
            return Err(fail("review_not_accepting", "review", json!(key)));
        }
    }
    // The store before the status: a crash between the two leaves `revoked`, and the repeated
    // command applies nothing and posts. One request id for every revoke of this review.
    if f["status"] == "closed" {
        let key = text(&r["review"], "native_key")?;
        let request = format!("rv-revoke-{}", &digest(key.as_bytes())[..40]);
        let parameters = json!({"review": r["review"], "reason": reason});
        apply(s, r, "revoke_review", request, parameters)?;
    }
    post(s, &f, "failure", &format!("revoked: {reason}"))?;
    Ok(json!({"status":"revoked","head":f["head"]}))
}
/// Execute one request with every subprocess supplied by the caller.
pub fn execute(s: &mut impl System, verb: &str, r: &Value) -> Result<(Value, u8), Failure> {
    require!(r.is_object() => "invalid_input", "stdin");
    for key in ["task_dir", "prompt", "report", "worktree", "store"] {
        if let Some(v) = r.get(key) {
            require!(v.as_str().is_some_and(|p| Path::new(p).is_absolute()) => "invalid_input", key);
        }
    }
    if matches!(verb, "close" | "accept" | "revoke") {
        require!(complete_id(&r["review"]) && r["review"]["object_type"] == "review" => "invalid_input", "review");
    }
    if verb == "accept" {
        text(r, "worktree")?;
    }
    let value = match verb {
        "start" => start(s, r)?,
        "close" => close(s, r)?,
        "accept" => accept(s, r)?,
        "revoke" => revoke(s, r)?,
        _ => return Err(error("usage", "args")),
    };
    Ok((value, 0))
}
pub fn cli(args: &[OsString]) -> ExitCode {
    json_cli(args, execute)
}
