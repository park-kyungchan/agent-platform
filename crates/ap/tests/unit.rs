//! `ap unit` against fakes: a real Git origin (a bare repository in a temporary directory), a clone
//! and the worktree the command creates; the fake `claude` of tests/fixtures/fake-claude.sh in mode
//! `script` (its account's script runs in the stage's cwd and its stdout is the answer); a fake
//! GitHub that reads the head from the origin and merges into it; a fake store; `systemctl` and the
//! unit launcher replaced as in tests/run.rs; a virtual clock for the gate. The work root is the
//! temporary directory's `work/` through `System::work_root`. The fake answers `gh` with JSON that
//! holds every member the command may ask for (no `--jq`).
use ap::unit::execute;
use ap::work::{Answer, Failure, Host, Reply, System};
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
    /// Comments of the pull request (the automatic reviewer's marker).
    comments: Vec<Value>,
    /// The read after the merge still says OPEN.
    merge_unconfirmed: bool,
    /// The merge leaves an untracked file in the worktree, so its removal without force fails.
    dirty_on_merge: bool,
    /// Someone pushes another commit to the branch right after the acceptance executable ran.
    move_head_after_acceptance: bool,
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
            comments: vec![],
            merge_unconfirmed: false,
            dirty_on_merge: false,
            move_head_after_acceptance: false,
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
            Ok(Reply {
                success: true,
                bytes: vec![],
                stdout: serde_json::to_vec(&v).unwrap(),
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
                ok(pr(self))
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
                ok(json!({}))
            }
            ("pr", "ready") => ok(json!({})),
            ("api", "graphql") => {
                let query = a.last().unwrap();
                if query.contains("reviewThreads") {
                    return ok(
                        json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
                        "pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":self.threads}}}}}),
                    );
                }
                let check = json!({"__typename":"CheckRun","name":"rust","status":"COMPLETED",
                    "conclusion":self.check,"isRequired":true,
                    "checkSuite":{"workflowRun":{"url":"https://github.com/o/r/actions/runs/1"}}});
                ok(
                    json!({"data":{"repository":{"pullRequest":{"state":self.pr_state,"isDraft":false,
                    "headRefOid":head,"commits":{"nodes":[{"commit":{"oid":head,
                    "statusCheckRollup":{"contexts":{"nodes":[check]}}}}]},
                    "comments":{"nodes":self.comments}}}}}),
                )
            }
            ("api", _) => {
                assert!(a.iter().any(|x| x == "context=lead-review"), "{a:?}");
                ok(json!({}))
            }
            _ => panic!("unexpected gh call {a:?}"),
        }
    }
    fn ont(&mut self, a: &[String], body: &Value) -> Answer {
        assert_eq!(&a[..4], ["action", "apply", "--client", "lead"], "{a:?}");
        let review = json!({"object_type":"review","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"rv-1"});
        let edits = if body["action_type"] == "start_review" {
            json!([{"change":"initial","id":review}])
        } else {
            json!([{"change":"update","id":review,"revision":"c#0"},{"change":"update","id":review,"revision":"c#1"}])
        };
        Ok(Reply {
            success: true,
            bytes: vec![],
            stdout: serde_json::to_vec(
                &json!({"status":"ok","result":{"outcome":"applied","edits":edits}}),
            )
            .unwrap(),
        })
    }
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
                    let origin = self.t.join("origin.git");
                    let head = self.head().unwrap();
                    let tree = git(&origin, &["rev-parse", &format!("{head}^{{tree}}")]);
                    let moved = git(&origin, &["commit-tree", &tree, "-p", &head, "-m", "moved"]);
                    git(
                        &origin,
                        &["update-ref", &format!("refs/heads/{BRANCH}"), &moved],
                    );
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
        assert_eq!(argv[at + 1], "claude");
        let unit = argv[..at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        let child = Command::new(self.t.join("bin/claude"))
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
        ("mechanical", json!(true), "invalid_input@verifier"),
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
fn i2_a_come_back_call_starts_no_stage_and_continues_no_record() {
    let mut u = U::new();
    u.script(
        "a2",
        "printf 'Verdict: repair\\n%s\\nL1 blocks' \"$(git rev-parse HEAD)\"",
    );
    u.stop(&u.request(), "verifier", "verdict_repair");
    let first = std::fs::read(u.t.join("work/task/u1.unit.json")).unwrap();
    // The Lead repairs by hand, pushes, and writes its own verdict on the new head.
    let wt = u.wt();
    std::fs::write(wt.join("notes.md"), "fixed\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "fix"]);
    git(
        &wt,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{BRANCH}")],
    );
    let head = u.head().unwrap();
    std::fs::write(
        u.t.join("work/task/lead-verdict.md"),
        format!("Verdict: accept\n{head}\n"),
    )
    .unwrap();
    let launches = u.launches;
    let mut r = u.request();
    r["out"] = json!(u.p("work/task/u2"));
    r["pull_request"] = json!(7);
    r["verdict"] = json!(u.p("work/task/lead-verdict.md"));
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(u.launches, launches, "a come-back starts no stage");
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
    assert_eq!(steps(&v)[6], ("verifier".to_owned(), "skipped".to_owned()));
    assert_eq!(
        std::fs::read(u.t.join("work/task/u1.unit.json")).unwrap(),
        first,
        "the earlier record is untouched"
    );
    assert_eq!(
        u.store("close_review")[0]["parameters"]["report"],
        u.p("work/task/lead-verdict.md")
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

#[test]
fn i4_a_mechanical_unit_is_judged_by_its_acceptance_without_a_verifier() {
    let mut u = U::new();
    let mut r = u.request();
    r["mechanical"] = json!(true);
    r.as_object_mut().unwrap().remove("verifier");
    let (v, code) = u.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("merged"), 0), "{v}");
    assert_eq!(u.launches, 1);
    assert_eq!(steps(&v)[6], ("verifier".to_owned(), "skipped".to_owned()));
    let report = std::fs::read_to_string(v["files"]["report"].as_str().unwrap()).unwrap();
    let mut lines = report.lines();
    assert_eq!(lines.next(), Some("Verdict: accept"));
    assert_eq!(lines.next(), v["head"].as_str());
    let close = &u.store("close_review")[0];
    assert_eq!(close["parameters"]["verdict"], "accept");
    assert_eq!(close["parameters"]["report_sha256"], sha(report.as_bytes()));
    assert_eq!(u.statuses().len(), 1);
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
        "mechanical",
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
