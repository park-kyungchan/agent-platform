//! `ap unit` on the fakes: the stages are `fixtures/fake-claude.sh` in its `script` mode (line n of
//! `bin/script` is the n-th stage's word), GitHub is a fake `gh` whose head is the test repository's
//! real `HEAD`, and pauses of a second or more move a virtual clock (the gate's readings, the sleep
//! to a reset). The chain runs in this process; `ci/host-run-scope.sh` runs it in its real unit.
use ap::unit::{chain, start, wait};
use ap::work::{Answer, Failure, Host, Reply, System};
use serde_json::{Value, json};
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

struct Fake {
    t: PathBuf,
    offset: Duration,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    /// `systemctl is-active` of the chain's unit answers `active`.
    chain_alive: bool,
    started: Vec<Vec<String>>,
}
impl Fake {
    fn new(script: &[&str]) -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-unit-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "repo/tests", "out"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        std::fs::write(t.join("bin/claude"), FAKE).unwrap();
        std::fs::set_permissions(t.join("bin/claude"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        for a in ["a1", "a2"] {
            std::fs::write(t.join(format!("bin/mode-{a}")), "script").unwrap();
        }
        std::fs::write(t.join("impl.md"), "Build the thing.\n").unwrap();
        std::fs::write(t.join("review.md"), "Review the thing.\n").unwrap();
        std::fs::write(t.join("repo/tests/x_test.rs"), "#[test]\nfn x() {}\n").unwrap();
        let f = Self {
            t,
            offset: Duration::ZERO,
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
            chain_alive: false,
            started: vec![],
        };
        for a in [
            &["init", "-q", "-b", "ap/x"][..],
            &["config", "user.email", "t@example.invalid"],
            &["config", "user.name", "t"],
            &["add", "-A"],
            &["commit", "-q", "-m", "base"],
        ] {
            f.git(a);
        }
        f.script(script);
        f
    }
    fn git(&self, a: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(self.t.join("repo"))
            .args(a)
            .output()
            .unwrap();
        assert!(o.status.success(), "git {a:?}");
        String::from_utf8(o.stdout).unwrap().trim().to_owned()
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    fn flag(&self, name: &str) -> bool {
        self.t.join("bin").join(name).exists()
    }
    fn set(&self, name: &str, on: bool) {
        let p = self.t.join("bin").join(name);
        if on {
            std::fs::write(p, "1").unwrap();
        } else {
            let _ = std::fs::remove_file(p);
        }
    }
    /// The words of the next stages, after the ones already run.
    fn script(&self, words: &[&str]) {
        let done: usize = std::fs::read_to_string(self.t.join("bin/count"))
            .map_or(0, |c| c.trim().parse().unwrap());
        let mut lines: Vec<String> = std::fs::read_to_string(self.t.join("bin/script"))
            .unwrap_or_default()
            .lines()
            .take(done)
            .map(str::to_owned)
            .collect();
        lines.resize(done, "fail".into());
        lines.extend(words.iter().map(|w| (*w).to_owned()));
        std::fs::write(self.t.join("bin/script"), lines.join("\n") + "\n").unwrap();
    }
    fn head(&self) -> String {
        std::fs::read_to_string(self.t.join("bin/moved"))
            .unwrap_or_else(|_| self.git(&["rev-parse", "HEAD"]))
    }
    fn request(&self) -> Value {
        json!({"repository": "o/r", "cwd": self.p("repo"), "out": self.p("out"),
            "implementer": {"account": self.p("a1"), "model": "haiku", "effort": "low", "prompt": self.p("impl.md")},
            "reviewer": {"account": self.p("a2"), "model": "sonnet", "effort": "low", "prompt": self.p("review.md")}})
    }
    /// `ap unit` then the chain it starts, here in this process.
    fn run(&mut self, r: &Value) -> Value {
        let (answer, code) = start(self, r, "/usr/local/bin/ap").unwrap();
        assert_eq!(code, 0, "{answer}");
        let unit = answer["unit"].as_str().unwrap().to_owned();
        self.chain_alive = true;
        let rec = chain(self, &self.p("out"), &unit);
        self.chain_alive = false;
        assert_eq!(
            rec,
            serde_json::from_slice::<Value>(&std::fs::read(self.p("out/unit.json")).unwrap())
                .unwrap()
        );
        rec
    }
    fn fresh(&mut self) -> Value {
        let r = self.request();
        self.run(&r)
    }
    /// A continuation of the record in `out`, with the Lead's findings when `findings`.
    fn again(&mut self, words: &[&str], findings: bool) -> Value {
        self.script(words);
        let mut r = self.request();
        r["continue"] = json!(self.p("out/unit.json"));
        if findings {
            let f = json!([{"id": "L1", "kind": 1, "path": "work.txt", "line": 1, "claim": "the Lead's", "shown_by": "git diff"}]);
            std::fs::write(self.t.join("findings.json"), f.to_string()).unwrap();
            r["findings"] = json!(self.p("findings.json"));
        }
        self.run(&r)
    }
    fn view(&self) -> Value {
        let head = self.head();
        let state = if self.flag("merged") {
            "MERGED"
        } else {
            "OPEN"
        };
        let red = std::fs::read_to_string(self.t.join("bin/red")).is_ok_and(|r| r == head);
        let (status, conclusion) = match (self.flag("pending"), red) {
            (true, _) => ("IN_PROGRESS", Value::Null),
            (_, true) => ("COMPLETED", json!("FAILURE")),
            _ => ("COMPLETED", json!("SUCCESS")),
        };
        let marker = format!(
            "codex-review-state:v1 {}",
            json!({"code_review": {"commit_sha": head, "status": "completed", "has_unresolved_findings": self.flag("threads")}})
        );
        json!({"data": {"repository": {"pullRequest": {"state": state, "isDraft": false, "headRefOid": head,
            "commits": {"nodes": [{"commit": {"oid": head, "statusCheckRollup": {"contexts": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                "nodes": [{"__typename": "CheckRun", "name": "rust", "status": status, "conclusion": conclusion, "isRequired": true,
                    "checkSuite": {"workflowRun": {"url": "https://example.invalid/run/1"}}}]}}}}]},
            "comments": {"nodes": [{"author": {"__typename": "Bot", "login": "chatgpt-codex-connector"}, "body": marker}]}}}}})
    }
    fn github(&self, a: &[String]) -> Value {
        let query = a.last().unwrap();
        let head = self.head();
        let state = if self.flag("merged") {
            "MERGED"
        } else {
            "OPEN"
        };
        if query.contains("reviewThreads") {
            let last = if self.flag("answered") {
                json!({"__typename": "User", "login": "palantirkc"})
            } else {
                json!({"__typename": "Bot", "login": "chatgpt-codex-connector"})
            };
            let nodes = if self.flag("threads") {
                json!([{"id": "T1", "isResolved": false, "path": "work.txt", "line": 1,
                    "first": {"nodes": [{"author": {"__typename": "Bot", "login": "chatgpt-codex-connector"}}]}, "last": {"nodes": [{"author": last}]}}])
            } else {
                json!([])
            };
            return json!({"data": {"repository": {"pullRequest": {"reviewThreads": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes}}}}});
        }
        if query.contains("statusCheckRollup") {
            return self.view();
        }
        let pr = json!({"number": 7, "state": state, "headRefOid": head, "baseRefName": "main", "headRepository": {"nameWithOwner": "o/r"}});
        if query.contains("pullRequests(") {
            assert!(a.contains(&"branch=ap/x".to_owned()));
            let nodes = if self.flag("nopr") {
                json!([])
            } else {
                json!([pr])
            };
            return json!({"data": {"repository": {"pullRequests": {"nodes": nodes}}}});
        }
        json!({"data": {"repository": {"pullRequest": pr}}})
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        for g in self.groups.values() {
            kill_group(*g);
        }
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(std::process::Stdio::null())
        .status();
}
fn reply(success: bool, stdout: Vec<u8>) -> Answer {
    Ok(Reply {
        success,
        bytes: stdout.clone(),
        stdout,
    })
}
impl System for Fake {
    fn call(&mut self, program: &str, a: &[String], input: &Value, ms: u64) -> Answer {
        match program {
            "gh" if a[0] == "--version" => reply(true, b"gh version 2\n".to_vec()),
            "gh" if a[0] == "pr" => reply(true, vec![]),
            "gh" if self.flag("unreadable") => Err(Failure {
                code: "process_failed",
                field: "gh".into(),
                detail: Value::Null,
            }),
            "gh" => reply(true, serde_json::to_vec(&self.github(a)).unwrap()),
            "systemd-run" => {
                self.started.push(a.to_vec());
                reply(true, vec![])
            }
            "systemctl" => {
                let unit = a.last().unwrap().clone();
                match a[0].as_str() {
                    "stop" => {
                        if let Some(g) = self.groups.get(&unit) {
                            kill_group(*g);
                        }
                        self.alive.remove(&unit);
                        reply(true, vec![])
                    }
                    "is-active" if self.alive.contains(&unit) => reply(true, b"active\n".to_vec()),
                    "is-active" if unit.starts_with("ap-unit-") && self.chain_alive => {
                        reply(true, b"active\n".to_vec())
                    }
                    _ => reply(false, b"inactive\n".to_vec()),
                }
            }
            _ => Host.call(program, a, input, ms),
        }
    }
    fn now(&self) -> Instant {
        Instant::now() + self.offset
    }
    fn pause(&mut self, d: Duration) {
        if d >= Duration::from_secs(1) {
            self.offset += d;
        } else {
            std::thread::sleep(d);
        }
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        let at = argv.iter().position(|a| a == "--").unwrap();
        assert!(
            argv[..at].iter().any(|a| a.starts_with("BindsTo=ap-unit-")),
            "{argv:?}"
        );
        let unit = argv[..at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        let [i, o, e] = files;
        let child = Command::new(self.t.join("bin/claude"))
            .args(&argv[at + 2..])
            .env_clear()
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(i)
            .stdout(o)
            .stderr(e)
            .process_group(0)
            .spawn()?;
        self.groups.insert(unit.clone(), child.id());
        self.alive.insert(unit);
        Ok(child)
    }
    fn units(&self) -> bool {
        true
    }
}

/// The roles of the stages in order, and the end.
fn shape(rec: &Value) -> (Vec<String>, String) {
    let roles = rec["stages"].as_array().unwrap().iter().map(|s| {
        format!(
            "{}:{}",
            s["role"].as_str().unwrap(),
            s["state"].as_str().unwrap()
        )
    });
    (roles.collect(), rec["state"].as_str().unwrap().to_owned())
}
fn roles(list: &[&str]) -> Vec<String> {
    list.iter()
        .map(|r| {
            if r.contains(':') {
                (*r).to_owned()
            } else {
                format!("{r}:ok")
            }
        })
        .collect()
}
fn assert_shape(rec: &Value, list: &[&str], end: &str) {
    assert_eq!(shape(rec), (roles(list), end.to_owned()), "{rec:#}");
}

#[test]
fn accept_at_once() {
    let mut f = Fake::new(&["impl", "accept"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "reviewer"], "accepted");
    let head = f.head();
    assert_eq!(rec["exit"], 0);
    assert_eq!(rec["pull_request"], 7);
    assert_eq!(rec["base"], "main");
    assert_eq!(rec["heads"], json!([head]));
    assert_eq!(rec["gates"][0]["head"], head.as_str());
    assert_eq!(rec["gates"][0]["answer"]["end"], "passed");
    assert_eq!(
        rec["verdicts"],
        json!([{"stage": 2, "head": head, "verdict": "accept", "blocking": 0, "notes": 0}])
    );
    assert_eq!(rec["autoreview"], "completed");
    assert_eq!(rec["rounds_used"], 0);
    assert_eq!(
        rec["left"],
        json!({"notes": [], "threads": [], "refused": []})
    );
    assert_eq!(
        rec["tokens"],
        json!({"input": 20, "output": 10, "cache_read": 200, "cache_creation": 2})
    );
    assert_eq!(rec["stages"][0]["tokens"]["input"], 10);
    assert!(rec["finish"].is_string() && rec["next"].is_null() && rec["step"].is_null());
    // The record holds no prompt text and no finding text; the prompts are the Lead's with facts.
    let text = std::fs::read_to_string(f.p("out/unit.json")).unwrap();
    assert!(!text.contains("Build the thing") && !text.contains("Review the thing"));
    let reviewer = std::fs::read_to_string(f.p("out/2-reviewer.prompt.md")).unwrap();
    assert!(reviewer.starts_with("Review the thing.\n\n---\nFacts of this review"));
    assert!(reviewer.contains(&format!("pull request 7, head {head}, base main")));
    let implementer = std::fs::read_to_string(f.p("out/1-implementer.prompt.md")).unwrap();
    assert!(implementer.starts_with("Build the thing.\n") && implementer.contains("branch ap/x"));
    // The reviewer's stage carried the schema; the answer file is the object.
    let run: Value =
        serde_json::from_slice(&std::fs::read(f.p("out/2-reviewer.md")).unwrap()).unwrap();
    assert_eq!(run["verdict"], "accept");
    // The chain's unit: its bound, its finisher, under agents.slice; the worktree names it.
    let s = &f.started[0];
    assert!(
        s.contains(&"--slice=agents.slice".to_owned())
            && s.contains(&"RuntimeMaxSec=29100s".to_owned())
    );
    assert!(
        s.iter()
            .any(|a| a == &format!("ExecStopPost=/usr/local/bin/ap unit finish {}", f.p("out")))
    );
    assert_eq!(
        &s[s.len() - 5..],
        [
            "/usr/local/bin/ap",
            "unit",
            "chain",
            &f.p("out"),
            rec["unit"].as_str().unwrap()
        ]
    );
}

#[test]
fn notes_only_take_one_free_round_then_are_listed() {
    let mut f = Fake::new(&["impl", "accept-notes", "fix", "accept-notes"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer", "fixer", "judge"],
        "accepted",
    );
    assert_eq!(rec["rounds_used"], 0);
    assert_eq!(rec["notes_round_used"], true);
    assert_eq!(rec["left"]["notes"], json!([{"stage": 4, "id": "N1"}]));
    assert_eq!(rec["dispositions"].as_array().unwrap().len(), 1);
    assert_eq!(rec["gates"].as_array().unwrap().len(), 2);
    // The judgment names the head of the second gate; the fixer resumed the implementer's session.
    assert_eq!(rec["verdicts"][1]["head"], rec["gates"][1]["head"]);
    let fixer: Value =
        serde_json::from_slice(&std::fs::read(f.p("out/3-fixer.md.run.json")).unwrap()).unwrap();
    assert_eq!(fixer["resumes"], f.p("out/1-implementer.md.run.json"));
    let judge: Value =
        serde_json::from_slice(&std::fs::read(f.p("out/4-judge.md.run.json")).unwrap()).unwrap();
    assert_eq!(judge["resumes"], f.p("out/2-reviewer.md.run.json"));
    assert_eq!(judge["account"], f.p("a2"));
}

#[test]
fn unanswered_threads_go_to_the_fixer_and_answered_ones_are_not_owed() {
    let mut f = Fake::new(&["impl", "accept", "fix", "accept"]);
    f.set("threads", true);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer", "fixer", "judge"],
        "accepted",
    );
    assert_eq!(rec["dispositions"][0]["id"], "T1");
    assert_eq!(rec["dispositions"][0]["kind"], "thread");
    // The fixer answered it: the last gate reports it answered, so nothing is left.
    assert_eq!(rec["left"]["threads"], json!([]));
}

#[test]
fn blocking_then_fix_then_accept() {
    let mut f = Fake::new(&["impl", "repair-notes", "fix", "accept"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer", "fixer", "judge"],
        "accepted",
    );
    assert_eq!(rec["rounds_used"], 1);
    assert_eq!(rec["verdicts"][0]["blocking"], 1);
    assert_eq!(rec["heads"].as_array().unwrap().len(), 2);
    let ids: Vec<&Value> = rec["dispositions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| &d["id"])
        .collect();
    assert_eq!(ids, [&json!("B1"), &json!("N1")]);
    // The judgment got the dispositions, the diff command and the list of test files.
    let judge = std::fs::read_to_string(f.p("out/4-judge.prompt.md")).unwrap();
    assert!(
        judge.contains("\"reason\":\"fixed\"")
            && judge.contains("Test files that diff changed or deleted: []")
    );
    assert!(judge.contains(&format!(
        "diff {}..{}",
        rec["heads"][0].as_str().unwrap(),
        rec["heads"][1].as_str().unwrap()
    )));
}

#[test]
fn blocking_twice_then_accept() {
    let mut f = Fake::new(&["impl", "repair", "fix-test", "repair", "fix", "accept"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &[
            "implementer",
            "reviewer",
            "fixer",
            "judge",
            "fixer",
            "judge",
        ],
        "accepted",
    );
    assert_eq!(rec["rounds_used"], 2);
    let judge = std::fs::read_to_string(f.p("out/4-judge.prompt.md")).unwrap();
    assert!(
        judge.contains("Test files that diff changed or deleted: [\"tests/x_test.rs\"]"),
        "{judge}"
    );
}

#[test]
fn a_blocking_finding_left_after_two_rounds() {
    let mut f = Fake::new(&["impl", "repair", "fix", "repair", "fix", "repair"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &[
            "implementer",
            "reviewer",
            "fixer",
            "judge",
            "fixer",
            "judge",
        ],
        "blocking_left",
    );
    assert_eq!(rec["exit"], 20);
    assert_eq!(rec["next"]["step"], "fix");
}

#[test]
fn a_refused_blocking_finding() {
    let mut f = Fake::new(&["impl", "repair-notes", "fix-refuse"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer", "fixer"],
        "blocking_refused",
    );
    assert_eq!(rec["exit"], 21);
    assert_eq!(rec["end_from"], json!({"stage": 3}));
    assert_eq!(rec["dispositions"][0]["disposition"], "refused");
}

#[test]
fn a_failed_check_goes_to_the_fixer_before_any_reviewer() {
    let mut f = Fake::new(&["impl-red", "fix", "accept"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "fixer", "reviewer"], "accepted");
    assert_eq!(rec["gates"][0]["answer"]["end"], "check_failed");
    assert_eq!(rec["dispositions"][0]["id"], "check");
    assert_eq!(rec["rounds_used"], 1);
    let items = std::fs::read_to_string(f.p("out/2-fixer.prompt.md")).unwrap();
    assert!(items.contains("\"name\":\"rust\"") && items.contains("https://example.invalid/run/1"));
}

#[test]
fn a_check_that_still_fails_after_the_rounds() {
    let mut f = Fake::new(&["impl-red", "fix-red", "fix-red"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "fixer", "fixer"], "check_failed");
    assert_eq!(rec["exit"], 22);
}

#[test]
fn a_verdict_for_another_head_is_no_verdict() {
    let mut f = Fake::new(&["impl", "other-head"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "reviewer:no_result"], "stage_failed");
    assert_eq!(
        rec["end_from"],
        json!({"stage": 2, "state": "no_result", "unmet": "verdict"})
    );
    assert_eq!(rec["verdicts"], json!([]));
}

#[test]
fn a_fixer_that_disposes_of_less_than_every_item() {
    let mut f = Fake::new(&["impl", "repair-notes", "fix-short"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer", "fixer:no_result"],
        "stage_failed",
    );
}

#[test]
fn a_stage_that_failed() {
    let mut f = Fake::new(&["fail"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer:failed"], "stage_failed");
    assert_eq!(rec["exit"], 26);
    assert_eq!(rec["end_from"], json!({"stage": 1, "state": "failed"}));
}

#[test]
fn a_cut_with_a_reset_inside_the_bound_sleeps_and_resumes_the_session() {
    let mut f = Fake::new(&["impl", "cut-in", "accept"]);
    let rec = f.fresh();
    assert_shape(
        &rec,
        &["implementer", "reviewer:cut", "reviewer"],
        "accepted",
    );
    let again: Value =
        serde_json::from_slice(&std::fs::read(f.p("out/3-reviewer.md.run.json")).unwrap()).unwrap();
    assert_eq!(again["resumes"], f.p("out/2-reviewer.md.run.json"));
    assert_eq!(again["account"], f.p("a2"));
    assert!(f.offset >= Duration::from_secs(300));
}

#[test]
fn a_cut_with_a_reset_outside_the_bound() {
    let mut f = Fake::new(&["impl", "cut-out"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "reviewer:cut"], "usage_limit");
    assert_eq!(rec["exit"], 27);
    assert!(rec["end_from"]["resets_at"].as_u64().unwrap() > 1_000_000_000);
}

#[test]
fn merged_during_a_stage() {
    let mut f = Fake::new(&["impl", "merge"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "reviewer"], "merged_or_closed");
    assert_eq!(rec["exit"], 24);
    assert!(rec["next"].is_null());
}

#[test]
fn a_head_moved_by_someone_elses_push() {
    let mut f = Fake::new(&["impl", "moved"]);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer", "reviewer"], "head_moved");
    assert_eq!(rec["end_from"]["head_now"], "9".repeat(40));
}

#[test]
fn no_pull_request_for_the_branch() {
    let mut f = Fake::new(&["impl"]);
    f.set("nopr", true);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer"], "no_pull_request");
    assert_eq!(rec["exit"], 23);
}

#[test]
fn github_unreadable() {
    let mut f = Fake::new(&["impl"]);
    f.set("unreadable", true);
    let rec = f.fresh();
    assert_shape(&rec, &["implementer"], "github_unreadable");
    assert_eq!(rec["end_from"], json!({"reading": "pull_request"}));
}

#[test]
fn the_chains_own_bound() {
    let mut f = Fake::new(&["impl"]);
    f.set("pending", true);
    let mut r = f.request();
    r["bound_minutes"] = json!(10);
    let rec = f.run(&r);
    assert_shape(&rec, &["implementer"], "bound");
    assert_eq!(rec["exit"], 28);
}

/// Runs `script` to its end `end`, then continues with `more` (and the Lead's findings when
/// asked) and checks the continuation's stages and end.
fn continued(
    script: &[&str],
    setup: &[&str],
    end: &str,
    more: &[&str],
    findings: bool,
    after: &[&str],
    last: &str,
) -> (Fake, Value) {
    let mut f = Fake::new(script);
    for s in setup {
        f.set(s, true);
    }
    let first = f.fresh();
    assert_eq!(first["state"], end, "{first:#}");
    for s in setup.iter().chain(&["moved"]) {
        f.set(s, false);
    }
    let n = first["stages"].as_array().unwrap().len();
    let rec = f.again(more, findings);
    assert_eq!(
        shape(&rec).0[..n],
        shape(&first).0[..],
        "the earlier stages stay"
    );
    assert_eq!(
        (shape(&rec).0[n..].to_vec(), shape(&rec).1),
        (roles(after), last.to_owned()),
        "{rec:#}"
    );
    assert_eq!(rec["continued"].as_array().unwrap().len(), 1);
    assert_eq!(rec["start"], first["start"]);
    if findings {
        assert!(
            rec["dispositions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["id"] == "L1" && d["kind"] == "lead")
        );
    }
    (f, rec)
}

#[test]
fn continuation_after_each_end_that_can_continue() {
    for findings in [false, true] {
        let fix: &[&str] = if findings { &["fix", "accept"] } else { &[] };
        let with = |rest: &[&'static str]| -> Vec<&'static str> { [fix, rest].concat() };
        // accepted: without findings the head is read, gated and judged again.
        let a: Vec<&str> = if findings { with(&[]) } else { vec!["accept"] };
        let r: Vec<&str> = if findings {
            vec!["fixer", "judge"]
        } else {
            vec!["judge"]
        };
        continued(
            &["impl", "accept"],
            &[],
            "accepted",
            &a,
            findings,
            &r,
            "accepted",
        );
        // blocking_left: the Lead's round uses none of the two; the rounds used are kept.
        let (_, rec) = continued(
            &["impl", "repair", "fix", "repair", "fix", "repair"],
            &[],
            "blocking_left",
            &["fix", "accept"],
            findings,
            &["fixer", "judge"],
            "accepted",
        );
        assert_eq!(rec["rounds_used"], 2);
        // blocking_refused: the judgment of the refusal, or the Lead's findings first.
        let after: Vec<&str> = if findings {
            vec!["fixer", "judge"]
        } else {
            vec!["judge"]
        };
        continued(
            &["impl", "repair", "fix-refuse"],
            &[],
            "blocking_refused",
            &with(&["accept"])[..if findings { 2 } else { 1 }],
            findings,
            &after,
            "accepted",
        );
        // check_failed after the rounds: the Lead's round.
        continued(
            &["impl-red", "fix-red", "fix-red"],
            &[],
            "check_failed",
            &["fix", "accept"],
            findings,
            &["fixer", "reviewer"],
            "accepted",
        );
        // no_pull_request, github_unreadable, bound: the reading is taken again; no stage repeats.
        for (setup, end) in [
            ("nopr", "no_pull_request"),
            ("unreadable", "github_unreadable"),
            ("pending", "bound"),
        ] {
            let script = with(&["accept"]);
            let after: Vec<&str> = if findings {
                vec!["fixer", "reviewer"]
            } else {
                vec!["reviewer"]
            };
            let mut f = Fake::new(&["impl"]);
            f.set(setup, true);
            let mut r = f.request();
            if setup == "pending" {
                r["bound_minutes"] = json!(10);
            }
            let first = f.run(&r);
            assert_eq!(first["state"], end);
            f.set(setup, false);
            let rec = f.again(&script[..if findings { 2 } else { 1 }], findings);
            assert_eq!(shape(&rec).0[1..], roles(&after)[..], "{end} {rec:#}");
            assert_eq!(rec["state"], "accepted");
        }
        // head_moved: the new head is gated and judged.
        let (_, rec) = continued(
            &["impl", "repair", "fix", "moved"],
            &[],
            "head_moved",
            &with(&["accept"])[..if findings { 2 } else { 1 }],
            findings,
            &if findings {
                vec!["fixer", "judge"]
            } else {
                vec!["judge"]
            },
            "accepted",
        );
        assert_eq!(
            rec["heads"].as_array().unwrap().last().unwrap(),
            &rec["verdicts"].as_array().unwrap().last().unwrap()["head"]
        );
        // stage_failed and usage_limit: the stage that is not ok is resumed; an ok one is not repeated.
        for (script, end) in [
            (&["impl", "fail"][..], "stage_failed"),
            (&["impl", "cut-out"][..], "usage_limit"),
        ] {
            let more: Vec<&str> = if findings {
                vec!["fix", "accept"]
            } else {
                vec!["accept"]
            };
            let after: Vec<&str> = if findings {
                vec!["fixer", "reviewer"]
            } else {
                vec!["reviewer"]
            };
            let (f, _) = continued(script, &[], end, &more, findings, &after, "accepted");
            if !findings {
                let again: Value = serde_json::from_slice(
                    &std::fs::read(f.p("out/3-reviewer.md.run.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(again["resumes"], f.p("out/2-reviewer.md.run.json"));
            }
        }
    }
}

#[test]
fn a_continued_record_says_running_from_its_first_step() {
    let mut f = Fake::new(&["impl", "accept"]);
    f.fresh();
    f.script(&["accept"]);
    let mut r = f.request();
    r["continue"] = json!(f.p("out/unit.json"));
    let (_, code) = start(&mut f, &r, "/usr/local/bin/ap").unwrap();
    assert_eq!(code, 0);
    let rec: Value = serde_json::from_slice(&std::fs::read(f.p("out/unit.json")).unwrap()).unwrap();
    assert_eq!(
        (
            rec["state"].as_str(),
            rec["exit"].as_u64(),
            rec["finish"].as_str()
        ),
        (Some("running"), None, None)
    );
}

#[test]
fn refusals() {
    let mut f = Fake::new(&["impl", "accept"]);
    let refuse = |f: &mut Fake, r: &Value| match start(f, r, "/ap") {
        Err(e) => format!("{}@{}", e.code, e.field),
        Ok(v) => panic!("{v:?}"),
    };
    let mut r = f.request();
    r["extra"] = json!(1);
    assert_eq!(refuse(&mut f, &r), "invalid_input@extra");
    let mut r = f.request();
    r["out"] = json!(f.p("repo/tests"));
    assert_eq!(refuse(&mut f, &r), "out_in_worktree@out");
    let mut r = f.request();
    r["reviewer"]["prompt"] = json!(f.p("missing.md"));
    assert_eq!(refuse(&mut f, &r), "prompt_unreadable@reviewer");
    let mut r = f.request();
    r["findings"] = json!(f.p("impl.md"));
    assert_eq!(refuse(&mut f, &r), "invalid_input@findings");
    let mut r = f.request();
    r["continue"] = json!(f.p("out/unit.json"));
    assert_eq!(refuse(&mut f, &r), "record_missing@continue");
    f.git(&["checkout", "-q", "--detach"]);
    let r = f.request();
    assert_eq!(refuse(&mut f, &r), "branch_missing@cwd");
    f.git(&["checkout", "-q", "ap/x"]);
    // Two chains on one worktree: the second is refused while the first's unit is active.
    f.chain_alive = true;
    assert_eq!(refuse(&mut f, &r), "chain_active@cwd");
    f.chain_alive = false;
    f.fresh();
    assert_eq!(refuse(&mut f, &r), "out_exists@out");
    // A merged pull request ends the unit: it cannot be continued.
    let mut g = Fake::new(&["impl", "merge"]);
    g.fresh();
    let mut r = g.request();
    r["continue"] = json!(g.p("out/unit.json"));
    assert_eq!(refuse(&mut g, &r), "chain_cannot_continue@continue");
    let mut g = Fake::new(&["impl", "accept"]);
    g.fresh();
    std::fs::write(g.t.join("findings.json"), "[{\"id\":\"X\"}]").unwrap();
    let mut r = g.request();
    r["continue"] = json!(g.p("out/unit.json"));
    r["findings"] = json!(g.p("findings.json"));
    assert_eq!(refuse(&mut g, &r), "findings_unreadable@findings");
}

#[test]
fn wait_blocks_until_the_record_is_final() {
    let mut f = Fake::new(&["impl", "accept"]);
    let rec = f.fresh();
    let r = json!({"record": f.p("out/unit.json")});
    let (v, code) = wait(&mut f, &r).unwrap();
    assert_eq!((v, code), (rec, 0));
    // A record that still says running: the wait ends at its time limit with exit 8.
    let mut running = f.request();
    running["continue"] = json!(f.p("out/unit.json"));
    f.script(&["accept"]);
    start(&mut f, &running, "/ap").unwrap();
    let t = f.offset;
    let r = json!({"record": f.p("out/unit.json"), "timeout_seconds": 60});
    let (v, code) = wait(&mut f, &r).unwrap();
    assert_eq!((v["state"].as_str(), code), (Some("running"), 8));
    assert!(f.offset - t >= Duration::from_secs(60));
    // `ap unit finish`: the record of a chain whose unit was stopped says so.
    let rec = ap::unit::finish(&f.p("out"), "signal");
    assert_eq!(
        (rec["state"].as_str(), rec["exit"].as_u64()),
        (Some("stopped"), Some(30))
    );
    let rec = ap::unit::finish(&f.p("out"), "timeout");
    assert_eq!(rec["state"], "stopped", "a final record is not rewritten");
    // A stopped chain continues from its record.
    let rec = f.again(&["accept"], false);
    assert_shape(&rec, &["implementer", "reviewer", "judge"], "accepted");
}
