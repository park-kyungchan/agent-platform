//! The gate: waits on a pull request's head until its required checks and the automatic review are
//! done, and prints one answer of what GitHub reported. It decides nothing a person decides: it
//! resolves no thread, posts no status, merges nothing and calls no store. Its only writes are the
//! ready mark of a draft and the one toggle of draft and ready that asks for a review of the head.
use crate::work::{Failure, System, args, error, require, text};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Read, io::Write, process::ExitCode, time::Duration};

/// The login of the automatic reviewer whose summary comment carries the marker.
const LOGIN: &str = "chatgpt-codex-connector[bot]";
const MARKER: &str = "codex-review-state:v1";
const POLL: Duration = Duration::from_secs(30);
const MEMBERS: [&str; 6] = [
    "repository",
    "pull_request",
    "head",
    "autoreview",
    "check_minutes",
    "autoreview_minutes",
];
const VIEW: &str = "query($owner:String!,$name:String!,$n:Int!){repository(owner:$owner,name:$name){pullRequest(number:$n){state isDraft headRefOid
commits(last:1){nodes{commit{oid statusCheckRollup{contexts(first:100){nodes{__typename
... on CheckRun{name status conclusion isRequired(pullRequestNumber:$n) checkSuite{workflowRun{url}}}
... on StatusContext{context state targetUrl isRequired(pullRequestNumber:$n)}}}}}}}
comments(last:100){nodes{author{__typename login} body}}}}}";
const THREADS: &str = "query($owner:String!,$name:String!,$n:Int!,$after:String){repository(owner:$owner,name:$name){pullRequest(number:$n){
reviewThreads(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{id isResolved path line
first:comments(first:1){nodes{author{__typename login}}} last:comments(last:1){nodes{author{__typename login}}}}}}}}";

struct Request {
    repository: String,
    number: u64,
    head: Option<String>,
    login: Option<String>,
    check: Duration,
    review: Duration,
}
/// One reading of the pull request.
struct View {
    state: String,
    draft: bool,
    head: String,
    /// Every entry of a required check on the head: `{name, state, run}`.
    checks: Vec<Value>,
    /// The `code_review` member of the last marker in a comment of the reviewer's login.
    marker: Value,
}

fn minutes(r: &Value, key: &str, default: u64) -> Result<Duration, Failure> {
    let m = match r.get(key) {
        None => default,
        Some(v) => v
            .as_u64()
            .filter(|m| (1..=1440).contains(m))
            .ok_or_else(|| error("invalid_input", key))?,
    };
    Ok(Duration::from_secs(m * 60))
}
fn request(r: &Value) -> Result<Request, Failure> {
    let o = r.as_object().ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(error("invalid_input", k));
    }
    let repository = text(r, "repository")?;
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with(['-', '@', '.'])
            && p.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    let parts: Vec<&str> = repository.split('/').collect();
    require!(parts.len() == 2 && parts.iter().all(|p| part(p)) => "invalid_input", "repository");
    let number = r["pull_request"].as_u64().filter(|n| *n > 0);
    let number = number.ok_or_else(|| error("invalid_input", "pull_request"))?;
    let head = match r.get("head") {
        None => None,
        Some(_) => Some(text(r, "head")?.to_owned()),
    };
    let hex = |h: &String| h.len() == 40 && h.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    require!(head.as_ref().is_none_or(hex) => "invalid_input", "head");
    let login = match r.get("autoreview") {
        None => Some(LOGIN.to_owned()),
        Some(Value::Bool(false)) => None,
        Some(Value::String(l)) if !l.is_empty() && l.len() <= 100 => Some(l.clone()),
        Some(_) => return Err(error("invalid_input", "autoreview")),
    };
    Ok(Request {
        repository: repository.to_owned(),
        number,
        head,
        login,
        check: minutes(r, "check_minutes", 90)?,
        review: minutes(r, "autoreview_minutes", 40)?,
    })
}
/// One GraphQL read; `None` when `gh` failed or the answer has no pull request.
fn graphql(s: &mut impl System, q: &Request, query: &str, after: Option<&str>) -> Option<Value> {
    let (owner, name) = q.repository.split_once('/')?;
    let (owner, name, n) = (format!("owner={owner}"), format!("name={name}"), format!("n={}", q.number));
    let mut a = args(&["api", "graphql", "-F", &owner, "-F", &name, "-F", &n]);
    if let Some(c) = after {
        a.extend(args(&["-f", &format!("after={c}")]));
    }
    a.extend(args(&["-f", &format!("query={query}")]));
    let reply = s.call("gh", &a, &Value::Null, 60_000).ok()?;
    let v: Value = serde_json::from_slice(&reply.stdout).ok()?;
    let pr = v.pointer("/data/repository/pullRequest")?;
    (reply.success && pr.is_object()).then(|| pr.clone())
}
/// A login as the REST API spells it: a bot's carries `[bot]`, so a user cannot pass for one.
fn login(author: &Value) -> Value {
    match (author["__typename"].as_str(), author["login"].as_str()) {
        (Some("Bot"), Some(l)) => json!(format!("{l}[bot]")),
        (_, Some(l)) => json!(l),
        _ => Value::Null,
    }
}
fn view(s: &mut impl System, q: &Request) -> Option<View> {
    let pr = graphql(s, q, VIEW, None)?;
    let head = pr["headRefOid"].as_str()?.to_owned();
    let commit = &pr["commits"]["nodes"][0]["commit"];
    let mut checks = vec![];
    // A rollup of another commit is no reading of the head: its checks wait.
    if commit["oid"] == head.as_str() {
        let nodes = commit["statusCheckRollup"]["contexts"]["nodes"].as_array();
        for c in nodes.into_iter().flatten().filter(|c| c["isRequired"] == true) {
            let state = match (c["status"].as_str(), c["state"].as_str()) {
                (Some("COMPLETED"), _) => c["conclusion"].clone(),
                (Some(_), _) | (None, Some("PENDING" | "EXPECTED")) => json!("PENDING"),
                (None, _) => c["state"].clone(),
            };
            let name = c.get("name").unwrap_or(&c["context"]);
            let run = c["checkSuite"]["workflowRun"]["url"].clone();
            let run = if run.is_null() { c["targetUrl"].clone() } else { run };
            checks.push(json!({"name": name, "state": state, "run": run}));
        }
    }
    let mut marker = Value::Null;
    for c in pr["comments"]["nodes"].as_array().into_iter().flatten() {
        let body = c["body"].as_str().unwrap_or("");
        if q.login.as_deref().is_none_or(|l| login(&c["author"]) != l) {
            continue;
        }
        if let Some(at) = body.rfind(MARKER) {
            let rest = body[at + MARKER.len()..].trim_start();
            let parsed = serde_json::Deserializer::from_str(rest).into_iter::<Value>().next();
            if let Some(Ok(v)) = parsed {
                marker = v["code_review"].clone();
            }
        }
    }
    Some(View {
        state: pr["state"].as_str()?.to_owned(),
        draft: pr["isDraft"].as_bool()?,
        head,
        checks,
        marker,
    })
}
/// Every unresolved review thread; `null` when a page could not be read.
fn threads(s: &mut impl System, q: &Request) -> Value {
    let (mut list, mut after) = (vec![], None::<String>);
    for _ in 0..50 {
        let Some(pr) = graphql(s, q, THREADS, after.as_deref()) else {
            return Value::Null;
        };
        let page = &pr["reviewThreads"];
        for t in page["nodes"].as_array().into_iter().flatten() {
            if t["isResolved"] == true {
                continue;
            }
            let last = login(&t["last"]["nodes"][0]["author"]);
            list.push(json!({"id": t["id"], "path": t["path"], "line": t["line"],
                "author": login(&t["first"]["nodes"][0]["author"]),
                "answered": q.login.as_deref().is_none_or(|l| last != l)}));
        }
        match page["pageInfo"]["endCursor"].as_str() {
            Some(c) if page["pageInfo"]["hasNextPage"] == true => after = Some(c.to_owned()),
            _ => return json!(list),
        }
    }
    Value::Null
}
fn ready(s: &mut impl System, q: &Request, undo: bool) -> bool {
    let mut a = args(&["pr", "ready", &q.number.to_string(), "-R", &q.repository]);
    if undo {
        a.push("--undo".to_owned());
    }
    s.call("gh", &a, &Value::Null, 60_000)
        .is_ok_and(|r| r.success)
}
/// The exit code of each end.
fn status(end: &str) -> u8 {
    match end {
        "passed" => 0,
        "merged" | "closed" | "head_differs" | "head_moved" => 10,
        "check_failed" | "check_timeout" => 11,
        _ => 12,
    }
}
/// Blocks until one end of the closed list: `passed`, `merged`, `closed`, `head_differs`,
/// `head_moved`, `check_failed`, `check_timeout`, `ready_failed`, `github_unreadable`.
pub fn execute(s: &mut impl System, r: &Value) -> Result<(Value, u8), Failure> {
    let q = request(r)?;
    let mut a = json!({"end": null, "repository": q.repository, "pull_request": q.number,
        "head": null, "marked_ready": false, "checks": [],
        "autoreview": {"login": q.login, "state": if q.login.is_some() { "waiting" } else { "off" },
            "commit": null, "has_unresolved_findings": null, "toggled": false}});
    let mut deadline = s.now() + q.check;
    // The head GitHub reported at the first reading; a caller's `head` is only compared with it.
    let (mut head, mut reviewing) = (None::<String>, false);
    let end = loop {
        let Some(v) = view(s, &q) else {
            if s.now() >= deadline {
                break "github_unreadable";
            }
            s.pause(POLL);
            continue;
        };
        let h = head.get_or_insert_with(|| v.head.clone());
        a["head"] = json!(h);
        match v.state.as_str() {
            "MERGED" => break "merged",
            "CLOSED" => break "closed",
            _ if q.head.as_ref().is_some_and(|e| e != h) => {
                a["expected"] = json!(q.head);
                break "head_differs";
            }
            _ if *h != v.head => {
                a["head_now"] = json!(v.head);
                break "head_moved";
            }
            _ => (),
        }
        if v.draft && a["marked_ready"] == false {
            // A stage that ended with its DONE line left nothing a draft protects.
            if !ready(s, &q, false) {
                break "ready_failed";
            }
            a["marked_ready"] = json!(true);
            continue;
        }
        a["checks"] = json!(v.checks);
        if !reviewing {
            let bad = v.checks.iter().find(|c| c["state"] != "SUCCESS" && c["state"] != "PENDING");
            if let Some(c) = bad {
                a["failed_check"] = c.clone();
                break "check_failed";
            }
            if v.checks.is_empty() || v.checks.iter().any(|c| c["state"] == "PENDING") {
                if s.now() >= deadline {
                    break "check_timeout";
                }
                s.pause(POLL);
                continue;
            }
            if q.login.is_none() {
                break "passed";
            }
            (reviewing, deadline) = (true, s.now() + q.review);
            // A push to a ready pull request is not reviewed: when the summary names another
            // commit, draft and ready once, unless this gate's own ready mark asked for a review.
            if v.marker["commit_sha"] != v.head.as_str() && a["marked_ready"] == false {
                if !(ready(s, &q, true) && ready(s, &q, false)) {
                    break "ready_failed";
                }
                a["autoreview"]["toggled"] = json!(true);
            }
        }
        let m = &v.marker;
        if m["commit_sha"] == v.head.as_str() && m["status"] == "completed" {
            a["autoreview"]["state"] = json!("completed");
            a["autoreview"]["commit"] = m["commit_sha"].clone();
            a["autoreview"]["has_unresolved_findings"] = m["has_unresolved_findings"].clone();
            break "passed";
        }
        if s.now() >= deadline {
            // Not a failure: the answer says the review did not complete in time.
            a["autoreview"]["state"] = json!("timeout");
            break "passed";
        }
        s.pause(POLL);
    };
    a["end"] = json!(end);
    a["threads"] = threads(s, &q);
    Ok((a, status(end)))
}
pub fn cli(argv: &[OsString]) -> ExitCode {
    let result = (|| {
        require!(argv.is_empty() => "usage", "args");
        let mut bytes = vec![];
        std::io::stdin()
            .take(crate::work::LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("io_error", "stdin"))?;
        require!(bytes.len() as u64 <= crate::work::LIMIT => "input_too_large", "stdin");
        let r: Value =
            serde_json::from_slice(&bytes).map_err(|_| error("invalid_input", "stdin"))?;
        execute(&mut crate::work::Host, &r)
    })();
    let (line, code, out) = match result {
        Ok((v, status)) => (v, status, true),
        Err(e) => (json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}}), 2, false),
    };
    let written = if out {
        writeln!(std::io::stdout(), "{line}")
    } else {
        writeln!(std::io::stderr(), "{line}")
    };
    ExitCode::from(if written.is_err() { 4 } else { code })
}
