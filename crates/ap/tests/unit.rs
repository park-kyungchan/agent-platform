//! `ap unit` against fakes: a real Git origin (a bare repository in a temporary directory), a clone
//! and the worktree the command creates; the fake `claude` of tests/fixtures/fake-claude.sh in mode
//! `script` (its account's script runs in the stage's cwd and its stdout is the answer); a fake
//! GitHub that reads the head from the origin and merges into it; a fake store; `systemctl` and the
//! unit launcher replaced as in tests/run.rs; a virtual clock for the gate. The work root is the
//! temporary directory's `work/` through `System::work_root`. The fake answers `gh` with JSON that
//! holds every member the command may ask for (no `--jq`).
use ap::host::{Answer, Failure, Host, Reply, System};
use ap::unit::execute;
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    collections::{BTreeMap, BTreeSet},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const FAKE: &str = include_str!("fixtures/fake-claude.sh");
const FAKE_CODEX: &str = include_str!("fixtures/fake-codex.sh");
const BRANCH: &str = "ap/unit-test";
const IMPLEMENTER_PROMPT: &str = "Implement the page.\n";
const VERIFIER_PROMPT: &str = "LEAD VERIFIER PROMPT: review the candidate under REVIEW.md.\n";
const ANSWER_TOKEN: &str = "IMPLEMENTER-ANSWER-TOKEN";
/// Commits `src/a.rs` (inside the write set), pushes the branch, answers with the token.
const IMPLEMENT: &str = "mkdir -p src && echo 'fn a() {}' > src/a.rs && git add -A && git -c user.name=t -c user.email=t@e commit -qm a && git push -q origin HEAD:refs/heads/ap/unit-test 2>/dev/null; echo IMPLEMENTER-ANSWER-TOKEN";
const VERIFY_ACCEPT: &str = "printf 'Verdict: accept\\n%s\\nno finding' \"$(git rev-parse HEAD)\"";

struct U {
    t: PathBuf,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    /// `gh` and `ont` calls, in order.
    calls: Vec<(String, Vec<String>, Value)>,
    launches: usize,
    clock: Duration,
    pr_state: &'static str,
    merge_commit: Option<String>,
    /// How many open pull requests `gh pr list` reports for the branch (when it was pushed).
    open_prs: usize,
    /// The conclusion of the required check `rust` on the head.
    check: &'static str,
    /// Unresolved review threads the gate reads.
    threads: Vec<Value>,
    threads_unreadable: bool,
    /// Comments of the pull request (the automatic reviewer's marker).
    comments: Vec<Value>,
    /// The read after the merge still says OPEN.
    merge_unconfirmed: bool,
    /// The merge leaves an untracked file in the worktree, so its removal without force fails.
    dirty_on_merge: bool,
    /// Someone pushes another commit to the branch right after the acceptance executable ran.
    move_head_after_acceptance: bool,
    /// Someone pushes another commit to the branch right after the review was closed.
    move_head_on_close: bool,
    /// `gh pr merge` merges and still exits non-zero.
    merge_call_fails: bool,
    /// The state of the status `lead-review` on the head, when it has one.
    lead_review: Option<&'static str>,
    /// The store refuses `start_review`.
    store_refuses: bool,
    /// The fake store's reviews: the parameters of `start_review`, `id`, then those of the close.
    reviews: Vec<Value>,
    /// What happens to the last thread when the review closes: `resolved` by another hand, or a `reply`.
    thread_change_on_close: Option<&'static str>,
    /// `resolveReviewThread` answers and resolves nothing.
    resolve_fails: bool,
    /// The fake store's boards, items, objectives and decisions: `id`, `rev` and the properties.
    objects: Vec<Value>,
    /// The request ids the store applied: a repeated id answers `already_applied` and changes nothing.
    applied: BTreeSet<String>,
    /// `start_item` / `close_item` are refused with this code (`None`: the store's own criteria).
    start_refuses: Option<&'static str>,
    close_refuses: Option<&'static str>,
    /// The state the item `k1` takes when the review closes (someone drops or closes it by hand).
    item_on_close: Option<&'static str>,
    /// The same when the status `lead-review` is posted (a call with `verdict` closes no review).
    item_on_status: Option<&'static str>,
}

fn git(dir: &PathBuf, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@e"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
fn write_exec(path: &PathBuf, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

impl U {
    fn new() -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-unit-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "home", "work/task", "judge"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        write_exec(&t.join("bin/claude"), FAKE);
        write_exec(&t.join("bin/codex"), FAKE_CODEX);
        std::fs::write(t.join("bin/mode-a1"), "script").unwrap();
        std::fs::write(t.join("bin/mode-a2"), "script").unwrap();
        std::fs::write(t.join("bin/script-a1.sh"), IMPLEMENT).unwrap();
        std::fs::write(t.join("bin/script-a2.sh"), VERIFY_ACCEPT).unwrap();
        let st = Command::new("git")
            .args(["-c", "init.defaultBranch=main", "init", "-q", "--bare"])
            .arg(t.join("origin.git"))
            .status()
            .unwrap();
        assert!(st.success());
        let st = Command::new("git")
            .args(["clone", "-q"])
            .arg(t.join("origin.git"))
            .arg(t.join("clone"))
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success());
        let clone = t.join("clone");
        git(&clone, &["checkout", "-q", "-b", "main"]);
        std::fs::write(clone.join("README.md"), "base\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-qm", "base"]);
        git(&clone, &["push", "-q", "origin", "main"]);
        std::fs::write(
            t.join("work/task/TASK.md"),
            "# TASK: a unit\nWrite set: src/**, notes.md\nOther line\n",
        )
        .unwrap();
        std::fs::write(t.join("judge/implementer.md"), IMPLEMENTER_PROMPT).unwrap();
        std::fs::write(t.join("judge/verifier.md"), VERIFIER_PROMPT).unwrap();
        let accept = format!(
            "#!/bin/sh\necho \"$(pwd) $(git rev-parse HEAD)\" >> {t}/accept-calls\necho acceptance ran\nexit $(cat {t}/accept-exit 2>/dev/null || echo 0)\n",
            t = t.display()
        );
        write_exec(&t.join("judge/accept.sh"), &accept);
        Self {
            t,
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
            calls: vec![],
            launches: 0,
            clock: Duration::ZERO,
            pr_state: "OPEN",
            merge_commit: None,
            open_prs: 1,
            check: "SUCCESS",
            threads: vec![],
            threads_unreadable: false,
            comments: vec![],
            merge_unconfirmed: false,
            dirty_on_merge: false,
            move_head_after_acceptance: false,
            move_head_on_close: false,
            merge_call_fails: false,
            lead_review: None,
            store_refuses: false,
            reviews: vec![],
            thread_change_on_close: None,
            resolve_fails: false,
            objects: vec![],
            applied: BTreeSet::new(),
            start_refuses: None,
            close_refuses: None,
            item_on_close: None,
            item_on_status: None,
        }
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    fn wt(&self) -> PathBuf {
        self.t.join("work/task/wt/repo")
    }
    fn request(&self) -> Value {
        json!({"repository":"o/r","clone":self.p("clone"),"worktree":self.p("work/task/wt/repo"),
            "branch":BRANCH,"task_key":"store-task","page":self.p("work/task/TASK.md"),
            "out":self.p("work/task/u1"),"acceptance":self.p("judge/accept.sh"),
            "implementer":{"account":self.p("a1"),"model":"opus","effort":"high","prompt":self.p("judge/implementer.md")},
            "verifier":{"account":self.p("a2"),"model":"sonnet","effort":"high","prompt":self.p("judge/verifier.md")},
            "gate":{"autoreview":false}})
    }
    fn script(&self, account: &str, text: &str) {
        std::fs::write(self.t.join(format!("bin/script-{account}.sh")), text).unwrap();
    }
    fn run(&mut self, r: &Value) -> Result<(Value, u8), Failure> {
        execute(self, r)
    }
    fn record(&self, out: &str) -> Value {
        serde_json::from_slice(
            &std::fs::read(self.p(&format!("work/task/{out}.unit.json"))).unwrap(),
        )
        .unwrap()
    }
    /// Pushes another commit (the same tree) onto the branch in the origin.
    fn move_head(&self) {
        let origin = self.t.join("origin.git");
        let head = self.head().unwrap();
        let tree = git(&origin, &["rev-parse", &format!("{head}^{{tree}}")]);
        let moved = git(&origin, &["commit-tree", &tree, "-p", &head, "-m", "moved"]);
        git(
            &origin,
            &["update-ref", &format!("refs/heads/{BRANCH}"), &moved],
        );
    }
    /// The origin's head of the branch, when it was pushed.
    fn head(&self) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.t.join("origin.git"))
            .args([
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/heads/{BRANCH}"),
            ])
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_owned())
    }
    fn gh_calls(&self, first: &str, second: &str) -> Vec<Vec<String>> {
        self.calls
            .iter()
            .filter(|(p, a, _)| p == "gh" && a.len() > 1 && a[0] == first && a[1] == second)
            .map(|(_, a, _)| a.clone())
            .collect()
    }
    fn statuses(&self) -> Vec<Vec<String>> {
        self.calls
            .iter()
            .filter(|(p, a, _)| p == "gh" && a.iter().any(|x| x.contains("/statuses/")))
            .map(|(_, a, _)| a.clone())
            .collect()
    }
    fn merges(&self) -> Vec<Vec<String>> {
        self.gh_calls("pr", "merge")
    }
    fn store(&self, action: &str) -> Vec<Value> {
        self.calls
            .iter()
            .filter(|(p, _, b)| p == "ont" && b["action_type"] == action)
            .map(|(_, _, b)| b.clone())
            .collect()
    }
    /// Runs `r` and asserts it stopped at `step` with `cause` (see `stopped`).
    fn stop(&mut self, r: &Value, step: &str, cause: &str) -> Value {
        let result = self.run(r);
        self.stopped(result, step, cause)
    }
    /// Asserts the call ended at `step` with `cause`, exit 3, and that nothing was posted or merged.
    fn stopped(&self, result: Result<(Value, u8), Failure>, step: &str, cause: &str) -> Value {
        let (v, code) = result.unwrap();
        assert_eq!(
            (
                v["state"].as_str(),
                v["stopped_at"].as_str(),
                v["cause"].as_str(),
                code
            ),
            (Some("stopped"), Some(step), Some(cause), 3),
            "{v}"
        );
        assert!(self.statuses().is_empty(), "a status was posted");
        assert!(self.merges().is_empty(), "a merge was attempted");
        assert_eq!(v, self.record("u1"));
        v
    }
    fn accept_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.t.join("accept-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn gh(&mut self, a: &[String]) -> Answer {
        let ok = |v: Value| {
            // As `Host` answers: `bytes` holds stdout when it is not empty.
            let stdout = serde_json::to_vec(&v).unwrap();
            Ok(Reply {
                success: true,
                bytes: stdout.clone(),
                stdout,
            })
        };
        for bad in ["--admin", "--auto", "--force", "--squash", "--rebase"] {
            assert!(!a.iter().any(|x| x == bad), "forbidden flag {bad}: {a:?}");
        }
        let head = self.head();
        let pr = |s: &Self| {
            json!({"number":7,"state":s.pr_state,"baseRefName":"main","headRefName":BRANCH,
                "headRefOid":head,"isDraft":false,"url":"https://github.com/o/r/pull/7",
                "mergeCommit":s.merge_commit.as_ref().map(|c| json!({"oid":c}))})
        };
        match (a[0].as_str(), a.get(1).map(String::as_str).unwrap_or("")) {
            ("--version", _) => Ok(Reply {
                success: true,
                bytes: vec![],
                stdout: b"gh version 2.102.0\n".to_vec(),
            }),
            ("pr", "list") => {
                assert!(a.iter().any(|x| x == BRANCH), "{a:?}");
                let n = if head.is_some() && self.pr_state == "OPEN" {
                    self.open_prs
                } else {
                    0
                };
                let mut list = vec![];
                for i in 0..n {
                    let mut one = pr(self);
                    one["number"] = json!(7 + i);
                    list.push(one);
                }
                ok(json!(list))
            }
            ("pr", "view") => {
                assert_eq!(a[2], "7", "{a:?}");
                let mut p = pr(self);
                if let Some(old) = self.lagging() {
                    p["headRefOid"] = json!(old);
                }
                ok(p)
            }
            ("pr", "merge") => {
                let head = head.clone().unwrap();
                let at = a
                    .iter()
                    .position(|x| x == "--match-head-commit")
                    .expect("no head guard");
                assert_eq!(a[at + 1], head);
                assert!(a.iter().any(|x| x == "--merge"));
                let origin = self.t.join("origin.git");
                let tree = git(&origin, &["rev-parse", &format!("{head}^{{tree}}")]);
                let merge = git(
                    &origin,
                    &[
                        "commit-tree",
                        &tree,
                        "-p",
                        "main",
                        "-p",
                        &head,
                        "-m",
                        "merge",
                    ],
                );
                git(&origin, &["update-ref", "refs/heads/main", &merge]);
                if !self.merge_unconfirmed {
                    self.pr_state = "MERGED";
                    self.merge_commit = Some(merge);
                }
                if self.dirty_on_merge {
                    std::fs::write(self.wt().join("left.txt"), "x").unwrap();
                }
                if self.merge_call_fails {
                    return Ok(Reply {
                        success: false,
                        bytes: b"merge failed".to_vec(),
                        stdout: vec![],
                    });
                }
                ok(json!({}))
            }
            ("pr", "ready") => ok(json!({})),
            ("api", "graphql") => {
                let query = a.last().unwrap();
                let id = a
                    .iter()
                    .find_map(|x| x.strip_prefix("id="))
                    .map(str::to_owned);
                if query.contains("resolveReviewThread") {
                    let t = self.threads.iter_mut().find(|t| t["id"] == json!(id));
                    let t = t.expect("a resolve of a thread the fake does not hold");
                    if !self.resolve_fails {
                        t["isResolved"] = json!(true);
                    }
                    let thread = json!({"isResolved": t["isResolved"]});
                    return ok(json!({"data":{"resolveReviewThread":{"thread":thread}}}));
                }
                if query.contains("node(id:") {
                    let t = self.threads.iter().find(|t| t["id"] == json!(id)).unwrap();
                    let count = t["first"]["totalCount"].clone();
                    return ok(json!({"data":{"node":{"isResolved":t["isResolved"],
                        "comments":{"totalCount":count}}}}));
                }
                if query.contains("reviewThreads") {
                    // The fixer's script opens a second thread of the reviewer.
                    let new = self.t.join("new-thread").exists();
                    if new && !self.threads.iter().any(|t| t["id"] == "T2") {
                        self.threads.push(given("T2", 9));
                    }
                    if self.threads_unreadable {
                        return ok(json!({"errors":[{"message":"unavailable"}]}));
                    }
                    return ok(
                        json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
                        "pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":self.threads}}}}}),
                    );
                }
                let check = json!({"__typename":"CheckRun","name":"rust","status":"COMPLETED",
                    "conclusion":self.check,"isRequired":true,
                    "checkSuite":{"workflowRun":{"url":"https://github.com/o/r/actions/runs/1"}}});
                let mut nodes = vec![check];
                if let Some(state) = self.lead_review {
                    nodes.push(json!({"__typename":"StatusContext","context":"lead-review",
                        "state":state,"isRequired":true,"targetUrl":null}));
                }
                ok(
                    json!({"data":{"repository":{"pullRequest":{"state":self.pr_state,"isDraft":false,
                    "headRefOid":head,"commits":{"nodes":[{"commit":{"oid":head,
                    "statusCheckRollup":{"contexts":{"nodes":nodes}}}}]},
                    "comments":{"nodes":self.comments}}}}}),
                )
            }
            ("api", _) => {
                assert!(a.iter().any(|x| x == "context=lead-review"), "{a:?}");
                if let Some(state) = self.item_on_status {
                    self.set("it-k1", "state", json!(state));
                }
                ok(json!({}))
            }
            _ => panic!("unexpected gh call {a:?}"),
        }
    }
    /// GitHub still reports the head in `old-head` for the next `view-lag` readings of the pull
    /// request (files the stage's script writes).
    fn lagging(&self) -> Option<String> {
        let lag = std::fs::read_to_string(self.t.join("view-lag")).ok()?;
        let lag: u64 = lag.trim().parse().ok()?;
        if lag == 0 {
            return None;
        }
        std::fs::write(self.t.join("view-lag"), (lag - 1).to_string()).unwrap();
        // A file named in `lag-append` gets one more line at each such reading.
        if let Ok(path) = std::fs::read_to_string(self.t.join("lag-append")) {
            let mut bytes = std::fs::read(path.trim()).unwrap();
            bytes.extend(b"appended during the wait\n");
            std::fs::write(path.trim(), bytes).unwrap();
        }
        let old = std::fs::read_to_string(self.t.join("old-head")).ok()?;
        Some(old.trim().to_owned())
    }
    /// A store of reviews: `search` by `all` of `eq`, `start_review` creates `rv-<n>`,
    /// `close_review` closes it.
    fn ont(&mut self, a: &[String], body: &Value) -> Answer {
        let reply = |v: Value| {
            let stdout = serde_json::to_vec(&v).unwrap();
            Ok(Reply {
                success: true,
                bytes: stdout.clone(),
                stdout,
            })
        };
        if a[0] == "get" {
            let found = self.objects.iter().find(|o| o["id"] == body["id"]);
            let Some(o) = found else {
                return Ok(Reply {
                    success: false,
                    bytes: br#"{"status":"refused","code":"unavailable"}"#.to_vec(),
                    stdout: vec![],
                });
            };
            let rev = format!("{}#{}", o["id"]["native_key"].as_str().unwrap(), o["rev"]);
            let fields = props(o);
            return reply(
                json!({"status":"ok","result":{"objects":[{"id":o["id"],"head":rev,"observations":[{"revision":rev,"change":"update","fields":fields}]}]}}),
            );
        }
        if a[0] == "search" {
            let kind = body["object_type"].as_str().unwrap();
            let rows: Vec<Value> = match kind {
                "review" => self.reviews.clone(),
                _ => {
                    let mine = |o: &&Value| o["id"]["object_type"] == kind;
                    self.objects.iter().filter(mine).cloned().collect()
                }
            };
            let all = body["where"]["all"].as_array().unwrap();
            let matches = |r: &&Value| {
                all.iter()
                    .all(|c| r[c["eq"][0].as_str().unwrap()] == c["eq"][1])
            };
            // As the engine: an object's properties under `head.fields`, and `qualified: false` for a
            // filter on an optional property, which a running or abandoned review lacks.
            let optional = ["repository", "verdict", "report_sha256"];
            let qualified = all
                .iter()
                .all(|c| !optional.contains(&c["eq"][0].as_str().unwrap()));
            let found: Vec<_> = rows
                .iter()
                .filter(matches)
                .map(|r| {
                    let rev = format!("{}#{}", r["id"]["native_key"].as_str().unwrap(), r["rev"]);
                    json!({"id": r["id"], "head": {"fields": props(r), "revision": rev}})
                })
                .collect();
            return reply(
                json!({"status":"ok","result":{"basis":{"qualified":qualified},"objects":found,"continuation":null}}),
            );
        }
        assert_eq!(&a[..4], ["action", "apply", "--client", "lead"], "{a:?}");
        let p = &body["parameters"];
        if matches!(
            body["action_type"].as_str(),
            Some("start_item" | "close_item")
        ) {
            return self.item_action(body);
        }
        let edits = if body["action_type"] == "start_review" {
            if self.store_refuses {
                return reply(json!({"status":"error","code":"refused"}));
            }
            let review = json!({"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},
                "native_key":format!("rv-{}", self.reviews.len() + 1)});
            let mut row = p.clone();
            (row["id"], row["status"]) = (review.clone(), json!("running"));
            self.reviews.push(row);
            json!([{"change":"initial","id":review}])
        } else {
            let row = self
                .reviews
                .iter_mut()
                .find(|r| r["id"] == p["review"])
                .unwrap();
            for k in ["verdict", "report_sha256", "account", "session"] {
                row[k] = p[k].clone();
            }
            row["status"] = json!("closed");
            if let Some(t) = self.threads.last_mut() {
                match self.thread_change_on_close {
                    Some("resolved") => t["isResolved"] = json!(true),
                    Some("reply") => t["first"]["totalCount"] = json!(2),
                    _ => (),
                }
            }
            if self.move_head_on_close {
                self.move_head();
            }
            if let Some(state) = self.item_on_close {
                self.set("it-k1", "state", json!(state));
            }
            json!([{"change":"update","id":p["review"],"revision":"c#0"},{"change":"update","id":p["review"],"revision":"c#1"}])
        };
        reply(json!({"status":"ok","result":{"outcome":"applied","edits":edits}}))
    }
    /// `start_item` and `close_item` as the engine applies them: the criteria of the schema that the
    /// fake can see, a refusal in the engine's form (exit 2), a repeated request id replayed.
    fn item_action(&mut self, body: &Value) -> Answer {
        let (action, p) = (body["action_type"].as_str().unwrap(), &body["parameters"]);
        let refused = |codes: Vec<&str>| {
            let v = json!({"status":"refused","code":"submission_criteria","failed":codes});
            Ok(Reply {
                success: false,
                bytes: serde_json::to_vec(&v).unwrap(),
                stdout: vec![],
            })
        };
        let applied = |outcome: &str| {
            let v = json!({"status":"ok","result":{"outcome":outcome,"edits":[{"change":"update","id":p["item"]}]}});
            let stdout = serde_json::to_vec(&v).unwrap();
            Ok(Reply {
                success: true,
                bytes: stdout.clone(),
                stdout,
            })
        };
        let id = body["request_id"].as_str().unwrap().to_owned();
        if self.applied.contains(&id) {
            return applied("already_applied");
        }
        let item = self.objects.iter().find(|o| o["id"] == p["item"]).cloned();
        let key = p["item"]["native_key"].as_str().unwrap().to_owned();
        let state = item.as_ref().map(|i| i["state"].clone());
        if action == "start_item" {
            let mut failed = vec![];
            if state != Some(json!("queued")) {
                failed.push("item_not_queued");
            }
            let task = item.as_ref().map(|i| i["task"].clone());
            let ordered = self.objects.iter().any(|b| {
                b["id"]["object_type"] == "board"
                    && Some(&b["task"]) == task.as_ref()
                    && b["order"].as_array().unwrap().contains(&p["item"])
            });
            if !ordered {
                failed.push("item_not_ordered");
            }
            failed.extend(self.start_refuses);
            if !failed.is_empty() {
                return refused(failed);
            }
            self.set(&key, "state", json!("in_work"));
            self.set(&key, "repository", p["repository"].clone());
            self.set(&key, "branch", p["branch"].clone());
        } else {
            let mut failed = vec![];
            if state != Some(json!("in_work")) {
                failed.push("item_not_in_work");
            }
            failed.extend(self.close_refuses);
            if !failed.is_empty() {
                return refused(failed);
            }
            self.set(&key, "state", json!("done"));
            self.set(&key, "review", p["review"].clone());
        }
        self.applied.insert(id);
        applied("applied")
    }
    /// Sets a property of the object whose native key is `key`, as an action does: a new revision.
    fn set(&mut self, key: &str, k: &str, v: Value) {
        let o = self
            .objects
            .iter_mut()
            .find(|o| o["id"]["native_key"] == key);
        let o = o.expect("an object of the fake store");
        o[k] = v;
        o["rev"] = json!(o["rev"].as_u64().unwrap() + 1);
    }
    fn object(&self, key: &str) -> Value {
        let o = self.objects.iter().find(|o| o["id"]["native_key"] == key);
        o.unwrap().clone()
    }
    /// A board for the task `store-task`: the objective `o1` stated from the user's current row
    /// `d1`, the items `(key, state)` with the objective `o1`, the order naming `ordered` (keys).
    fn board(&mut self, items: &[(&str, &str)], ordered: &[&str]) {
        let task = oid("task", "store-task");
        let d1 = json!({"id": oid("decision", "d1"), "rev": 0, "task": task, "topic": "t1",
            "decided_by": "user", "status": "current", "decided_at": "2026-11-15T10:00:00Z",
            "triage": "intent"});
        let o1 = json!({"id": oid("objective", "o1"), "rev": 0, "task": task, "name": "ship-board",
            "statement": "OBJECTIVE-STATEMENT the board orders the work", "rank": 1,
            "status": "stated", "source": oid("decision", "d1")});
        self.objects.extend([d1, o1]);
        for (key, state) in items {
            self.objects.push(json!({"id": oid("item", &format!("it-{key}")), "rev": 0,
                "task": task, "key": key, "title": format!("TITLE of {key}"),
                "statement": format!("STATEMENT of {key}"), "first_use": format!("FIRST-USE of {key}"),
                "objective": oid("objective", "o1"), "origin": oid("decision", "d1"),
                "state": state, "added_at": "2026-11-15T11:00:00Z"}));
        }
        let order: Vec<_> = ordered
            .iter()
            .map(|k| oid("item", &format!("it-{k}")))
            .collect();
        self.objects.push(json!({"id": oid("board", "store-task"), "rev": 0, "task": task,
            "order": order, "reason": "the order of the next work", "set_at": "2026-11-15T12:00:00Z"}));
    }
    /// The effects of a call: stages launched, GitHub calls, store writes other than `start_item`.
    fn effects(&self) -> (usize, usize, usize) {
        let gh = self.calls.iter().filter(|(p, ..)| p == "gh").count();
        let writes = self
            .calls
            .iter()
            .filter(|(p, a, b)| p == "ont" && a[0] == "action" && b["action_type"] != "start_item");
        (self.launches, gh, writes.count())
    }
}
/// The properties of a row of the fake store, in the engine's form.
fn props(r: &Value) -> serde_json::Map<String, Value> {
    let o = r.as_object().unwrap().iter();
    let mine = o.filter(|(k, _)| *k != "id" && *k != "rev");
    mine.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect()
}
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
impl Drop for U {
    fn drop(&mut self) {
        for pgid in self.groups.values() {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{pgid}")])
                .stderr(std::process::Stdio::null())
                .status();
        }
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
impl System for U {
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        if path == "/root/.claude" {
            return Ok(path.into());
        }
        Host.directory(path)
    }
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        match program {
            "gh" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                self.gh(args)
            }
            "ont" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                self.ont(args, input)
            }
            "systemctl" => {
                let unit = args.last().unwrap().clone();
                let (success, stdout) = match args[0].as_str() {
                    "stop" => {
                        if let Some(pgid) = self.groups.get(&unit) {
                            let _ = Command::new("kill")
                                .args(["-KILL", "--", &format!("-{pgid}")])
                                .stderr(std::process::Stdio::null())
                                .status();
                        }
                        self.alive.remove(&unit);
                        (true, vec![])
                    }
                    "is-active" if self.alive.contains(&unit) => (true, b"active\n".to_vec()),
                    "is-active" => (false, b"inactive\n".to_vec()),
                    _ => (false, vec![]),
                };
                Ok(Reply {
                    success,
                    bytes: stdout.clone(),
                    stdout,
                })
            }
            _ => {
                let accepting = program.ends_with("judge/accept.sh")
                    || args.iter().any(|a| a.ends_with("judge/accept.sh"));
                let answer = Host.call(program, args, input, ms);
                if accepting && self.move_head_after_acceptance {
                    self.move_head();
                }
                answer
            }
        }
    }
    fn now(&self) -> Instant {
        Instant::now() + self.clock
    }
    fn pause(&mut self, d: Duration) {
        self.clock += d;
        std::thread::sleep(d.min(Duration::from_millis(50)));
    }
    fn work_root(&self) -> PathBuf {
        self.t.join("work")
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        self.launches += 1;
        let at = argv.iter().position(|a| a == "--").unwrap();
        assert!(["claude", "codex"].contains(&argv[at + 1].as_str()));
        let unit = argv[..at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        let child = Command::new(self.t.join("bin").join(&argv[at + 1]))
            .args(&argv[at + 2..])
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(files[0].try_clone()?)
            .stdout(files[1].try_clone()?)
            .stderr(files[2].try_clone()?)
            .process_group(0)
            .spawn()?;
        self.groups.insert(unit.clone(), child.id());
        self.alive.insert(unit);
        Ok(child)
    }
    fn units(&self) -> bool {
        true
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("HOME".into(), self.p("home")),
            ("LANG".into(), "C.UTF-8".into()),
        ]
    }
}
fn code(r: Result<(Value, u8), Failure>) -> String {
    match r {
        Err(f) => format!("{}@{}", f.code, f.field),
        Ok((v, s)) => panic!("expected a refusal, got {s} {v}"),
    }
}
fn branches(u: &U) -> String {
    git(&u.t.join("clone"), &["branch", "--list"])
}
fn names(dir: &PathBuf) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    v.sort();
    v
}
fn steps(v: &Value) -> Vec<(String, String)> {
    v["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["result"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}
fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

// ---- I1 refusal before effect ----------------------------------------------------------------

#[test]
fn i1_unusable_requests_are_refused_before_any_effect() {
    let mut u = U::new();
    std::fs::write(u.t.join("work/task/none.md"), "# no write set\n").unwrap();
    std::fs::write(u.t.join("work/task/two.md"), "Write set: a\nWrite set: b\n").unwrap();
    std::fs::write(u.t.join("work/task/empty.md"), "Write set:  \n").unwrap();
    std::fs::write(u.t.join("judge/plain.sh"), "#!/bin/sh\n").unwrap();
    let cases: Vec<(&str, Value, &str)> = vec![
        ("extra", json!(1), "invalid_input@extra"),
        ("page", json!("work/task/TASK.md"), "invalid_input@page"),
        (
            "page",
            json!(u.p("work/task/missing.md")),
            "page_unreadable@page",
        ),
        (
            "page",
            json!(u.p("work/task/none.md")),
            "write_set_missing@page",
        ),
        (
            "page",
            json!(u.p("work/task/two.md")),
            "write_set_missing@page",
        ),
        (
            "page",
            json!(u.p("work/task/empty.md")),
            "write_set_missing@page",
        ),
        (
            "page",
            json!(u.p("judge/implementer.md")),
            "outside_work_root@page",
        ),
        ("out", json!(u.p("judge/u1")), "outside_work_root@out"),
        ("task_key", json!("Bad/Key"), "invalid_input@task_key"),
        (
            "acceptance",
            json!(u.p("judge/plain.sh")),
            "acceptance_unreadable@acceptance",
        ),
        (
            "acceptance",
            json!(u.p("work/task/wt/repo/accept.sh")),
            "inside_worktree@acceptance",
        ),
        ("mechanical", json!(true), "invalid_input@mechanical"),
        (
            "verdict",
            json!(u.p("work/task/v.md")),
            "invalid_input@verdict",
        ),
        (
            "acceptance_seconds",
            json!(0),
            "invalid_input@acceptance_seconds",
        ),
    ];
    for (member, value, want) in cases {
        let mut r = u.request();
        r[member] = value;
        assert_eq!(code(u.run(&r)), want, "{member}");
    }
    let mut r = u.request();
    r["implementer"]["prompt"] = json!(u.p("judge/missing.md"));
    assert_eq!(code(u.run(&r)), "prompt_unreadable@implementer.prompt");
    let mut r = u.request();
    r["verifier"]["prompt"] = json!(u.p("work/task/wt/repo/v.md"));
    assert_eq!(code(u.run(&r)), "inside_worktree@verifier.prompt");
    let mut r = u.request();
    r["implementer"]["bogus"] = json!(1);
    assert_eq!(code(u.run(&r)), "invalid_input@implementer.bogus");
    let mut r = u.request();
    r.as_object_mut().unwrap().remove("verifier");
    assert_eq!(code(u.run(&r)), "invalid_input@verifier");

    // An existing record, and an existing worktree path on a fresh call, are refused untouched.
    std::fs::write(u.t.join("work/task/u1.unit.json"), "earlier").unwrap();
    assert_eq!(code(u.run(&u.request())), "out_exists@out");
    assert_eq!(
        std::fs::read(u.t.join("work/task/u1.unit.json")).unwrap(),
        b"earlier"
    );
    std::fs::remove_file(u.t.join("work/task/u1.unit.json")).unwrap();
    std::fs::create_dir_all(u.wt()).unwrap();
    assert_eq!(code(u.run(&u.request())), "worktree_exists@worktree");
    std::fs::remove_dir(u.wt()).unwrap();
    let mut r = u.request();
    r["pull_request"] = json!(7);
    assert_eq!(code(u.run(&r)), "worktree_missing@worktree");

    // Nothing exists afterwards: no file of the call, no branch, no worktree, no process, no call.
    assert_eq!(
        names(&u.t.join("work/task")),
        ["TASK.md", "empty.md", "none.md", "two.md", "wt"]
    );
    assert_eq!(names(&u.t.join("work/task/wt")), Vec::<String>::new());
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn i1_the_binary_refuses_with_exit_2_and_the_error_form_of_ap_run() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("unit")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"bogus":1}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(e["error"]["code"], "invalid_input");
    assert_eq!(e["error"]["field"], "bogus");
    assert!(e["error"].as_object().unwrap().contains_key("detail"));
}

// ---- I2 one pass -------------------------------------------------------------------------------

#[test]
fn i2_a_repair_verdict_ends_the_call_with_one_implementer_and_one_verifier() {
    let mut u = U::new();
    u.script(
        "a2",
        "printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"",
    );
    let v = u.stop(&u.request(), "verifier", "verdict_repair");
    assert_eq!(
        u.launches, 2,
        "one implementer and one verifier, nothing more"
    );
    let roles: Vec<String> = ["implementer", "verifier"]
        .iter()
        .map(|k| {
            let rec: Value =
                serde_json::from_slice(&std::fs::read(v["stages"][k].as_str().unwrap()).unwrap())
                    .unwrap();
            rec["role"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(roles, ["implementer", "reviewer"]);
    assert_eq!(
        u.gh_calls("api", "graphql")
            .iter()
            .filter(|a| !a.last().unwrap().contains("reviewThreads"))
            .count(),
        1
    );
    assert_eq!(u.accept_calls().len(), 1);
    assert!(u.store("start_review").is_empty() && u.store("close_review").is_empty());
}

#[test]
fn i2_a_come_back_call_starts_no_implementer_and_resumes_the_verifier_that_reviewed() {
    let mut u = U::new();
    u.script(
        "a2",
        "printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"",
    );
    u.stop(&u.request(), "verifier", "verdict_repair");
    let first = std::fs::read(u.t.join("work/task/u1.unit.json")).unwrap();
    // The Lead repairs by hand and pushes; the repaired head returns to the reviewer's session.
    let wt = u.wt();
    std::fs::write(wt.join("notes.md"), "fixed\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "fix"]);
    git(
        &wt,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{BRANCH}")],
    );
    let head = u.head().unwrap();
    u.script("a2", VERIFY_ACCEPT);
    let launches = u.launches;
    let earlier = u.p("work/task/u1.verifier.md.run.json");
    let mut r = u.request();
    r["out"] = json!(u.p("work/task/u2"));
    r["pull_request"] = json!(7);
    r["verifier"]["resume"] = json!({"record": earlier});
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(u.launches, launches + 1, "the verifier only");
    assert_eq!(v["head"], head);
    assert_eq!(
        steps(&v)[..4],
        pairs(&[
            ("worktree", "skipped"),
            ("implementer", "skipped"),
            ("pull_request", "skipped"),
            ("gate", "passed")
        ])
    );
    let rec: Value =
        serde_json::from_slice(&std::fs::read(v["stages"]["verifier"].as_str().unwrap()).unwrap())
            .unwrap();
    let old: Value = serde_json::from_slice(&std::fs::read(&earlier).unwrap()).unwrap();
    assert_eq!(rec["resumes"], json!(earlier));
    assert_eq!(rec["session"], old["session"]);
    assert_eq!(
        std::fs::read(u.t.join("work/task/u1.unit.json")).unwrap(),
        first,
        "the earlier record is untouched"
    );
    let close = &u.store("close_review")[0]["parameters"];
    assert_eq!(close["report"], u.p("work/task/u2.verifier.md"));
    assert_eq!(
        (&close["account"], &close["session"]),
        (&json!(u.p("a2")), &old["session"])
    );
}

// ---- I3 the head -------------------------------------------------------------------------------

#[test]
fn i3_a_head_that_moves_after_the_gate_ends_without_status_or_merge() {
    let mut u = U::new();
    u.move_head_after_acceptance = true;
    let v = u.stop(&u.request(), "review", "review_head_differs");
    assert_eq!(u.launches, 2);
    assert_ne!(v["head"].as_str(), u.head().as_deref());
}

/// GitHub's head moves after the review was closed: the step `status` reads it, posts nothing.
#[test]
fn i3_a_head_that_moves_after_the_review_closed_ends_at_the_status() {
    let mut u = U::new();
    u.move_head_on_close = true;
    let v = u.stop(&u.request(), "status", "head_differs");
    assert_eq!(u.store("close_review").len(), 1);
    assert_eq!(v["detail"], json!({"head": u.head(), "state": "OPEN"}));
    assert_ne!(v["head"].as_str(), u.head().as_deref());
}

/// The acceptance executable commits in the worktree: the verifier is not launched.
#[test]
fn i3_a_local_head_that_moves_after_the_acceptance_starts_no_verifier() {
    let mut u = U::new();
    let commit = "git -c user.name=t -c user.email=t@e commit -q --allow-empty -m local";
    write_exec(
        &u.t.join("judge/accept.sh"),
        &format!("#!/bin/sh\n{commit}\n"),
    );
    let v = u.stop(&u.request(), "verifier", "head_differs");
    assert_eq!(u.launches, 1);
    assert_ne!(v["detail"]["head"], v["head"]);
    assert!(v["stages"]["verifier"].is_null());
}

#[test]
fn i3_a_dirty_worktree_or_another_local_head_runs_no_acceptance() {
    let mut u = U::new();
    u.script("a1", &format!("{IMPLEMENT}; echo x > stray.txt"));
    u.stop(&u.request(), "acceptance", "worktree_dirty");
    assert!(u.accept_calls().is_empty());

    let mut u = U::new();
    u.script(
        "a1",
        &format!(
            "{IMPLEMENT}; git -c user.name=t -c user.email=t@e commit -q --allow-empty -m local"
        ),
    );
    u.stop(&u.request(), "acceptance", "head_differs");
    assert!(u.accept_calls().is_empty());
}

#[test]
fn i3_acceptance_runs_in_the_worktree_at_the_gated_head_and_everything_names_it() {
    let mut u = U::new();
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(code, 0, "{v}");
    let head = v["head"].as_str().unwrap().to_owned();
    assert_eq!(u.accept_calls(), [format!("{} {head}", u.wt().display())]);
    assert_eq!(u.store("start_review")[0]["parameters"]["head"], head);
    assert!(
        u.statuses()[0]
            .iter()
            .any(|a| a.ends_with(&format!("/statuses/{head}")))
    );
    let m = &u.merges()[0];
    let at = m.iter().position(|a| a == "--match-head-commit").unwrap();
    assert_eq!(m[at + 1], head);
}

// ---- I4 accepting needs all of it --------------------------------------------------------------

#[test]
fn i4_a_failed_check_or_an_unresolved_thread_or_finding_stops_at_the_gate() {
    let mut u = U::new();
    u.check = "FAILURE";
    u.stop(&u.request(), "gate", "check_failed");
    assert!(u.accept_calls().is_empty());

    let mut u = U::new();
    u.threads = vec![
        json!({"id":"T1","isResolved":false,"path":"src/a.rs","line":1,
        "first":{"nodes":[{"author":{"login":"someone"}}]},"last":{"nodes":[{"author":{"login":"someone"}}]}}),
    ];
    u.stop(&u.request(), "gate", "unresolved_threads");

    // The automatic reviewer's marker on the head reports an unresolved finding. The head is pushed
    // before the call so that the marker can name it; the implementer only takes it.
    let mut u = U::new();
    let clone = u.t.join("clone");
    git(&clone, &["checkout", "-q", "-b", "pre"]);
    std::fs::create_dir_all(clone.join("src")).unwrap();
    std::fs::write(clone.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "a"]);
    git(
        &clone,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{BRANCH}")],
    );
    let head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "main"]);
    git(&clone, &["branch", "-q", "-D", "pre"]);
    u.script(
        "a1",
        &format!("git fetch -q origin {BRANCH} && git reset -q --hard FETCH_HEAD; echo done"),
    );
    let state = json!({"code_review":{"commit_sha":head,"has_any_findings":true,"has_unresolved_findings":true,"status":"completed"}});
    u.comments = vec![
        json!({"author":{"__typename":"Bot","login":"chatgpt-codex-connector"},
        "body":format!("<!-- codex-review-state:v1 {state} -->")}),
    ];
    let mut r = u.request();
    r["gate"] = json!({"autoreview_minutes":1});
    u.stop(&r, "gate", "unresolved_findings");
    assert!(u.accept_calls().is_empty());
}

/// A review thread as the gate reads it: the author of its first comment, the commit that comment
/// was made on, whether it is resolved.
fn thread(author: (&str, &str), commit: &str, resolved: bool) -> Value {
    let first = json!({"nodes":[{"author":{"__typename":author.0,"login":author.1},
        "originalCommit":{"oid":commit}}]});
    json!({"id":"T1","isResolved":resolved,"path":"src/a.rs","line":1,"first":first,"last":first})
}
const REVIEWER: (&str, &str) = ("Bot", "chatgpt-codex-connector");
/// A unit whose branch is already pushed, and whose head the automatic reviewer's summary names with
/// `has_unresolved_findings: true`; the implementer only takes the pushed head. Returns the unit, its
/// request and the head.
fn finding_on_the_head() -> (U, Value, String) {
    let mut u = U::new();
    let clone = u.t.join("clone");
    git(&clone, &["checkout", "-q", "-b", "pre"]);
    std::fs::create_dir_all(clone.join("src")).unwrap();
    std::fs::write(clone.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "a"]);
    git(
        &clone,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{BRANCH}")],
    );
    let head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "main"]);
    git(&clone, &["branch", "-q", "-D", "pre"]);
    u.script(
        "a1",
        &format!("git fetch -q origin {BRANCH} && git reset -q --hard FETCH_HEAD; echo done"),
    );
    let state = json!({"code_review":{"commit_sha":head,"has_any_findings":true,"has_unresolved_findings":true,"status":"completed"}});
    u.comments = vec![
        json!({"author":{"__typename":"Bot","login":"chatgpt-codex-connector"},
        "body":format!("<!-- codex-review-state:v1 {state} -->")}),
    ];
    let mut r = u.request();
    r["gate"] = json!({"autoreview_minutes":1});
    (u, r, head)
}

/// The summary's flag does not change when a person answers and resolves a finding. A finding that
/// is visible as a thread of the reviewer on the head, every such thread resolved, passes the gate
/// step: the threads decide.
#[test]
fn r3_resolved_threads_on_the_head_pass_the_gate_step() {
    let (mut u, r, head) = finding_on_the_head();
    u.threads = vec![thread(REVIEWER, &head, true)];
    let (v, _) = u.run(&r).unwrap();
    let gate = v["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "gate")
        .unwrap();
    assert_eq!(gate["result"], "passed", "{v}");
    assert_ne!(v["cause"], "unresolved_findings", "{v}");
    assert_eq!(v["state"], "merged", "{v}");
}

/// Nothing shows that anyone answered a finding that is no thread of the reviewer on the head: the
/// summary's flag stops the step, whatever other threads there are.
#[test]
fn r3_findings_without_a_thread_on_the_head_still_stop() {
    let other = "0000000000000000000000000000000000000000";
    let cases: [(&str, Vec<Value>); 3] = [
        ("no thread", vec![]),
        (
            "a resolved thread of the reviewer on another commit",
            vec![thread(REVIEWER, other, true)],
        ),
        (
            "a resolved thread of a person on the head",
            vec![thread(("User", "someone"), "HEAD", true)],
        ),
    ];
    for (what, mut threads) in cases {
        let (mut u, r, head) = finding_on_the_head();
        for t in &mut threads {
            if t["first"]["nodes"][0]["originalCommit"]["oid"] == "HEAD" {
                t["first"]["nodes"][0]["originalCommit"]["oid"] = json!(head);
            }
        }
        u.threads = threads;
        let v = u.stop(&r, "gate", "unresolved_findings");
        assert!(u.accept_calls().is_empty(), "{what}");
        // The gate counted no thread of the reviewer on the head.
        let answer: Value =
            serde_json::from_slice(&std::fs::read(v["files"]["gate"].as_str().unwrap()).unwrap())
                .unwrap();
        assert_eq!(answer["autoreview"]["threads_on_head"], 0, "{what}");
    }
}

/// An unresolved thread of the pull request stops the step, with or without the summary's flag, on
/// any commit and by any author.
#[test]
fn r3_an_unresolved_thread_still_stops() {
    let (mut u, r, head) = finding_on_the_head();
    u.threads = vec![
        thread(REVIEWER, &head, true),
        thread(REVIEWER, &head, false),
    ];
    let v = u.stop(&r, "gate", "unresolved_threads");
    assert_eq!(
        v["detail"]["threads"].as_array().map(Vec::len),
        Some(1),
        "{v}"
    );
    assert!(u.accept_calls().is_empty());

    let other = "0000000000000000000000000000000000000000";
    for person in [false, true] {
        let (mut u, r, head) = finding_on_the_head();
        let unresolved = if person {
            thread(("User", "someone"), &head, false)
        } else {
            thread(REVIEWER, other, false)
        };
        u.threads = vec![thread(REVIEWER, &head, true), unresolved];
        u.stop(&r, "gate", "unresolved_threads");
    }
    // No flag: the unresolved thread alone stops it.
    let mut u = U::new();
    u.threads = vec![thread(REVIEWER, other, false)];
    u.stop(&u.request(), "gate", "unresolved_threads");
}

#[test]
fn i4_a_failed_acceptance_or_a_path_outside_the_write_set_posts_nothing() {
    let mut u = U::new();
    std::fs::write(u.t.join("accept-exit"), "1").unwrap();
    let v = u.stop(&u.request(), "acceptance", "acceptance_failed");
    assert_eq!(v["detail"]["exit"], 1);
    let log = std::fs::read_to_string(v["files"]["acceptance_output"].as_str().unwrap()).unwrap();
    assert!(log.contains("acceptance ran"));
    assert_eq!(u.launches, 1, "no verifier after a failed acceptance");

    let mut u = U::new();
    u.script(
        "a1",
        &IMPLEMENT.replace("git add -A", "echo x > other.txt && git add -A"),
    );
    let v = u.stop(&u.request(), "write_set", "path_outside_write_set");
    assert_eq!(v["detail"], json!({"paths":["other.txt"],"count":1}));
    assert_eq!(v["write_set"], json!(["src/**", "notes.md"]));
}

#[test]
fn i4_a_verdict_must_accept_and_name_the_head() {
    let mut u = U::new();
    u.script(
        "a2",
        "printf 'Verdict: accept\\n%s\\n' 0000000000000000000000000000000000000000",
    );
    u.stop(&u.request(), "verifier", "verdict_head_differs");
    assert!(u.store("close_review").is_empty());

    let mut u = U::new();
    u.script("a2", "echo 'Looks fine to me'");
    let v = u.stop(&u.request(), "verifier", "no_result");
    assert!(v["stages"]["verifier"].is_string());
}

/// The status `lead-review` on the head is not judged by the gate; its state is in the step's
/// detail, so that a revoked acceptance is seen.
#[test]
fn i4_the_gate_step_names_the_state_of_lead_review() {
    let mut u = U::new();
    let (v, _) = u.run(&u.request()).unwrap();
    assert_eq!(v["steps"][3]["detail"], json!({"lead_review": null}));
    let mut u = U::new();
    u.lead_review = Some("FAILURE");
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v["steps"][3]["detail"], json!({"lead_review": "FAILURE"}));
}

// ---- I8 one verdict of a model per accept ------------------------------------------------------

#[test]
fn p2_head_line_with_a_label_is_the_head() {
    for line in ["Head: %s", "`%s`", ":%s:", "head_%s_suffix", "«%s»"] {
        let mut u = U::new();
        u.script(
            "a2",
            &format!("printf 'Verdict: accept\\n{line}\\nno finding' \"$(git rev-parse HEAD)\""),
        );
        let mut r = u.request();
        r["merge"] = json!(false);
        let (first, code) = u.run(&r).unwrap();
        assert_eq!(
            (first["state"].as_str(), code),
            (Some("accepted"), 0),
            "{line}: {first}"
        );
        assert_eq!(u.store("close_review").len(), 1);
        // The same labeled report is also valid on a come-back with `verdict`.
        r.as_object_mut().unwrap().remove("verifier");
        r["pull_request"] = json!(7);
        r["verdict"] = json!(u.p("work/task/u1.verifier.md"));
        r["out"] = json!(u.p("work/task/u2"));
        let launches = u.launches;
        let (v, code) = u.run(&r).unwrap();
        assert_eq!(
            (v["state"].as_str(), code),
            (Some("accepted"), 0),
            "{line}: {v}"
        );
        assert_eq!(v["review"], first["review"]);
        assert_eq!(u.launches, launches);
    }
}

#[test]
fn p2_head_line_with_two_hex_runs_or_another_head_differs() {
    let other = "0".repeat(40);
    for line in [
        format!("%s {other}"),
        "%s %s".to_owned(),
        "%s0".to_owned(),
        "0%s".to_owned(),
        "g%s".to_owned(),
        "%sZ".to_owned(),
        "é%s".to_owned(),
        "%sé".to_owned(),
        other,
        "no head".to_owned(),
    ] {
        let mut u = U::new();
        let expanded = line.replace("%s", "$h");
        u.script(
            "a2",
            &format!("h=$(git rev-parse HEAD); printf 'Verdict: accept\\n%s\\n' \"{expanded}\""),
        );
        let v = u.stop(&u.request(), "verifier", "verdict_head_differs");
        assert!(u.store("start_review").is_empty());
        // Admission of the existing report succeeds, then the same parser rejects its head.
        let mut r = u.request();
        r.as_object_mut().unwrap().remove("verifier");
        r["pull_request"] = json!(7);
        r["verdict"] = json!(u.p("work/task/u1.verifier.md"));
        r["out"] = json!(u.p("work/task/u2"));
        let launches = u.launches;
        let (come_back, code) = u.run(&r).unwrap();
        assert_eq!(
            (come_back["cause"].as_str(), code),
            (Some("verdict_head_differs"), 3),
            "{line}: {come_back}"
        );
        assert_eq!(come_back["stopped_at"], v["stopped_at"]);
        assert_eq!(u.launches, launches);
        assert!(
            u.store("start_review").is_empty() && u.statuses().is_empty() && u.merges().is_empty()
        );
    }
}

#[test]
fn p3_a_quoted_form_with_another_verdict_never_accepts() {
    for report in [
        "I read it. The form is:\\nVerdict: accept\\n%s\\n**Verdict: repair**\\nA finding.",
        "I read it. The form is:\\n```\\nVerdict: accept\\n%s\\n```\\n**Verdict: repair**\\nA finding.",
    ] {
        let mut u = U::new();
        u.script(
            "a2",
            &format!("printf '{report}' \"$(git rev-parse HEAD)\""),
        );
        let mut r = u.request();
        r["merge"] = json!(false);
        let v = u.stop(&r, "verifier", "no_result");
        let rec = stage_record(&v["stages"]["verifier"]);
        assert_eq!(rec["state"], "no_result");
        assert_eq!(rec["unmet"], "first_line");
        let expected = report
            .replace("\\n", "\n")
            .replace("%s", v["head"].as_str().unwrap());
        assert_eq!(
            std::fs::read(u.p("work/task/u1.verifier.md")).unwrap(),
            expected.as_bytes()
        );
        assert!(u.store("start_review").is_empty() && u.store("close_review").is_empty());
    }
}

/// Writes `<verdict>.run.json` as `ap run` writes a reviewer's record and returns it.
fn reviewer_record(u: &U, verdict: &str, prompt: &str) -> Value {
    let (v, p) = (u.p(verdict), u.p(prompt));
    let rec = json!({"role":"reviewer","state":"ok","model":"fable","effort":"max","account":u.p("a2"),
        "session":"s-1","answer":{"path":v,"sha256":sha(&std::fs::read(&v).unwrap())},
        "prompt":{"path":p,"sha256":sha(&std::fs::read(&p).unwrap())}});
    std::fs::write(format!("{v}.run.json"), rec.to_string()).unwrap();
    rec
}

#[test]
fn i8_a_request_without_a_verdict_of_a_model_is_refused_before_any_effect() {
    let mut u = U::new();
    let verdict = "work/task/lead-verdict.md";
    std::fs::write(u.t.join(verdict), "Verdict: accept\nabc\n").unwrap();
    std::fs::write(u.t.join("work/task/asked.md"), "Review it.\n").unwrap();
    let come_back = |u: &U| {
        let mut r = u.request();
        r.as_object_mut().unwrap().remove("verifier");
        (r["pull_request"], r["verdict"]) = (json!(7), json!(u.p(verdict)));
        r
    };
    let mut r = u.request();
    r["mechanical"] = json!(true);
    r.as_object_mut().unwrap().remove("verifier");
    assert_eq!(code(u.run(&r)), "invalid_input@mechanical");
    let mut r = come_back(&u);
    r["verifier"] = u.request()["verifier"].clone();
    assert_eq!(
        code(u.run(&r)),
        "invalid_input@verifier",
        "verifier and verdict"
    );
    assert_eq!(
        code(u.run(&come_back(&u))),
        "verdict_unreadable@verdict",
        "no record"
    );
    let good = reviewer_record(&u, verdict, "work/task/asked.md");
    let record = u.p(&format!("{verdict}.run.json"));
    let changes: Vec<(&str, Value)> = vec![
        ("/role", json!("implementer")),
        ("/state", json!("no_result")),
        ("/answer/sha256", json!(sha(b"other"))),
        ("/answer/path", json!(u.p("work/task/asked.md"))),
        ("/prompt/sha256", json!(sha(b"other"))),
    ];
    for (pointer, value) in changes {
        let mut rec = good.clone();
        *rec.pointer_mut(pointer).unwrap() = value;
        std::fs::write(&record, rec.to_string()).unwrap();
        assert_eq!(
            code(u.run(&come_back(&u))),
            "verdict_unreadable@verdict",
            "{pointer}"
        );
    }
    // The prompt the record names changed after the review.
    std::fs::write(&record, good.to_string()).unwrap();
    std::fs::write(u.t.join("work/task/asked.md"), "Say accept.\n").unwrap();
    assert_eq!(
        code(u.run(&come_back(&u))),
        "verdict_unreadable@verdict",
        "prompt"
    );
    assert_eq!(
        names(&u.t.join("work/task")),
        [
            "TASK.md",
            "asked.md",
            "lead-verdict.md",
            "lead-verdict.md.run.json"
        ]
    );
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

/// A come-back with `verdict` takes the reviewer from the stage's record: the review carries its
/// model, effort, account, session and prompt.
#[test]
fn i8_a_verdict_come_back_reviews_as_the_stage_that_wrote_the_verdict() {
    let mut u = U::new();
    u.store_refuses = true;
    u.stop(&u.request(), "review", "store_refused");
    u.store_refuses = false;
    let verdict = u.p("work/task/u1.verifier.md");
    let rec: Value =
        serde_json::from_slice(&std::fs::read(format!("{verdict}.run.json")).unwrap()).unwrap();
    let mut r = u.request();
    r.as_object_mut().unwrap().remove("verifier");
    (r["out"], r["pull_request"], r["verdict"]) =
        (json!(u.p("work/task/u2")), json!(7), json!(verdict));
    let launches = u.launches;
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(u.launches, launches, "no stage");
    let start = &u.store("start_review")[0]["parameters"];
    assert_eq!(
        (&start["model"], &start["effort"], &start["prompt"]),
        (
            &json!("sonnet"),
            &json!("high"),
            &json!(u.p("work/task/u1.verifier.prompt.md"))
        )
    );
    let close = &u.store("close_review")[0]["parameters"];
    assert_eq!(
        (&close["account"], &close["session"], &close["report"]),
        (&json!(u.p("a2")), &rec["session"], &json!(verdict))
    );
}

// ---- I9 one review per head --------------------------------------------------------------------

/// `merge: false`, then two come-backs with the verifier's answer: one review object; each later
/// record names it and its step `review` says `reused`.
#[test]
fn i9_calls_on_one_head_with_one_report_leave_one_review() {
    let mut u = U::new();
    let mut r = u.request();
    r["merge"] = json!(false);
    let (first, code) = u.run(&r).unwrap();
    assert_eq!(
        (first["state"].as_str(), code),
        (Some("accepted"), 0),
        "{first}"
    );
    let verdict = u.p("work/task/u1.verifier.md");
    let mut r = u.request();
    r.as_object_mut().unwrap().remove("verifier");
    (r["pull_request"], r["verdict"], r["merge"]) = (json!(7), json!(verdict), json!(false));
    for (out, merge, end) in [("u2", false, "accepted"), ("u3", true, "merged")] {
        (r["out"], r["merge"]) = (json!(u.p(&format!("work/task/{out}"))), json!(merge));
        let (v, code) = u.run(&r).unwrap();
        assert_eq!((v["state"].as_str(), code), (Some(end), 0), "{v}");
        assert_eq!(v["review"], first["review"]);
        let review = v["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "review");
        assert_eq!(review.unwrap()["detail"], json!({"reused": true}));
    }
    assert_eq!(u.reviews.len(), 1);
    assert_eq!(u.store("start_review").len(), 1);
    assert_eq!(u.store("close_review").len(), 1);
    assert_eq!((u.statuses().len(), u.merges().len()), (1, 1));
}

// ---- I10 a branch behind main ------------------------------------------------------------------

#[test]
fn i10_a_branch_behind_main_stops_at_the_write_set() {
    let mut u = U::new();
    // `main` moves while the unit runs: the branch no longer contains it.
    let other = "git push -q origin $(git -c user.name=t -c user.email=t@e commit-tree origin/main^{tree} -p origin/main -m other):refs/heads/main";
    u.script("a1", &format!("{IMPLEMENT}; {other}"));
    let v = u.stop(&u.request(), "write_set", "behind_main");
    let main = git(&u.t.join("origin.git"), &["rev-parse", "main"]);
    assert_eq!(v["detail"], json!(main));
    assert_eq!(u.launches, 1, "no verifier");
    // The one read of the store before the worktree is the task's board.
    let board =
        |(p, _, b): &(String, Vec<String>, Value)| p != "ont" || b["object_type"] == "board";
    assert!(u.calls.iter().all(board), "no review");
}

// ---- I5 the merge ------------------------------------------------------------------------------

#[test]
fn i5_an_accepted_unit_merges_once_on_its_head_and_removes_worktree_and_branch() {
    let mut u = U::new();
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        steps(&v),
        pairs(&[
            ("worktree", "passed"),
            ("implementer", "passed"),
            ("pull_request", "passed"),
            ("gate", "passed"),
            ("acceptance", "passed"),
            ("write_set", "passed"),
            ("verifier", "passed"),
            ("review", "passed"),
            ("status", "passed"),
            ("merge", "passed"),
            ("cleanup", "passed"),
        ])
    );
    assert_eq!(v["pull_request"], 7);
    assert_eq!(v["merge_commit"].as_str(), u.merge_commit.as_deref());
    assert_eq!(u.statuses().len(), 1);
    assert!(u.statuses()[0].iter().any(|a| a == "state=success"));
    assert_eq!(u.merges().len(), 1);
    // The status comes after the review was closed, the merge after the status.
    let order: Vec<&str> = u
        .calls
        .iter()
        .filter_map(|(p, a, b)| match p.as_str() {
            "ont" => b["action_type"].as_str(),
            _ if a.iter().any(|x| x.contains("/statuses/")) => Some("status"),
            _ if a.len() > 1 && a[1] == "merge" => Some("merge"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["start_review", "close_review", "status", "merge"]);
    // The review names the reviewer: the verifier stage's account and session.
    let rec: Value =
        serde_json::from_slice(&std::fs::read(v["stages"]["verifier"].as_str().unwrap()).unwrap())
            .unwrap();
    let close = &u.store("close_review")[0]["parameters"];
    assert_eq!(
        (&close["account"], &close["session"]),
        (&json!(u.p("a2")), &rec["session"])
    );
    assert!(!u.wt().exists(), "worktree removed");
    assert_eq!(branches(&u), "* main", "branch deleted");
}

#[test]
fn i5_merge_false_ends_after_the_review_with_no_status() {
    let mut u = U::new();
    let mut r = u.request();
    r["merge"] = json!(false);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    assert_eq!(steps(&v).last().unwrap().0, "review");
    assert!(u.statuses().is_empty() && u.merges().is_empty());
    assert_eq!(u.store("close_review").len(), 1);
    assert!(u.wt().exists());
}

#[test]
fn i5_a_merge_that_does_not_read_back_is_not_merged_and_removes_nothing() {
    let mut u = U::new();
    u.merge_unconfirmed = true;
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(
        (
            v["state"].as_str(),
            v["stopped_at"].as_str(),
            v["cause"].as_str(),
            code
        ),
        (Some("stopped"), Some("merge"), Some("merge_unconfirmed"), 3)
    );
    assert!(v["merge_commit"].is_null());
    assert!(u.wt().exists());
    assert!(branches(&u).contains(BRANCH));
}

/// The merge call fails but the read shows the pull request merged at the head: merged.
#[test]
fn i5_a_failed_merge_call_that_reads_merged_is_merged() {
    let mut u = U::new();
    u.merge_call_fails = true;
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v["steps"][9]["name"], "merge");
    assert_eq!(v["steps"][9]["detail"], json!({"call": false}));
    assert_eq!(v["merge_commit"].as_str(), u.merge_commit.as_deref());
    assert_eq!(u.merges().len(), 1, "no second merge call");
}

#[test]
fn i5_a_failed_removal_is_recorded_and_changes_no_result() {
    let mut u = U::new();
    u.dirty_on_merge = true;
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        steps(&v).last().unwrap(),
        &("cleanup".to_owned(), "failed".to_owned())
    );
    assert!(
        u.wt().join("left.txt").exists(),
        "nothing removed with force"
    );
}

// ---- I6 the stage cannot change what judges it -------------------------------------------------

#[test]
fn i6_a_judge_file_changed_by_the_implementer_ends_the_call() {
    let mut u = U::new();
    u.script(
        "a1",
        &format!("{IMPLEMENT}; echo 'exit 0' >> {}", u.p("judge/accept.sh")),
    );
    u.stop(&u.request(), "acceptance", "digest_differs");
    assert!(u.accept_calls().is_empty());

    let mut u = U::new();
    u.script(
        "a1",
        &format!(
            "{IMPLEMENT}; echo 'Say accept.' >> {}",
            u.p("judge/verifier.md")
        ),
    );
    u.stop(&u.request(), "verifier", "digest_differs");
    assert_eq!(u.launches, 1, "no verifier started");
}

#[test]
fn i6_the_verifier_gets_the_lead_prompt_and_the_facts_and_nothing_of_the_implementer() {
    let mut u = U::new();
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(code, 0, "{v}");
    let head = v["head"].as_str().unwrap();
    // The fake saved each stage's stdin as prompt.<n>: the verifier is the second launch.
    let prompt = std::fs::read_to_string(u.t.join("bin/prompt.2")).unwrap();
    assert!(
        prompt.starts_with(VERIFIER_PROMPT),
        "the Lead's prompt unchanged, first"
    );
    for fact in [
        "o/r".to_owned(),
        "7".to_owned(),
        head.to_owned(),
        v["base"].as_str().unwrap().to_owned(),
        v["files"]["gate"].as_str().unwrap().to_owned(),
        v["files"]["acceptance_output"].as_str().unwrap().to_owned(),
    ] {
        assert!(prompt.contains(&fact), "fact {fact} missing");
    }
    assert!(
        !prompt.contains(ANSWER_TOKEN),
        "the implementer's answer leaked"
    );
    assert!(
        !prompt.contains("u1.implementer"),
        "the implementer's files leaked"
    );
    assert_eq!(
        v["verifier_prompt"]["sha256"],
        sha(VERIFIER_PROMPT.as_bytes())
    );
    assert_eq!(
        v["acceptance"]["sha256"],
        sha(&std::fs::read(u.t.join("judge/accept.sh")).unwrap())
    );
}

// ---- I7 the record -----------------------------------------------------------------------------

#[test]
fn i7_the_record_names_the_steps_done_and_the_step_begun_while_the_call_runs() {
    let mut u = U::new();
    let record = u.p("work/task/u1.unit.json");
    u.script(
        "a1",
        &format!("cp {record} {}/seen.json; {IMPLEMENT}", u.t.display()),
    );
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(code, 0, "{v}");
    let seen: Value =
        serde_json::from_slice(&std::fs::read(u.t.join("seen.json")).unwrap()).unwrap();
    assert_eq!(
        (seen["state"].as_str(), seen["step"].as_str()),
        (Some("running"), Some("implementer"))
    );
    assert_eq!(steps(&seen), pairs(&[("worktree", "passed")]));
    let done = &seen["steps"][0];
    assert!(done["start"].is_string() && done["end"].is_string());
    // The digests of what judges the stage are on disk before it starts.
    assert_eq!(seen["acceptance"]["sha256"], v["acceptance"]["sha256"]);
    assert_eq!(
        seen["verifier_prompt"]["sha256"],
        v["verifier_prompt"]["sha256"]
    );
    assert_eq!(
        seen["page"]["sha256"],
        sha(&std::fs::read(u.t.join("work/task/TASK.md")).unwrap())
    );

    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want = vec![
        "mode",
        "repository",
        "clone",
        "worktree",
        "branch",
        "task_key",
        "base",
        "page",
        "write_set",
        "acceptance",
        "verifier_prompt",
        "verdict_file",
        "merge",
        "pull_request",
        "head",
        "review",
        "merge_commit",
        "stages",
        "files",
        "state",
        "step",
        "stopped_at",
        "cause",
        "detail",
        "steps",
        "start",
        "end",
    ];
    want.sort_unstable();
    assert_eq!(keys, want);
    assert!(v["step"].is_null() && v["end"].is_string());
    assert_eq!(v["review"]["native_key"], "rv-1");
    let text = serde_json::to_string(&v).unwrap();
    for leaked in [
        ANSWER_TOKEN,
        "Implement the page",
        "LEAD VERIFIER PROMPT",
        "acceptance ran",
    ] {
        assert!(!text.contains(leaked), "{leaked} in the record");
    }
}

#[test]
fn i7_a_failed_first_step_leaves_a_readable_record() {
    let mut u = U::new();
    let mut r = u.request();
    r["clone"] = json!(u.p("home"));
    let v = u.stop(&r, "worktree", "fetch_failed");
    assert_eq!(steps(&v), pairs(&[("worktree", "stopped")]));
    assert!(v["step"].is_null());
    assert_eq!(u.launches, 0);
}

#[test]
fn i2_next_accounts_go_to_ap_run_and_a_cut_before_work_moves_within_the_one_stage() {
    let mut u = U::new();
    std::fs::create_dir_all(u.t.join("a3")).unwrap();
    std::fs::write(u.t.join("bin/mode-a1"), "limit1").unwrap();
    std::fs::write(u.t.join("bin/mode-a3"), "script").unwrap();
    u.script("a3", IMPLEMENT);
    let mut r = u.request();
    r["implementer"]["next_accounts"] = json!([u.p("a3")]);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    let path = v["stages"]["implementer"].as_str().unwrap();
    let rec: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        (rec["account"].as_str(), rec["state"].as_str()),
        (Some(u.p("a3").as_str()), Some("ok"))
    );
    // Two launches of the one implementer stage (one record), then the verifier.
    assert_eq!(u.launches, 3);
    assert_eq!(
        steps(&v)[1],
        ("implementer".to_owned(), "passed".to_owned())
    );
}

/// A1: the Lead's own account is a stage's account like any other: as the verifier's `account`,
/// and in its `next_accounts` when the first account is cut before any work.
#[test]
fn a1_unit_takes_the_lead_account() {
    for in_next in [false, true] {
        let mut u = U::new();
        std::fs::write(u.t.join("bin/mode-.claude"), "script").unwrap();
        u.script(".claude", VERIFY_ACCEPT);
        let mut r = u.request();
        if in_next {
            std::fs::write(u.t.join("bin/mode-a2"), "limit1").unwrap();
            r["verifier"]["next_accounts"] = json!(["/root/.claude"]);
        } else {
            r["verifier"]["account"] = json!("/root/.claude");
        }
        let (v, code) = u.run(&r).unwrap();
        assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
        let path = v["stages"]["verifier"].as_str().unwrap();
        let rec: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(rec["account"], "/root/.claude");
    }
}

#[test]
fn i1_stage_members_are_judged_by_ap_run_before_any_effect() {
    let mut u = U::new();
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "timeout_seconds",
            json!(0),
            "invalid_input@verifier.timeout_seconds",
        ),
        (
            "permission_mode",
            json!("invalid"),
            "invalid_input@verifier.permission_mode",
        ),
        (
            "account",
            json!(u.p("no-such-account")),
            "account_missing@verifier.account",
        ),
    ];
    for (member, value, want) in cases {
        let mut r = u.request();
        r["verifier"][member] = value;
        assert_eq!(code(u.run(&r)), want, "{member}");
    }
    let mut r = u.request();
    r["implementer"]["next_accounts"] = json!(["relative"]);
    assert_eq!(code(u.run(&r)), "invalid_input@implementer.next_accounts");
    let mut r = u.request();
    r["verifier"]["resume"] = json!({"record": u.p("judge/none.run.json")});
    assert_eq!(code(u.run(&r)), "resume_record_missing@verifier.resume");
    // A verifier prompt the stage would refuse once the block of facts is appended (`ap run`
    // takes at most 1 MiB): refused here, not at the verifier after the implementer ran.
    let big = u.p("judge/big.md");
    std::fs::write(&big, vec![b'x'; 1_048_576 - 19]).unwrap();
    let mut r = u.request();
    r["verifier"]["prompt"] = json!(big);
    assert_eq!(code(u.run(&r)), "prompt_unreadable@verifier.prompt");
    // Nothing exists afterwards: no file of the call, no worktree, no branch, no process, no call.
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert!(!u.wt().exists());
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn i7_the_path_of_a_stage_record_is_on_disk_before_the_stage_starts() {
    let mut u = U::new();
    let record = u.p("work/task/u1.unit.json");
    let t = u.t.display().to_string();
    u.script("a1", &format!("cp {record} {t}/seen-i.json; {IMPLEMENT}"));
    u.script(
        "a2",
        &format!("cp {record} {t}/seen-v.json; {VERIFY_ACCEPT}"),
    );
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(code, 0, "{v}");
    for (file, stage) in [("seen-i.json", "implementer"), ("seen-v.json", "verifier")] {
        let seen: Value = serde_json::from_slice(&std::fs::read(u.t.join(file)).unwrap()).unwrap();
        assert_eq!(seen["step"], stage);
        let want = u.p(&format!("work/task/u1.{stage}.md.run.json"));
        assert_eq!(seen["stages"][stage].as_str(), Some(want.as_str()));
    }
}

// ---- D8 a revoked review is not reused ---------------------------------------------------------

/// A come-back on a head whose only review was revoked (`revoke_review` set `status: revoked`)
/// runs exactly as a come-back on a head with no review: it starts and closes a new review and
/// posts `success` only for that one, never for the revoked review.
#[test]
fn d8_unit_does_not_reuse_revoked() {
    let mut ends = vec![];
    for revoked in [true, false] {
        let mut u = U::new();
        let mut r = u.request();
        r["merge"] = json!(false);
        let (first, code) = u.run(&r).unwrap();
        assert_eq!(
            (first["state"].as_str(), code),
            (Some("accepted"), 0),
            "{first}"
        );
        let old = first["review"]["native_key"].as_str().unwrap().to_owned();
        if revoked {
            u.reviews[0]["status"] = json!("revoked");
        } else {
            u.reviews.clear();
        }
        let before = u.calls.len();
        let verdict = u.p("work/task/u1.verifier.md");
        let mut r = u.request();
        r.as_object_mut().unwrap().remove("verifier");
        (r["pull_request"], r["verdict"]) = (json!(7), json!(verdict));
        r["out"] = json!(u.p("work/task/u2"));
        let (v, code) = u.run(&r).unwrap();
        assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
        let review = v["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "review");
        assert_eq!(review.unwrap()["detail"], json!({"reused": false}));
        let later = &u.calls[before..];
        let count = |a: &str| {
            later
                .iter()
                .filter(|(_, _, b)| b["action_type"] == a)
                .count()
        };
        assert_eq!((count("start_review"), count("close_review")), (1, 1));
        let new = v["review"]["native_key"].as_str().unwrap().to_owned();
        if revoked {
            assert_ne!(new, old);
        }
        let posted: Vec<_> = later
            .iter()
            .filter(|(p, a, _)| p == "gh" && a.iter().any(|x| x.contains("/statuses/")))
            .map(|(_, a, _)| a.last().unwrap().clone())
            .collect();
        assert_eq!(posted, [format!("description=review {new} accept")]);
        ends.push(steps(&v));
    }
    assert_eq!(ends[0], ends[1]);
}

// ---- D14 one reader of an accepting review ------------------------------------------------------

/// A come-back reads its closed accepting review by properties every review has (`kind`, `head`,
/// `status`, the task) and compares `repository`, `verdict` and the report's digest on the answer:
/// it reuses the review that matches in all three, never one that differs in one of them, and a
/// review without a verdict elsewhere in the store does not stop it (the fake answers
/// `qualified: false` to a filter on an optional property, as the engine does).
#[test]
fn d14_reuse_reads_by_required_properties() {
    for keep in [true, false] {
        let mut u = U::new();
        let mut r = u.request();
        r["merge"] = json!(false);
        let (first, code) = u.run(&r).unwrap();
        assert_eq!(
            (first["state"].as_str(), code),
            (Some("accepted"), 0),
            "{first}"
        );
        let accepted = u.reviews[0].clone();
        let mut noise = vec![];
        for (k, v) in [
            ("repository", json!("x/other")),
            ("verdict", json!("repair")),
            ("report_sha256", json!("0".repeat(64))),
        ] {
            let mut row = accepted.clone();
            row[k] = v;
            row["id"]["native_key"] = json!(format!("rv-{k}"));
            noise.push(row);
        }
        let mut running = accepted.clone();
        running.as_object_mut().unwrap().remove("verdict");
        running.as_object_mut().unwrap().remove("report_sha256");
        (running["status"], running["id"]["native_key"]) = (json!("running"), json!("rv-running"));
        noise.push(running);
        if keep {
            noise.push(accepted.clone());
        }
        u.reviews = noise;
        let before = u.calls.len();
        let mut r = u.request();
        r.as_object_mut().unwrap().remove("verifier");
        (r["pull_request"], r["verdict"], r["merge"]) = (
            json!(7),
            json!(u.p("work/task/u1.verifier.md")),
            json!(false),
        );
        r["out"] = json!(u.p("work/task/u2"));
        let (v, code) = u.run(&r).unwrap();
        assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
        let search = u.calls[before..]
            .iter()
            .find(|(p, a, b)| p == "ont" && a[0] == "search" && b["object_type"] == "review")
            .unwrap()
            .2
            .clone();
        let head = accepted["head"].clone();
        let task = json!({"object_type":"task","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"store-task"});
        assert_eq!(
            search,
            json!({"object_type":"review","where":{"all":[{"eq":["kind","packet"]},{"eq":["head",head]},
                {"eq":["status","closed"]},{"eq":["task",task]}]},"limit":200})
        );
        let review = v["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "review");
        assert_eq!(review.unwrap()["detail"], json!({"reused": keep}), "{v}");
        if keep {
            assert_eq!(v["review"], accepted["id"]);
        } else {
            assert_eq!(u.store("start_review").len(), 2);
        }
    }
}

// ---- The case call (README "One case") -------------------------------------------------------

/// The test of the accepted head: it passes only when the code it judges says `done`.
const CASE_TEST: &str = "#!/bin/sh\ntest \"$(cat src/lib.txt 2>/dev/null)\" = done\n";
const CASE_PAGE: &str = "# A past unit\nMake src/lib.txt say done.\n";
/// The candidate implements the page and answers.
const CANDIDATE: &str = "mkdir -p src && echo done > src/lib.txt; echo CANDIDATE-ANSWER";

struct Case {
    base: String,
    head: String,
    file: String,
}
/// The suite's evaluator, by a path without `..`.
fn evaluator() -> String {
    let p =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/engineering/evaluate");
    std::fs::canonicalize(p)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}
/// A past unit in the clone: `base` is `main`, the accepted `head` (on another branch) adds the
/// test `tests/t.sh` (executable) and the code it judges; the case file lies outside the work root.
fn case(u: &U) -> Case {
    let clone = u.t.join("clone");
    let base = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "-b", "accepted"]);
    std::fs::create_dir_all(clone.join("tests")).unwrap();
    std::fs::create_dir_all(clone.join("src")).unwrap();
    write_exec(&clone.join("tests/t.sh"), CASE_TEST);
    std::fs::write(clone.join("src/lib.txt"), "done\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "accepted"]);
    let head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "main"]);
    std::fs::create_dir_all(u.t.join("cases")).unwrap();
    std::fs::write(u.t.join("work/task/case.md"), CASE_PAGE).unwrap();
    let c = json!({"id": "tc-1", "repository": "o/r", "pull_request": 3, "base": base, "head": head,
        "tests": ["tests/t.sh"], "commands": [["./tests/t.sh"]], "page_sha256": sha(CASE_PAGE.as_bytes())});
    let file = u.p("cases/tc-1.json");
    std::fs::write(&file, serde_json::to_vec(&c).unwrap()).unwrap();
    u.script("a1", CANDIDATE);
    Case { base, head, file }
}
fn case_request(u: &U, c: &Case) -> Value {
    json!({"case": c.file, "clone": u.p("clone"), "worktree": u.p("work/task/copy"),
        "page": u.p("work/task/case.md"), "out": u.p("work/task/c1"), "acceptance": evaluator(),
        "implementer": {"account": u.p("a1"), "model": "opus", "effort": "high",
            "prompt": u.p("judge/implementer.md")}})
}
fn no_stage(mut r: Value) -> Value {
    r.as_object_mut().unwrap().remove("implementer");
    r
}
fn copy(u: &U) -> PathBuf {
    u.t.join("work/task/copy")
}
/// The result of a case call: state, exit, `stopped_at`, `cause`, `detail`.
fn ended(r: Result<(Value, u8), Failure>) -> (Value, (String, u8, Value, Value, Value)) {
    let (v, code) = r.unwrap();
    let state = v["state"].as_str().unwrap().to_owned();
    let end = (
        state,
        code,
        v["stopped_at"].clone(),
        v["cause"].clone(),
        v["detail"].clone(),
    );
    (v, end)
}
fn evaluate_lines(u: &U) -> Vec<String> {
    std::fs::read_to_string(u.t.join("work/task/c1.accept.log"))
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("evaluate: "))
        .map(str::to_owned)
        .collect()
}
fn clone_state(u: &U) -> (String, String, String) {
    let clone = u.t.join("clone");
    (
        git(&clone, &["status", "--porcelain", "--untracked-files=all"]),
        git(&clone, &["for-each-ref"]),
        git(&clone, &["worktree", "list", "--porcelain"]),
    )
}

#[test]
fn c1_a_unit_record_says_mode_unit_and_a_stage_takes_env() {
    let mut u = U::new();
    let mut r = u.request();
    r["implementer"]["env"] = json!({"CASE_SUITE_PROBE": "x1"});
    let (v, exit) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), exit), (Some("merged"), 0), "{v}");
    assert_eq!(v["mode"], "unit");
    let env = std::fs::read_to_string(u.t.join("bin/env.1")).unwrap();
    assert!(env.lines().any(|l| l == "CASE_SUITE_PROBE=x1"), "{env}");
    // A stage's `env` is judged by `ap run`'s code before any effect.
    let mut r = u.request();
    r["verifier"]["env"] = json!({"HOME": "/"});
    r["out"] = json!(u.p("work/task/u2"));
    assert_eq!(code(u.run(&r)), "env_forbidden@verifier.env");
}

#[test]
fn c2_a_refused_case_call_leaves_nothing_behind() {
    let mut u = U::new();
    let c = case(&u);
    let before = clone_state(&u);
    let r0 = case_request(&u, &c);
    let mut cases: Vec<(Value, String)> = vec![];
    for member in [
        "repository",
        "branch",
        "task_key",
        "verifier",
        "verdict",
        "gate",
        "store",
        "merge",
        "pull_request",
    ] {
        let mut r = r0.clone();
        r[member] = json!("o/r");
        cases.push((r, format!("invalid_input@{member}")));
    }
    let mut add = |k: &str, v: Value, want: &str| {
        let mut r = r0.clone();
        r[k] = v;
        cases.push((r, want.to_owned()));
    };
    add("at", json!("middle"), "invalid_input@at");
    add("at", json!("head"), "invalid_input@at");
    add("case", json!("cases/tc-1.json"), "invalid_input@case");
    add(
        "case",
        json!(u.p("cases/none.json")),
        "case_unreadable@case",
    );
    add("case", json!(u.p("cases")), "case_unreadable@case");
    add(
        "page",
        json!(u.p("work/task/TASK.md")),
        "case_page_differs@page",
    );
    add(
        "page",
        json!(u.p("judge/implementer.md")),
        "outside_work_root@page",
    );
    add("out", json!(u.p("judge/c1")), "outside_work_root@out");
    add(
        "acceptance",
        json!(u.p("work/task/copy/evaluate")),
        "inside_worktree@acceptance",
    );
    add(
        "acceptance",
        json!(u.p("work/task/case.md")),
        "acceptance_unreadable@acceptance",
    );
    add(
        "acceptance_seconds",
        json!(0),
        "invalid_input@acceptance_seconds",
    );
    let mut r = r0.clone();
    r["implementer"]["resume"] = json!({"record": u.p("judge/x.run.json")});
    cases.push((r, "invalid_input@implementer.resume".into()));
    let mut r = r0.clone();
    r["implementer"]["prompt"] = json!(u.p("judge/none.md"));
    cases.push((r, "prompt_unreadable@implementer.prompt".into()));
    let mut r = r0.clone();
    r["implementer"]["timeout_seconds"] = json!(0);
    cases.push((r, "invalid_input@implementer.timeout_seconds".into()));
    // An `implementer` without a prompt is no validity call: it is refused.
    for prompt in [Value::Null, json!(7)] {
        let mut r = r0.clone();
        r["implementer"] = json!({"prompt": prompt});
        cases.push((r, "invalid_input@implementer.prompt".into()));
    }
    let mut r = r0.clone();
    r["implementer"] = json!({});
    cases.push((r, "invalid_input@implementer.prompt".into()));

    // The case file: each member missing, unknown or malformed; the file named after another id.
    let good: Value = serde_json::from_slice(&std::fs::read(&c.file).unwrap()).unwrap();
    let mut n = 0;
    let mut bad = |name: &str, f: &dyn Fn(&mut Value)| {
        let mut v = good.clone();
        f(&mut v);
        n += 1;
        std::fs::create_dir_all(u.t.join(format!("cases/{n}"))).unwrap();
        let file = u.p(&format!("cases/{n}/{name}.json"));
        std::fs::write(&file, serde_json::to_vec(&v).unwrap()).unwrap();
        let mut r = r0.clone();
        r["case"] = json!(file);
        cases.push((r, "case_unreadable@case".into()));
    };
    bad("tc-1x", &|_| {});
    bad("tc-2", &|v| v["id"] = json!("tc-2x"));
    bad("Tc-3", &|v| v["id"] = json!("Tc-3"));
    bad("tc-4", &|v| {
        v["id"] = json!("tc-4");
        v.as_object_mut().unwrap().remove("commands");
    });
    let set = |k: &'static str, x: Value| {
        move |v: &mut Value| {
            v[k] = x.clone();
        }
    };
    bad("tc-1", &set("extra", json!(1)));
    bad("tc-1", &set("repository", json!("no-slash")));
    bad("tc-1", &set("pull_request", json!(0)));
    bad("tc-1", &set("base", json!("abc")));
    bad("tc-1", &set("head", json!("G".repeat(40))));
    bad("tc-1", &set("tests", json!([])));
    bad("tc-1", &set("tests", json!(["../x"])));
    bad("tc-1", &set("tests", json!(["/etc/passwd"])));
    bad("tc-1", &set("commands", json!([])));
    bad("tc-1", &set("commands", json!([[]])));
    bad("tc-1", &set("commands", json!(["./check"])));
    bad("tc-1", &set("page_sha256", json!("0".repeat(63))));
    std::fs::create_dir_all(u.t.join("cases/0")).unwrap();
    std::fs::write(u.t.join("cases/0/tc-1.json"), "not json").unwrap();
    let mut r = r0.clone();
    r["case"] = json!(u.p("cases/0/tc-1.json"));
    cases.push((r, "case_unreadable@case".into()));
    for (r, want) in cases {
        assert_eq!(code(u.run(&r)), want, "{r}");
    }
    // Commits and tests that the clone does not hold.
    let missing = "0123456789abcdef0123456789abcdef01234567";
    for (k, v, want) in [
        ("base", json!(missing), "case_commit_missing@case.base"),
        ("head", json!(missing), "case_commit_missing@case.head"),
        (
            "tests",
            json!(["tests/none.sh"]),
            "case_commit_missing@case.tests",
        ),
        ("tests", json!(["tests"]), "case_commit_missing@case.tests"),
    ] {
        let mut v2 = good.clone();
        v2[k] = v;
        std::fs::write(&c.file, serde_json::to_vec(&v2).unwrap()).unwrap();
        assert_eq!(code(u.run(&r0)), want, "{k}");
    }
    std::fs::write(&c.file, serde_json::to_vec(&good).unwrap()).unwrap();
    let mut r = r0.clone();
    r["clone"] = json!(u.p("home"));
    assert_eq!(code(u.run(&r)), "case_commit_missing@case.base");
    // An existing file of the call, or an existing copy, is refused untouched.
    std::fs::write(u.t.join("work/task/c1.candidate.diff"), "earlier").unwrap();
    assert_eq!(code(u.run(&r0)), "out_exists@out");
    std::fs::remove_file(u.t.join("work/task/c1.candidate.diff")).unwrap();
    std::fs::create_dir(copy(&u)).unwrap();
    assert_eq!(code(u.run(&r0)), "worktree_exists@worktree");
    std::fs::remove_dir(copy(&u)).unwrap();

    // Nothing exists afterwards: no file of the call, no copy, no process, no call; the clone is
    // as it was.
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "case.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
    assert_eq!(clone_state(&u), before);
    // The page of a case needs no line `Write set:`: the same request, unchanged, is admitted.
    let (v, _) = u.run(&no_stage(r0)).unwrap();
    assert_eq!(v["mode"], "case");
}

#[test]
fn c3_a_case_call_calls_no_github_or_store_and_writes_only_its_files() {
    let mut u = U::new();
    let c = case(&u);
    let before = clone_state(&u);
    let top = names(&u.t);
    let (v, code) = u.run(&case_request(&u, &c)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    assert!(u.calls.is_empty(), "{:?}", u.calls);
    assert_eq!(
        names(&u.t.join("work/task")),
        [
            "TASK.md",
            "c1.accept.log",
            "c1.candidate.diff",
            "c1.implementer.md",
            "c1.implementer.md.run.json",
            "c1.implementer.md.stream.jsonl",
            "c1.implementer.prompt.md",
            "c1.unit.json",
            "case.md"
        ]
    );
    assert_eq!(names(&u.t), top);
    assert_eq!(clone_state(&u), before);
}

#[test]
fn c4_the_copy_at_base_holds_nothing_of_the_head_or_the_clone() {
    let mut u = U::new();
    let c = case(&u);
    let t = u.t.display().to_string();
    let clone = u.p("clone");
    // What the candidate can find in its copy, written out before it answers.
    u.script(
        "a1",
        &format!(
            "{{ git cat-file -e {head} 2>/dev/null && echo head=found || echo head=absent; git cat-file -e {base} 2>/dev/null && echo base=found || echo base=absent; \
             echo commits=$(git rev-list --all --count); echo remotes=$(git remote | wc -l); \
             grep -rqF '{clone}' . ; echo clone=$?; test -e .git/objects/info/alternates; echo alternates=$?; \
             echo author=$(git log -1 --format=%an); echo test=$(cat tests/t.sh | sha256sum | cut -c1-64); \
             echo mode=$(git ls-files -s tests/t.sh | cut -c1-6); test -e src/lib.txt; echo lib=$?; \
             git status --porcelain | wc -l | sed 's/^/dirty=/'; }} > {t}/c4.txt; {CANDIDATE}",
            head = c.head,
            base = c.base,
        ),
    );
    let (v, code) = u.run(&case_request(&u, &c)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    let seen = std::fs::read_to_string(u.t.join("c4.txt")).unwrap();
    // The author is the call's own identity given on the command line: any nonempty name.
    let (author, rest): (Vec<_>, Vec<_>) = seen.lines().partition(|l| l.starts_with("author="));
    assert!(
        author.len() == 1 && author[0].len() > "author=".len(),
        "{seen}"
    );
    let test = format!("test={}", sha(CASE_TEST.as_bytes()));
    let want = [
        "head=absent",
        "base=absent",
        "commits=1",
        "remotes=0",
        "clone=1",
        "alternates=1",
        &test,
        "mode=100755",
        "lib=1",
        "dirty=0",
    ];
    assert_eq!(rest, want, "{seen}");
    // The copy's record names neither the head nor anything the candidate could follow to it.
    assert_eq!(v["base"], c.base.as_str());
}

#[test]
fn c4_the_copy_at_head_holds_the_tree_of_head_in_one_commit() {
    let mut u = U::new();
    let c = case(&u);
    let mut r = no_stage(case_request(&u, &c));
    r["at"] = json!("head");
    let t = u.t.display().to_string();
    let accept = format!(
        "#!/bin/sh\n{{ git rev-list --all --count; cat src/lib.txt; git remote | wc -l; }} > {t}/c4h.txt\n"
    );
    write_exec(&u.t.join("judge/look.sh"), &accept);
    r["acceptance"] = json!(u.p("judge/look.sh"));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(
        std::fs::read_to_string(u.t.join("c4h.txt")).unwrap(),
        "1\ndone\n0\n"
    );
}

#[test]
fn c5_one_pass_at_most_one_stage_and_no_step_repeated() {
    let mut u = U::new();
    let c = case(&u);
    let (v, _) = u.run(&case_request(&u, &c)).unwrap();
    assert_eq!(u.launches, 1);
    assert_eq!(
        steps(&v),
        pairs(&[
            ("worktree", "passed"),
            ("implementer", "passed"),
            ("acceptance", "passed"),
            ("cleanup", "passed")
        ])
    );
    let mut r = no_stage(case_request(&u, &c));
    r["out"] = json!(u.p("work/task/c2"));
    let (v, _) = u.run(&r).unwrap();
    assert_eq!(u.launches, 1, "no stage without `implementer`");
    let names: Vec<_> = steps(&v).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["worktree", "acceptance", "cleanup"]);
    // No stage ever works in a copy of the head.
    let mut r = case_request(&u, &c);
    (r["at"], r["out"]) = (json!("head"), json!(u.p("work/task/c3")));
    assert_eq!(code(u.run(&r)), "invalid_input@at");
    assert_eq!(u.launches, 1);
}

#[test]
fn c6_the_validity_pair_fails_at_base_and_passes_at_head_with_the_evaluator_line() {
    let mut u = U::new();
    let c = case(&u);
    let r = no_stage(case_request(&u, &c));
    let (_, end) = ended(u.run(&r));
    assert_eq!(
        end,
        (
            "failed".into(),
            4,
            Value::Null,
            Value::Null,
            json!({"exit": 1})
        )
    );
    let lines = evaluate_lines(&u);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let rest = lines[0]
        .strip_prefix("evaluate: command=1 exit=1 seconds=")
        .unwrap();
    assert!(
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()),
        "{lines:?}"
    );
    assert!(!copy(&u).exists());
    let mut r = r.clone();
    (r["at"], r["out"]) = (json!("head"), json!(u.p("work/task/c2")));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(v["at"], "head");
}

#[test]
fn c6_a_stage_that_makes_the_tests_pass_ends_passed() {
    let mut u = U::new();
    let c = case(&u);
    let (_, end) = ended(u.run(&case_request(&u, &c)));
    assert_eq!(
        end,
        ("passed".into(), 0, Value::Null, Value::Null, Value::Null)
    );
    let lines = evaluate_lines(&u);
    assert!(
        lines[0].starts_with("evaluate: command=1 exit=0 seconds="),
        "{lines:?}"
    );
}

#[test]
fn c6_the_evaluator_restores_a_test_the_candidate_emptied() {
    let mut u = U::new();
    let c = case(&u);
    // An empty test would exit 0: the candidate empties it and implements nothing.
    u.script("a1", ": > tests/t.sh; echo done");
    let (_, end) = ended(u.run(&case_request(&u, &c)));
    assert_eq!(
        end,
        (
            "failed".into(),
            4,
            Value::Null,
            Value::Null,
            json!({"exit": 1})
        )
    );
    // The diff is taken after the evaluator ran: the restored test is no change.
    let diff = std::fs::read_to_string(u.t.join("work/task/c1.candidate.diff")).unwrap();
    assert!(!diff.contains("tests/t.sh"), "{diff}");
}

#[test]
fn c6_the_evaluators_bound_ends_failed_with_exit_124() {
    let mut u = U::new();
    let c = case(&u);
    write_exec(&u.t.join("judge/slow.sh"), "#!/bin/sh\nsleep 30\n");
    let mut r = no_stage(case_request(&u, &c));
    (r["acceptance"], r["acceptance_seconds"]) = (json!(u.p("judge/slow.sh")), json!(1));
    let (_, end) = ended(u.run(&r));
    assert_eq!(
        end,
        (
            "failed".into(),
            4,
            Value::Null,
            Value::Null,
            json!({"exit": 124})
        )
    );
    assert!(!copy(&u).exists());
}

#[test]
fn c6_the_stages_timeout_ends_failed_and_runs_no_evaluator() {
    let mut u = U::new();
    let c = case(&u);
    std::fs::write(u.t.join("bin/mode-a1"), "other").unwrap();
    let mut r = case_request(&u, &c);
    r["acceptance"] = json!(u.p("judge/accept.sh"));
    r["implementer"]["timeout_seconds"] = json!(1);
    let (v, end) = ended(u.run(&r));
    assert_eq!(
        end,
        (
            "failed".into(),
            4,
            Value::Null,
            Value::Null,
            json!({"stage": "timeout"})
        )
    );
    assert!(u.accept_calls().is_empty());
    assert_eq!(
        steps(&v),
        pairs(&[
            ("worktree", "passed"),
            ("implementer", "failed"),
            ("cleanup", "passed")
        ])
    );
    assert_eq!(v["stage_seconds"], 1);
    assert!(!copy(&u).exists());
}

#[test]
fn c6_another_exit_of_the_evaluator_or_a_cut_stage_stops_and_keeps_the_copy() {
    let mut u = U::new();
    let c = case(&u);
    std::fs::write(u.t.join("accept-exit"), "3").unwrap();
    let mut r = no_stage(case_request(&u, &c));
    r["acceptance"] = json!(u.p("judge/accept.sh"));
    let (_, end) = ended(u.run(&r));
    assert_eq!(
        end,
        (
            "stopped".into(),
            3,
            json!("acceptance"),
            json!("acceptance_failed"),
            json!({"exit": 3})
        )
    );
    assert!(copy(&u).is_dir(), "a stopped call keeps its copy");
    assert_eq!(u.accept_calls().len(), 1);
    // A stage cut before it did any work, with no next account.
    let mut u = U::new();
    let c = case(&u);
    std::fs::write(u.t.join("bin/mode-a1"), "limit1").unwrap();
    let (v, end) = ended(u.run(&case_request(&u, &c)));
    assert_eq!(end.0, "stopped");
    assert_eq!(
        (end.1, &end.2, &end.3),
        (3, &json!("implementer"), &json!("cut"))
    );
    assert!(copy(&u).is_dir());
    assert!(
        !steps(&v)
            .iter()
            .any(|(n, _)| n == "acceptance" || n == "cleanup")
    );
}

#[test]
fn c7_the_case_record_has_the_unit_members_and_the_case_members() {
    let mut u = U::new();
    let c = case(&u);
    // The candidate commits part of its work and leaves a new file uncommitted.
    u.script(
        "a1",
        "mkdir -p src && echo done > src/lib.txt && git add -A && git -c user.name=c -c user.email=c@e commit -qm work && echo note > NOTES.txt; echo CANDIDATE-ANSWER",
    );
    let mut r = case_request(&u, &c);
    r["implementer"]["timeout_seconds"] = json!(600);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    assert_eq!(v, u.record("c1"));
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want = vec![
        "mode",
        "repository",
        "clone",
        "worktree",
        "branch",
        "task_key",
        "base",
        "page",
        "write_set",
        "acceptance",
        "verifier_prompt",
        "verdict_file",
        "merge",
        "pull_request",
        "head",
        "review",
        "merge_commit",
        "stages",
        "files",
        "state",
        "step",
        "stopped_at",
        "cause",
        "detail",
        "steps",
        "start",
        "end",
        "case",
        "at",
        "prompt",
        "stage_seconds",
    ];
    want.sort_unstable();
    assert_eq!(keys, want);
    for k in [
        "branch",
        "task_key",
        "write_set",
        "verifier_prompt",
        "verdict_file",
        "merge",
        "pull_request",
        "head",
        "review",
        "merge_commit",
        "step",
        "stopped_at",
        "cause",
    ] {
        assert!(v[k].is_null(), "{k}: {v}");
    }
    assert!(v["stages"]["verifier"].is_null() && v["files"]["gate"].is_null());
    assert_eq!(
        (v["mode"].as_str(), v["at"].as_str()),
        (Some("case"), Some("base"))
    );
    assert_eq!(
        (v["repository"].as_str(), v["base"].as_str()),
        (Some("o/r"), Some(c.base.as_str()))
    );
    let case_bytes = std::fs::read(&c.file).unwrap();
    assert_eq!(
        v["case"],
        json!({"path": c.file, "sha256": sha(&case_bytes), "id": "tc-1"})
    );
    let prompt = u.p("judge/implementer.md");
    assert_eq!(
        v["prompt"],
        json!({"path": prompt, "sha256": sha(IMPLEMENTER_PROMPT.as_bytes())})
    );
    assert_eq!(v["stage_seconds"], 600);
    assert_eq!(v["page"]["sha256"], sha(CASE_PAGE.as_bytes()));
    assert_eq!(
        v["acceptance"]["sha256"],
        sha(&std::fs::read(evaluator()).unwrap())
    );
    // The stage's prompt: the request's prompt unchanged, then the block of facts.
    let run: Value = serde_json::from_slice(
        &std::fs::read(v["stages"]["implementer"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(run["role"], "implementer");
    assert_eq!(run["cwd"], u.p("work/task/copy"));
    let given = std::fs::read_to_string(u.t.join("work/task/c1.implementer.prompt.md")).unwrap();
    assert!(given.starts_with(IMPLEMENTER_PROMPT), "{given}");
    for fact in [
        u.p("work/task/case.md"),
        u.p("work/task/copy"),
        "tests/t.sh".into(),
        r#"["./tests/t.sh"]"#.into(),
        "no remote".into(),
    ] {
        assert!(given.contains(&fact), "{fact} not in {given}");
    }
    assert!(
        !given.contains(&c.head) && !given.contains(&u.p("clone")),
        "{given}"
    );
    assert_eq!(v["files"]["report"], u.p("work/task/c1.implementer.md"));
    assert_eq!(
        v["files"]["acceptance_output"],
        u.p("work/task/c1.accept.log")
    );
    // The candidate's difference from the one commit: committed and uncommitted, new files too.
    let diff_path = u.p("work/task/c1.candidate.diff");
    assert_eq!(v["files"]["candidate_diff"], diff_path);
    let diff = std::fs::read_to_string(&diff_path).unwrap();
    assert!(
        diff.contains("+++ b/src/lib.txt") && diff.contains("+++ b/NOTES.txt"),
        "{diff}"
    );
    // Without a stage: no prompt and no bound.
    let mut r = no_stage(case_request(&u, &c));
    r["out"] = json!(u.p("work/task/c2"));
    let (v, _) = u.run(&r).unwrap();
    assert!(
        v["prompt"].is_null()
            && v["stage_seconds"].is_null()
            && v["stages"]["implementer"].is_null()
    );
    assert!(v["files"]["report"].is_null());
    // The default bound of an implementer stage is `ap run`'s.
    let mut r = case_request(&u, &c);
    r["out"] = json!(u.p("work/task/c3"));
    let (v, _) = u.run(&r).unwrap();
    assert_eq!(v["stage_seconds"], 5400);
}

#[test]
fn c7_a_killed_case_call_leaves_a_record_naming_the_step_begun() {
    let mut u = U::new();
    let c = case(&u);
    let record = u.p("work/task/c1.unit.json");
    let t = u.t.display().to_string();
    write_exec(
        &u.t.join("judge/look.sh"),
        &format!("#!/bin/sh\ncp {record} {t}/seen.json\n"),
    );
    let mut r = no_stage(case_request(&u, &c));
    r["acceptance"] = json!(u.p("judge/look.sh"));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("passed"), 0), "{v}");
    let seen: Value =
        serde_json::from_slice(&std::fs::read(u.t.join("seen.json")).unwrap()).unwrap();
    assert_eq!(
        (
            seen["mode"].as_str(),
            seen["state"].as_str(),
            seen["step"].as_str()
        ),
        (Some("case"), Some("running"), Some("acceptance"))
    );
    assert_eq!(steps(&seen), pairs(&[("worktree", "passed")]));
    // A stopped first step leaves a readable record too.
    let mut r = no_stage(case_request(&u, &c));
    r["out"] = json!(u.p("work/task/c2"));
    r["worktree"] = json!(u.p("work/task/none/deeper/copy"));
    std::fs::write(
        u.t.join("work/task/none"),
        "a file where a directory must be",
    )
    .unwrap();
    let (v, end) = ended(u.run(&r));
    assert_eq!(
        (end.0.as_str(), end.1, &end.3),
        ("stopped", 3, &json!("copy_failed"))
    );
    assert_eq!(v, u.record("c2"));
}

#[test]
fn c8_a_case_file_changed_by_the_stage_stops_the_call_before_the_evaluator() {
    let mut u = U::new();
    let c = case(&u);
    // The stage turns the case's commands into `true`, which would pass anything.
    let mut changed: Value = serde_json::from_slice(&std::fs::read(&c.file).unwrap()).unwrap();
    changed["commands"] = json!([["true"]]);
    let staged = u.p("case-true.json");
    std::fs::write(&staged, serde_json::to_vec(&changed).unwrap()).unwrap();
    u.script("a1", &format!("cp {staged} {}; {CANDIDATE}", c.file));
    let (v, end) = ended(u.run(&case_request(&u, &c)));
    assert_eq!(
        (end.0.as_str(), end.1, &end.2, &end.3),
        ("stopped", 3, &json!("acceptance"), &json!("digest_differs"))
    );
    assert!(
        !u.t.join("work/task/c1.accept.log").exists(),
        "the evaluator ran"
    );
    assert!(v["files"]["acceptance_output"].is_null());
}

#[test]
fn c9_a_test_path_that_is_a_link_at_head_is_refused_before_any_effect() {
    let mut u = U::new();
    let c = case(&u);
    let clone = u.t.join("clone");
    git(&clone, &["checkout", "-q", "accepted"]);
    std::os::unix::fs::symlink("t.sh", clone.join("tests/link.sh")).unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "a link"]);
    let head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "main"]);
    let mut v: Value = serde_json::from_slice(&std::fs::read(&c.file).unwrap()).unwrap();
    (v["head"], v["tests"]) = (json!(head), json!(["tests/t.sh", "tests/link.sh"]));
    std::fs::write(&c.file, serde_json::to_vec(&v).unwrap()).unwrap();
    let before = clone_state(&u);
    let r = case_request(&u, &c);
    assert_eq!(code(u.run(&r)), "case_commit_missing@case.tests");
    assert_eq!(code(u.run(&no_stage(r))), "case_commit_missing@case.tests");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "case.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
    assert_eq!(clone_state(&u), before);
}

/// A `refs/replace/` entry of the clone for the base commit and for the head commit makes a plain
/// `git` read another tree and another judging file. The copy holds the real tree of `base` with the
/// judging file as it is at the real `head`.
#[test]
fn c10_a_replace_ref_in_the_clone_changes_nothing_in_the_copy() {
    let mut u = U::new();
    let c = case(&u);
    let clone = u.t.join("clone");
    git(&clone, &["checkout", "-q", "--detach", &c.base]);
    std::fs::write(clone.join("fake.txt"), "fake\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "fake base"]);
    let fake_base = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "--detach", &c.head]);
    write_exec(&clone.join("tests/t.sh"), "#!/bin/sh\nexit 0\n");
    git(&clone, &["commit", "-qam", "fake head"]);
    let fake_head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["checkout", "-q", "main"]);
    git(&clone, &["replace", &c.base, &fake_base]);
    git(&clone, &["replace", &c.head, &fake_head]);
    assert!(git(&clone, &["ls-tree", "-r", "--name-only", &c.base]).contains("fake.txt"));
    let t = u.t.display().to_string();
    let look =
        format!("#!/bin/sh\n{{ git ls-files; sha256sum tests/t.sh; }} > {t}/c10.txt\nexit 1\n");
    write_exec(&u.t.join("judge/look.sh"), &look);
    let mut r = no_stage(case_request(&u, &c));
    r["acceptance"] = json!(u.p("judge/look.sh"));
    let (v, _) = u.run(&r).unwrap();
    assert_eq!(v["state"], "failed", "{v}");
    let seen = std::fs::read_to_string(u.t.join("c10.txt")).unwrap();
    assert!(!seen.contains("fake.txt"), "{seen}");
    assert!(seen.contains(&sha(CASE_TEST.as_bytes())), "{seen}");
}

/// Replaces the case's copy by a link to the real directory, as a process of the candidate could:
/// to be run with the copy as the working directory.
const SWAP: &str = r#"mv "$PWD" "$PWD.real" && ln -s "$PWD.real" "$PWD""#;

/// What a swapped copy leaves: the link is still there for a reader, the real directory is kept, the
/// call ended `stopped` at `step` with `copy_replaced`, and nothing was removed or written after it.
fn swapped(u: &U, v: &Value, code: u8, step: &str) {
    assert_eq!(
        (
            v["state"].as_str(),
            v["stopped_at"].as_str(),
            v["cause"].as_str(),
            code
        ),
        (Some("stopped"), Some(step), Some("copy_replaced"), 3),
        "{v}"
    );
    let copy = copy(u);
    assert!(copy.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(PathBuf::from(format!("{}.real", copy.display())).is_dir());
    assert!(!u.t.join("work/task/c1.candidate.diff").exists());
}

/// A copy that the stage swapped for a link ends the call before the evaluator starts.
#[test]
fn c11_a_copy_swapped_for_a_link_stops_before_acceptance() {
    let mut u = U::new();
    let c = case(&u);
    u.script("a1", &format!("{SWAP}; echo done"));
    let (v, code) = u.run(&case_request(&u, &c)).unwrap();
    swapped(&u, &v, code, "acceptance");
    assert!(
        !u.t.join("work/task/c1.accept.log").exists(),
        "no evaluator started"
    );
}

/// A copy that the evaluator's run left swapped for a link is not removed, and no diff is written
/// through the link.
#[test]
fn c11_a_copy_swapped_for_a_link_stops_before_cleanup() {
    let mut u = U::new();
    let c = case(&u);
    write_exec(
        &u.t.join("judge/swap.sh"),
        &format!("#!/bin/sh\n{SWAP}\nexit 0\n"),
    );
    let mut r = no_stage(case_request(&u, &c));
    r["acceptance"] = json!(u.p("judge/swap.sh"));
    let (v, code) = u.run(&r).unwrap();
    swapped(&u, &v, code, "cleanup");
}

// ---- runtime codex: a stage's `runtime` --------------------------------------------------------

fn stage_record(path: &Value) -> Value {
    serde_json::from_slice(&std::fs::read(path.as_str().unwrap()).unwrap()).unwrap()
}

#[test]
fn x7_ap_unit_passes_the_runtime_to_its_stages() {
    let mut u = U::new();
    let mut r = u.request();
    r["implementer"]["runtime"] = json!("codex");
    r["implementer"]["permission_mode"] = json!("bypassPermissions");
    r["implementer"]["model"] = json!("gpt-6.1-terra");
    let (v, exit) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), exit), (Some("merged"), 0), "{v}");
    let implementer = stage_record(&v["stages"]["implementer"]);
    assert_eq!(
        (
            implementer["runtime"].as_str(),
            implementer["state"].as_str()
        ),
        (Some("codex"), Some("ok"))
    );
    let verifier = stage_record(&v["stages"]["verifier"]);
    assert_eq!(verifier["runtime"], "claude");
    let calls = std::fs::read_to_string(u.t.join("bin/calls.log")).unwrap();
    let first = calls.lines().next().unwrap();
    assert!(
        first.starts_with(&format!("{}|--no-daemon exec --json ", u.p("a1"))),
        "{first}"
    );
    assert!(
        first.contains(" --dangerously-bypass-approvals-and-sandbox -m gpt-6.1-terra "),
        "{first}"
    );
    // The stage of a case call.
    let mut u = U::new();
    let c = case(&u);
    let mut r = case_request(&u, &c);
    r["implementer"]["runtime"] = json!("codex");
    r["implementer"]["permission_mode"] = json!("bypassPermissions");
    let (v, end) = ended(u.run(&r));
    assert_eq!(end.0, "passed", "{v}");
    assert_eq!(
        stage_record(&v["stages"]["implementer"])["runtime"],
        "codex"
    );
}

#[test]
fn x7_ap_unit_refuses_a_codex_stage_without_a_stated_mode() {
    let mut u = U::new();
    let mut r = u.request();
    r["implementer"]["runtime"] = json!("codex");
    assert_eq!(code(u.run(&r)), "invalid_input@implementer.permission_mode");
    r["implementer"]["permission_mode"] = json!("auto");
    assert_eq!(code(u.run(&r)), "invalid_input@implementer.permission_mode");
    let c = case(&u);
    let mut r = case_request(&u, &c);
    r["implementer"]["runtime"] = json!("codex");
    assert_eq!(code(u.run(&r)), "invalid_input@implementer.permission_mode");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "case.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn x7_ap_unit_refuses_a_codex_verifier() {
    let mut u = U::new();
    let mut r = u.request();
    r["verifier"]["runtime"] = json!("codex");
    assert_eq!(code(u.run(&r)), "invalid_input@verifier.role");
    r["verifier"]["permission_mode"] = json!("bypassPermissions");
    assert_eq!(code(u.run(&r)), "invalid_input@verifier.role");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn r3_flag_true_with_unreadable_threads_stops_unresolved_findings() {
    let (mut u, r, _) = finding_on_the_head();
    u.threads_unreadable = true;
    let v = u.stop(&r, "gate", "unresolved_findings");
    assert!(u.accept_calls().is_empty());
    let answer: Value =
        serde_json::from_slice(&std::fs::read(v["files"]["gate"].as_str().unwrap()).unwrap())
            .unwrap();
    assert!(answer["autoreview"]["threads_on_head"].is_null());
}

#[test]
fn x7_ap_unit_starts_a_claude_stage_without_a_mode_as_auto() {
    let mut u = U::new();
    let r = u.request();
    assert!(r["implementer"].get("permission_mode").is_none());
    let (v, exit) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), exit), (Some("merged"), 0), "{v}");
    let calls = std::fs::read_to_string(u.t.join("bin/calls.log")).unwrap();
    assert!(
        calls
            .lines()
            .all(|line| line.contains(" --permission-mode auto ")),
        "{calls}"
    );
    assert_eq!(calls.lines().count(), 2);
    assert_eq!(
        stage_record(&v["stages"]["implementer"])["runtime"],
        "claude"
    );
}

// ---- R4 one round after `Verdict: repair` ------------------------------------------------------

const FIXER_PROMPT: &str = "LEAD FIXER PROMPT: repair the blocking findings of the report.\n";
const VERIFIER2_PROMPT: &str = "LEAD SECOND VERIFIER PROMPT: look again at the repaired head.\n";
/// Commits `notes.md` (inside the write set) and pushes: the fixer's work.
const FIX: &str = "echo fixed > notes.md && git add -A && git -c user.name=t -c user.email=t@e commit -qm fix && git push -q origin HEAD:refs/heads/ap/unit-test 2>/dev/null; echo FIXER-ANSWER";
/// `repair` until the fixer's file is there, then `accept` of the head.
const VERIFY_AFTER_FIX: &str = "if [ -f notes.md ]; then printf 'Verdict: accept\\n%s\\nno finding' \"$(git rev-parse HEAD)\"; else printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"; fi";
const VERIFY_REPAIR: &str = "printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"";

/// The implementer's account: it implements on the first call and runs `fix` on the resume (the
/// implementer's file is already there).
fn fixing(u: &U, fix: &str) {
    u.script(
        "a1",
        &format!("if [ -f src/a.rs ]; then {fix}; else {IMPLEMENT}; fi"),
    );
}
fn r4_request(u: &U) -> Value {
    std::fs::write(u.t.join("judge/fixer.md"), FIXER_PROMPT).unwrap();
    std::fs::write(u.t.join("judge/verifier2.md"), VERIFIER2_PROMPT).unwrap();
    let mut r = u.request();
    r["repair"] = json!({"prompt": u.p("judge/fixer.md"),
        "verifier_prompt": u.p("judge/verifier2.md"), "timeout_seconds": 600});
    r
}
fn run_record(path: &Value) -> Value {
    serde_json::from_slice(&std::fs::read(path.as_str().unwrap()).unwrap()).unwrap()
}
fn exists(u: &U, rel: &str) -> bool {
    u.t.join(rel).symlink_metadata().is_ok()
}

#[test]
fn r4_a_repair_verdict_is_returned_once_and_the_come_back_merges() {
    let mut u = U::new();
    fixing(&u, FIX);
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v, u.record("u1-2"), "stdout is the come-back's record");
    assert_eq!(u.launches, 4, "implementer, verifier, fixer, verifier");

    // The first record: stopped as today, with the round's two paths and the digests of `repair`.
    let first = u.record("u1");
    assert_eq!(
        (&first["state"], &first["stopped_at"], &first["cause"]),
        (
            &json!("stopped"),
            &json!("verifier"),
            &json!("verdict_repair")
        )
    );
    assert_eq!(
        first["repair"],
        json!({"prompt": {"path": u.p("judge/fixer.md"), "sha256": sha(FIXER_PROMPT.as_bytes())},
            "verifier_prompt": {"path": u.p("judge/verifier2.md"), "sha256": sha(VERIFIER2_PROMPT.as_bytes())},
            "fixer": u.p("work/task/u1.fixer.md.run.json"), "next": u.p("work/task/u1-2.unit.json"),
            "state": "ok"})
    );

    // The fixer resumes the implementer's session on its account, runtime and model, in the worktree.
    let fixer = run_record(&first["repair"]["fixer"]);
    let implementer = run_record(&first["stages"]["implementer"]);
    assert_eq!(fixer["role"], "fixer");
    assert_eq!(fixer["resumes"], first["stages"]["implementer"]);
    for k in ["session", "account", "runtime", "model", "effort", "cwd"] {
        assert_eq!(fixer[k], implementer[k], "{k}");
    }
    assert_eq!(fixer["answer"]["path"], u.p("work/task/u1.fixer.md"));
    let prompt = std::fs::read_to_string(u.t.join("work/task/u1.fixer.prompt.md")).unwrap();
    assert_eq!(fixer["prompt"]["path"], u.p("work/task/u1.fixer.prompt.md"));
    let facts = prompt
        .strip_prefix(FIXER_PROMPT)
        .expect("the Lead's prompt unchanged first");
    let old = first["head"].as_str().unwrap();
    for fact in [
        "Repository: o/r".to_owned(),
        "Pull request: 7".to_owned(),
        format!("Head judged: {old}"),
        format!("Verifier report: {}", u.p("work/task/u1.verifier.md")),
        format!("Page: {}", u.p("work/task/TASK.md")),
    ] {
        assert!(facts.contains(&fact), "{fact} in {facts}");
    }

    // The come-back: no implementer, the first verifier's session with the second prompt, the new head.
    let new = u.head().unwrap();
    assert_ne!(new, old);
    assert_eq!(
        (v["head"].as_str(), v["pull_request"].as_u64()),
        (Some(new.as_str()), Some(7))
    );
    assert_eq!(
        steps(&v)[..4],
        pairs(&[
            ("worktree", "skipped"),
            ("implementer", "skipped"),
            ("pull_request", "skipped"),
            ("gate", "passed")
        ])
    );
    assert!(v.get("repair").is_none(), "the come-back holds no repair");
    let second = run_record(&v["stages"]["verifier"]);
    let earlier = run_record(&first["stages"]["verifier"]);
    assert_eq!(second["resumes"], first["stages"]["verifier"]);
    assert_eq!(second["session"], earlier["session"]);
    let asked = std::fs::read_to_string(second["prompt"]["path"].as_str().unwrap()).unwrap();
    assert!(asked.starts_with(VERIFIER2_PROMPT), "{asked}");
    assert!(asked.contains(&format!("Head: {new}")), "{asked}");
    assert_eq!(
        v["verifier_prompt"]["sha256"],
        sha(VERIFIER2_PROMPT.as_bytes())
    );
    assert_eq!(u.merges().len(), 1);
    assert_eq!(u.statuses().len(), 1);
    assert!(v["merge_commit"].is_string());
    assert_eq!(
        u.store("close_review").len(),
        1,
        "one review, of the new head"
    );
    assert_eq!(u.reviews[0]["head"], json!(new));
}

#[test]
fn r4_a_second_repair_verdict_stops() {
    let mut u = U::new();
    fixing(&u, FIX);
    u.script("a2", VERIFY_REPAIR);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(
        (
            v["state"].as_str(),
            v["stopped_at"].as_str(),
            v["cause"].as_str(),
            code
        ),
        (Some("stopped"), Some("verifier"), Some("verdict_repair"), 3),
        "{v}"
    );
    assert_eq!(v, u.record("u1-2"));
    assert_eq!(u.launches, 4, "no second fixer");
    assert!(u.statuses().is_empty() && u.merges().is_empty());
    assert!(u.store("close_review").is_empty());
    assert!(!exists(&u, "work/task/u1-2.fixer.md.run.json"));
    assert!(!exists(&u, "work/task/u1-2-2.unit.json"));
    assert_eq!(u.record("u1")["repair"]["state"], "ok");
}

#[test]
fn r4_an_accept_of_the_old_head_after_a_new_push_does_not_merge() {
    let mut u = U::new();
    fixing(&u, FIX);
    u.script(
        "a2",
        "if [ -f notes.md ]; then printf 'Verdict: accept\\n%s\\n' \"$(git rev-parse HEAD~1)\"; else printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"; fi",
    );
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(
        (
            v["state"].as_str(),
            v["stopped_at"].as_str(),
            v["cause"].as_str(),
            code
        ),
        (
            Some("stopped"),
            Some("verifier"),
            Some("verdict_head_differs"),
            3
        ),
        "{v}"
    );
    assert_eq!(v, u.record("u1-2"));
    assert_eq!(v["head"].as_str(), u.head().as_deref());
    assert_ne!(v["head"], u.record("u1")["head"]);
    assert!(u.statuses().is_empty() && u.merges().is_empty());
    assert!(u.store("close_review").is_empty());
}

#[test]
fn r4_a_fixer_that_is_not_ok_ends_the_call() {
    let mut u = U::new();
    // The resumed session answers nothing: `no_result`.
    fixing(&u, "true");
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"), "stdout is the first record");
    assert_eq!(
        (&v["state"], &v["cause"], &v["repair"]["state"]),
        (
            &json!("stopped"),
            &json!("verdict_repair"),
            &json!("no_result")
        )
    );
    assert_eq!(v["repair"]["fixer"], u.p("work/task/u1.fixer.md.run.json"));
    assert_eq!(run_record(&v["repair"]["fixer"])["state"], "no_result");
    assert_eq!(u.launches, 3);
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

#[test]
fn r4_only_a_repair_verdict_starts_the_round() {
    // A failed acceptance, with and without `repair`: the same end and the same steps.
    let ends = |with: bool| {
        let mut u = U::new();
        std::fs::write(u.t.join("accept-exit"), "1").unwrap();
        let r = if with { r4_request(&u) } else { u.request() };
        let v = u.stop(&r, "acceptance", "acceptance_failed");
        assert_eq!(u.launches, 1);
        assert!(!exists(&u, "work/task/u1.fixer.md.run.json"));
        assert!(!exists(&u, "work/task/u1.fixer.prompt.md"));
        (steps(&v), v["detail"].clone(), v["repair"].clone())
    };
    let (with, without) = (ends(true), ends(false));
    assert_eq!((&with.0, &with.1), (&without.0, &without.1));
    assert_eq!(
        with.2.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["prompt", "verifier_prompt"],
        "no round: the record holds only the digests"
    );
    assert!(without.2.is_null());

    // A verdict that cannot be read.
    let mut u = U::new();
    fixing(&u, FIX);
    u.script(
        "a2",
        "printf 'Verdict: maybe\\n%s\\n' \"$(git rev-parse HEAD)\"",
    );
    let v = u.stop(&r4_request(&u), "verifier", "verdict_unreadable");
    assert_eq!(u.launches, 2, "no fixer stage");
    assert!(v["repair"].get("fixer").is_none());
    assert!(!exists(&u, "work/task/u1.fixer.md.run.json"));
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
}

#[test]
fn r4_repair_is_refused_where_it_cannot_apply() {
    let mut u = U::new();
    let r0 = r4_request(&u);
    let mut cases: Vec<(Value, &str)> = vec![];
    let mut add = |f: &dyn Fn(&mut Value), want: &'static str| {
        let mut r = r0.clone();
        f(&mut r);
        cases.push((r, want));
    };
    add(&|r| r["repair"] = json!("yes"), "invalid_input@repair");
    add(
        &|r| r["repair"]["extra"] = json!(1),
        "invalid_input@repair.extra",
    );
    add(
        &|r| r["repair"]["prompt"] = json!("judge/fixer.md"),
        "invalid_input@repair.prompt",
    );
    add(
        &|r| r["repair"]["verifier_prompt"] = json!(7),
        "invalid_input@repair.verifier_prompt",
    );
    add(
        &|r| {
            r["repair"]
                .as_object_mut()
                .unwrap()
                .remove("timeout_seconds");
        },
        "invalid_input@repair.timeout_seconds",
    );
    add(
        &|r| r["repair"]["timeout_seconds"] = json!(0),
        "invalid_input@repair.timeout_seconds",
    );
    add(&|r| r["pull_request"] = json!(7), "invalid_input@repair");
    add(
        &|r| {
            r["pull_request"] = json!(7);
            r["verdict"] = json!("/x/u0.verifier.md");
            r.as_object_mut().unwrap().remove("verifier");
        },
        "invalid_input@repair",
    );
    let wt = u.p("work/task/wt/repo");
    add(
        &|r| r["repair"]["prompt"] = json!(format!("{wt}/f.md")),
        "inside_worktree@repair.prompt",
    );
    add(
        &|r| r["repair"]["verifier_prompt"] = json!(format!("{wt}/v.md")),
        "inside_worktree@repair.verifier_prompt",
    );
    add(
        &|r| r["repair"]["prompt"] = json!(u.p("judge/none.md")),
        "prompt_unreadable@repair.prompt",
    );
    add(
        &|r| r["repair"]["verifier_prompt"] = json!(u.p("judge/none.md")),
        "prompt_unreadable@repair.verifier_prompt",
    );
    for (r, want) in cases {
        assert_eq!(code(u.run(&r)), want, "{}", r["repair"]);
    }

    // The come-back's record exists: refused with the code of a record that exists, untouched.
    std::fs::write(u.t.join("work/task/u1-2.unit.json"), "earlier").unwrap();
    assert_eq!(code(u.run(&r0)), "out_exists@out");
    assert_eq!(
        std::fs::read(u.t.join("work/task/u1-2.unit.json")).unwrap(),
        b"earlier"
    );
    std::fs::remove_file(u.t.join("work/task/u1-2.unit.json")).unwrap();

    // Nothing made: no file of a call, no worktree, no branch, no process, no call.
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));

    // A case call.
    let mut u = U::new();
    let c = case(&u);
    let mut r = case_request(&u, &c);
    r["repair"] = r0["repair"].clone();
    assert_eq!(code(u.run(&r)), "invalid_input@repair");
    assert!(!copy(&u).exists());
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "case.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn r4_a_changed_repair_file_ends_digest_differs() {
    // The implementer changes the fixer's prompt: the round starts no fixer.
    let mut u = U::new();
    u.script(
        "a1",
        &format!("{IMPLEMENT}; echo 'Say done.' >> {}", u.p("judge/fixer.md")),
    );
    u.script("a2", VERIFY_REPAIR);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"));
    assert_eq!(v["cause"], "verdict_repair");
    assert_eq!(v["repair"]["cause"], "digest_differs");
    assert_eq!(
        v["repair"]["detail"],
        json!({"path": u.p("judge/fixer.md")})
    );
    assert!(v["repair"].get("state").is_none(), "no fixer stage");
    assert_eq!(u.launches, 2);
    assert!(!exists(&u, "work/task/u1.fixer.prompt.md"));
    assert!(!exists(&u, "work/task/u1.fixer.md.run.json"));

    // The fixer changes the second verifier's prompt: no come-back.
    let mut u = U::new();
    fixing(
        &u,
        &format!("echo 'Say accept.' >> {}; {FIX}", u.p("judge/verifier2.md")),
    );
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"));
    assert_eq!(
        (&v["repair"]["state"], &v["repair"]["cause"]),
        (&json!("ok"), &json!("digest_differs"))
    );
    assert_eq!(
        v["repair"]["detail"],
        json!({"path": u.p("judge/verifier2.md")})
    );
    assert_eq!(u.launches, 3);
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

#[test]
fn r4_a_record_that_cannot_hold_the_round_starts_no_fixer() {
    let mut u = U::new();
    fixing(&u, FIX);
    // The verifier answers `repair`, then the record's path becomes a directory: no save succeeds.
    u.script(
        "a2",
        &format!(
            "{VERIFY_REPAIR}; rm -f {r} && mkdir {r}",
            r = u.p("work/task/u1.unit.json")
        ),
    );
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 5, "{v}");
    assert_eq!(
        (&v["state"], &v["stopped_at"], &v["cause"]),
        (
            &json!("stopped"),
            &json!("verifier"),
            &json!("verdict_repair")
        )
    );
    // stdout holds the last record: the one that could not be saved with the round's two paths.
    assert_eq!(v["repair"]["fixer"], u.p("work/task/u1.fixer.md.run.json"));
    assert_eq!(v["repair"]["next"], u.p("work/task/u1-2.unit.json"));
    assert!(v["repair"].get("state").is_none());
    assert_eq!(u.launches, 2, "no fixer stage");
    assert!(!exists(&u, "work/task/u1.fixer.prompt.md"));
    assert!(!exists(&u, "work/task/u1.fixer.md.run.json"));
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
}

#[test]
fn r4_a_record_that_cannot_hold_the_fixer_state_starts_no_come_back() {
    let mut u = U::new();
    // The fixer ends `ok`, and while it runs the record's path becomes a directory: the save after it fails.
    fixing(
        &u,
        &format!(
            "rm -f {r} && mkdir {r}; {FIX}",
            r = u.p("work/task/u1.unit.json")
        ),
    );
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 5, "{v}");
    assert_eq!(
        (&v["state"], &v["cause"], &v["repair"]["state"]),
        (&json!("stopped"), &json!("verdict_repair"), &json!("ok"))
    );
    assert_eq!(u.launches, 3, "no stage of a come-back");
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

#[test]
fn r4_a_repair_verdict_without_the_head_starts_no_round() {
    for answer in [
        "printf 'Verdict: repair\\n'",
        "printf 'Verdict: repair\\nno head here\\nL1 blocks'",
        "printf 'Verdict: repair\\n%s\\nL1 blocks' 0000000000000000000000000000000000000000",
        "printf 'Verdict: repair\\n%s %s\\n' \"$(git rev-parse HEAD)\" 1111111111111111111111111111111111111111",
    ] {
        let mut u = U::new();
        fixing(&u, FIX);
        u.script("a2", answer);
        let v = u.stop(&r4_request(&u), "verifier", "verdict_repair");
        assert_eq!(u.launches, 2, "no fixer stage: {answer}");
        let keys: Vec<_> = v["repair"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["prompt", "verifier_prompt"], "{answer}");
        assert!(!exists(&u, "work/task/u1.fixer.md.run.json"));
        assert!(!exists(&u, "work/task/u1-2.unit.json"));
    }
}

#[test]
fn r4_the_come_back_verifier_is_admitted_before_any_effect() {
    let mut u = U::new();
    let r = r4_request(&u);
    // The come-back's verifier answers into `u1-2.verifier.md`: a prompt that names it.
    std::fs::write(
        u.t.join("judge/verifier2.md"),
        "Write your report to u1-2.verifier.md.\n",
    )
    .unwrap();
    assert_eq!(
        code(u.run(&r)),
        "out_named_in_prompt@repair.verifier_prompt"
    );
    std::fs::write(u.t.join("judge/verifier2.md"), "").unwrap();
    assert_eq!(code(u.run(&r)), "prompt_unreadable@repair.verifier_prompt");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

/// The fixer pushes and GitHub reports the old head for the next `lag` readings.
fn lagging_fix(u: &U, lag: u64) {
    let t = u.t.display();
    fixing(
        u,
        &format!("git rev-parse HEAD > {t}/old-head; echo {lag} > {t}/view-lag; {FIX}"),
    );
}
fn head_readings(u: &U) -> usize {
    u.gh_calls("pr", "view")
        .iter()
        .filter(|a| a.iter().any(|x| x == "state,baseRefName,headRefOid"))
        .count()
}

#[test]
fn r4_the_come_back_waits_until_github_reports_the_pushed_head() {
    let mut u = U::new();
    lagging_fix(&u, 2);
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v, u.record("u1-2"));
    assert_eq!(v["head"].as_str(), u.head().as_deref());
    // Three readings until the pushed head is seen, then the come-back's own: `ap review start`
    // and `status`.
    assert_eq!(head_readings(&u), 5);
    assert!(u.clock >= Duration::from_secs(10), "{:?}", u.clock);
    assert!(u.record("u1")["repair"].get("cause").is_none());
}

#[test]
fn r4_the_fixer_state_is_saved_before_the_wait() {
    let mut u = U::new();
    let t = u.t.display();
    // The record's path becomes a directory during the fixer and GitHub never reports the pushed
    // head: the save before the wait fails, so the head is never read and no cause is set.
    fixing(
        &u,
        &format!(
            "rm -f {r} && mkdir {r}; git rev-parse HEAD > {t}/old-head; \
             echo 1000 > {t}/view-lag; {FIX}",
            r = u.p("work/task/u1.unit.json")
        ),
    );
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 5, "{v}");
    assert_eq!(v["repair"]["state"], "ok");
    assert!(v["repair"].get("cause").is_none(), "{v}");
    assert_eq!(head_readings(&u), 0, "no wait for a head");
    assert_eq!(u.launches, 3, "no stage of a come-back");
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
}

#[test]
fn r4_a_verifier_prompt_changed_during_the_wait_ends_digest_differs() {
    let mut u = U::new();
    let t = u.t.display();
    // While GitHub still reports the old head, a line is appended to the come-back's verifier
    // prompt: the file is not the one the first record holds.
    fixing(
        &u,
        &format!(
            "git rev-parse HEAD > {t}/old-head; echo 2 > {t}/view-lag; \
             echo {t}/judge/verifier2.md > {t}/lag-append; {FIX}"
        ),
    );
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"), "stdout is the first record");
    assert_eq!(
        (&v["repair"]["state"], &v["repair"]["cause"]),
        (&json!("ok"), &json!("digest_differs"))
    );
    assert_eq!(u.launches, 3, "no stage of a come-back");
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

#[test]
fn r4_a_pushed_head_that_github_never_reports_ends_head_not_seen() {
    let mut u = U::new();
    lagging_fix(&u, 1000);
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"), "stdout is the first record");
    assert_eq!(
        (&v["repair"]["state"], &v["repair"]["cause"]),
        (&json!("ok"), &json!("head_not_seen"))
    );
    assert_eq!(v["repair"]["detail"], json!({"head": u.head()}));
    assert!(u.clock >= Duration::from_secs(120), "{:?}", u.clock);
    assert!(head_readings(&u) >= 2);
    assert_eq!(u.launches, 3, "no come-back");
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

// ---- R5 the verifier judges the automatic reviewer's threads -----------------------------------

/// An unresolved thread whose one comment is the automatic reviewer's (on a commit that is not the
/// head).
fn given(id: &str, line: u64) -> Value {
    let first = json!({"totalCount":1,"nodes":[{"author":{"__typename":REVIEWER.0,"login":REVIEWER.1},
        "originalCommit":{"oid":"0000000000000000000000000000000000000000"}}]});
    json!({"id":id,"isResolved":false,"path":"src/a.rs","line":line,"first":first,"last":first})
}
/// A unit call with the member and the automatic reviewer on: no marker, so the review's wait ends
/// at its bound on the fake clock.
fn r5_request(u: &U) -> Value {
    let mut r = u.request();
    (r["gate"], r["judge_threads"]) = (json!({"autoreview_minutes":1}), json!(true));
    r
}
/// The verifier accepts the head; `lines` (with `\\n` between them) follow.
fn accepting(u: &U, lines: &str) {
    u.script(
        "a2",
        &format!("printf 'Verdict: accept\\n%s\\n{lines}\\n' \"$(git rev-parse HEAD)\""),
    );
}
fn graphql_with(u: &U, part: &str) -> Vec<Vec<String>> {
    let calls = u.gh_calls("api", "graphql");
    calls
        .into_iter()
        .filter(|a| a.last().unwrap().contains(part))
        .collect()
}
/// The ids the runner resolved, in order.
fn resolves(u: &U) -> Vec<String> {
    graphql_with(u, "resolveReviewThread")
        .iter()
        .map(|a| {
            a.iter()
                .find_map(|x| x.strip_prefix("id="))
                .unwrap()
                .to_owned()
        })
        .collect()
}
/// The positions in `calls` of the `gh` calls `f` names.
fn at(u: &U, f: impl Fn(&[String]) -> bool) -> Vec<usize> {
    let gh =
        |(i, (p, a, _)): (usize, &(String, Vec<String>, Value))| (p == "gh" && f(a)).then_some(i);
    u.calls.iter().enumerate().filter_map(gh).collect()
}
fn ids(threads: &Value) -> Vec<Value> {
    let list = threads.as_array().unwrap().iter();
    list.map(|t| t["id"].clone()).collect()
}
/// The call ended at `step` with `cause` (exit 3) and the record on disk is the answer.
fn r5_stopped(u: &U, (v, code): (Value, u8), out: &str, step: &str, cause: &str) -> Value {
    assert_eq!(
        (
            v["state"].as_str(),
            v["stopped_at"].as_str(),
            v["cause"].as_str(),
            code
        ),
        (Some("stopped"), Some(step), Some(cause), 3),
        "{v}"
    );
    assert_eq!(v, u.record(out));
    v
}

#[test]
fn r5_a_thread_of_the_automatic_reviewer_is_judged_and_resolved_before_the_status() {
    let mut u = U::new();
    u.threads = vec![given("T1", 42)];
    accepting(&u, "Thread T1: wrong");
    let (v, code) = u.run(&r5_request(&u)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    let prompt = std::fs::read_to_string(u.t.join("work/task/u1.verifier.prompt.md")).unwrap();
    assert!(
        prompt
            .lines()
            .any(|l| l.contains("T1") && l.contains("src/a.rs") && l.contains("42")),
        "{prompt}"
    );
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":42,"word":"wrong","resolved":true}])
    );
    assert_eq!(resolves(&u), ["T1"]);
    assert_eq!(u.threads[0]["isResolved"], true);
    // Read, resolved, read back, then the status, then the merge.
    let reads = at(&u, |a| a.last().unwrap().contains("node(id:"));
    let resolve = at(&u, |a| a.last().unwrap().contains("resolveReviewThread"));
    let status = at(&u, |a| a.iter().any(|x| x.contains("/statuses/")));
    let merge = at(&u, |a| a[0] == "pr" && a[1] == "merge");
    assert_eq!((reads.len(), resolve.len(), status.len()), (2, 1, 1));
    assert!(reads[0] < resolve[0] && resolve[0] < reads[1] && reads[1] < status[0]);
    assert!(status[0] < merge[0]);
    assert_eq!(v, u.record("u1"));
}

#[test]
fn r5_judge_threads_is_a_boolean_and_not_a_member_of_a_case_call() {
    let mut u = U::new();
    for bad in [json!("yes"), json!(1), json!(null), json!({})] {
        let mut r = r5_request(&u);
        r["judge_threads"] = bad;
        assert_eq!(code(u.run(&r)), "invalid_input@judge_threads");
    }
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert_eq!((u.launches, u.calls.len()), (0, 0));
    let mut u = U::new();
    let c = case(&u);
    for value in [true, false] {
        let mut r = case_request(&u, &c);
        r["judge_threads"] = json!(value);
        assert_eq!(code(u.run(&r)), "invalid_input@judge_threads");
    }
    assert!(!copy(&u).exists());
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn r5_without_the_member_a_thread_stops_as_today() {
    for member in [None, Some(false)] {
        let mut u = U::new();
        u.threads = vec![given("T1", 42)];
        accepting(&u, "Thread T1: wrong");
        let mut r = r5_request(&u);
        match member {
            None => {
                r.as_object_mut().unwrap().remove("judge_threads");
            }
            Some(b) => r["judge_threads"] = json!(b),
        }
        let v = u.stop(&r, "gate", "unresolved_threads");
        assert_eq!(
            v["detail"]["threads"],
            json!([{"id":"T1","path":"src/a.rs","line":42,"author":"chatgpt-codex-connector[bot]",
                "answered":false,"comments":1}])
        );
        assert!(v.get("threads").is_none(), "{v}");
        assert_eq!(u.launches, 1, "no verifier stage");
        assert!(graphql_with(&u, "node(id:").is_empty() && resolves(&u).is_empty());
    }
    // With the member and no thread to give, the block of facts is today's, byte for byte.
    let mut u = U::new();
    let (v, code) = u.run(&r5_request(&u)).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v["threads"], json!([]));
    let prompt = std::fs::read_to_string(u.t.join("work/task/u1.verifier.prompt.md")).unwrap();
    let (head, base, out) = (
        v["head"].as_str().unwrap(),
        v["base"].as_str().unwrap(),
        u.p("work/task/u1"),
    );
    let wt = u.p("work/task/wt/repo");
    assert_eq!(
        prompt,
        format!(
            "{VERIFIER_PROMPT}\n## Facts of this unit (ap unit)\n\nRepository: o/r\nPull request: 7\nHead: {head}\nBase: {base}\nDiff: git -C {wt} diff {base} {head}\nGate answer: {out}.gate.json\nAcceptance output: {out}.accept.log\n"
        )
    );
    assert!(graphql_with(&u, "node(id:").is_empty() && resolves(&u).is_empty());
}

#[test]
fn r5_a_thread_with_a_reply_or_of_another_author_stops() {
    let mut reply = given("T2", 5);
    reply["first"]["totalCount"] = json!(2);
    reply["last"] = json!({"nodes":[{"author":{"__typename":"User","login":"someone"}}]});
    let mut person = given("T3", 6);
    person["first"]["nodes"][0]["author"] = json!({"__typename":"User","login":"someone"});
    // A user whose login is the reviewer's without `[bot]`.
    let mut same_name = given("T4", 7);
    same_name["first"]["nodes"][0]["author"]["__typename"] = json!("User");
    for other in [reply, person, same_name] {
        let mut u = U::new();
        u.threads = vec![given("T1", 42), other.clone()];
        accepting(
            &u,
            "Thread T1: wrong\\nThread T2: wrong\\nThread T3: wrong\\nThread T4: wrong",
        );
        let v = u.stop(&r5_request(&u), "gate", "unresolved_threads");
        assert_eq!(ids(&v["detail"]["threads"]), [other["id"].clone()], "{v}");
        assert_eq!(u.launches, 1, "no verifier stage");
        assert!(resolves(&u).is_empty());
    }
    // Without the automatic reviewer the gate names no login: no thread is given.
    let mut u = U::new();
    u.threads = vec![given("T1", 42)];
    let mut r = r5_request(&u);
    r["gate"] = json!({"autoreview": false});
    let v = u.stop(&r, "gate", "unresolved_threads");
    assert_eq!(ids(&v["detail"]["threads"]), [json!("T1")]);
    assert_eq!(u.launches, 1);
}

#[test]
fn r5_a_report_that_does_not_judge_every_thread_stops() {
    for (lines, what) in [
        ("Thread T2: fixed", "a missing line"),
        (
            "Thread T1: fixed\\nThread T2: fixed\\nThread T1: fixed",
            "a doubled line",
        ),
        (
            "Thread T1: fixed\\nThread T2: fixed\\nThread T1: wrong",
            "two words",
        ),
        ("Thread T1: right\\nThread T2: fixed", "an unknown word"),
        (
            "Thread T1: fixed.\\nThread T2: fixed",
            "a line that is not exact",
        ),
    ] {
        let mut u = U::new();
        u.threads = vec![given("T1", 1), given("T2", 2)];
        accepting(&u, lines);
        let v = u.stop(&r5_request(&u), "verifier", "threads_unjudged");
        assert_eq!(v["detail"], json!({"threads":["T1"]}), "{what}");
        assert!(u.store("start_review").is_empty() && u.store("close_review").is_empty());
        assert!(graphql_with(&u, "node(id:").is_empty() && resolves(&u).is_empty());
        let words = v["threads"].as_array().unwrap().iter();
        assert!(words.into_iter().all(|t| t.get("word").is_none()), "{v}");
    }
    // A `Verdict: repair` is read as today: its thread lines are not inspected.
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    u.script("a2", VERIFY_REPAIR);
    u.stop(&r5_request(&u), "verifier", "verdict_repair");
}

#[test]
fn r5_an_accept_with_a_blocking_thread_stops() {
    let mut u = U::new();
    u.threads = vec![given("T1", 1), given("T2", 2)];
    accepting(&u, "Thread T1: fixed\\nThread T2: blocking");
    let v = u.stop(&r5_request(&u), "verifier", "thread_blocking");
    assert_eq!(v["detail"], json!({"threads":["T2"]}));
    assert!(u.store("close_review").is_empty());
    assert!(graphql_with(&u, "node(id:").is_empty() && resolves(&u).is_empty());
    // An id without its line is named first.
    let mut u = U::new();
    u.threads = vec![given("T1", 1), given("T2", 2)];
    accepting(&u, "Thread T2: blocking");
    let v = u.stop(&r5_request(&u), "verifier", "threads_unjudged");
    assert_eq!(v["detail"], json!({"threads":["T1"]}));
}

#[test]
fn r5_a_thread_that_changed_before_the_resolve_stops() {
    for change in ["resolved", "reply"] {
        let mut u = U::new();
        u.threads = vec![given("T1", 1), given("T2", 2)];
        accepting(&u, "Thread T1: fixed\\nThread T2: note");
        u.thread_change_on_close = Some(change);
        let v = u.stop(&r5_request(&u), "status", "thread_changed");
        assert_eq!(v["detail"], json!({"threads":["T2"]}), "{change}");
        assert!(resolves(&u).is_empty(), "nothing resolved: {change}");
        assert_eq!(u.threads[0]["isResolved"], false);
        assert_eq!(u.store("close_review").len(), 1);
    }
}

#[test]
fn r5_a_thread_that_does_not_read_back_resolved_stops() {
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: fixed");
    u.resolve_fails = true;
    let v = u.stop(&r5_request(&u), "status", "thread_not_resolved");
    assert_eq!(v["detail"], json!({"threads":["T1"]}));
    assert_eq!(resolves(&u), ["T1"]);
    assert!(v["threads"][0].get("resolved").is_none(), "{v}");
}

#[test]
fn r5_a_verdict_come_back_resolves_only_what_the_report_judged() {
    let come_back = |u: &U, out: &str| {
        let mut r = r5_request(u);
        r.as_object_mut().unwrap().remove("verifier");
        (r["pull_request"], r["verdict"]) = (json!(7), json!(u.p("work/task/u1.verifier.md")));
        r["out"] = json!(u.p(&format!("work/task/{out}")));
        r
    };
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: fixed");
    let mut r = r5_request(&u);
    r["merge"] = json!(false);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    // A thread the pinned report has no line for stops at the gate.
    u.threads.push(given("T2", 2));
    let ended = u.run(&come_back(&u, "u2")).unwrap();
    let v = r5_stopped(&u, ended, "u2", "gate", "unresolved_threads");
    assert_eq!(ids(&v["detail"]["threads"]), [json!("T2")]);
    assert!(resolves(&u).is_empty() && u.statuses().is_empty());
    u.threads.pop();
    let (v, code) = u.run(&come_back(&u, "u3")).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1,"word":"fixed","resolved":true}])
    );
    assert_eq!(resolves(&u), ["T1"]);
    assert_eq!(u.launches, 2, "no stage in a come-back");

    // A thread whose line says `blocking` stops at the gate of a come-back.
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: blocking");
    u.stop(&r5_request(&u), "verifier", "thread_blocking");
    let ended = u.run(&come_back(&u, "u2")).unwrap();
    let v = r5_stopped(&u, ended, "u2", "gate", "unresolved_threads");
    assert_eq!(ids(&v["detail"]["threads"]), [json!("T1")]);
    assert!(resolves(&u).is_empty() && u.statuses().is_empty() && u.merges().is_empty());
}

#[test]
fn r5_the_round_judges_the_threads_of_the_new_head() {
    let mut u = U::new();
    let t = u.t.display().to_string();
    // The fixer pushes and the automatic reviewer opens a second thread.
    fixing(&u, &format!("touch {t}/new-thread; {FIX}"));
    u.script(
        "a2",
        "if [ -f notes.md ]; then printf 'Verdict: accept\\n%s\\nThread T1: fixed\\nThread T2: wrong\\n' \"$(git rev-parse HEAD)\"; else printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"; fi",
    );
    u.threads = vec![given("T1", 1)];
    let mut r = r4_request(&u);
    (r["gate"], r["judge_threads"]) = (json!({"autoreview_minutes":1}), json!(true));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v, u.record("u1-2"));
    let first = u.record("u1");
    assert_eq!(first["cause"], "verdict_repair");
    assert_eq!(
        first["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1}])
    );
    let prompt = std::fs::read_to_string(u.t.join("work/task/u1-2.verifier.prompt.md")).unwrap();
    for (id, line) in [("T1", "1"), ("T2", "9")] {
        let named = |l: &&str| l.contains(id) && l.contains("src/a.rs") && l.contains(line);
        assert!(prompt.lines().any(|l| named(&l)), "{prompt}");
    }
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1,"word":"fixed","resolved":true},
            {"id":"T2","path":"src/a.rs","line":9,"word":"wrong","resolved":true}])
    );
    assert_eq!(resolves(&u), ["T1", "T2"]);
    assert_eq!(u.statuses().len(), 1);
}

#[test]
fn r5_a_call_that_does_not_merge_resolves_nothing() {
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: note");
    let mut r = r5_request(&u);
    r["merge"] = json!(false);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1,"word":"note"}])
    );
    assert!(graphql_with(&u, "node(id:").is_empty() && resolves(&u).is_empty());
    assert_eq!(u.threads[0]["isResolved"], false);
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

/// `finding_on_the_head` with the member: the summary names an unresolved finding for the head.
fn r6_unit() -> (U, Value) {
    let (u, mut r, _) = finding_on_the_head();
    r["judge_threads"] = json!(true);
    (u, r)
}

#[test]
fn r5_a_given_thread_passes_the_marker_of_the_reviewer_on_the_head() {
    let (mut u, r) = r6_unit();
    // A thread that is given, on another commit than the head: the summary's finding is that thread.
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: fixed");
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    let gate: Value =
        serde_json::from_slice(&std::fs::read(v["files"]["gate"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(gate["autoreview"]["has_unresolved_findings"], true);
    assert_eq!(gate["autoreview"]["threads_on_head"], 0);
    // The given thread goes on as without the marker: named, judged, resolved in `status`.
    let prompt = std::fs::read_to_string(u.t.join("work/task/u1.verifier.prompt.md")).unwrap();
    assert!(
        prompt
            .lines()
            .any(|l| l.contains("T1") && l.contains("src/a.rs")),
        "{prompt}"
    );
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1,"word":"fixed","resolved":true}])
    );
    assert_eq!(resolves(&u), ["T1"]);
}

#[test]
fn r5_a_given_thread_passes_the_marker_in_a_verdict_come_back() {
    let (mut u, mut r) = r6_unit();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: wrong");
    r["merge"] = json!(false);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    r.as_object_mut().unwrap().remove("verifier");
    (r["pull_request"], r["verdict"]) = (json!(7), json!(u.p("work/task/u1.verifier.md")));
    (r["out"], r["merge"]) = (json!(u.p("work/task/u2")), json!(true));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        v["threads"],
        json!([{"id":"T1","path":"src/a.rs","line":1,"word":"wrong","resolved":true}])
    );
    assert_eq!(resolves(&u), ["T1"]);
    // The report judges a thread `blocking`: it is not given, the marker alone stops again.
    let (mut u, mut r) = r6_unit();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: blocking");
    u.stop(&r, "verifier", "thread_blocking");
    r.as_object_mut().unwrap().remove("verifier");
    (r["pull_request"], r["verdict"]) = (json!(7), json!(u.p("work/task/u1.verifier.md")));
    r["out"] = json!(u.p("work/task/u2"));
    let ended = u.run(&r).unwrap();
    r5_stopped(&u, ended, "u2", "gate", "unresolved_findings");
    assert!(resolves(&u).is_empty() && u.statuses().is_empty());
}

#[test]
fn r5_the_marker_alone_with_no_thread_given_still_stops() {
    let other = "0000000000000000000000000000000000000000";
    let mut person = given("T1", 1);
    person["first"]["nodes"][0]["author"] = json!({"__typename":"User","login":"someone"});
    let mut reply = given("T1", 1);
    reply["first"]["totalCount"] = json!(2);
    let cases: [(&str, Vec<Value>); 4] = [
        ("no thread", vec![]),
        ("a resolved thread", vec![thread(REVIEWER, other, true)]),
        ("a thread of a person", vec![person]),
        ("a thread with a reply", vec![reply]),
    ];
    for (what, threads) in cases {
        let (mut u, r) = r6_unit();
        u.threads = threads;
        accepting(&u, "Thread T1: fixed");
        // A thread that is not given and unresolved is seen as the marker alone: the cause as today.
        u.stop(&r, "gate", "unresolved_findings");
        assert_eq!(u.launches, 1, "{what}");
        assert!(resolves(&u).is_empty(), "{what}");
    }
    // The threads could not be read: `threads_on_head` is `null`.
    let (mut u, r) = r6_unit();
    u.threads_unreadable = true;
    u.stop(&r, "gate", "unresolved_findings");
    assert_eq!(u.launches, 1);
}

#[test]
fn r5_a_given_thread_does_not_pass_the_marker_without_the_member() {
    for member in [None, Some(false)] {
        let (mut u, mut r, _) = finding_on_the_head();
        if let Some(b) = member {
            r["judge_threads"] = json!(b);
        }
        u.threads = vec![given("T1", 1)];
        accepting(&u, "Thread T1: fixed");
        u.stop(&r, "gate", "unresolved_findings");
        assert_eq!(u.launches, 1, "{member:?}");
        assert!(resolves(&u).is_empty());
    }
}

#[test]
fn r5_a_thread_that_is_not_given_stops_beside_given_ones_and_the_marker() {
    let mut person = given("T2", 2);
    person["first"]["nodes"][0]["author"] = json!({"__typename":"User","login":"someone"});
    let (mut u, r) = r6_unit();
    u.threads = vec![given("T1", 1), person];
    accepting(&u, "Thread T1: fixed");
    let v = u.stop(&r, "gate", "unresolved_threads");
    assert_eq!(ids(&v["detail"]["threads"]), [json!("T2")], "{v}");
    assert_eq!(u.launches, 1, "no verifier stage");
    assert!(resolves(&u).is_empty());
}

#[test]
fn r5_a_fixer_prompt_that_would_be_refused_is_refused_before_any_effect() {
    let mut u = U::new();
    // The page's path holds the fixer's answer name `u1.fixer.md`, and the fixer's facts name the page.
    let dir = u.t.join("work/task/u1.fixer.md.d");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(u.t.join("work/task/TASK.md"), dir.join("TASK.md")).unwrap();
    let mut r = r4_request(&u);
    r["page"] = json!(u.p("work/task/u1.fixer.md.d/TASK.md"));
    assert_eq!(code(u.run(&r)), "out_named_in_prompt@repair.prompt");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "u1.fixer.md.d"]);
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn r5_a_stage_whose_record_could_not_be_written_ends_the_step() {
    let unwritable = |u: &U, which: &str| {
        let r = u.p(&format!("work/task/u1.{which}.md.run.json"));
        format!("; rm -f {r} && mkdir {r}")
    };
    let mut u = U::new();
    u.script(
        "a1",
        &format!("{IMPLEMENT}{}", unwritable(&u, "implementer")),
    );
    u.stop(&u.request(), "implementer", "stage_record_unwritten");
    assert_eq!(u.launches, 1);
    // The one call before the implementer is the read of the task's board.
    let board =
        |(p, _, b): &(String, Vec<String>, Value)| p == "ont" && b["object_type"] == "board";
    assert!(u.calls.iter().all(board), "no pull request read");

    let mut u = U::new();
    u.script(
        "a2",
        &format!("{VERIFY_ACCEPT}{}", unwritable(&u, "verifier")),
    );
    u.stop(&u.request(), "verifier", "stage_record_unwritten");
    assert!(u.store("start_review").is_empty());

    let mut u = U::new();
    fixing(&u, &format!("{FIX}{}", unwritable(&u, "fixer")));
    u.script("a2", VERIFY_AFTER_FIX);
    let (v, code) = u.run(&r4_request(&u)).unwrap();
    assert_eq!(code, 3, "{v}");
    assert_eq!(v, u.record("u1"), "stdout is the first record");
    assert_eq!(
        (&v["repair"]["state"], &v["repair"]["cause"]),
        (&json!("ok"), &json!("stage_record_unwritten"))
    );
    assert_eq!(u.launches, 3, "no come-back");
    assert!(!exists(&u, "work/task/u1-2.unit.json"));
    assert!(u.statuses().is_empty() && u.merges().is_empty());
}

/// The threads are resolved only after every condition of the status held: a head that moved after
/// the review closed stops `status` before any thread is read or resolved.
#[test]
fn r5_a_head_that_moved_before_the_status_resolves_nothing() {
    let mut u = U::new();
    u.threads = vec![given("T1", 1)];
    accepting(&u, "Thread T1: fixed");
    u.move_head_on_close = true;
    u.stop(&r5_request(&u), "status", "head_differs");
    assert!(graphql_with(&u, "node(id:").is_empty(), "no thread read");
    assert!(resolves(&u).is_empty(), "no thread resolved");
    assert_eq!(u.threads[0]["isResolved"], false);
}

/// No cause of the threads starts a round: with `repair`, each ends the call with no fixer stage and
/// no come-back.
#[test]
fn r5_no_cause_of_the_threads_starts_a_round() {
    let cases: [(&str, &str, Option<&'static str>, &str); 3] = [
        ("Thread T1: blocking", "verifier", None, "thread_blocking"),
        ("Thread T2: fixed", "verifier", None, "threads_unjudged"),
        (
            "Thread T1: fixed",
            "status",
            Some("reply"),
            "thread_changed",
        ),
    ];
    for (lines, step, change, cause) in cases {
        let mut u = U::new();
        fixing(&u, FIX);
        u.threads = vec![given("T1", 1)];
        accepting(&u, lines);
        u.thread_change_on_close = change;
        let mut r = r4_request(&u);
        (r["gate"], r["judge_threads"]) = (json!({"autoreview_minutes":1}), json!(true));
        let v = u.stop(&r, step, cause);
        assert_eq!(u.launches, 2, "no fixer stage: {cause}");
        let keys: Vec<_> = v["repair"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["prompt", "verifier_prompt"], "{cause}");
        assert!(!exists(&u, "work/task/u1.fixer.prompt.md"), "{cause}");
        assert!(!exists(&u, "work/task/u1.fixer.md.run.json"), "{cause}");
        assert!(!exists(&u, "work/task/u1-2.unit.json"), "{cause}");
        assert!(resolves(&u).is_empty(), "{cause}");
    }
}

// ---- S9 a unit call names an item of the task's board ------------------------------------------

fn item_request(u: &U, key: &str) -> Value {
    let mut r = u.request();
    r["item"] = json!(key);
    r
}
/// A refusal before any effect: the code, and no stage, no GitHub call, no store write other than
/// `start_item`, no file of the call, no worktree, no branch.
fn refused_untouched(u: &mut U, r: &Value, want: &str) {
    assert_eq!(code(u.run(r)), want);
    assert_eq!(u.effects(), (0, 0, 0), "{want}");
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"], "{want}");
    assert!(!u.wt().exists(), "{want}");
    assert_eq!(branches(u), "* main", "{want}");
}
/// The `ont` actions applied, in order.
fn actions(u: &U) -> Vec<String> {
    let applied = u
        .calls
        .iter()
        .filter(|(p, a, _)| p == "ont" && a[0] == "action");
    applied
        .map(|(_, _, b)| b["action_type"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn s9_1_item_is_a_string_of_at_most_60_bytes_and_no_member_of_a_case_call() {
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    for bad in [
        json!(1),
        json!(""),
        json!("x".repeat(61)),
        json!(null),
        json!(["k1"]),
    ] {
        let mut r = u.request();
        r["item"] = bad.clone();
        assert_eq!(code(u.run(&r)), "invalid_input@item", "{bad}");
    }
    assert_eq!((u.launches, u.calls.len()), (0, 0));
    // 60 bytes pass the member's check and reach the store: no item of the task has that key.
    let r = item_request(&u, &"x".repeat(60));
    refused_untouched(&mut u, &r, "item_unknown@item");
    let c = case(&u);
    let mut r = case_request(&u, &c);
    r["item"] = json!("k1");
    assert_eq!(code(u.run(&r)), "invalid_input@item");
    assert!(!u.t.join("work/task/c1.unit.json").exists());
}

#[test]
fn s9_2a_without_a_board_an_item_is_refused_and_a_call_without_one_runs_as_before() {
    let mut u = U::new();
    let r = item_request(&u, "k1");
    refused_untouched(&mut u, &r, "item_not_ordered@item");
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert!(v.get("item").is_none(), "{v}");
    assert_eq!(actions(&u), ["start_review", "close_review"]);
    // The verifier's block of facts is the one of a call before the member existed, byte for byte.
    let prompt = std::fs::read_to_string(u.t.join("bin/prompt.2")).unwrap();
    let (head, base) = (v["head"].as_str().unwrap(), v["base"].as_str().unwrap());
    let (wt, out) = (u.p("work/task/wt/repo"), u.p("work/task/u1"));
    assert_eq!(
        prompt,
        format!(
            "{VERIFIER_PROMPT}\n## Facts of this unit (ap unit)\n\nRepository: o/r\nPull request: 7\nHead: {head}\nBase: {base}\nDiff: git -C {wt} diff {base} {head}\nGate answer: {out}.gate.json\nAcceptance output: {out}.accept.log\n"
        )
    );
}

#[test]
fn s9_2b_2c_2d_refusals_of_an_item_come_before_any_effect() {
    type Change = fn(&mut U);
    let cases: [(&str, &str, Change); 7] = [
        ("", "item_required@item", |_| ()),
        ("k9", "item_unknown@item", |_| ()),
        ("k1", "item_done@item", |u| {
            u.set("it-k1", "state", json!("done"))
        }),
        ("k1", "item_dropped@item", |u| {
            u.set("it-k1", "state", json!("dropped"))
        }),
        ("k1", "objective_not_current@item", |u| {
            u.set("o1", "status", json!("retired"))
        }),
        ("k1", "objective_source_superseded@item", |u| {
            u.set("d1", "status", json!("superseded"))
        }),
        // A key of another task's item is no item of this task.
        ("k1", "item_unknown@item", |u| {
            u.set("it-k1", "task", oid("task", "other-task"))
        }),
    ];
    for (key, want, change) in cases {
        let mut u = U::new();
        u.board(&[("k1", "queued")], &["k1"]);
        change(&mut u);
        let r = match key {
            "" => u.request(),
            k => item_request(&u, k),
        };
        refused_untouched(&mut u, &r, want);
        assert!(actions(&u).is_empty(), "{want}");
    }
}

#[test]
fn s9_2e_an_item_in_work_goes_on_only_for_its_own_repository_and_branch() {
    for (repo, branch) in [("o/other", BRANCH), ("o/r", "ap/another")] {
        let mut u = U::new();
        u.board(&[("k1", "in_work")], &["k1"]);
        u.set("it-k1", "repository", json!(repo));
        u.set("it-k1", "branch", json!(branch));
        let r = item_request(&u, "k1");
        refused_untouched(&mut u, &r, "item_of_another_unit@item");
        assert!(actions(&u).is_empty());
    }
    // A call repeated after a death: the item is in work for this repository and branch.
    let mut u = U::new();
    u.board(&[("k1", "in_work")], &["k1"]);
    u.set("it-k1", "repository", json!("o/r"));
    u.set("it-k1", "branch", json!(BRANCH));
    let (v, code) = u.run(&item_request(&u, "k1")).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        actions(&u),
        ["start_review", "close_review", "close_item"],
        "no second start"
    );
}

#[test]
fn s9_2f_a_queued_item_is_started_before_any_other_effect_or_the_call_is_refused() {
    // Not named by the board: the store refuses the start; nothing else happens.
    let mut u = U::new();
    u.board(&[("k1", "queued"), ("k2", "queued")], &["k2"]);
    let r = item_request(&u, "k1");
    refused_untouched(&mut u, &r, "item_not_ordered@item");
    assert_eq!(actions(&u), ["start_item"]);
    assert_eq!(u.object("it-k1")["state"], "queued");
    // Any other refusal of the store: `item_unwritten` with the store's code.
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    u.start_refuses = Some("repository_and_branch");
    let f = u.run(&item_request(&u, "k1")).unwrap_err();
    assert_eq!((f.code, f.field.as_str()), ("item_unwritten", "item"));
    assert_eq!(f.detail, json!({"code": "repository_and_branch"}));
    assert_eq!(u.effects(), (0, 0, 0));
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    // Started with the request's repository and branch, before the first call to GitHub.
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    let mut r = item_request(&u, "k1");
    r["merge"] = json!(false);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    let start = &u.store("start_item")[0]["parameters"];
    assert_eq!(
        start,
        &json!({"item": oid("item", "it-k1"), "repository": "o/r", "branch": BRANCH})
    );
    let first_gh = u.calls.iter().position(|(p, ..)| p == "gh").unwrap();
    let started = u
        .calls
        .iter()
        .position(|(_, _, b)| b["action_type"] == "start_item");
    assert!(started.unwrap() < first_gh);
    let item = u.object("it-k1");
    assert_eq!(
        (&item["state"], &item["repository"], &item["branch"]),
        (&json!("in_work"), &json!("o/r"), &json!(BRANCH))
    );
    // Released by hand: a new call for the same item, repository and branch starts it again.
    u.set("it-k1", "state", json!("queued"));
    std::fs::remove_dir_all(u.wt()).unwrap();
    git(&u.t.join("clone"), &["worktree", "prune"]);
    git(&u.t.join("clone"), &["branch", "-D", BRANCH]);
    let ref_ = format!("refs/heads/{BRANCH}");
    git(&u.t.join("origin.git"), &["update-ref", "-d", &ref_]);
    let mut r = item_request(&u, "k1");
    (r["merge"], r["out"]) = (json!(false), json!(u.p("work/task/u2")));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("accepted"), 0), "{v}");
    let starts = u.store("start_item");
    assert_eq!(starts.len(), 2);
    assert_ne!(starts[0]["request_id"], starts[1]["request_id"]);
    assert_eq!(u.object("it-k1")["state"], "in_work");
}

#[test]
fn s9_3_5_the_record_names_the_item_the_verifier_reads_it_and_the_merge_closes_it() {
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    let (v, code) = u.run(&item_request(&u, "k1")).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(
        v["item"],
        json!({"key": "k1", "id": oid("item", "it-k1"),
            "objective": {"name": "ship-board", "id": oid("objective", "o1")}, "closed": true})
    );
    let prompt = std::fs::read_to_string(u.t.join("bin/prompt.2")).unwrap();
    let facts = prompt.strip_prefix(VERIFIER_PROMPT).unwrap();
    for fact in [
        "k1",
        "TITLE of k1",
        "STATEMENT of k1",
        "FIRST-USE of k1",
        "ship-board",
        "OBJECTIVE-STATEMENT the board orders the work",
    ] {
        assert!(facts.contains(fact), "{fact} in {facts}");
    }
    // The close comes after the merge and names the accepting review of this unit.
    let order: Vec<&str> = u
        .calls
        .iter()
        .filter_map(|(p, a, b)| match p.as_str() {
            "ont" => b["action_type"].as_str(),
            _ if a.len() > 1 && a[1] == "merge" => Some("merge"),
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        [
            "start_item",
            "start_review",
            "close_review",
            "merge",
            "close_item"
        ]
    );
    let item = u.object("it-k1");
    assert_eq!(
        (&item["state"], &item["review"]),
        (&json!("done"), &v["review"])
    );
}

#[test]
fn s9_4_an_item_dropped_or_done_before_the_merge_stops_the_call_unmerged() {
    for (state, cause) in [("dropped", "item_dropped"), ("done", "item_done")] {
        let mut u = U::new();
        u.board(&[("k1", "queued")], &["k1"]);
        u.item_on_close = Some(state);
        let (v, code) = u.run(&item_request(&u, "k1")).unwrap();
        assert_eq!(
            (&v["state"], &v["stopped_at"], &v["cause"], code),
            (&json!("stopped"), &json!("merge"), &json!(cause), 3),
            "{v}"
        );
        assert!(u.merges().is_empty(), "{cause}");
        assert!(u.store("close_item").is_empty(), "{cause}");
        assert!(v["item"].get("closed").is_none(), "{v}");
    }
}

#[test]
fn s9_5_a_close_the_store_refuses_leaves_the_unit_merged_and_the_item_in_work() {
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    u.close_refuses = Some("review_closes_another_item");
    let (v, code) = u.run(&item_request(&u, "k1")).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(v["item"]["closed"], false);
    assert_eq!(v["item"]["code"], "review_closes_another_item");
    assert_eq!(v, u.record("u1"));
    assert_eq!(u.object("it-k1")["state"], "in_work");
    // `close_item` applied later by hand succeeds: the item is still in work.
    u.close_refuses = None;
    let by_hand = json!({"action_type": "close_item", "request_id": "by-hand",
        "parameters": {"item": oid("item", "it-k1"), "review": v["review"]}});
    assert!(u.item_action(&by_hand).unwrap().success);
    assert_eq!(u.object("it-k1")["state"], "done");
}

#[test]
fn s9_5_a_call_that_does_not_merge_closes_nothing() {
    // `merge: false`, and a stop: the item stays in work, nothing closes or releases it.
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    let mut r = item_request(&u, "k1");
    r["merge"] = json!(false);
    let (v, _) = u.run(&r).unwrap();
    assert_eq!(v["state"], "accepted");
    assert!(v["item"].get("closed").is_none(), "{v}");
    assert_eq!(actions(&u), ["start_item", "start_review", "close_review"]);
    assert_eq!(u.object("it-k1")["state"], "in_work");
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    u.script("a2", VERIFY_REPAIR);
    u.stop(&item_request(&u, "k1"), "verifier", "verdict_repair");
    assert_eq!(actions(&u), ["start_item"]);
    assert_eq!(u.object("it-k1")["state"], "in_work");
}

#[test]
fn s9_2_the_come_back_of_a_round_goes_on_for_the_item_it_started() {
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    fixing(&u, FIX);
    u.script("a2", VERIFY_AFTER_FIX);
    let mut r = r4_request(&u);
    r["item"] = json!("k1");
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(u.store("start_item").len(), 1);
    assert_eq!(v["item"]["closed"], true);
    assert!(u.record("u1")["item"].get("closed").is_none());
    assert_eq!(u.object("it-k1")["state"], "done");
}

// ---- S9 `ap backlog` ---------------------------------------------------------------------------

fn backlog(u: &mut U, args: &[&str]) -> Result<String, Failure> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    ap::backlog::execute(u, &args)
}
/// A board of every kind of element: k1 queued, k2 in work, k3 done, k4 dropped, k7 of a retired
/// objective, an item of another task and a missing element; k5 queued and k6 in work unlisted.
fn full_board(u: &mut U) {
    let states = [
        ("k1", "queued"),
        ("k2", "in_work"),
        ("k3", "done"),
        ("k4", "dropped"),
        ("k5", "queued"),
        ("k6", "in_work"),
        ("k7", "queued"),
    ];
    u.board(&states, &["k2", "k1", "k3", "k4", "k7", "kx", "missing"]);
    for (k, repo, branch) in [("it-k2", "o/r", "b2"), ("it-k6", "o/s", "b6")] {
        u.set(k, "repository", json!(repo));
        u.set(k, "branch", json!(branch));
    }
    let task = oid("task", "store-task");
    u.objects.extend([
        json!({"id": oid("objective", "o3"), "rev": 0, "task": task, "name": "old-aim",
            "statement": "an aim given up", "rank": 3, "status": "retired",
            "source": oid("decision", "d1")}),
        json!({"id": oid("objective", "o2"), "rev": 0, "task": task, "name": "measured",
            "statement": "a measured aim", "rank": 2, "status": "confirmed",
            "source": oid("decision", "d1"), "check": "true", "last_exit": 0, "last_line": "ok",
            "last_checked_at": "2026-11-15T12:30:00Z"}),
        json!({"id": oid("item", "it-kx"), "rev": 0, "task": oid("task", "other-task"),
            "key": "kx", "state": "queued"}),
    ]);
    u.set("it-k7", "objective", oid("objective", "o3"));
    for (key, by, status, triage) in [
        ("d2", "user", "current", None),
        ("d3", "lead", "current", None),
        ("d4", "user", "superseded", None),
        ("d5", "user", "current", Some("none")),
    ] {
        let mut d = json!({"id": oid("decision", key), "rev": 0, "task": task,
            "topic": format!("topic-{key}"), "decided_by": by, "status": status,
            "decided_at": "2026-11-15T10:00:00Z"});
        if let Some(t) = triage {
            d["triage"] = json!(t);
        }
        u.objects.push(d);
    }
}

#[test]
fn s9_6_backlog_prints_the_board_with_the_refusal_of_each_item_and_writes_nothing() {
    let mut u = U::new();
    full_board(&mut u);
    let v: Value = serde_json::from_str(&backlog(&mut u, &["store-task"]).unwrap()).unwrap();
    assert_eq!(
        v["board"],
        json!({"set_at": "2026-11-15T12:00:00Z", "reason": "the order of the next work"})
    );
    let names: Vec<_> = v["objectives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].clone())
        .collect();
    assert_eq!(names, ["ship-board", "measured", "old-aim"], "by rank");
    assert_eq!(
        v["objectives"][1],
        json!({"name": "measured", "rank": 2, "status": "confirmed", "statement": "a measured aim",
            "last_check": {"exit": 0, "line": "ok", "at": "2026-11-15T12:30:00Z"}})
    );
    assert_eq!(v["objectives"][0]["last_check"], Value::Null);
    let order: Vec<_> = v["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["key"].clone(), i["state"].clone(), i["refusal"].clone()))
        .collect();
    assert_eq!(
        order,
        [
            (json!("k2"), json!("in_work"), json!("item_of_another_unit")),
            (json!("k1"), json!("queued"), Value::Null),
            (json!("k3"), json!("done"), json!("item_done")),
            (json!("k4"), json!("dropped"), json!("item_dropped")),
            (json!("k7"), json!("queued"), json!("objective_not_current")),
        ]
    );
    assert_eq!(
        v["order"][0],
        json!({"key": "k2", "title": "TITLE of k2", "state": "in_work", "objective": "ship-board",
            "repository": "o/r", "branch": "b2", "refusal": "item_of_another_unit"})
    );
    assert!(v["order"][1].get("repository").is_none());
    let bad: Vec<_> = v["bad"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["id"]["native_key"].clone(), b["problem"].clone()))
        .collect();
    assert_eq!(
        bad,
        [
            (json!("it-k3"), json!("done")),
            (json!("it-k4"), json!("dropped")),
            (json!("it-kx"), json!("another_task")),
            (json!("it-missing"), json!("missing")),
        ]
    );
    assert_eq!(
        v["unplaced"],
        json!([{"key": "k5", "title": "TITLE of k5", "added_at": "2026-11-15T11:00:00Z"}])
    );
    assert_eq!(
        v["in_work_unlisted"],
        json!([{"key": "k6", "title": "TITLE of k6", "repository": "o/s", "branch": "b6"}])
    );
    assert_eq!(v["untriaged"], json!({"count": 1, "topics": ["topic-d2"]}));
    // The verb reads only.
    assert!(actions(&u).is_empty());
    assert!(
        u.calls
            .iter()
            .all(|(p, a, _)| p == "ont" && ["search", "get"].contains(&a[0].as_str()))
    );
    // The refusal printed is the one a unit call for the item is refused with.
    for (key, _, refusal) in &order {
        if let Some(code_) = refusal.as_str() {
            let r = item_request(&u, key.as_str().unwrap());
            assert_eq!(code(u.run(&r)), format!("{code_}@item"));
        }
    }
    assert!(actions(&u).is_empty());
}

#[test]
fn s9_6_backlog_text_is_one_line_per_item_and_at_most_80_lines() {
    let mut u = U::new();
    full_board(&mut u);
    let text = backlog(&mut u, &["store-task", "--text"]).unwrap();
    for word in [
        "k1",
        "k2",
        "k3",
        "k4",
        "k5",
        "k6",
        "k7",
        "item_of_another_unit",
        "topic-d2",
    ] {
        assert!(text.contains(word), "{word} in {text}");
    }
    let lines = |t: &str| t.lines().filter(|l| l.contains("TITLE of k")).count();
    assert_eq!(lines(&text), 7, "{text}");
    let task = oid("task", "store-task");
    for n in 0..100 {
        u.objects.push(
            json!({"id": oid("item", &format!("it-m{n}")), "rev": 0, "task": task,
            "key": format!("m{n}"), "title": "a\nqueued item", "state": "queued",
            "objective": oid("objective", "o1"), "added_at": "2026-11-15T11:00:00Z"}),
        );
    }
    let text = backlog(&mut u, &["store-task", "--text"]).unwrap();
    assert!(text.lines().count() <= 80, "{}", text.lines().count());
}

#[test]
fn s9_6_backlog_of_a_task_without_a_board() {
    let mut u = U::new();
    full_board(&mut u);
    u.objects.retain(|o| o["id"]["object_type"] != "board");
    let v: Value = serde_json::from_str(&backlog(&mut u, &["store-task"]).unwrap()).unwrap();
    let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["board", "objectives", "unplaced", "untriaged"]);
    assert_eq!(v["board"], Value::Null);
    let unplaced: Vec<_> = v["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["key"].clone())
        .collect();
    assert_eq!(unplaced, ["k1", "k5", "k7"]);
    assert_eq!(v["untriaged"]["count"], 1);
    for bad in [&["store-task", "--bogus"][..], &["Bad/Key"], &[]] {
        assert_eq!(backlog(&mut u, bad).unwrap_err().code, "usage", "{bad:?}");
    }
    assert!(actions(&u).is_empty());
}

#[test]
fn s9_2_a_verdict_call_resolves_the_item_and_closes_it_with_the_reused_review() {
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    let mut r = item_request(&u, "k1");
    r["merge"] = json!(false);
    let (first, exit) = u.run(&r).unwrap();
    assert_eq!(
        (first["state"].as_str(), exit),
        (Some("accepted"), 0),
        "{first}"
    );
    // The merging call with the first call's verdict.
    let mut r = u.request();
    r.as_object_mut().unwrap().remove("verifier");
    (r["pull_request"], r["verdict"], r["out"]) = (
        json!(7),
        json!(u.p("work/task/u1.verifier.md")),
        json!(u.p("work/task/u2")),
    );
    // Without `item`: refused before any effect.
    let (calls, launches) = (u.calls.len(), u.launches);
    assert_eq!(code(u.run(&r)), "item_required@item");
    let new = &u.calls[calls..];
    assert!(
        new.iter().all(|(p, a, _)| p == "ont" && a[0] == "search"),
        "{new:?}"
    );
    assert_eq!(u.launches, launches);
    assert!(!u.t.join("work/task/u2.unit.json").exists());
    // With `item`: the item in work for this branch goes on, merges and closes with the reused review.
    r["item"] = json!("k1");
    let (v, exit) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), exit), (Some("merged"), 0), "{v}");
    assert_eq!(u.store("start_item").len(), 1, "no second start");
    let review = v["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "review");
    assert_eq!(review.unwrap()["detail"], json!({"reused": true}));
    assert_eq!(v["review"], first["review"]);
    assert_eq!(v["item"]["closed"], true);
    let close = &u.store("close_item")[0]["parameters"];
    assert_eq!(
        close,
        &json!({"item": oid("item", "it-k1"), "review": first["review"]})
    );
    assert_eq!(u.object("it-k1")["state"], "done");
    // Dropped after the resolution and before the merge: the call with `verdict` stops unmerged.
    let mut u = U::new();
    u.board(&[("k1", "queued")], &["k1"]);
    let mut r = item_request(&u, "k1");
    r["merge"] = json!(false);
    u.run(&r).unwrap();
    r.as_object_mut().unwrap().remove("verifier");
    (r["merge"], r["pull_request"], r["verdict"], r["out"]) = (
        json!(true),
        json!(7),
        json!(u.p("work/task/u1.verifier.md")),
        json!(u.p("work/task/u2")),
    );
    u.item_on_status = Some("dropped");
    let (v, exit) = u.run(&r).unwrap();
    assert_eq!(
        (&v["state"], &v["stopped_at"], &v["cause"], exit),
        (
            &json!("stopped"),
            &json!("merge"),
            &json!("item_dropped"),
            3
        ),
        "{v}"
    );
    assert!(u.merges().is_empty() && u.store("close_item").is_empty());
}

#[test]
fn s9_6_backlog_counts_every_untriaged_row_and_names_at_most_ten_topics() {
    let mut u = U::new();
    full_board(&mut u);
    // Eleven more current rows of the user without triage: twelve with `d2`.
    let task = oid("task", "store-task");
    for n in 0..11 {
        u.objects.push(
            json!({"id": oid("decision", &format!("e{n}")), "rev": 0, "task": task,
            "topic": format!("topic-e{n:02}"), "decided_by": "user", "status": "current",
            "decided_at": format!("2026-11-15T11:{n:02}:00Z")}),
        );
    }
    let v: Value = serde_json::from_str(&backlog(&mut u, &["store-task"]).unwrap()).unwrap();
    assert_eq!(v["untriaged"]["count"], 12);
    let mut want = vec!["topic-d2".to_owned()];
    want.extend((0..9).map(|n| format!("topic-e{n:02}")));
    assert_eq!(v["untriaged"]["topics"], json!(want), "the ten oldest");
}

// ---- the page's block: a unit call without `acceptance` ----------------------------------------

/// A page with a typed block and no line `Write set:`.
fn block_page(u: &U, block: &Value) {
    let text = format!("# TASK: a unit\n```unit\n{block}\n```\nOther line\n");
    std::fs::write(u.t.join("work/task/TASK.md"), text).unwrap();
}
fn block(checks: Value) -> Value {
    json!({"unit": "U-1", "write_set": ["src/**", "notes.md"], "checks": checks})
}
fn without_acceptance(u: &U) -> Value {
    let mut r = u.request();
    r.as_object_mut().unwrap().remove("acceptance");
    r
}
fn accept_in(t: &std::path::Path) -> (i32, Vec<String>) {
    let page = t.join("work/task/TASK.md");
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("accept")
        .arg(page)
        .current_dir(t.join("work/task/wt/repo"))
        .output()
        .unwrap();
    let lines = String::from_utf8(out.stdout).unwrap();
    (
        out.status.code().unwrap(),
        lines.lines().map(str::to_owned).collect(),
    )
}

#[test]
fn p1_a_unit_call_without_acceptance_evaluates_the_blocks_checks_in_process() {
    let mut u = U::new();
    let checks = json!({
        "unchanged": ["README.md"],
        "adds": [{"path": "src/a.rs", "words": ["fn a"]}],
        "holds": [{"path": "src/a.rs", "exact": "fn a() {}\n"}],
        "lines": [{"path": "src/**", "max": 1, "min": 1}]
    });
    block_page(&u, &block(checks));
    let page = std::fs::read(u.t.join("work/task/TASK.md")).unwrap();
    let (v, code) = u.run(&without_acceptance(&u)).unwrap();
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["state"], "merged");
    assert_eq!(v["acceptance"], Value::Null);
    assert_eq!(v["write_set"], json!(["src/**", "notes.md"]));
    assert_eq!(v["page"]["sha256"], sha(&page));
    assert_eq!(
        steps(&v).iter().find(|(n, _)| n == "acceptance"),
        Some(&("acceptance".to_owned(), "passed".to_owned()))
    );
    assert!(u.accept_calls().is_empty(), "no executable ran");
    let log = v["files"]["acceptance_output"].as_str().unwrap();
    assert_eq!(std::fs::read_to_string(log).unwrap(), "accept: ok\n");
    assert_eq!(log, u.p("work/task/u1.accept.log"));
}

#[test]
fn p2_a_failed_check_stops_the_step_and_ap_accept_fails_the_same_checks() {
    let mut u = U::new();
    let checks = json!({
        "unchanged": ["src/a.rs"],
        "adds": [{"path": "src/a.rs", "words": ["fn b"]}],
        "holds": [{"path": "src/a.rs", "exact": "fn a() {}"}],
        "lines": [{"path": "src/**", "max": 0}]
    });
    block_page(&u, &block(checks));
    let v = u.stop(&without_acceptance(&u), "acceptance", "acceptance_failed");
    assert_eq!(v["detail"]["count"], 4);
    let failed: Vec<String> = v["detail"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(failed.len(), 4);
    let kinds: Vec<_> = failed
        .iter()
        .map(|l| l.split(':').next().unwrap())
        .collect();
    assert_eq!(kinds, ["unchanged", "adds", "holds", "lines"]);
    // The log holds the lines of `ap accept`, and `ap accept` in the worktree prints the same.
    let log = std::fs::read_to_string(v["files"]["acceptance_output"].as_str().unwrap()).unwrap();
    let want: Vec<String> = failed
        .iter()
        .map(|l| format!("accept: FAILED: {l}"))
        .collect();
    assert_eq!(log.lines().map(str::to_owned).collect::<Vec<_>>(), want);
    assert_eq!(accept_in(&u.t), (1, want));
    assert_eq!(u.launches, 1, "no verifier after a failed check");
    assert!(u.accept_calls().is_empty());
}

#[test]
fn p2_the_detail_names_at_most_ten_checks_and_the_count() {
    let mut u = U::new();
    let words: Vec<String> = (0..12).map(|i| format!("absent{i}")).collect();
    let checks = json!({"adds": [{"path": "src/a.rs", "words": words}]});
    block_page(&u, &block(checks));
    let v = u.stop(&without_acceptance(&u), "acceptance", "acceptance_failed");
    assert_eq!(v["detail"]["count"], 12);
    assert_eq!(v["detail"]["checks"].as_array().unwrap().len(), 10);
    let log = std::fs::read_to_string(u.p("work/task/u1.accept.log")).unwrap();
    assert_eq!(log.lines().count(), 12);
}

#[test]
fn p3_the_step_keeps_its_conditions_and_fetches_before_the_checks() {
    // The clean tree and the gated head, with today's causes, and no check ran.
    let mut u = U::new();
    block_page(&u, &block(json!({})));
    u.script("a1", &format!("{IMPLEMENT}; echo x > stray.txt"));
    u.stop(&without_acceptance(&u), "acceptance", "worktree_dirty");
    assert!(!u.t.join("work/task/u1.accept.log").exists());
    let mut u = U::new();
    block_page(&u, &block(json!({})));
    let local = "git -c user.name=t -c user.email=t@e commit -q --allow-empty -m local";
    u.script("a1", &format!("{IMPLEMENT}; {local}"));
    u.stop(&without_acceptance(&u), "acceptance", "head_differs");
    // `git fetch origin main` in the clone.
    let mut u = U::new();
    block_page(&u, &block(json!({})));
    let gone = format!(
        "git -C {t}/clone remote set-url origin {t}/gone.git",
        t = u.t.display()
    );
    u.script("a1", &format!("{IMPLEMENT}; {gone}"));
    u.stop(&without_acceptance(&u), "acceptance", "fetch_failed");
    assert!(!u.t.join("work/task/u1.accept.log").exists());
}

#[test]
fn p4_the_block_is_the_one_of_the_bytes_read_at_admission() {
    let mut u = U::new();
    block_page(
        &u,
        &block(json!({"adds": [{"path": "src/a.rs", "words": ["absent"]}]})),
    );
    let before = sha(&std::fs::read(u.t.join("work/task/TASK.md")).unwrap());
    // The implementer replaces the page by one whose checks hold; the call judges the first.
    let page = u.p("work/task/TASK.md");
    let ok =
        "# T\\n```unit\\n{\\\"unit\\\":\\\"U-1\\\",\\\"write_set\\\":[\\\"src/**\\\"]}\\n```\\n";
    u.script("a1", &format!("{IMPLEMENT}; printf '{ok}' > {page}"));
    let v = u.stop(&without_acceptance(&u), "acceptance", "acceptance_failed");
    assert_eq!(v["page"]["sha256"], before);
    assert_eq!(v["detail"]["count"], 1);
    assert_ne!(sha(&std::fs::read(&page).unwrap()), before);
}

#[test]
fn p5_a_page_without_a_block_needs_acceptance_and_the_page_refusals_come_first() {
    let mut u = U::new();
    let r = without_acceptance(&u);
    assert_eq!(code(u.run(&r)), "invalid_input@acceptance");
    std::fs::write(u.t.join("work/task/none.md"), "# no write set\n").unwrap();
    let mut other = r.clone();
    other["page"] = json!(u.p("work/task/none.md"));
    assert_eq!(code(u.run(&other)), "write_set_missing@page");
    other["page"] = json!(u.p("work/task/missing.md"));
    assert_eq!(code(u.run(&other)), "page_unreadable@page");
    block_page(&u, &json!({"unit": "U-1"}));
    assert_eq!(code(u.run(&r)), "page_block_invalid@page");
    // Nothing exists afterwards.
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md", "none.md"]);
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn p6_the_blocks_refusals_are_those_of_ap_accept_before_any_effect() {
    let mut u = U::new();
    let ok = json!({"unit": "U-1", "write_set": ["src/**"]});
    let two = format!("```unit\n{ok}\n```\n```unit\n{ok}\n```\n");
    let unclosed = format!("```unit\n{ok}\n");
    let mut unknown = ok.clone();
    unknown["bogus"] = json!(1);
    let mut wrong = ok.clone();
    wrong["merge"] = json!("yes");
    let cases = [
        (two, "two_blocks", json!(null)),
        (unclosed, "unclosed", json!(null)),
        (
            "```unit\n[]\n```\n".to_owned(),
            "not_an_object",
            json!(null),
        ),
        (
            format!("```unit\n{unknown}\n```\n"),
            "unknown_member",
            json!("bogus"),
        ),
        (format!("```unit\n{wrong}\n```\n"), "shape", json!("merge")),
    ];
    for r in [u.request(), without_acceptance(&u)] {
        for (text, reason, member) in &cases {
            std::fs::write(u.t.join("work/task/TASK.md"), text).unwrap();
            let Err(f) = u.run(&r) else {
                panic!("{reason}")
            };
            assert_eq!((f.code, f.field.as_str()), ("page_block_invalid", "page"));
            assert_eq!(f.detail, json!({"reason": reason, "member": member}));
        }
        // A block and a line `Write set:` in one page.
        let both = format!("Write set: src/**\n```unit\n{ok}\n```\n");
        std::fs::write(u.t.join("work/task/TASK.md"), both).unwrap();
        assert_eq!(code(u.run(&r)), "write_set_twice@page");
    }
    assert_eq!(names(&u.t.join("work/task")), ["TASK.md"]);
    assert_eq!(branches(&u), "* main");
    assert_eq!((u.launches, u.calls.len()), (0, 0));
}

#[test]
fn p7_a_request_with_acceptance_runs_the_executable_and_ignores_the_blocks_checks() {
    let mut u = U::new();
    // The checks cannot hold; the executable decides, and the block's write set is the page's.
    let checks = json!({"adds": [{"path": "src/a.rs", "words": ["absent"]}]});
    block_page(&u, &block(checks));
    let (v, code) = u.run(&u.request()).unwrap();
    assert_eq!(code, 0, "{v}");
    assert_eq!(u.accept_calls().len(), 1);
    assert_eq!(v["acceptance"]["path"], u.p("judge/accept.sh"));
    let log = std::fs::read_to_string(u.p("work/task/u1.accept.log")).unwrap();
    assert_eq!(log, "acceptance ran\n");

    let mut u = U::new();
    block_page(&u, &json!({"unit": "U-1", "write_set": ["notes.md"]}));
    let v = u.stop(&u.request(), "write_set", "path_outside_write_set");
    assert_eq!(v["write_set"], json!(["notes.md"]));
    assert_eq!(v["detail"]["paths"], json!(["src/a.rs"]));
}

#[test]
fn p8_without_acceptance_the_write_set_is_judged_by_its_own_step() {
    let mut u = U::new();
    block_page(&u, &json!({"unit": "U-1", "write_set": ["src/**"]}));
    let other = IMPLEMENT.replace("git add -A", "echo x > other.txt && git add -A");
    u.script("a1", &other);
    let v = u.stop(
        &without_acceptance(&u),
        "write_set",
        "path_outside_write_set",
    );
    assert_eq!(v["detail"], json!({"paths": ["other.txt"], "count": 1}));
    let acceptance = steps(&v).into_iter().find(|(n, _)| n == "acceptance");
    assert_eq!(
        acceptance,
        Some(("acceptance".to_owned(), "passed".to_owned()))
    );
    let log = std::fs::read_to_string(u.p("work/task/u1.accept.log")).unwrap();
    assert_eq!(log, "accept: ok\n");
}

// ---- RF1 a resumed stage of the round goes on at the next account when a usage limit cuts it ------

#[test]
fn rf1_a_fixer_cut_on_the_implementers_account_goes_on_at_the_next_account_of_its_block() {
    let mut u = U::new();
    std::fs::create_dir_all(u.t.join("a3")).unwrap();
    std::fs::write(u.t.join("bin/mode-a3"), "script").unwrap();
    // The implementer's account is at its limit once it has implemented: the resume is cut.
    let limit = format!("printf limit45 > {}", u.p("bin/mode-a1"));
    u.script(
        "a1",
        &format!("if [ -f src/a.rs ]; then false; else {IMPLEMENT}; {limit}; fi"),
    );
    u.script("a3", FIX);
    u.script("a2", VERIFY_AFTER_FIX);
    let mut r = r4_request(&u);
    r["implementer"]["next_accounts"] = json!([u.p("a3")]);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    // Implementer, verifier, the fixer's two launches, the come-back's verifier.
    assert_eq!(u.launches, 5);
    let first = u.record("u1");
    assert_eq!(first["repair"]["state"], "ok");
    let fixer = run_record(&first["repair"]["fixer"]);
    let implementer = run_record(&first["stages"]["implementer"]);
    // Runtime, model and effort are the implementer's; the accounts are the block's.
    assert_eq!(fixer["resumes"], first["stages"]["implementer"]);
    assert_eq!(implementer["account"], u.p("a1"));
    let attempts = fixer["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        (
            &attempts[0]["account"],
            &attempts[0]["state"],
            &attempts[0]["session"]
        ),
        (&json!(u.p("a1")), &json!("cut"), &implementer["session"])
    );
    assert_eq!(fixer["account"], u.p("a3"));
    assert_eq!(fixer["state"], "ok");
    assert_ne!(fixer["session"], implementer["session"]);
    for k in ["runtime", "model", "effort", "cwd"] {
        assert_eq!(fixer[k], implementer[k], "{k}");
    }
    // The new session's input: the text with the implementer's two paths, then the fixer's prompt.
    let input = std::fs::read(u.t.join("bin/prompt.4")).unwrap();
    let input = String::from_utf8(input).unwrap();
    let (p, a) = (
        &implementer["prompt"]["path"],
        &implementer["answer"]["path"],
    );
    let want = format!(
        "the file {}, and the answer it gave, the file {}.",
        p.as_str().unwrap(),
        a.as_str().unwrap()
    );
    let (head, prompt) = input.split_once("\n\n").unwrap();
    assert!(head.starts_with("This stage was to resume an earlier session, and a usage limit cut that session on its account."), "{head}");
    assert!(
        head.contains(&want) && head.contains("do not start over"),
        "{head}"
    );
    assert_eq!(
        prompt.as_bytes(),
        std::fs::read(u.t.join("work/task/u1.fixer.prompt.md")).unwrap()
    );
    assert_eq!(u.merges().len(), 1);
}

#[test]
fn rf1_a_come_back_whose_resumed_verifier_is_cut_takes_its_verdict_from_the_new_session() {
    let mut u = U::new();
    std::fs::create_dir_all(u.t.join("a3")).unwrap();
    std::fs::write(u.t.join("bin/mode-a3"), "script").unwrap();
    fixing(&u, FIX);
    // The first look makes the verifier's account cut from then on.
    let limit = format!("printf limit45 > {}", u.p("bin/mode-a2"));
    u.script("a2", &format!("{limit}; {VERIFY_AFTER_FIX}"));
    u.script("a3", VERIFY_AFTER_FIX);
    let mut r = r4_request(&u);
    r["verifier"]["next_accounts"] = json!([u.p("a3")]);
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    // Implementer, verifier, fixer, the second look's two launches.
    assert_eq!(u.launches, 5);
    let second = run_record(&v["stages"]["verifier"]);
    let attempts = second["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["account"], u.p("a2"));
    assert_eq!(attempts[0]["state"], "cut");
    assert_eq!(
        (&second["account"], &second["state"]),
        (&json!(u.p("a3")), &json!("ok"))
    );
    assert_ne!(second["session"], attempts[0]["session"]);
    assert_eq!(u.merges().len(), 1);
}
