//! `ap run` against a fake `claude` (tests/fixtures/fake-claude.sh). The unit launcher is replaced
//! only through the `System` trait: the `systemd-run … --` prefix is checked and dropped, the fake
//! runs in its own process group, and `systemctl stop <unit>` kills that group. The real unit is
//! exercised by ci/host-run-scope.sh.
use ap::run::execute;
use ap::work::{Answer, Failure, Host, Reply, System};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const FAKE: &str = include_str!("fixtures/fake-claude.sh");

struct Fake {
    t: PathBuf,
    env: Vec<(String, String)>,
    units: bool,
    groups: BTreeMap<String, u32>,
    /// The units `systemctl is-active` reports active: launched and not yet stopped.
    alive: std::collections::BTreeSet<String>,
    /// `systemctl stop` does nothing (a stop that fails).
    stop_fails: bool,
    /// `systemctl is-active` does not answer (the call fails).
    silent: bool,
    /// `systemctl stop` waits this long before it kills (a slow shutdown).
    stop_delay: std::time::Duration,
    /// The answer to `git rev-parse` in place of the real one.
    rev_parse: Option<(bool, Vec<u8>)>,
    /// Every `env -i … git …` call.
    git_calls: Vec<Vec<String>>,
    launches: Vec<Vec<String>>,
    /// `directory()` answers for these paths instead of the file system.
    dirs: BTreeMap<String, String>,
}
impl Fake {
    fn new() -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-run-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "a1", "a2", "cwd"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let fake = t.join("bin/claude");
        std::fs::write(&fake, FAKE).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(t.join("prompt.md"), "review this\n").unwrap();
        let env = vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("HOME".into(), "/root".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("HERDR_ENV".into(), "1".into()),
            ("HERDR_PANE_ID".into(), "w36:p1".into()),
            ("CLAUDE_CONFIG_DIR".into(), "/root/.claude".into()),
            ("CLAUDE_CODE_ENTRYPOINT".into(), "cli".into()),
            ("FOO".into(), "bar".into()),
        ];
        Self {
            t,
            env,
            units: true,
            groups: BTreeMap::new(),
            alive: std::collections::BTreeSet::new(),
            stop_fails: false,
            silent: false,
            stop_delay: std::time::Duration::ZERO,
            rev_parse: None,
            git_calls: vec![],
            launches: vec![],
            dirs: BTreeMap::new(),
        }
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    fn mode(&self, account: &str, mode: &str) {
        std::fs::write(self.t.join(format!("bin/mode-{account}")), mode).unwrap();
    }
    fn clear(&self, account: &str) {
        let _ = std::fs::remove_file(self.t.join(format!("bin/mode-{account}")));
    }
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.t.join("bin/calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    /// The number of the fake's last call (its `env.<n>` file).
    fn count(&self) -> String {
        std::fs::read_to_string(self.t.join("bin/count"))
            .unwrap()
            .trim()
            .to_owned()
    }
    fn reset_calls(&self) {
        let _ = std::fs::remove_file(self.t.join("bin/calls.log"));
    }
    fn request(&self, out: &str) -> Value {
        json!({"role":"reviewer","account":self.p("a1"),"model":"sonnet","effort":"high",
               "permission_mode":"auto","cwd":self.p("cwd"),"prompt":self.p("prompt.md"),"out":self.p(out)})
    }
    /// A request on `out` that continues the run whose out was `earlier`.
    fn resume(&self, mut r: Value, out: &str, earlier: &str) -> Value {
        r["out"] = json!(self.p(out));
        r["resume"] = json!({"record": self.p(&format!("{earlier}.run.json"))});
        r
    }
    fn sha(&self, rel: &str) -> String {
        format!(
            "{:x}",
            sha2::Sha256::digest(std::fs::read(self.p(rel)).unwrap())
        )
    }
    fn record(&self, out: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.p(&format!("{out}.run.json"))).unwrap()).unwrap()
    }
    fn run(&mut self, r: &Value) -> Result<(Value, u8), Failure> {
        execute(self, r)
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(std::process::Stdio::null())
        .status();
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        if program == "env" {
            self.git_calls.push(args.to_vec());
            if args.iter().any(|a| a == "rev-parse")
                && let Some((success, bytes)) = self.rev_parse.clone()
            {
                return Ok(Reply {
                    success,
                    bytes,
                    stdout: vec![],
                });
            }
        }
        if program != "systemctl" {
            return Host.call(program, args, input, ms);
        }
        let unit = args.last().unwrap();
        let (success, stdout) = match args[0].as_str() {
            "stop" if self.stop_fails => (false, vec![]),
            "stop" => {
                std::thread::sleep(self.stop_delay);
                if let Some(pgid) = self.groups.get(unit) {
                    kill_group(*pgid);
                }
                self.alive.remove(unit);
                (true, vec![])
            }
            "is-active" if self.silent => {
                return Err(Failure {
                    code: "process_failed",
                    field: "systemctl".into(),
                    detail: Value::Null,
                });
            }
            "is-active" if self.alive.contains(unit) => (true, b"active\n".to_vec()),
            "is-active" => (false, b"inactive\n".to_vec()),
            _ => (false, vec![]),
        };
        Ok(Reply {
            success,
            bytes: stdout.clone(),
            stdout,
        })
    }
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        match self.dirs.get(path) {
            Some(p) => Ok(p.into()),
            None => Host.directory(path),
        }
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        self.launches.push(argv.to_vec());
        let at = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &argv[..4],
            ["systemd-run", "--scope", "--quiet", "--collect"]
        );
        assert!(argv[..at].contains(&"--slice=agents.slice".to_owned()));
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
        self.units
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}
fn code(r: Result<(Value, u8), Failure>) -> String {
    match r {
        Err(f) => format!("{}@{}", f.code, f.field),
        Ok((v, s)) => panic!("expected a refusal, got {s} {v}"),
    }
}
fn gone(pid_file: &Path) -> bool {
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
    // Gone, or a zombie waiting for init: either way nothing runs.
    stat.map_or(true, |s| s.split_whitespace().nth(2) == Some("Z"))
}

// ---- A1 ------------------------------------------------------------------------------------

#[test]
fn a1_answer_is_saved_byte_for_byte_and_the_record_has_every_member() {
    let mut f = Fake::new();
    let answer = "Verdict: accept\nlíne «ü» \"quoted\" \\ back\ttab\r\nlast line, no newline";
    std::fs::write(f.t.join("bin/answer"), answer).unwrap();
    f.mode("a1", "answer");
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(std::fs::read(f.p("x.md")).unwrap(), answer.as_bytes());
    let rec = f.record("x.md");
    assert_eq!(rec, v);
    let keys: Vec<&str> = rec
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut want = vec![
        "role",
        "runtime",
        "model",
        "effort",
        "account",
        "session",
        "cwd",
        "unit",
        "prompt",
        "answer",
        "start",
        "end",
        "state",
        "turns",
        "tokens",
        "permission_denials",
        "expect_file_before",
        "results",
        "expect_met",
        "unmet",
        "resumes",
        "resets_at",
    ];
    want.sort_unstable();
    assert_eq!(keys, want);
    assert_eq!(rec["state"], "ok");
    assert_eq!(rec["role"], "reviewer");
    assert_eq!(rec["runtime"], "claude");
    assert_eq!(rec["model"], "sonnet");
    assert_eq!(rec["effort"], "high");
    assert_eq!(rec["account"], f.p("a1"));
    assert_eq!(rec["cwd"], f.p("cwd"));
    assert_eq!(rec["session"].as_str().unwrap().len(), 36);
    assert!(rec["unit"].as_str().unwrap().starts_with("ap-run-"));
    assert!(rec["unit"].as_str().unwrap().ends_with(".scope"));
    assert_eq!(rec["prompt"]["path"], f.p("prompt.md"));
    assert_eq!(rec["prompt"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(rec["answer"]["path"], f.p("x.md"));
    assert_eq!(rec["answer"]["sha256"].as_str().unwrap().len(), 64);
    assert!(rec["start"].as_str().unwrap().ends_with('Z'));
    assert!(rec["end"].as_str().unwrap().ends_with('Z'));
    assert_eq!(rec["turns"], 2);
    assert_eq!(
        (rec["results"].as_u64(), rec["expect_met"].as_bool()),
        (Some(1), Some(true))
    );
    assert_eq!(
        rec["tokens"],
        json!({"input":7,"output":9,"cache_read":null,"cache_creation":null})
    );
    // No prompt or answer text in the record.
    let text = std::fs::read_to_string(f.p("x.md.run.json")).unwrap();
    assert!(!text.contains("review this") && !text.contains("quoted"));
    // The prompt reached the CLI on stdin, byte for byte.
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.1")).unwrap(),
        b"review this\n"
    );
}

#[test]
fn a1_the_running_record_exists_while_the_cli_runs() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, _) = f.run(&f.request("w.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(seen["state"], "running");
    assert_eq!(seen["session"], v["session"]);
    assert_eq!(seen["unit"], v["unit"]);
    assert_eq!(seen["account"], f.p("a1"));
    assert!(seen["end"].is_null() && seen["answer"].is_null());
    assert_eq!(v["state"], "ok");
}

// ---- A3 ------------------------------------------------------------------------------------

#[test]
fn a3_no_herdr_or_inherited_claude_variable_and_hooks_and_mcp_off() {
    let mut f = Fake::new();
    let mut r = f.request("e.md");
    r["env"] = json!({"CARGO_TARGET_DIR":"/x/target","CARGO_BUILD_JOBS":"2"});
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok");
    let env = std::fs::read_to_string(f.t.join("bin/env.1")).unwrap();
    let names: Vec<&str> = env.lines().filter_map(|l| l.split('=').next()).collect();
    assert!(!names.iter().any(|n| n.starts_with("HERDR_")), "{env}");
    assert_eq!(
        names
            .iter()
            .filter(|n| n.starts_with("CLAUDE_"))
            .collect::<Vec<_>>(),
        [&"CLAUDE_CONFIG_DIR"]
    );
    assert!(
        env.contains(&format!("\nCLAUDE_CONFIG_DIR={}\n", f.p("a1")))
            || env.starts_with(&format!("CLAUDE_CONFIG_DIR={}\n", f.p("a1")))
    );
    assert!(!names.contains(&"FOO"), "only the fixed list is inherited");
    assert!(env.contains("CARGO_TARGET_DIR=/x/target") && env.contains("CARGO_BUILD_JOBS=2"));
    assert!(names.contains(&"PATH") && names.contains(&"HOME"));
    let call = &f.calls()[0];
    assert!(
        call.contains(r#"--settings {"autoMemoryEnabled":false,"disableAllHooks":true,"env":{"#),
        "{call}"
    );
    assert!(call.contains("--strict-mcp-config") && !call.contains("--mcp-config"));
    assert!(call.contains("--permission-mode auto") && call.contains("--permission-prompts none"));
    assert!(call.contains("--output-format stream-json"));
    let launch = &f.launches[0];
    assert!(
        launch.contains(&"RuntimeMaxSec=3310s".to_owned()),
        "{launch:?}"
    );
    // A request that names an MCP configuration gets exactly that one.
    f.reset_calls();
    let mut r = f.request("m.md");
    r["mcp_config"] = json!(["/etc/ap-test/mcp.json"]);
    f.run(&r).unwrap();
    assert!(f.calls()[0].contains("--strict-mcp-config --mcp-config /etc/ap-test/mcp.json"));
    // No request can put a HERDR_ or CLAUDE_ variable back.
    for name in ["HERDR_ENV", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CONFIG_DIR"] {
        let mut r = f.request("n.md");
        r["env"] = json!({name: "1"});
        assert_eq!(code(f.run(&r)), "env_forbidden@env");
    }
}

// ---- A2: the cases of ci/test-lead-review.sh -------------------------------------------------

/// (a) "integration on the Lead's account": the account and model come from the request; the
/// Lead's account is admitted for a reviewer only with `integration`.
#[test]
fn a2_account_and_model_from_the_request_and_the_lead_account_rule() {
    let mut f = Fake::new();
    f.dirs
        .insert("/root/.claude".into(), "/root/.claude".into());
    f.dirs
        .insert("/root/.claude-other".into(), "/root/.claude-other".into());
    f.dirs.insert(f.p("alias"), "/root/.claude".into());
    let mut r = f.request("i.md");
    r["account"] = json!("/root/.claude");
    r["model"] = json!("fable");
    r["integration"] = json!(true);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 0);
    let calls = f.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("/root/.claude|") && calls[0].contains("--model fable"));
    assert!(
        std::fs::read_to_string(f.p("i.md"))
            .unwrap()
            .starts_with("Verdict: accept\n")
    );
    assert_eq!(v["account"], "/root/.claude");
    f.reset_calls();
    let alias = f.p("alias");
    for (role, account, integration) in [
        ("implementer", "/root/.claude", false),
        ("fixer", "/root/.claude/", false),
        ("reviewer", "/root/.claude", false),
        ("reviewer", "/root/./.claude", false),
        ("implementer", alias.as_str(), false),
    ] {
        let mut r = f.request("j.md");
        r["role"] = json!(role);
        r["account"] = json!(account);
        if integration {
            r["integration"] = json!(true);
        }
        assert_eq!(
            code(f.run(&r)),
            "account_forbidden@account",
            "{role} {account}"
        );
    }
    let mut r = f.request("j.md");
    r["role"] = json!("implementer");
    r["integration"] = json!(true);
    assert_eq!(code(f.run(&r)), "invalid_input@integration");
    let mut r = f.request("j.md");
    r["next_accounts"] = json!(["/root/.claude"]);
    assert_eq!(code(f.run(&r)), "account_forbidden@next_accounts");
    assert!(f.calls().is_empty());
    // A name that only begins with the Lead's directory is another account.
    let mut r = f.request("k.md");
    r["role"] = json!("fixer");
    r["account"] = json!("/root/.claude-other");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    // The account is required.
    let mut r = f.request("k.md");
    r.as_object_mut().unwrap().remove("account");
    assert_eq!(code(f.run(&r)), "invalid_input@account");
}

/// (b) cut after work, (c) resume on the recorded account; "an earlier answer under the same
/// name" becomes: an existing `out` refuses and nothing of it is touched; a failed resume leaves
/// the session resumable from its own record.
#[test]
fn a2_cut_after_work_resume_and_an_existing_out_refuses() {
    let mut f = Fake::new();
    let mut r = f.request("v.md");
    r["next_accounts"] = json!([f.p("a2")]);
    // (b) a cut after work: exit 75, no answer, a resume record, the next account is NOT tried.
    f.mode("a1", "limit45");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert!(!Path::new(&f.p("v.md")).exists());
    assert_eq!(f.calls().len(), 1);
    let rec = f.record("v.md");
    assert_eq!(rec["account"], f.p("a1"));
    let session = rec["session"].as_str().unwrap().to_owned();
    assert!(rec["answer"].is_null());
    assert_eq!(rec["turns"], 45);
    let before = (f.sha("v.md.run.json"), f.sha("v.md.stream.jsonl"));
    // (c) the resume names the earlier record; its own out and record; the recorded account.
    f.clear("a1");
    f.reset_calls();
    let (v, status) = f.run(&f.resume(r.clone(), "v2.md", "v.md")).unwrap();
    assert_eq!(status, 0);
    let calls = f.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with(&format!("{}|", f.p("a1"))));
    assert!(calls[0].ends_with(&format!("--resume {session}")));
    assert_eq!(v["session"], session.as_str());
    assert_eq!(v["resumes"], f.p("v.md.run.json"));
    assert_eq!(f.record("v2.md"), v);
    assert!(std::fs::metadata(f.p("v2.md")).unwrap().len() > 0);
    assert_eq!(
        before,
        (f.sha("v.md.run.json"), f.sha("v.md.stream.jsonl")),
        "run 1 untouched"
    );
    // An existing out (its answer, or only its record) refuses; nothing of it is touched.
    f.mode("a1", "limit45");
    let mut again = r.clone();
    again["out"] = json!(f.p("v2.md"));
    assert_eq!(code(f.run(&again)), "out_exists@out");
    assert_eq!(f.record("v2.md"), v);
    assert_eq!(code(f.run(&r)), "out_exists@out");
    // A resume that ends without a result event: its own record keeps the session, and the next
    // resume names that record.
    f.mode("a1", "noout");
    let (v, status) = f.run(&f.resume(r.clone(), "v3.md", "v.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")));
    assert_eq!(f.record("v3.md")["session"], session.as_str());
    f.clear("a1");
    let (v, _) = f.run(&f.resume(r.clone(), "v4.md", "v3.md")).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["session"].as_str()),
        (Some("ok"), Some(session.as_str()))
    );
}

/// "relative prompt and out paths": every path is absolute; a relative one is refused and nothing
/// starts.
#[test]
fn a2_relative_paths_are_refused() {
    let mut f = Fake::new();
    for (key, value) in [
        ("prompt", "prompt.md"),
        ("out", "rel.md"),
        ("cwd", "cwd"),
        ("account", "a1"),
    ] {
        let mut r = f.request("rel.md");
        r[key] = json!(value);
        assert_eq!(code(f.run(&r)), format!("invalid_input@{key}"));
    }
    assert!(f.calls().is_empty());
}

/// "watchdog ends a leftover shell below the reviewer": the session answers and then waits for a
/// shell it started; the run ends at the answer, and nothing of it is alive after the return.
#[test]
fn a2_a_leftover_shell_does_not_hold_the_run() {
    let mut f = Fake::new();
    f.mode("a1", "hang");
    let mut r = f.request("h.md");
    r["timeout_seconds"] = json!(60);
    let s0 = Instant::now();
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 0, "{v}");
    assert!(s0.elapsed().as_secs() < 15, "ended at the result");
    assert!(std::fs::metadata(f.p("h.md")).unwrap().len() > 0);
    assert!(
        gone(&f.t.join("bin/child.pid")),
        "the shell below the CLI is gone"
    );
}

/// "another session's verdict does not end this run's shells": only this run's own output is
/// read; a verdict in another session's record of the same account changes nothing, and the run
/// ends at its time bound.
#[test]
fn a2_another_sessions_record_does_not_end_the_run() {
    let mut f = Fake::new();
    f.mode("a1", "other");
    std::fs::create_dir_all(f.t.join("a1/projects/q")).unwrap();
    std::fs::write(
        f.t.join("a1/projects/q/other.jsonl"),
        r#"{"type":"result","is_error":false,"result":"Verdict: accept"}"#.to_owned() + "\n",
    )
    .unwrap();
    let mut r = f.request("o.md");
    r["timeout_seconds"] = json!(3);
    let s0 = Instant::now();
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("timeout")));
    assert!(s0.elapsed().as_secs() >= 3);
    assert!(!Path::new(&f.p("o.md")).exists());
    assert!(gone(&f.t.join("bin/child.pid")));
    // The record keeps the session, so the same request can resume.
    assert_eq!(v["session"].as_str().unwrap().len(), 36);
}

/// (b2) cut before any work: the next account takes the prompt; all accounts cut: exit 75.
#[test]
fn a2_cut_before_work_tries_the_next_account() {
    let mut f = Fake::new();
    let mut r = f.request("w.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "limit1");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 0);
    assert_eq!(f.calls().len(), 2);
    assert_eq!(v["account"], f.p("a2"));
    assert_eq!(f.record("w.md")["account"], f.p("a2"));
    f.mode("a2", "limit1");
    let (v, status) = f.run(&f_request_with_next(&f, "x.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert!(!Path::new(&f.p("x.md")).exists());
    // A resume never goes to another account.
    f.reset_calls();
    let resume = f.resume(f_request_with_next(&f, "x.md"), "x2.md", "x.md");
    let (_, status) = f.run(&resume).unwrap();
    assert_eq!(status, 75);
    assert_eq!(f.calls().len(), 1);
}
fn f_request_with_next(f: &Fake, out: &str) -> Value {
    let mut r = f.request(out);
    r["next_accounts"] = json!([f.p("a2")]);
    r
}

/// (d) "unknown role" and "out file named in the prompt": refused, nothing started, and a
/// refusal writes no file at all.
#[test]
fn a2_unknown_role_and_out_named_in_prompt() {
    let mut f = Fake::new();
    let mut r = f.request("n.md");
    r["role"] = json!("nobody");
    assert_eq!(code(f.run(&r)), "invalid_input@role");
    std::fs::write(f.t.join("p2.md"), "write n.md").unwrap();
    let mut r = f.request("n.md");
    r["prompt"] = json!(f.p("p2.md"));
    assert_eq!(code(f.run(&r)), "out_named_in_prompt@prompt");
    assert!(f.calls().is_empty());
    for name in ["n.md", "n.md.run.json", "n.md.stream.jsonl", "n.md.err"] {
        assert!(!Path::new(&f.p(name)).exists(), "{name}");
    }
}

// ---- the other invariants ------------------------------------------------------------------

#[test]
fn refusals_codex_systemd_members() {
    let mut f = Fake::new();
    let mut r = f.request("r.md");
    r["runtime"] = json!("codex");
    assert_eq!(code(f.run(&r)), "runtime_not_built@runtime");
    let mut r = f.request("r.md");
    r["roles_file"] = json!("/root/.config/lead/roles.tsv");
    assert_eq!(code(f.run(&r)), "invalid_input@roles_file");
    let mut r = f.request("r.md");
    r["permission_mode"] = json!("yolo");
    assert_eq!(code(f.run(&r)), "invalid_input@permission_mode");
    f.units = false;
    assert_eq!(code(f.run(&f.request("r.md"))), "systemd_absent@host");
    assert!(f.calls().is_empty() && f.launches.is_empty());
}

#[test]
fn a_fresh_implementer_refuses_a_dirty_worktree() {
    let mut f = Fake::new();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(f.p("cwd"))
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    std::fs::write(f.t.join("cwd/half.txt"), "half").unwrap();
    let mut r = f.request("d.md");
    r["role"] = json!("implementer");
    assert_eq!(code(f.run(&r)), "worktree_dirty@cwd");
    r["continue_dirty"] = json!(true);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok");
    assert!(f.launches[0].contains(&"RuntimeMaxSec=5410s".to_owned()));
    // A reviewer reads a dirty tree without refusal.
    let mut r = f.request("d2.md");
    r["role"] = json!("reviewer");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
}

#[test]
fn ok_means_the_expected_result_and_denials_are_recorded() {
    let mut f = Fake::new();
    f.mode("a1", "file");
    let mut r = f.request("f.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"file": f.p("cwd/a.txt"), "first_line": "done"});
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    r["expect"] = json!({"file": f.p("cwd/missing.txt")});
    r["out"] = json!(f.p("f2.md"));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("file"))
    );
    f.clear("a1");
    let mut r = f.request("g.md");
    r["expect"] = json!({"first_line": "Verdict: repair*"});
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str()),
        (Some("no_result"), Some("first_line"))
    );
    assert!(
        std::fs::metadata(f.p("g.md")).is_ok(),
        "the answer is kept for the reader"
    );
    r["expect"] = json!({"first_line": "Verdict: *"});
    r["out"] = json!(f.p("g2.md"));
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    f.mode("a1", "denied");
    let (v, _) = f.run(&f.request("p.md")).unwrap();
    assert_eq!(
        v["permission_denials"],
        json!([{"tool_name":"Bash","tool_use_id":"t1"}])
    );
    assert!(
        !std::fs::read_to_string(f.p("p.md.run.json"))
            .unwrap()
            .contains("git push")
    );
}

// ---- repair 1 --------------------------------------------------------------------------------

/// R1: a result is not final while a shell the session started lives. An implementer's stage runs
/// to the exit and keeps the LAST answer; a reviewer's ends at the first.
#[test]
fn r1_an_implementer_runs_to_the_exit_a_reviewer_ends_at_the_first_result() {
    let mut f = Fake::new();
    f.mode("a1", "twice");
    let mut r = f.request("t.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"first_line": "Verdict: accept"});
    let s0 = Instant::now();
    let (v, status) = f.run(&r).unwrap();
    assert!(s0.elapsed().as_secs() >= 3, "it waited for the shell");
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(
        std::fs::read_to_string(f.p("t.md")).unwrap(),
        "Verdict: accept\nsecond"
    );
    assert_eq!(
        (v["results"].as_u64(), v["turns"].as_u64()),
        (Some(2), Some(3))
    );
    assert_eq!(
        v["tokens"]["output"], 8,
        "usage is per result: the total is the sum"
    );
    let mut r = f.request("u.md");
    r["role"] = json!("reviewer");
    let s0 = Instant::now();
    let (v, _) = f.run(&r).unwrap();
    assert!(
        s0.elapsed().as_secs() < 3,
        "the reviewer ended at the first result"
    );
    assert_eq!(
        std::fs::read_to_string(f.p("u.md")).unwrap(),
        "Verdict: accept\nfirst"
    );
    assert_eq!(
        (v["state"].as_str(), v["results"].as_u64()),
        (Some("ok"), Some(1))
    );
    assert!(gone(&f.t.join("bin/child.pid")));
}

/// A fixer whose shell never ends runs to the bound: state `timeout`, the last answer saved, and
/// the record says whether `expect` was met.
#[test]
fn r1_a_fixer_whose_shell_never_ends_times_out_with_its_last_answer() {
    let mut f = Fake::new();
    f.mode("a1", "hang");
    let mut r = f.request("z.md");
    r["role"] = json!("fixer");
    r["timeout_seconds"] = json!(3);
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("timeout")), "{v}");
    assert_eq!(v["expect_met"], true);
    assert!(
        std::fs::read_to_string(f.p("z.md"))
            .unwrap()
            .starts_with("Verdict: accept")
    );
    assert!(gone(&f.t.join("bin/child.pid")));
    r["expect"] = json!({"first_line": "Verdict: repair"});
    r["out"] = json!(f.p("z2.md"));
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (
            v["state"].as_str(),
            v["expect_met"].as_bool(),
            v["unmet"].as_str()
        ),
        (Some("timeout"), Some(false), Some("first_line"))
    );
}

// ---- repair 2: one test per item of TASK-AP-RUN.repair-2.md ----------------------------------

/// Item 1: a finished answer is never read as a cut. Without `is_error`, the usage-limit text is
/// a cut only with a rejected rate-limit event in the stream.
#[test]
fn item01_a_finished_answer_is_not_a_cut() {
    let mut f = Fake::new();
    let mut r = f.request("a.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "resets");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(f.calls().len(), 1, "the finished review is not run again");
    assert!(
        std::fs::read_to_string(f.p("a.md"))
            .unwrap()
            .contains("resets the counter")
    );
    f.mode("a1", "limitok");
    let (v, _) = f.run(&f.request("b.md")).unwrap();
    assert_eq!(v["state"], "ok", "no rejected event: an answer");
    f.mode("a1", "limitrej");
    let (v, status) = f.run(&f.request("c.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert!(!Path::new(&f.p("c.md")).exists());
}

/// Item 2: `expect.file` is met only by a file this session wrote; a resume keeps the first
/// launch as the reference.
#[test]
fn item02_expect_file_is_met_only_by_a_file_the_session_wrote() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("cwd/REPORT.md"), "an old report\n").unwrap();
    let mut r = f.request("e.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"file": f.p("cwd/REPORT.md")});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("file")),
        "{v}"
    );
    // A file the session rewrote with the same bytes was written by it.
    std::fs::write(f.t.join("cwd/a.txt"), "ready\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    f.mode("a1", "file");
    r["expect"] = json!({"file": f.p("cwd/a.txt")});
    r["out"] = json!(f.p("e2.md"));
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    // A resume of the same unfinished stage keeps the earlier run's reference (absent), not its own.
    std::fs::remove_file(f.t.join("cwd/a.txt")).unwrap();
    f.mode("a1", "limit45");
    let mut r = f.request("g.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"file": f.p("cwd/a.txt")});
    assert_eq!(f.run(&r).unwrap().0["state"], "cut");
    assert_eq!(
        f.record("g.md")["expect_file_before"]["sha256"],
        Value::Null
    );
    std::fs::write(f.t.join("cwd/a.txt"), "written before the cut\n").unwrap();
    f.clear("a1");
    let (v, _) = f.run(&f.resume(r, "g2.md", "g.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
}

/// Item 3 (repair 3): a fresh run on an `out` whose record exists is refused by existence; a
/// resume of a `running` record is refused while its unit is active.
#[test]
fn item03_an_existing_record_refuses_and_a_live_unit_refuses_its_resume() {
    let mut f = Fake::new();
    let rec = json!({"state":"running","unit":"ap-run-live.scope","session":"11111111-2222-3333-4444-555555555555",
                     "account":f.p("a1"),"cwd":f.p("cwd"),"role":"implementer"});
    std::fs::write(f.p("r.md.run.json"), rec.to_string()).unwrap();
    f.alive.insert("ap-run-live.scope".into());
    let mut r = f.request("r.md");
    r["role"] = json!("implementer");
    assert_eq!(code(f.run(&r)), "out_exists@out");
    assert_eq!(
        std::fs::read_to_string(f.p("r.md.run.json")).unwrap(),
        rec.to_string()
    );
    let resume = f.resume(r, "r2.md", "r.md");
    assert_eq!(code(f.run(&resume)), "run_still_active@resume");
    assert!(f.launches.is_empty());
    f.alive.clear();
    let (v, _) = f.run(&resume).unwrap();
    assert_eq!(v["state"], "ok", "the unit has ended: admitted");
    assert_eq!(v["session"], "11111111-2222-3333-4444-555555555555");
}

/// Item 4: the stream is read through the handle opened before the launch: a file put under its
/// name cannot feed the run another run's lines, and a pipe under its name cannot block it.
#[test]
fn item04_the_stream_is_read_through_its_own_handle() {
    let mut f = Fake::new();
    f.mode("a1", "swap");
    let (v, _) = f.run(&f.request("s.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert_eq!(
        std::fs::read_to_string(f.p("s.md")).unwrap(),
        "Verdict: accept\nown"
    );
    assert_ne!(v["session"], "other");
    f.mode("a1", "fifo");
    let mut r = f.request("p.md");
    r["timeout_seconds"] = json!(3);
    let s0 = Instant::now();
    let (v, _) = f.run(&r).unwrap();
    assert!(s0.elapsed().as_secs() < 10);
    assert_eq!(v["state"], "ok", "{v}");
}

/// Item 5: after the stop the unit is confirmed not active; a stop that cannot be confirmed is
/// `failed` with `unmet: stop`, never `ok`.
#[test]
fn item05_a_stop_that_is_not_confirmed_is_failed() {
    let mut f = Fake::new();
    f.stop_fails = true;
    let (v, status) = f.run(&f.request("t.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("stop")),
        "{v}"
    );
    assert!(v.get("others_at_stop").is_none());
    for pgid in f.groups.values() {
        kill_group(*pgid);
    }
}

/// Item 6: exit 2 is a refusal before any effect; a write that fails after the launch leaves a
/// final record that is not `running` and exits 5.
#[test]
fn item06_a_write_failure_after_the_launch_is_not_a_refusal() {
    let mut f = Fake::new();
    f.mode("a1", "blockout");
    let (v, status) = f.run(&f.request("w.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (5, Some("failed"), Some("write")),
        "{v}"
    );
    assert_eq!(f.record("w.md")["state"], "failed");
    // (Repair 3: a directory at the record's path is no longer a write failure but an existing
    // record: `out_exists`, a refusal before any effect.)
    std::fs::create_dir(f.t.join("x.md.run.json")).unwrap();
    assert_eq!(code(f.run(&f.request("x.md"))), "out_exists@out");
    assert_eq!(f.launches.len(), 1);
}

/// Item 7 (repair 3): a refusal writes no file; the earlier record and its session stay.
#[test]
fn item07_a_refusal_writes_no_file_and_keeps_the_earlier_record() {
    let mut f = Fake::new();
    f.mode("a1", "other");
    let mut r = f.request("k.md");
    r["role"] = json!("implementer");
    r["timeout_seconds"] = json!(1);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "timeout");
    let before = f.sha("k.md.run.json");
    let mut bad = f.resume(r.clone(), "k2.md", "k.md");
    bad["env"] = json!({"CLAUDE_X": "1"});
    assert_eq!(code(f.run(&bad)), "env_forbidden@env");
    assert_eq!(f.sha("k.md.run.json"), before);
    assert!(!Path::new(&f.p("k2.md.run.json")).exists());
    f.clear("a1");
    let (w, _) = f.run(&f.resume(r, "k2.md", "k.md")).unwrap();
    assert_eq!(
        (w["state"].as_str(), &w["session"]),
        (Some("ok"), &v["session"])
    );
}

/// Item 8: the dirty-worktree rule fails closed and runs `git` in the CLI's clean environment.
#[test]
fn item08_the_worktree_check_fails_closed_in_a_clean_environment() {
    let mut f = Fake::new();
    f.env.push(("GIT_DIR".into(), "/nonexistent".into()));
    let git = |dir: &str, args: &[&str]| {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&f.p("cwd"), &["init", "-q"]);
    std::fs::write(f.t.join("cwd/half.txt"), "half").unwrap();
    let mut r = f.request("g.md");
    r["role"] = json!("implementer");
    assert_eq!(code(f.run(&r)), "worktree_dirty@cwd");
    let call = &f.git_calls[0];
    assert_eq!(call[0], "-i");
    assert!(
        call.contains(&"LC_ALL=C".to_owned()) && !call.iter().any(|a| a.starts_with("GIT_DIR=")),
        "{call:?}"
    );
    f.rev_parse = Some((
        false,
        b"fatal: detected dubious ownership in repository at '/x'\n".to_vec(),
    ));
    assert_eq!(code(f.run(&r)), "worktree_unreadable@cwd");
    f.rev_parse = Some((
        false,
        b"fatal: not a git repository (or any of the parent directories): .git\n".to_vec(),
    ));
    assert_eq!(
        f.run(&r).unwrap().0["state"],
        "ok",
        "not a repository: no check"
    );
}

/// Item 9: the request's `env` cannot change the account that answers.
#[test]
fn item09_env_refuses_anthropic_names_and_home() {
    let mut f = Fake::new();
    for name in ["ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "HOME"] {
        let mut r = f.request("n.md");
        r["env"] = json!({name: "x"});
        assert_eq!(code(f.run(&r)), "env_forbidden@env", "{name}");
    }
    let mut r = f.request("n.md");
    r["env"] = json!({"CARGO_TARGET_DIR": "/x", "NODE_OPTIONS": "--max-old-space-size=512"});
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
}

/// Item 10: a resume needs the record's `cwd`, and its role or `fixer` over an implementer's or
/// a fixer's record.
#[test]
fn item10_a_resume_keeps_the_cwd_and_the_role() {
    let mut f = Fake::new();
    std::fs::create_dir(f.t.join("cwd2")).unwrap();
    f.mode("a1", "limit45");
    f.run(&f.request("v.md")).unwrap();
    f.clear("a1");
    let mut r = f.resume(f.request("x"), "v2.md", "v.md");
    r["cwd"] = json!(f.p("cwd2"));
    assert_eq!(code(f.run(&r)), "resume_cwd_differs@cwd");
    r["cwd"] = json!(f.p("cwd"));
    r["role"] = json!("fixer");
    assert_eq!(
        code(f.run(&r)),
        "resume_role_differs@role",
        "a reviewer's session"
    );
    for bad in [
        json!(true),
        json!({"record": "relative.json"}),
        json!({"record": f.p("v.md.run.json"), "x": 1}),
    ] {
        r["resume"] = bad;
        assert_eq!(code(f.run(&r)), "invalid_input@resume");
    }
    f.mode("a1", "limit45");
    let mut i = f.request("i.md");
    i["role"] = json!("implementer");
    f.run(&i).unwrap();
    f.clear("a1");
    i["role"] = json!("fixer");
    let (v, _) = f.run(&f.resume(i, "i2.md", "i.md")).unwrap();
    assert_eq!(v["state"], "ok", "a fixer continues an implementer");
}

/// Item 11: a result line without a final newline at the CLI's exit is parsed.
#[test]
fn item11_a_last_line_without_newline_is_parsed() {
    let mut f = Fake::new();
    f.mode("a1", "nonl");
    let mut r = f.request("l.md");
    r["role"] = json!("implementer");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
}

/// Item 12: only the account's settings load (`--setting-sources user`), for every role.
#[test]
fn item12_project_and_local_settings_are_not_loaded() {
    let mut f = Fake::new();
    for role in ["reviewer", "implementer", "fixer"] {
        let mut r = f.request(&format!("{role}.md"));
        r["role"] = json!(role);
        f.run(&r).unwrap();
    }
    for call in f.calls() {
        assert!(call.contains("--setting-sources user"), "{call}");
    }
}

// ---- repair 3: the five sequences of the resumed attack on b026ac8 ---------------------------

/// "Two runs start at the same instant on one `out`": the exclusive creation of the record lets
/// exactly one pass; the record and the answer are the winner's.
#[test]
fn same_instant_on_one_out_only_one_run_passes() {
    let (mut a, mut b) = (Fake::new(), Fake::new());
    a.mode("a1", "twice");
    let out = a.p("o.md");
    let ra = a.request("o.md");
    let mut rb = b.request("unused");
    rb["out"] = json!(out);
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let g = gate.clone();
    let tb = std::thread::spawn(move || {
        g.wait();
        let r = execute(&mut b, &rb)
            .map(|(v, _)| v["session"].clone())
            .map_err(|f| f.code);
        (r, b)
    });
    gate.wait();
    let first = execute(&mut a, &ra)
        .map(|(v, _)| v["session"].clone())
        .map_err(|f| f.code);
    let (second, b) = tb.join().unwrap();
    let results = [first, second];
    assert_eq!(
        results.iter().filter(|r| r == &&Err("out_exists")).count(),
        1,
        "{results:?}"
    );
    let won = results.iter().find_map(|r| r.clone().ok()).unwrap();
    let rec: Value =
        serde_json::from_slice(&std::fs::read(format!("{out}.run.json")).unwrap()).unwrap();
    assert_eq!(rec["session"], won);
    assert_eq!(rec["state"], "ok");
    drop(b);
}

/// "`is-active` does not answer": a resume of a `running` record is refused, and a stop whose
/// state cannot be read is `failed` with `unmet: stop`.
#[test]
fn is_active_that_does_not_answer_refuses_and_fails_the_stop() {
    let mut f = Fake::new();
    let rec = json!({"state":"running","unit":"ap-run-quiet.scope","session":"11111111-2222-3333-4444-555555555555",
                     "account":f.p("a1"),"cwd":f.p("cwd"),"role":"reviewer"});
    std::fs::write(f.p("q.md.run.json"), rec.to_string()).unwrap();
    f.silent = true;
    let resume = f.resume(f.request("x"), "q2.md", "q.md");
    assert_eq!(code(f.run(&resume)), "run_still_active@resume");
    assert!(f.launches.is_empty());
    let (v, status) = f.run(&f.request("s.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("stop"))
    );
    f.silent = false;
    assert_eq!(f.run(&resume).unwrap().0["state"], "ok");
}

/// "A fixer over an `ok` implementer that writes nothing": its reference is its own launch's, so
/// the implementer's file does not meet its `expect`: `no_result`.
#[test]
fn a_fixer_over_an_ok_implementer_that_writes_nothing_is_no_result() {
    let mut f = Fake::new();
    f.mode("a1", "file");
    let mut r = f.request("im.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"file": f.p("cwd/a.txt")});
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    f.clear("a1");
    let mut fix = f.resume(r, "fx.md", "im.md");
    fix["role"] = json!("fixer");
    let (v, status) = f.run(&fix).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("file")),
        "{v}"
    );
}

/// "A refused request leaves the earlier record and its session resumable".
#[test]
fn a_refused_request_leaves_the_earlier_record_resumable() {
    let mut f = Fake::new();
    let mut r = f.request("ok.md");
    r["role"] = json!("implementer");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok");
    let before = (
        f.sha("ok.md.run.json"),
        f.sha("ok.md"),
        f.sha("ok.md.stream.jsonl"),
    );
    let mut fix = f.resume(r, "fx.md", "ok.md");
    fix["role"] = json!("fixer");
    let mut bad = fix.clone();
    bad["timeout_seconds"] = json!(0);
    assert_eq!(code(f.run(&bad)), "invalid_input@timeout_seconds");
    assert_eq!(
        before,
        (
            f.sha("ok.md.run.json"),
            f.sha("ok.md"),
            f.sha("ok.md.stream.jsonl")
        )
    );
    let (w, _) = f.run(&fix).unwrap();
    assert_eq!(
        (w["state"].as_str(), &w["session"]),
        (Some("ok"), &v["session"])
    );
}

/// "The request's `GIT_DIR` does not skip the dirty check" (nor `GIT_CEILING_DIRECTORIES`).
#[test]
fn the_requests_git_dir_does_not_skip_the_dirty_check() {
    let mut f = Fake::new();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(f.p("cwd"))
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    std::fs::create_dir(f.t.join("cwd/sub")).unwrap();
    std::fs::write(f.t.join("cwd/half.txt"), "half").unwrap();
    let mut r = f.request("g.md");
    r["role"] = json!("implementer");
    r["env"] = json!({"GIT_DIR": "/nonexistent"});
    assert_eq!(code(f.run(&r)), "worktree_dirty@cwd");
    r["env"] = json!({"GIT_CEILING_DIRECTORIES": f.p("cwd")});
    r["cwd"] = json!(f.p("cwd/sub"));
    assert_eq!(code(f.run(&r)), "worktree_dirty@cwd");
    assert!(f.launches.is_empty());
}

// ---- closing fix: the third attack on 1c95517 ---------------------------------------------------

/// "A later run deletes an earlier run's answer": a run whose `out` is another run's old
/// temporary name (`a.md.tmp`, `b.md.run.json.tmp`) keeps its files when that other run writes.
#[test]
fn a_later_run_never_removes_a_file_named_like_its_temporary() {
    let mut f = Fake::new();
    for (earlier, later) in [("a.md.tmp", "a.md"), ("b.md.run.json.tmp", "b.md")] {
        let (y, _) = f.run(&f.request(earlier)).unwrap();
        assert_eq!(y["state"], "ok");
        let (x, _) = f.run(&f.request(later)).unwrap();
        assert_eq!(x["state"], "ok");
        assert_eq!(
            f.sha(earlier),
            y["answer"]["sha256"].as_str().unwrap(),
            "{earlier}"
        );
    }
    let leftovers: Vec<_> = std::fs::read_dir(&f.t)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".ap-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// "A record that says something may still live can be resumed": a resume asks `is-active` for
/// the earlier unit whatever the record's state (here `failed`, `unmet: stop`).
#[test]
fn a_resume_of_a_failed_stop_is_refused_while_its_unit_lives() {
    let mut f = Fake::new();
    f.stop_fails = true;
    let (v, _) = f.run(&f.request("st.md")).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str()),
        (Some("failed"), Some("stop"))
    );
    f.stop_fails = false;
    let resume = f.resume(f.request("x"), "st2.md", "st.md");
    assert_eq!(code(f.run(&resume)), "run_still_active@resume");
    assert!(!Path::new(&f.p("st2.md.run.json")).exists());
    for pgid in f.groups.values() {
        kill_group(*pgid);
    }
    f.alive.clear();
    assert_eq!(f.run(&resume).unwrap().0["state"], "ok");
}

// ---- automatic review of 02a619e -----------------------------------------------------------------

/// "Preserve the timeout decision through shutdown": a reviewer whose result arrives only while
/// the stage is being stopped is `timeout`; the late answer is saved and `expect_met` set.
#[test]
fn a_result_written_during_the_shutdown_keeps_the_timeout() {
    let mut f = Fake::new();
    f.mode("a1", "late");
    f.stop_delay = std::time::Duration::from_millis(1500);
    let mut r = f.request("late.md");
    r["timeout_seconds"] = json!(2);
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("timeout")), "{v}");
    assert_eq!(
        (v["results"].as_u64(), v["expect_met"].as_bool()),
        (Some(1), Some(true))
    );
    assert_eq!(
        std::fs::read_to_string(f.p("late.md")).unwrap(),
        "Verdict: accept\nlate"
    );
}

/// "Drain the stopped stream completely": a result after 40 MiB of other lines is read.
#[test]
fn the_stopped_stream_is_read_to_its_end() {
    let mut f = Fake::new();
    f.mode("a1", "big");
    let mut r = f.request("big.md");
    r["role"] = json!("implementer");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert_eq!(
        std::fs::read_to_string(f.p("big.md")).unwrap(),
        "Verdict: accept\nafter 40 MiB"
    );
}

/// "Reserve the answer path against overlapping concurrent runs": an `out` named like a file
/// `ap run` derives from another `out` is refused, so no answer path is another run's file.
#[test]
fn an_out_named_like_a_derived_file_is_refused() {
    let mut f = Fake::new();
    for name in [
        "answer.run.json",
        "a.stream.jsonl",
        "a.err",
        "a.md.1-2-0.ap-tmp",
    ] {
        assert_eq!(code(f.run(&f.request(name))), "invalid_input@out", "{name}");
    }
    assert!(f.launches.is_empty());
}

// ---- AP-GATE -----------------------------------------------------------------------------------

/// Invariant 1 and 3: an implementer's and a fixer's launch carries the CLI's own settings with
/// nothing in the request: the Bash time limits, no move to the background, no `Monitor`, no
/// listing of skills and no auto-memory. `skills` and `memory` bring the two back; `bind` binds.
#[test]
fn gate_settings_of_every_implementer_and_fixer_launch() {
    let mut f = Fake::new();
    for role in ["implementer", "fixer"] {
        f.reset_calls();
        let mut r = f.request(&format!("{role}.md"));
        r["role"] = json!(role);
        let (v, _) = f.run(&r).unwrap();
        assert_eq!(v["state"], "ok", "{v}");
        let call = &f.calls()[0];
        let at = call.find("--settings ").unwrap() + "--settings ".len();
        let json_end = at + call[at..].find(" --").unwrap();
        let settings: Value = serde_json::from_str(&call[at..json_end]).unwrap();
        assert_eq!(
            settings,
            json!({"disableAllHooks": true, "autoMemoryEnabled": false, "env": {
                "BASH_DEFAULT_TIMEOUT_MS": "2700000", "BASH_MAX_TIMEOUT_MS": "2700000",
                "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"}}),
            "{role}"
        );
        assert!(call.contains(" --disallowedTools Monitor "), "{call}");
        assert!(call.contains(" --disable-slash-commands "), "{call}");
        // The settings come from ap run, not the request: the request's env stays free of them.
        let env = std::fs::read_to_string(f.t.join(format!("bin/env.{}", f.count()))).unwrap();
        assert!(!env.contains("BASH_MAX_TIMEOUT_MS"), "{env}");
    }
    f.reset_calls();
    let mut r = f.request("asked.md");
    r["skills"] = json!(true);
    r["memory"] = json!(true);
    f.run(&r).unwrap();
    let call = &f.calls()[0];
    assert!(!call.contains("--disable-slash-commands") && !call.contains("autoMemoryEnabled"));
    assert!(
        call.contains("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS")
            && call.contains("--disallowedTools Monitor")
    );
    // bind: the launch binds the scope to an active unit; an inactive or malformed one refuses.
    f.alive.insert("chain-x.service".into());
    let mut r = f.request("bound.md");
    r["bind"] = json!("chain-x.service");
    f.run(&r).unwrap();
    let launch = f.launches.last().unwrap();
    let at = launch.iter().position(|a| a == "--").unwrap();
    let props = launch[..at].join(" ");
    assert!(
        props.contains("-p BindsTo=chain-x.service -p After=chain-x.service"),
        "{props}"
    );
    r["out"] = json!(f.p("bound2.md"));
    r["bind"] = json!("gone.service");
    assert_eq!(code(f.run(&r)), "bind_inactive@bind");
    for bad in [
        json!("-x.service"),
        json!("a b.service"),
        json!("noext"),
        json!(1),
    ] {
        r["bind"] = bad;
        assert_eq!(code(f.run(&r)), "invalid_input@bind");
    }
    assert!(!Path::new(&f.p("bound2.md.run.json")).exists());
}

/// Invariant 2, on the skeleton of the real stream that emitted three results (the check moved to
/// the background, three `Monitor` watches): the second result met `expect`, the third did not. The
/// stage is `ok` with the second answer; a later result does not unmake it.
#[test]
fn gate_a_later_result_does_not_unmake_one_that_met_expect() {
    let mut f = Fake::new();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/three-results.skeleton"),
        f.t.join("bin/skeleton"),
    )
    .unwrap();
    let answers = "the check is still running\nDONE AP-GATE https://example.invalid/pull/1\\nthe report\nthat notification was the first watch";
    std::fs::write(f.t.join("bin/answers"), answers).unwrap();
    f.mode("a1", "skeleton");
    let mut r = f.request("s.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"first_line": "DONE AP-GATE *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(
        (v["results"].as_u64(), v["expect_met"].as_bool()),
        (Some(3), Some(true))
    );
    assert_eq!(
        std::fs::read_to_string(f.p("s.md")).unwrap(),
        "DONE AP-GATE https://example.invalid/pull/1\nthe report"
    );
    assert_eq!(
        v["turns"], 3,
        "turns and tokens stay the sums over every result"
    );
    // No result met it: the last one decides, as before.
    r["out"] = json!(f.p("s2.md"));
    r["expect"] = json!({"first_line": "DONE OTHER *"});
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str()),
        (Some("no_result"), Some("first_line"))
    );
    assert_eq!(
        std::fs::read_to_string(f.p("s2.md")).unwrap(),
        "that notification was the first watch"
    );
}

// ---- P2: structured output (`schema`) and the reset of a cut ----------------------------------

fn with_schema(f: &Fake, out: &str, object: Option<Value>) -> Value {
    let schema = json!({"type": "object", "additionalProperties": false, "required": ["verdict", "head"],
        "properties": {"verdict": {"enum": ["accept", "repair"]}, "head": {"type": "string", "pattern": "^[0-9a-f]{40}$"}}});
    std::fs::write(f.t.join("schema.json"), schema.to_string()).unwrap();
    match object {
        Some(o) => std::fs::write(f.t.join("bin/object"), o.to_string()).unwrap(),
        None => {
            let _ = std::fs::remove_file(f.t.join("bin/object"));
        }
    }
    f.mode("a1", "object");
    let mut r = f.request(out);
    r["schema"] = json!(f.p("schema.json"));
    r
}

#[test]
fn schema_a_result_with_an_object_that_validates() {
    let mut f = Fake::new();
    let object = json!({"verdict": "accept", "head": "a".repeat(40)});
    let r = with_schema(&f, "s.md", Some(object.clone()));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((v["state"].as_str(), status), (Some("ok"), 0), "{v}");
    // The answer file is the object; the launch passed the schema.
    let answer: Value = serde_json::from_slice(&std::fs::read(f.p("s.md")).unwrap()).unwrap();
    assert_eq!(answer, object);
    let launch = &f.launches[0];
    let at = launch.iter().position(|a| a == "--json-schema").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&launch[at + 1]).unwrap()["required"],
        json!(["verdict", "head"])
    );
}

#[test]
fn schema_a_result_without_an_object() {
    let mut f = Fake::new();
    let r = with_schema(&f, "s.md", None);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str(), status),
        (Some("no_result"), Some("structured_output"), 3),
        "{v}"
    );
    // The text is still saved.
    assert_eq!(
        std::fs::read_to_string(f.p("s.md")).unwrap(),
        "Verdict: accept"
    );
}

#[test]
fn schema_a_result_whose_object_does_not_validate() {
    let mut f = Fake::new();
    // Free text in a revision field: the pattern stops it.
    let r = with_schema(
        &f,
        "s.md",
        Some(json!({"verdict": "accept", "head": "the head of the branch"})),
    );
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str(), status),
        (Some("no_result"), Some("schema"), 3),
        "{v}"
    );
    let r = with_schema(
        &f,
        "t.md",
        Some(json!({"verdict": "maybe", "head": "a".repeat(40)})),
    );
    assert_eq!(f.run(&r).unwrap().0["unmet"], "schema");
}

#[test]
fn schema_with_a_keyword_the_validator_lacks_is_refused() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("schema.json"), r#"{"type":"object","oneOf":[]}"#).unwrap();
    let mut r = f.request("s.md");
    r["schema"] = json!(f.p("schema.json"));
    assert_eq!(code(f.run(&r)), "invalid_input@schema");
    r["schema"] = json!("schema.json");
    assert_eq!(code(f.run(&r)), "invalid_input@schema");
    assert!(!f.t.join("s.md.run.json").exists());
}

#[test]
fn a_cut_records_the_reset_the_stream_reported() {
    let mut f = Fake::new();
    f.mode("a1", "limit1");
    let (v, status) = f.run(&f.request("c.md")).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["resets_at"].as_u64(), status),
        (Some("cut"), Some(1_794_648_600), 75),
        "{v}"
    );
}
