//! Store-owned reviews; a derived GitHub status is posted only after local checks.
use crate::host::{
    Failure, LIMIT, System, apply_answer, args, codes, complete_id, digest, error, fail, id,
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
pub(crate) fn fields(
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
    require!(observations.len() == 1 && observations[0]["change"] != "retraction" => code, field);
    let fields = observations[0]["fields"]
        .as_object()
        .ok_or_else(|| error(code, field))?;
    Ok(Value::Object(
        fields
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
pub(crate) fn apply(
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
    // A call that failed says why in its reply, redacted as every `gh` reply that reaches a detail.
    if !reply.success {
        let said = crate::unit::redact(&reply.bytes);
        return Err(fail("review_head_differs", "pull_request", json!(said)));
    }
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
/// The words before the command word of every Git call of the driver: no hook and no fsmonitor
/// program of the repository runs (`specs/ap/unit-run.md` R39).
pub const GIT_SAFE: [&str; 4] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
];
pub(crate) fn git(
    s: &mut impl System,
    worktree: &str,
    command: &[&str],
) -> Result<String, Failure> {
    let mut argv = args(&["--no-replace-objects", "-C", worktree]);
    argv.extend(args(&GIT_SAFE));
    argv.extend(args(command));
    let reply = s.call("git", &argv, &Value::Null, 30_000)?;
    require!(reply.success => "process_failed", "git");
    String::from_utf8(reply.stdout).map_err(|_| error("process_failed", "git"))
}
/// The fixed script of the App's RS256 signature: the signing input and the key's path are its
/// arguments, never part of its text (`specs/ap/unit-run.md` R42).
pub(crate) const SIGN: &str = r#"printf %s "$1" | openssl dgst -sha256 -sign "$2" -binary"#;
/// Base64url without padding.
fn b64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
    for chunk in bytes.chunks(3) {
        let n =
            (chunk.iter().enumerate()).fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(A[(n >> (18 - 6 * i)) as usize & 63] as char);
        }
    }
    out
}
/// The Lead's App for a status on `repository` (R42): `None` (the one call of before) with no policy
/// file, no JSON object, no `units.github.lead_app`, or a `stage_tokens` that does not name the
/// repository, whose authors hold the host's login.
fn lead_app(s: &impl System, repository: &str) -> Result<Option<(u64, String)>, Failure> {
    let Some(file) = s.policy_file() else {
        return Ok(None);
    };
    let Ok(bytes) = s.read(&file.to_string_lossy()) else {
        return Ok(None);
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return Ok(None);
    };
    let g = &v["units"]["github"];
    let named = g["stage_tokens"].get(repository).is_some();
    match g.get("lead_app").filter(|_| named) {
        None => Ok(None),
        Some(app) => crate::unit::drive::lead_app(app).map(Some),
    }
}
/// A step of the App that failed: `process_failed` with the reply through R22's one redaction.
fn app_failed(field: &str, reply: &[u8]) -> Failure {
    fail("process_failed", field, json!(crate::unit::redact(reply)))
}
/// One `gh` call of the App within 30 seconds: the member `member` of its answer, or `Err` with the
/// redacted reply of a failed call, or the member's name alone (no byte of the answer) (R42).
fn app_call(
    s: &mut impl System,
    words: &[&str],
    input: &Value,
    member: &str,
) -> Result<Value, Failure> {
    let reply = s.call("gh", &args(words), input, 30_000);
    let reply = reply.map_err(|_| app_failed("gh", b""))?;
    if !reply.success {
        return Err(app_failed("gh", &reply.bytes));
    }
    let v: Option<Value> = serde_json::from_slice(&reply.stdout).ok();
    let found = v.map(|v| v[member].clone()).filter(|m| !m.is_null());
    let word = if member == "id" {
        "installation"
    } else {
        "access_tokens"
    };
    found.ok_or_else(|| fail("process_failed", "gh", json!(word)))
}
/// A fresh installation token of the App `id` for `repository`, with the statuses alone: the JWT
/// signed by `openssl` with the key at `key` (never opened here), the installation, its token (R42).
fn installation_token(
    s: &mut impl System,
    repository: &str,
    (id, key): (u64, &str),
) -> Result<String, Failure> {
    let epoch = std::time::UNIX_EPOCH;
    let w = s.wall().duration_since(epoch).map_or(0, |d| d.as_secs());
    let header = b64url(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = format!(
        r#"{{"iat":{},"exp":{},"iss":"{id}"}}"#,
        w.saturating_sub(60),
        w + 540
    );
    let input = format!("{header}.{}", b64url(claims.as_bytes()));
    let signed = s.call(
        "sh",
        &args(&["-c", SIGN, "sh", &input, key]),
        &Value::Null,
        30_000,
    );
    let signature = match signed {
        Ok(r) if r.success && !r.stdout.is_empty() => r.stdout,
        Ok(r) => return Err(app_failed("openssl", &r.bytes)),
        Err(_) => return Err(app_failed("openssl", b"")),
    };
    let bearer = format!("Authorization: Bearer {input}.{}", b64url(&signature));
    let path = format!("repos/{repository}/installation");
    let found = app_call(s, &["api", "-H", &bearer, &path], &Value::Null, "id")?;
    let installation = found.as_u64().filter(|n| *n > 0);
    let installation =
        installation.ok_or_else(|| fail("process_failed", "gh", json!("installation")))?;
    let name = repository.split_once('/').map_or(repository, |(_, n)| n);
    let body = json!({"repositories": [name], "permissions": {"statuses": "write"}});
    let path = format!("app/installations/{installation}/access_tokens");
    let words = [
        "api", "--method", "POST", "-H", &bearer, &path, "--input", "-",
    ];
    let token = app_call(s, &words, &body, "token")?;
    let token = token.as_str().filter(|t| !t.is_empty()).map(str::to_owned);
    token.ok_or_else(|| fail("process_failed", "gh", json!("access_tokens")))
}
/// The status `lead-review` on a head: as the Lead's App where the policy names it and a stage token
/// of the repository, else the one call of before with the host's login (`specs/ap/unit-run.md` R42).
pub(crate) fn post(
    s: &mut impl System,
    f: &Value,
    state: &str,
    description: &str,
) -> Result<(), Failure> {
    require!(description.len() <= 140 => "invalid_input", "description");
    let (repository, head) = (text(f, "repository")?, text(f, "head")?);
    let mut words = args(&["api"]);
    if let Some((id, key)) = lead_app(s, repository)? {
        let token = installation_token(s, repository, (id, &key))?;
        words.extend(args(&["-H", &format!("Authorization: Bearer {token}")]));
    }
    words.extend(args(&[
        "--method",
        "POST",
        &format!("repos/{repository}/statuses/{head}"),
        "-f",
        &format!("state={state}"),
        "-f",
        "context=lead-review",
        "-f",
        &format!("description={description}"),
    ]));
    let reply = s.call("gh", &words, &Value::Null, 30_000)?;
    // A call that failed says why in its reply: redacted here, so every caller gets it redacted.
    match reply.success {
        true => Ok(()),
        false => Err(fail(
            "process_failed",
            "gh",
            json!(crate::unit::redact(&reply.bytes)),
        )),
    }
}
/// The state of the status `lead-review` on `head` as GitHub reads it now (the newest of that
/// context in the combined status), or `None` when the read fails or names none.
pub(crate) fn lead_review(s: &mut impl System, repository: &str, head: &str) -> Option<String> {
    let path = format!("repos/{repository}/commits/{head}/status");
    let reply = s
        .call("gh", &args(&["api", &path]), &Value::Null, 30_000)
        .ok()?;
    let v: Value = serde_json::from_slice(&reply.stdout)
        .ok()
        .filter(|_| reply.success)?;
    let mut all = v["statuses"].as_array()?.iter();
    let lead = all.find(|st| st["context"] == "lead-review")?;
    lead["state"].as_str().map(str::to_owned)
}
/// The two forms of a write-set pattern, as the refusals of `specs/ap/unit.md` R11, R38 and R39 name them.
pub(crate) const WRITE_SET_FORMS: [&str; 2] = ["<path>", "<dir>/**"];
/// How the matcher reads a write-set pattern: the one path equal to the text, or the descendants of the
/// literal text before a final `/**`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reading<'a> {
    Exact(&'a str),
    Below(&'a str),
}
/// A write-set pattern: whether its text is of the grammar of `specs/ap/unit.md` R11 and how the matcher
/// reads it, which is the same reading for every text, of the grammar or not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WritePattern<'a> {
    pub grammar: bool,
    pub reading: Reading<'a>,
}
/// An exact path: components joined by single `/`, none empty, `.` or `..`, none holding `*`, `?` or `[`.
fn exact_path(t: &str) -> bool {
    !t.is_empty()
        && t.split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains(['*', '?', '[']))
}
/// The one reading of a write-set pattern's text (`specs/ap/unit.md` R11, `specs/ap/unit-run.md` R16).
pub(crate) fn write_set_pattern(text: &str) -> WritePattern<'_> {
    let reading = match text.strip_suffix("/**") {
        Some(dir) => Reading::Below(dir),
        None => Reading::Exact(text),
    };
    let path = match reading {
        Reading::Exact(p) | Reading::Below(p) => p,
    };
    let grammar =
        (1..=200).contains(&text.len()) && !text.chars().any(char::is_control) && exact_path(path);
    WritePattern { grammar, reading }
}
/// The paths no pattern covers: a pattern of the grammar matches its exact path or the descendants of its
/// directory, not the directory itself or another name with the same prefix; a text of neither form is read
/// as before (`write_set_pattern`).
pub(crate) fn outside<'a>(patterns: &[String], paths: &[&'a str]) -> Vec<&'a str> {
    paths
        .iter()
        .filter(|path| {
            !patterns
                .iter()
                .any(|pattern| match write_set_pattern(pattern).reading {
                    Reading::Exact(exact) => exact == **path,
                    Reading::Below(dir) => path
                        .strip_prefix(dir)
                        .is_some_and(|tail| tail.starts_with('/') && tail.len() > 1),
                })
        })
        .copied()
        .collect()
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
    for key in ["task_dir", "prompt", "report", "store"] {
        if let Some(v) = r.get(key) {
            require!(v.as_str().is_some_and(|p| Path::new(p).is_absolute()) => "invalid_input", key);
        }
    }
    if matches!(verb, "close" | "revoke") {
        require!(complete_id(&r["review"]) && r["review"]["object_type"] == "review" => "invalid_input", "review");
    }
    let value = match verb {
        "start" => start(s, r)?,
        "close" => close(s, r)?,
        "revoke" => revoke(s, r)?,
        _ => return Err(error("usage", "args")),
    };
    Ok((value, 0))
}
pub fn cli(args: &[OsString]) -> ExitCode {
    json_cli(args, execute)
}
