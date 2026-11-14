//! `ap run` against a fake `claude` (tests/fixtures/fake-claude.sh). The unit launcher is replaced
//! only through the `System` trait: the `systemd-run … --` prefix is checked and dropped, the fake
//! runs in its own process group, and `systemctl stop <unit>` kills that group. The real unit is
//! exercised by ci/host-run-scope.sh.
use ap::run::execute;
use ap::work::{Answer, Failure, Host, Reply, System};
use serde_json::{Value, json};
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
    fn reset_calls(&self) {
        let _ = std::fs::remove_file(self.t.join("bin/calls.log"));
    }
    fn request(&self, out: &str) -> Value {
        json!({"role":"reviewer","account":self.p("a1"),"model":"sonnet","effort":"high",
               "permission_mode":"auto","cwd":self.p("cwd"),"prompt":self.p("prompt.md"),"out":self.p(out)})
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
fn group_members(pgid: u32) -> String {
    let out = Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        if program != "systemctl" {
            return Host.call(program, args, input, ms);
        }
        let unit = args.last().unwrap();
        let (success, stdout) = match args[0].as_str() {
            "stop" => {
                if let Some(pgid) = self.groups.get(unit) {
                    kill_group(*pgid);
                }
                (true, vec![])
            }
            "show" => (true, format!("/ap-test/{unit}\n").into_bytes()),
            _ => (false, vec![]),
        };
        Ok(Reply {
            success,
            bytes: stdout.clone(),
            stdout,
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if let Some(unit) = path
            .strip_prefix("/sys/fs/cgroup/ap-test/")
            .and_then(|p| p.strip_suffix("/cgroup.procs"))
        {
            let pgid = self.groups[unit];
            return Ok(group_members(pgid).into_bytes());
        }
        Host.read(path)
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
        files: [&Path; 3],
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
            .stdin(std::fs::File::open(files[0])?)
            .stdout(std::fs::File::create(files[1])?)
            .stderr(std::fs::File::create(files[2])?)
            .process_group(0)
            .spawn()?;
        self.groups.insert(unit, child.id());
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
        "others_at_stop",
        "unmet",
        "refusal",
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
        call.contains(r#"--settings {"disableAllHooks":true}"#),
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

/// (b) cut after work, (c) resume on the recorded account, the earlier answer moved aside, and
/// a failed resume keeps the session.
#[test]
fn a2_cut_after_work_resume_and_stale_answer() {
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
    // (c) the resume uses the recorded account and session.
    f.clear("a1");
    f.reset_calls();
    let mut resume = r.clone();
    resume["resume"] = json!(true);
    let (v, status) = f.run(&resume).unwrap();
    assert_eq!(status, 0);
    let calls = f.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with(&format!("{}|", f.p("a1"))));
    assert!(calls[0].ends_with(&format!("--resume {session}")));
    assert_eq!(v["session"], session.as_str());
    assert!(std::fs::metadata(f.p("v.md")).unwrap().len() > 0);
    // An earlier answer under the same name does not survive a cut run: it is moved aside.
    f.mode("a1", "limit45");
    let (_, status) = f.run(&r).unwrap();
    assert_eq!(status, 75);
    assert!(!Path::new(&f.p("v.md")).exists());
    assert!(
        std::fs::read_to_string(f.p("v.md.prev"))
            .unwrap()
            .starts_with("Verdict: accept")
    );
    // A resume that ends without a result event keeps the recorded session for the next resume.
    let session = f.record("v.md")["session"].as_str().unwrap().to_owned();
    f.mode("a1", "noout");
    let (v, status) = f.run(&resume).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")));
    assert_eq!(f.record("v.md")["session"], session.as_str());
    f.clear("a1");
    let (v, _) = f.run(&resume).unwrap();
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
    assert!(v["others_at_stop"].as_u64().unwrap() >= 1, "{v}");
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
    let mut resume = f_request_with_next(&f, "x.md");
    resume["resume"] = json!(true);
    let (_, status) = f.run(&resume).unwrap();
    assert_eq!(status, 75);
    assert_eq!(f.calls().len(), 1);
}
fn f_request_with_next(f: &Fake, out: &str) -> Value {
    let mut r = f.request(out);
    r["next_accounts"] = json!([f.p("a2")]);
    r
}

/// (d) "unknown role" and "out file named in the prompt": refused, nothing started; a refusal
/// writes a `refused` record only where no record exists.
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
    let rec = f.record("n.md");
    assert_eq!(rec["state"], "refused");
    assert_eq!(
        rec["refusal"],
        json!({"code":"invalid_input","field":"role"})
    );
    // A resumable record is never replaced by a refusal.
    f.mode("a1", "limit45");
    f.run(&f.request("c.md")).unwrap();
    std::fs::write(f.t.join("p3.md"), "write c.md").unwrap();
    let mut r = f.request("c.md");
    r["prompt"] = json!(f.p("p3.md"));
    assert_eq!(code(f.run(&r)), "out_named_in_prompt@prompt");
    assert_eq!(f.record("c.md")["state"], "cut");
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
