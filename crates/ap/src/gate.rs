//! The gate: waits on a pull request's head until its required checks and the automatic review are
//! done, and prints one answer of what GitHub reported. It decides nothing a person decides: it
//! resolves no thread, posts no status, merges nothing and calls no store. Its only writes are the
//! ready mark of a draft and the one toggle of draft and ready that asks for a review of the head.
use crate::work::{Failure, System, args, error, fail, require, text};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Read, io::Write, process::ExitCode, time::Duration};

/// The login of the automatic reviewer whose summary comment carries the marker.
const LOGIN: &str = "chatgpt-codex-connector[bot]";
const LEAD_REVIEW: &str = "lead-review";
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
const VIEW: &str = "query($owner:String!,$name:String!,$n:Int!,$after:String){repository(owner:$owner,name:$name){pullRequest(number:$n){state isDraft headRefOid
commits(last:1){nodes{commit{oid statusCheckRollup{contexts(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{__typename
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
    /// Every entry of a required check on the head (`{name, state, run}`), and the status
    /// `lead-review` with `judged: false`.
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
    let o = r
        .as_object()
        .ok_or_else(|| error("invalid_input", "stdin"))?;
    if let Some(k) = o.keys().find(|k| !MEMBERS.contains(&k.as_str())) {
        return Err(error("invalid_input", k));
    }
    let repository = text(r, "repository")?;
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with(['-', '@', '.'])
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    let parts: Vec<&str> = repository.split('/').collect();
    require!(parts.len() == 2 && parts.iter().all(|p| part(p)) => "invalid_input", "repository");
    let number = r["pull_request"].as_u64().filter(|n| *n > 0);
    let number = number.ok_or_else(|| error("invalid_input", "pull_request"))?;
    let head = match r.get("head") {
        None => None,
        Some(_) => Some(text(r, "head")?.to_owned()),
    };
    let hex = |h: &String| {
        h.len() == 40
            && h.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    };
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
/// A failed reading: `Some(cause)` when waiting cannot make it pass, `None` when it can.
type Unread = Option<&'static str>;
/// One GraphQL read of the pull request.
/// The request check alone, for a caller that validates before its first effect.
pub(crate) fn check(r: &Value) -> Result<(), Failure> {
    request(r).map(|_| ())
}
fn graphql(
    s: &mut impl System,
    q: &Request,
    query: &str,
    after: Option<&str>,
) -> Result<Value, Unread> {
    let (owner, name) = q.repository.split_once('/').ok_or(None)?;
    // `-f`: the owner and the name stay strings (a repository named `123` or `true`).
    let (owner, name, n) = (
        format!("owner={owner}"),
        format!("name={name}"),
        format!("n={}", q.number),
    );
    let mut a = args(&["api", "graphql", "-f", &owner, "-f", &name, "-F", &n]);
    if let Some(c) = after {
        a.extend(args(&["-f", &format!("after={c}")]));
    }
    a.extend(args(&["-f", &format!("query={query}")]));
    let Ok(reply) = s.call("gh", &a, &Value::Null, 60_000) else {
        let started = s
            .call("gh", &args(&["--version"]), &Value::Null, 10_000)
            .is_ok();
        return Err((!started).then_some("gh_not_started"));
    };
    let v: Value = serde_json::from_slice(&reply.stdout).unwrap_or(Value::Null);
    let errors = v["errors"].as_array().into_iter().flatten();
    if errors.into_iter().any(|e| e["type"] == "NOT_FOUND") {
        return Err(Some("not_found"));
    }
    // `gh` exits 4 with this text when it has no login.
    if !reply.success && String::from_utf8_lossy(&reply.bytes).contains("gh auth login") {
        return Err(Some("no_login"));
    }
    match v.pointer("/data/repository/pullRequest") {
        Some(pr) if reply.success && pr.is_object() => Ok(pr.clone()),
        _ => Err(None),
    }
}
/// A login as the REST API spells it: a bot's carries `[bot]`, so a user cannot pass for one.
fn login(author: &Value) -> Value {
    match (author["__typename"].as_str(), author["login"].as_str()) {
        (Some("Bot"), Some(l)) => json!(format!("{l}[bot]")),
        (_, Some(l)) => json!(l),
        _ => Value::Null,
    }
}
fn view(s: &mut impl System, q: &Request) -> Result<View, Unread> {
    let pr = graphql(s, q, VIEW, None)?;
    let head = pr["headRefOid"].as_str().ok_or(None)?.to_owned();
    let mut commit = pr["commits"]["nodes"][0]["commit"].clone();
    let mut checks = vec![];
    // A rollup of another commit is no reading of the head: its checks wait. Every page of the
    // entries is read; a reading that could not be completed is no reading.
    let mut pages = 0;
    while commit["oid"] == head.as_str() {
        let contexts = &commit["statusCheckRollup"]["contexts"];
        let nodes = contexts["nodes"].as_array();
        // The status `lead-review` is the output of the accept, not a check before the review: it
        // is listed with `judged: false` and not judged. A check run of that name is judged.
        for c in nodes.into_iter().flatten() {
            let status = c["__typename"] == "StatusContext" && c["context"] == LEAD_REVIEW;
            if c["isRequired"] != true && !status {
                continue;
            }
            let state = match (c["status"].as_str(), c["state"].as_str()) {
                (Some("COMPLETED"), _) => c["conclusion"].clone(),
                (Some(_), _) | (None, Some("PENDING" | "EXPECTED")) => json!("PENDING"),
                (None, _) => c["state"].clone(),
            };
            let name = c.get("name").unwrap_or(&c["context"]);
            let run = c["checkSuite"]["workflowRun"]["url"].clone();
            let run = if run.is_null() {
                c["targetUrl"].clone()
            } else {
                run
            };
            let mut entry = json!({"name": name, "state": state, "run": run});
            if status {
                entry["judged"] = json!(false);
            }
            checks.push(entry);
        }
        if contexts["pageInfo"]["hasNextPage"] != true {
            break;
        }
        // More entries and no cursor to read them with: no reading.
        let cursor = contexts["pageInfo"]["endCursor"].as_str().ok_or(None)?;
        pages += 1;
        if pages == 50 {
            return Err(None);
        }
        commit = graphql(s, q, VIEW, Some(cursor))?["commits"]["nodes"][0]["commit"].clone();
        if commit["oid"] != head.as_str() {
            return Err(None);
        }
    }
    let mut marker = Value::Null;
    for c in pr["comments"]["nodes"].as_array().into_iter().flatten() {
        let body = c["body"].as_str().unwrap_or("");
        if q.login.as_deref().is_none_or(|l| login(&c["author"]) != l) {
            continue;
        }
        // Of the marker's occurrences in one comment, the last one that parses counts.
        for (at, _) in body.match_indices(MARKER) {
            let rest = body[at + MARKER.len()..].trim_start();
            let parsed = serde_json::Deserializer::from_str(rest)
                .into_iter::<Value>()
                .next();
            if let Some(Ok(v)) = parsed {
                marker = v["code_review"].clone();
            }
        }
    }
    Ok(View {
        state: pr["state"].as_str().ok_or(None)?.to_owned(),
        draft: pr["isDraft"].as_bool().ok_or(None)?,
        head,
        checks,
        marker,
    })
}
/// Every unresolved review thread; `null` when a page could not be read.
fn threads(s: &mut impl System, q: &Request) -> Value {
    let (mut list, mut after) = (vec![], None::<String>);
    for _ in 0..50 {
        let Ok(pr) = graphql(s, q, THREADS, after.as_deref()) else {
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
        let v = match view(s, &q) {
            Ok(v) => v,
            // A cause that cannot pass, at the first reading, ends at once.
            Err(Some(cause)) if head.is_none() => {
                a["unreadable"] = json!(cause);
                break "github_unreadable";
            }
            Err(cause) if s.now() >= deadline => {
                a["unreadable"] = json!(cause.unwrap_or("no_answer"));
                break "github_unreadable";
            }
            Err(_) => {
                s.pause(POLL);
                continue;
            }
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
        // The checks are judged at every reading, also while the gate waits for the review: a
        // rerun that is pending is waited for (until the bound of the phase).
        a["checks"] = json!(v.checks);
        let judged: Vec<_> = v.checks.iter().filter(|c| c["judged"] != false).collect();
        let bad = judged
            .iter()
            .find(|c| c["state"] != "SUCCESS" && c["state"] != "PENDING");
        if let Some(c) = bad {
            a["failed_check"] = (*c).clone();
            break "check_failed";
        }
        if judged.is_empty() || judged.iter().any(|c| c["state"] == "PENDING") {
            if s.now() >= deadline {
                break "check_timeout";
            }
            s.pause(POLL);
            continue;
        }
        if q.login.is_none() {
            break "passed";
        }
        if !reviewing {
            (reviewing, deadline) = (true, s.now() + q.review);
            // A push to a ready pull request is not reviewed: when the summary names another
            // commit, draft and ready once, unless this gate's own ready mark asked for a review.
            if v.marker["commit_sha"] != v.head.as_str() && a["marked_ready"] == false {
                if !ready(s, &q, true) {
                    break "ready_failed";
                }
                // After the draft mark, ready is tried three times before the answer says so.
                let mut back = ready(s, &q, false);
                for _ in 0..2 {
                    if back {
                        break;
                    }
                    s.pause(POLL);
                    back = ready(s, &q, false);
                }
                if !back {
                    a["left_draft"] = json!(true);
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
    a["threads"] = if end == "github_unreadable" {
        Value::Null
    } else {
        threads(s, &q)
    };
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
        Err(e) => (
            json!({"error":{"code":e.code,"field":e.field,"detail":e.detail}}),
            2,
            false,
        ),
    };
    let written = if out {
        writeln!(std::io::stdout(), "{line}")
    } else {
        writeln!(std::io::stderr(), "{line}")
    };
    ExitCode::from(if written.is_err() { 4 } else { code })
}
