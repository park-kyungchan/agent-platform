//! `ap gate` against a fake GitHub: every `gh api graphql` reading is scripted, `gh pr ready` is
//! recorded, and the clock is virtual (a pause moves it). Each test names the case of
//! `await-autoreview.sh` or of the gate part of `deliver.sh` it now holds.
use ap::gate::execute;
use ap::work::{Answer, Failure, Reply, System};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const H: &str = "1111111111111111111111111111111111111111";
const H2: &str = "2222222222222222222222222222222222222222";
const OLD: &str = "0000000000000000000000000000000000000000";

struct Gh {
    /// The readings in order; the last one repeats.
    views: Vec<Value>,
    threads: Vec<Value>,
    calls: Vec<Vec<String>>,
    base: Instant,
    clock: Duration,
    /// `gh pr ready` fails.
    ready_fails: bool,
}
impl Gh {
    fn new(views: Vec<Value>) -> Self {
        Self {
            views,
            threads: vec![],
            calls: vec![],
            base: Instant::now(),
            clock: Duration::ZERO,
            ready_fails: false,
        }
    }
    fn readies(&self) -> Vec<String> {
        let ready = self.calls.iter().filter(|c| c[0] == "pr");
        ready.map(|c| c[c.len() - 1].clone()).collect()
    }
}
impl System for Gh {
    fn call(&mut self, program: &str, args: &[String], _: &Value, _: u64) -> Answer {
        assert_eq!(program, "gh");
        self.calls.push(args.to_vec());
        let ok = |v: Value| Reply {
            success: true,
            bytes: vec![],
            stdout: serde_json::to_vec(&v).unwrap(),
        };
        if args[0] == "pr" {
            assert_eq!(&args[..5], ["pr", "ready", "7", "-R", "o/r"]);
            return Ok(Reply {
                success: !self.ready_fails,
                bytes: vec![],
                stdout: vec![],
            });
        }
        assert_eq!(
            &args[..8],
            [
                "api", "graphql", "-F", "owner=o", "-F", "name=r", "-F", "n=7"
            ]
        );
        let query = args.last().unwrap();
        if query.contains("reviewThreads") {
            let page = if args.iter().any(|a| a == "after=c1") {
                1
            } else {
                0
            };
            return Ok(ok(self.threads.get(page).cloned().unwrap_or(json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}}))));
        }
        let v = if self.views.len() > 1 {
            self.views.remove(0)
        } else {
            self.views[0].clone()
        };
        if v.is_null() {
            return Err(Failure {
                code: "process_failed",
                field: "gh".into(),
                detail: Value::Null,
            });
        }
        Ok(ok(v))
    }
    fn now(&self) -> Instant {
        self.base + self.clock
    }
    fn pause(&mut self, d: Duration) {
        self.clock += d;
    }
}

fn check(name: &str, status: &str, conclusion: Option<&str>, required: bool) -> Value {
    json!({"__typename":"CheckRun","name":name,"status":status,"conclusion":conclusion,"isRequired":required,
           "checkSuite":{"workflowRun":{"url":format!("https://github.com/o/r/actions/runs/{name}")}}})
}
fn marker(author: &str, kind: &str, commit: &str, status: &str) -> Value {
    let state = json!({"code_review":{"commit_sha":commit,"has_any_findings":false,"has_unresolved_findings":false,"status":status}});
    json!({"author":{"__typename":kind,"login":author},
           "body":format!("## Codex Review\nsummary\n<!-- codex-review-state:v1 {state} -->\n")})
}
fn bot(commit: &str, status: &str) -> Value {
    marker("chatgpt-codex-connector", "Bot", commit, status)
}
/// One reading: state, draft, head, the checks of the head, the comments.
fn view(state: &str, draft: bool, head: &str, checks: Vec<Value>, comments: Vec<Value>) -> Value {
    json!({"data":{"repository":{"pullRequest":{"state":state,"isDraft":draft,"headRefOid":head,
        "commits":{"nodes":[{"commit":{"oid":head,"statusCheckRollup":{"contexts":{"nodes":checks}}}}]},
        "comments":{"nodes":comments}}}}})
}
fn green(comments: Vec<Value>) -> Value {
    view(
        "OPEN",
        false,
        H,
        vec![check("rust", "COMPLETED", Some("SUCCESS"), true)],
        comments,
    )
}
fn gate(gh: &mut Gh, extra: Value) -> (Value, u8) {
    let mut r = json!({"repository":"o/r","pull_request":7,"head":H});
    for (k, v) in extra.as_object().unwrap() {
        r[k] = v.clone();
    }
    execute(gh, &r).unwrap()
}

/// deliver.sh step 2 "passed" and await-autoreview "reviewed": a passed head ends `passed`, exit 0,
/// with the reviewer's summary of that head; the gate writes nothing.
#[test]
fn a_passed_head() {
    let mut gh = Gh::new(vec![green(vec![bot(H, "completed")])]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(v["head"], H);
    assert_eq!(
        v["checks"],
        json!([{"name":"rust","state":"SUCCESS","run":"https://github.com/o/r/actions/runs/rust"}])
    );
    assert_eq!(
        v["autoreview"],
        json!({"login":"chatgpt-codex-connector[bot]","state":"completed",
        "commit":H,"has_unresolved_findings":false,"toggled":false})
    );
    assert!(gh.readies().is_empty() && gh.clock.is_zero());
    assert_eq!(v["threads"], json!([]));
}

/// deliver.sh "check_failed": a failed entry of a required check ends `check_failed`, naming the
/// check and its run; a failed check that is not required is not read.
#[test]
fn a_failed_check() {
    let checks = vec![
        check("lint", "COMPLETED", Some("FAILURE"), false),
        check("rust", "COMPLETED", Some("SUCCESS"), true),
        check("rust", "COMPLETED", Some("FAILURE"), true),
    ];
    let mut gh = Gh::new(vec![view("OPEN", false, H, checks, vec![])]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("check_failed"), 11));
    assert_eq!(
        v["failed_check"],
        json!({"name":"rust","state":"FAILURE","run":"https://github.com/o/r/actions/runs/rust"})
    );
    assert_eq!(v["autoreview"]["state"], "waiting");
}

/// deliver.sh "wait" then "passed": no entry yet, then a pending one, then a pass. Every entry
/// must pass, and at least one must exist.
#[test]
fn a_pending_check_that_passes() {
    let pending = check("rust", "IN_PROGRESS", None, true);
    let mut gh = Gh::new(vec![
        view("OPEN", false, H, vec![], vec![]),
        view(
            "OPEN",
            false,
            H,
            vec![
                pending.clone(),
                check("rust", "COMPLETED", Some("SUCCESS"), true),
            ],
            vec![],
        ),
        green(vec![bot(H, "completed")]),
    ]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(gh.clock, Duration::from_secs(60));
}

/// deliver.sh "check_timeout": a check that never ends ends `check_timeout` at `check_minutes`.
#[test]
fn a_check_that_never_ends_times_out() {
    let mut gh = Gh::new(vec![view(
        "OPEN",
        false,
        H,
        vec![check("rust", "QUEUED", None, true)],
        vec![],
    )]);
    let (v, code) = gate(&mut gh, json!({"check_minutes": 2}));
    assert_eq!((v["end"].as_str(), code), (Some("check_timeout"), 11));
    assert_eq!(gh.clock, Duration::from_secs(120));
}

/// await-autoreview took the last marker of any author: a marker in a user's comment does not
/// count, even one that names the head as completed (the review then times out).
#[test]
fn a_marker_of_another_author() {
    let forged = marker("chatgpt-codex-connector", "User", H, "completed");
    let other = marker("someone", "User", H, "completed");
    let mut gh = Gh::new(vec![green(vec![bot(OLD, "completed"), forged, other])]);
    let (v, code) = gate(&mut gh, json!({"autoreview_minutes": 1}));
    assert_eq!((v["end"].as_str(), code), (Some("passed"), 0));
    assert_eq!(v["autoreview"]["state"], "timeout", "{v}");
    assert_eq!(v["autoreview"]["commit"], Value::Null);
}

/// await-autoreview "summary names another commit": the marker of another head does not count;
/// the gate waits until the summary names this head as completed (in progress first).
#[test]
fn a_marker_for_another_head() {
    let mut gh = Gh::new(vec![
        green(vec![bot(OLD, "completed")]),
        green(vec![bot(H, "in_progress")]),
        green(vec![bot(H, "completed")]),
    ]);
    let (v, _) = gate(&mut gh, json!({}));
    assert_eq!(
        (v["end"].as_str(), v["autoreview"]["state"].as_str()),
        (Some("passed"), Some("completed"))
    );
    assert_eq!(gh.clock, Duration::from_secs(60));
}

/// await-autoreview "marks it draft and ready again, ONCE": the toggle happens once however long
/// the summary names another commit, and never when the summary already names the head.
#[test]
fn the_toggle_happens_once() {
    let mut gh = Gh::new(vec![green(vec![bot(OLD, "completed")])]);
    let (v, _) = gate(&mut gh, json!({"autoreview_minutes": 5}));
    assert_eq!(gh.readies(), ["--undo", "o/r"], "draft, then ready");
    assert_eq!(v["autoreview"]["toggled"], true);
    let mut gh = Gh::new(vec![
        green(vec![bot(H, "in_progress")]),
        green(vec![bot(H, "completed")]),
    ]);
    let (v, _) = gate(&mut gh, json!({}));
    assert!(gh.readies().is_empty() && v["autoreview"]["toggled"] == false);
    // No summary at all is another commit too (await-autoreview compared an empty field).
    let mut gh = Gh::new(vec![green(vec![]), green(vec![bot(H, "completed")])]);
    gate(&mut gh, json!({}));
    assert_eq!(gh.readies(), ["--undo", "o/r"]);
}

/// await-autoreview "autoreview_timeout" (exit 5) and deliver.sh "autoreview_unanswered": the time
/// limit passing is not a failure; the answer says so and the gate ends `passed`.
#[test]
fn the_time_limit_of_the_review() {
    let mut gh = Gh::new(vec![green(vec![bot(H, "in_progress")])]);
    let (v, code) = gate(&mut gh, json!({"autoreview_minutes": 3}));
    assert_eq!(
        (v["end"].as_str(), code, v["autoreview"]["state"].as_str()),
        (Some("passed"), 0, Some("timeout"))
    );
    assert_eq!(gh.clock, Duration::from_secs(180));
    // deliver.sh "autoreview_findings": the findings flag is reported, not decided.
    let mut done = bot(H, "completed");
    let body = done["body"].as_str().unwrap().replace(
        "\"has_unresolved_findings\":false",
        "\"has_unresolved_findings\":true",
    );
    done["body"] = json!(body);
    let (v, code) = gate(&mut Gh::new(vec![green(vec![done])]), json!({}));
    assert_eq!(
        (code, &v["autoreview"]["has_unresolved_findings"]),
        (0, &json!(true))
    );
}

/// await-autoreview "pr_is_draft" (exit 4) and deliver.sh "gh pr ready": a draft is marked ready by
/// the gate, once, and that mark asks for the review: no toggle follows. A failed mark is an end.
#[test]
fn a_draft() {
    let mut gh = Gh::new(vec![
        view(
            "OPEN",
            true,
            H,
            vec![check("rust", "COMPLETED", Some("SUCCESS"), true)],
            vec![bot(OLD, "completed")],
        ),
        green(vec![bot(OLD, "completed")]),
        green(vec![bot(H, "completed")]),
    ]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(gh.readies(), ["o/r"], "one ready mark, no undo");
    assert_eq!(
        (
            v["marked_ready"].as_bool(),
            v["autoreview"]["toggled"].as_bool()
        ),
        (Some(true), Some(false))
    );
    let mut gh = Gh::new(vec![view("OPEN", true, H, vec![], vec![])]);
    gh.ready_fails = true;
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("ready_failed"), 12));
}

/// deliver.sh "head_moved_or_not_open" (not open): merged while waiting, and closed.
#[test]
fn merged_while_waiting_and_closed() {
    let pending = vec![check("rust", "IN_PROGRESS", None, true)];
    let mut gh = Gh::new(vec![
        view("OPEN", false, H, pending.clone(), vec![]),
        view("MERGED", false, H, pending.clone(), vec![]),
    ]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!(
        (v["end"].as_str(), code, v["head"].as_str()),
        (Some("merged"), 10, Some(H))
    );
    let (v, code) = gate(
        &mut Gh::new(vec![view("CLOSED", false, H, pending, vec![])]),
        json!({}),
    );
    assert_eq!((v["end"].as_str(), code), (Some("closed"), 10));
}

/// deliver.sh "head_moved_or_not_open" (moved) and await-autoreview "head_differs": the head GitHub
/// reports is the one waited on; a caller's head that differs is an end of its own, and a head that
/// moves while the gate waits (during the check or the review) is another.
#[test]
fn a_head_that_moves_while_waiting() {
    let pending = vec![check("rust", "IN_PROGRESS", None, true)];
    let mut gh = Gh::new(vec![
        view("OPEN", false, H, pending.clone(), vec![]),
        view("OPEN", false, H2, pending, vec![]),
    ]);
    let (v, code) = gate(&mut gh, json!({}));
    assert_eq!((v["end"].as_str(), code), (Some("head_moved"), 10));
    assert_eq!(
        (v["head"].as_str(), v["head_now"].as_str()),
        (Some(H), Some(H2))
    );
    let mut gh = Gh::new(vec![green(vec![]), view("OPEN", false, H2, vec![], vec![])]);
    let (v, _) = gate(&mut gh, json!({}));
    assert_eq!(v["end"], "head_moved", "a push during the review");
    let mut gh = Gh::new(vec![green(vec![bot(H, "completed")])]);
    let (v, code) = gate(&mut gh, json!({"head": H2}));
    assert_eq!(
        (
            v["end"].as_str(),
            code,
            v["expected"].as_str(),
            v["head"].as_str()
        ),
        (Some("head_differs"), 10, Some(H2), Some(H))
    );
    // Without a caller's head the gate takes GitHub's.
    let mut gh = Gh::new(vec![green(vec![bot(H, "completed")])]);
    let (v, _) = execute(&mut gh, &json!({"repository":"o/r","pull_request":7})).unwrap();
    assert_eq!(
        (v["end"].as_str(), v["head"].as_str()),
        (Some("passed"), Some(H))
    );
}

/// Every unresolved thread, over two pages, with id, path, line, author and whether it is answered
/// (its last comment is not the automatic reviewer's); resolved threads are left out.
#[test]
fn unresolved_threads_answered_and_not() {
    let author =
        |kind: &str, login: &str| json!({"nodes":[{"author":{"__typename":kind,"login":login}}]});
    let t = |id: &str, resolved: bool, last: Value| {
        json!({"id":id,"isResolved":resolved,"path":"src/a.rs","line":4,
        "first":author("Bot","chatgpt-codex-connector"),"last":last})
    };
    let page = |nodes: Vec<Value>, next: bool| {
        json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
        "pageInfo":{"hasNextPage":next,"endCursor":if next { json!("c1") } else { Value::Null }},"nodes":nodes}}}}})
    };
    let mut gh = Gh::new(vec![green(vec![bot(H, "completed")])]);
    gh.threads = vec![
        page(
            vec![
                t("T1", false, author("User", "park-kyungchan")),
                t("T2", true, author("Bot", "chatgpt-codex-connector")),
            ],
            true,
        ),
        page(
            vec![t("T3", false, author("Bot", "chatgpt-codex-connector"))],
            false,
        ),
    ];
    let (v, _) = gate(&mut gh, json!({}));
    assert_eq!(
        v["threads"],
        json!([
        {"id":"T1","path":"src/a.rs","line":4,"author":"chatgpt-codex-connector[bot]","answered":true},
        {"id":"T3","path":"src/a.rs","line":4,"author":"chatgpt-codex-connector[bot]","answered":false}])
    );
}

/// GitHub that cannot be read ends `github_unreadable` at the bound; a failed reading in between
/// is read again. `autoreview: false` (deliver.sh --no-autoreview) ends at the passed check.
#[test]
fn unreadable_github_and_no_automatic_reviewer() {
    let mut gh = Gh::new(vec![Value::Null]);
    let (v, code) = gate(&mut gh, json!({"check_minutes": 1}));
    assert_eq!((v["end"].as_str(), code), (Some("github_unreadable"), 12));
    let mut gh = Gh::new(vec![Value::Null, green(vec![])]);
    let (v, _) = gate(&mut gh, json!({"autoreview": false}));
    assert_eq!(
        (v["end"].as_str(), v["autoreview"]["state"].as_str()),
        (Some("passed"), Some("off"))
    );
    assert!(gh.readies().is_empty());
}

#[test]
fn refusals() {
    let mut gh = Gh::new(vec![green(vec![])]);
    for (r, field) in [
        (json!({"repository":"o","pull_request":7}), "repository"),
        (json!({"repository":"o/r/x","pull_request":7}), "repository"),
        (json!({"repository":"-o/r","pull_request":7}), "repository"),
        (json!({"repository":"o/r","pull_request":0}), "pull_request"),
        (
            json!({"repository":"o/r","pull_request":7,"head":"ABC"}),
            "head",
        ),
        (
            json!({"repository":"o/r","pull_request":7,"autoreview":true}),
            "autoreview",
        ),
        (
            json!({"repository":"o/r","pull_request":7,"check_minutes":0}),
            "check_minutes",
        ),
        (
            json!({"repository":"o/r","pull_request":7,"other":1}),
            "other",
        ),
    ] {
        let e = execute(&mut gh, &r).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field));
    }
    assert!(gh.calls.is_empty(), "a refusal reads nothing");
}
