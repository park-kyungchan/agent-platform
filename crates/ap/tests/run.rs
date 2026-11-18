//! `ap run` against a fake `claude` (tests/fixtures/fake-claude.sh). The unit launcher is replaced
//! only through the `System` trait: the `systemd-run … --` prefix is checked and dropped, the fake
//! runs in its own process group, and `systemctl stop <unit>` kills that group. The real unit is
//! exercised by ci/host-run-scope.sh.
use ap::host::{Answer, Failure, Host, Reply, System};
use ap::run::execute;
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
const FAKE_CODEX: &str = include_str!("fixtures/fake-codex.sh");
const FAKE_MUSE: &str = include_str!("fixtures/fake-muse.sh");
const FAKE_AGY: &str = include_str!("fixtures/fake-agy.sh");
const BWRAP: &str = "/usr/bin/bwrap";

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
    /// `environment()` creates this file: the call's own file then exists when it is opened.
    precreate: Option<String>,
    /// `environment()` creates this directory: it appears after the request's checks.
    precreate_dir: Option<String>,
    /// The answer of `/usr/bin/bwrap --version` (the probe of `limits`), and every such call with
    /// its arguments and bound.
    bwrap: bool,
    probes: Vec<(Vec<String>, u64)>,
    /// The environment list each launch was given, before a later value of a name replaces an
    /// earlier one.
    launch_envs: Vec<Vec<(String, String)>>,
    /// The policy file the host names (`policy_file()`); none unless a test sets one.
    policy: Option<PathBuf>,
    /// The store engine the stage runner calls: a call to `ont` without one is a failed test.
    ont: Option<FakeOnt>,
    /// The launches and the calls to `ont`, in the order they happened.
    events: Vec<String>,
}
/// What one call of the fake `ont` does, in place of the engine's answer.
#[derive(Clone)]
enum Way {
    /// The engine refuses with this code, as `{"status":"refused","code":..}`.
    Refuse(&'static str),
    /// The call applies nothing and gives no answer (its bound, or a failure).
    Lost,
    /// The call applies and then gives no answer (the engine locks after its commit).
    Silent,
}
/// The engine as the stage runner meets it: `get` of a unit or an intent, `start_run`, `close_run`,
/// the replay of a request id (`already_applied`, or `request_id_reused` for other parameters).
#[derive(Default)]
struct FakeOnt {
    /// The `kind:key` of the objects that exist.
    exist: Vec<String>,
    /// Every call: its arguments, its input and its bound in milliseconds.
    calls: Vec<(Vec<String>, Value, u64)>,
    /// The next ways, by action (`get` for the read), one for each call, then the engine's own.
    script: BTreeMap<String, Vec<Way>>,
    /// The request ids applied, with their parameters and edits.
    applied: BTreeMap<String, (Value, Value)>,
    /// The runs and their status.
    runs: BTreeMap<String, String>,
}
impl FakeOnt {
    fn with(exist: &[&str]) -> Self {
        Self {
            exist: exist.iter().map(|e| (*e).to_owned()).collect(),
            ..Self::default()
        }
    }
    fn bodies(&self, action: &str) -> Vec<Value> {
        let name = |c: &(Vec<String>, Value, u64)| match c.0[0].as_str() {
            "action" => c.1["action_type"].as_str().unwrap_or_default().to_owned(),
            other => other.to_owned(),
        };
        let all = self.calls.iter();
        all.filter(|c| name(c) == action)
            .map(|c| c.1.clone())
            .collect()
    }
    fn answer(&mut self, args: &[String], input: &Value) -> Result<(bool, Value), ()> {
        let action = match args[0].as_str() {
            "action" => input["action_type"].as_str().unwrap_or_default().to_owned(),
            other => other.to_owned(),
        };
        let way = self
            .script
            .get_mut(&action)
            .filter(|w| !w.is_empty())
            .map(|w| w.remove(0));
        let refuse = |code: &str| Ok((false, json!({"status": "refused", "code": code})));
        if let Some(Way::Refuse(code)) = way {
            return refuse(code);
        }
        if action == "get" {
            if matches!(way, Some(Way::Lost)) {
                return Err(());
            }
            let id = &input["id"];
            let known = format!(
                "{}:{}",
                id["object_type"].as_str().unwrap(),
                id["native_key"].as_str().unwrap()
            );
            return match self.exist.contains(&known) {
                true => Ok((true, json!({"status": "ok", "result": {"objects": []}}))),
                false => refuse("unavailable"),
            };
        }
        assert_eq!(args[..4], ["action", "apply", "--client", "ap"], "{args:?}");
        let (rid, params) = (
            input["request_id"].as_str().unwrap().to_owned(),
            input["parameters"].clone(),
        );
        if let Some((before, edits)) = self.applied.get(&rid) {
            return match *before == params {
                true => Ok((
                    true,
                    json!({"status": "ok", "result": {"outcome": "already_applied", "edits": edits}}),
                )),
                false => refuse("request_id_reused"),
            };
        }
        if matches!(way, Some(Way::Lost)) {
            return Err(());
        }
        let mut edits = json!([]);
        if action == "start_run" {
            let key = format!("run-{}", self.runs.len() + 1);
            edits = json!([{"change": "initial", "id": {"object_type": "run", "native_key": key}}]);
            self.runs.insert(key, "running".into());
        } else if action == "close_run" {
            let key = params["run"]["native_key"].as_str().unwrap().to_owned();
            if self.runs.get(&key).map(String::as_str) != Some("running") {
                return refuse("run_not_running");
            }
            self.runs.insert(key, "closed".into());
        }
        self.applied.insert(rid, (params, edits.clone()));
        if matches!(way, Some(Way::Silent)) {
            return Err(());
        }
        Ok((
            true,
            json!({"status": "ok", "result": {"outcome": "applied", "edits": edits}}),
        ))
    }
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
        for (name, text) in [
            ("claude", FAKE),
            ("codex", FAKE_CODEX),
            ("muse", FAKE_MUSE),
            ("agy", FAKE_AGY),
        ] {
            let fake = t.join("bin").join(name);
            std::fs::write(&fake, text).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
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
            precreate: None,
            precreate_dir: None,
            bwrap: true,
            probes: vec![],
            launch_envs: vec![],
            policy: None,
            ont: None,
            events: vec![],
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
        if program == "ont" {
            let ont = self
                .ont
                .as_mut()
                .expect("a stage reached ont without a fake store");
            ont.calls.push((args.to_vec(), input.clone(), ms));
            let name = ont.calls.last().map(|c| match c.0[0].as_str() {
                "action" => c.1["action_type"].as_str().unwrap_or_default().to_owned(),
                other => other.to_owned(),
            });
            self.events
                .push(format!("ont {}", name.unwrap_or_default()));
            let ont = self.ont.as_mut().unwrap();
            return match ont.answer(args, input) {
                Ok((success, v)) => {
                    let bytes = serde_json::to_vec(&v).unwrap();
                    Ok(Reply {
                        success,
                        bytes: bytes.clone(),
                        stdout: bytes,
                    })
                }
                Err(()) => Err(Failure {
                    code: "process_failed",
                    field: "ont".into(),
                    detail: Value::Null,
                }),
            };
        }
        if program == BWRAP {
            self.probes.push((args.to_vec(), ms));
            self.events.push("probe".into());
            return Ok(Reply {
                success: self.bwrap,
                bytes: b"bubblewrap 0.9.0\n".to_vec(),
                stdout: b"bubblewrap 0.9.0\n".to_vec(),
            });
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
        self.launch_envs.push(env.to_vec());
        self.events.push("launch".into());
        let unit_at = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &argv[..4],
            ["systemd-run", "--scope", "--quiet", "--collect"]
        );
        assert!(argv[..unit_at].contains(&"--slice=agents.slice".to_owned()));
        // `bwrap … --` in front of the CLI (limits) is checked by the tests and dropped here.
        let at = match argv[unit_at + 1] == BWRAP {
            true => unit_at + 1 + argv[unit_at + 1..].iter().position(|a| a == "--").unwrap(),
            false => unit_at,
        };
        assert!(["claude", "codex", "muse", "agy"].contains(&argv[at + 1].as_str()));
        let unit = argv[..unit_at]
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
        self.units
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        if let Some(p) = &self.precreate {
            std::fs::write(p, b"").unwrap();
        }
        if let Some(p) = &self.precreate_dir {
            let _ = std::fs::create_dir(p);
        }
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
        "limit",
        "attempts",
        "compact_window",
        "policy",
        "run_object",
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

/// (a) "integration on the Lead's account": the account and model come from the request, also
/// for a reviewer with `integration` on the Lead's account; `integration` is a reviewer's member
/// only (another role refuses `invalid_input`); a directory whose name only begins with the Lead's
/// is another account; the account is required.
#[test]
fn a2_account_and_model_from_the_request() {
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
    let mut r = f.request("j.md");
    r["role"] = json!("implementer");
    r["integration"] = json!(true);
    assert_eq!(code(f.run(&r)), "invalid_input@integration");
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

/// A1: `/root/.claude` is an account like any other, for every role, as `account` and in
/// `next_accounts`, by its path and by an alias that resolves to it. A request that names it as
/// `account` runs there; one that names it in `next_accounts` reaches it when the first account is
/// cut before any work.
#[test]
fn a1_lead_account_is_admitted_for_every_role() {
    let mut f = Fake::new();
    // Every spelling is answered by the fake: the test must not read the machine it runs on.
    for spelling in ["/root/.claude", "/root/.claude/", "/root/./.claude"] {
        f.dirs.insert(spelling.into(), "/root/.claude".into());
    }
    f.dirs.insert(f.p("alias"), "/root/.claude".into());
    let alias = f.p("alias");
    let mut n = 0;
    for (role, integration) in [
        ("implementer", false),
        ("fixer", false),
        ("reviewer", false),
        ("reviewer", true),
    ] {
        for lead in [
            "/root/.claude",
            "/root/.claude/",
            "/root/./.claude",
            alias.as_str(),
        ] {
            // As `account`: the one launch is on it.
            n += 1;
            let out = format!("o{n}.md");
            let mut r = f.request(&out);
            r["role"] = json!(role);
            r["account"] = json!(lead);
            if integration {
                r["integration"] = json!(true);
            }
            f.reset_calls();
            let (v, status) = f.run(&r).unwrap();
            assert_eq!(
                (status, v["state"].as_str()),
                (0, Some("ok")),
                "{role} {lead}"
            );
            assert_eq!(v["account"], lead);
            assert_eq!(f.calls().len(), 1, "{role} {lead}");
            assert!(
                f.calls()[0].starts_with(&format!("{lead}|")),
                "{role} {lead}"
            );
            // In `next_accounts`: admitted, and the run reaches it when the first is cut early.
            n += 1;
            let out = format!("o{n}.md");
            let mut r = f.request(&out);
            r["role"] = json!(role);
            r["next_accounts"] = json!([lead]);
            if integration {
                r["integration"] = json!(true);
            }
            f.mode("a1", "limit1");
            f.reset_calls();
            let (v, status) = f.run(&r).unwrap();
            assert_eq!(
                (status, v["state"].as_str()),
                (0, Some("ok")),
                "{role} {lead}"
            );
            assert_eq!(v["account"], lead);
            assert_eq!(f.calls().len(), 2, "{role} {lead}");
            f.clear("a1");
        }
    }
    // `integration` is still a member of the reviewer only.
    let mut r = f.request("j.md");
    r["role"] = json!("fixer");
    r["integration"] = json!(true);
    assert_eq!(code(f.run(&r)), "invalid_input@integration");
    // A missing directory is `account_missing`, for the Lead's own account as for any other.
    let mut r = f.request("k.md");
    r["next_accounts"] = json!([f.p("no-such-account")]);
    assert_eq!(code(f.run(&r)), "account_missing@next_accounts");
}

/// A1: a reviewer changes no file, so a reviewer cut at any point, after tools ran too, is run
/// again on the next account with a new session, in the same run and its one record.
#[test]
fn a1_reviewer_cut_after_work_goes_to_the_next_account() {
    let mut f = Fake::new();
    let mut r = f.request("rv.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "limit45");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let calls = f.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].starts_with(&format!("{}|", f.p("a1"))));
    assert!(calls[1].starts_with(&format!("{}|", f.p("a2"))));
    // A new session on the next account.
    let session = |call: &str| call.split("--session-id ").nth(1).unwrap()[..36].to_owned();
    assert_ne!(session(&calls[0]), session(&calls[1]));
    assert!(!calls[1].contains("--resume"));
    // One record: the account that answered; the stream and the error file hold that run only.
    assert_eq!(v["account"], f.p("a2"));
    assert_eq!(f.record("rv.md"), v);
    assert_eq!(v["session"], session(&calls[1]).as_str());
    assert!(std::fs::metadata(f.p("rv.md")).unwrap().len() > 0);
    let stream = std::fs::read_to_string(f.p("rv.md.stream.jsonl")).unwrap();
    assert!(!stream.contains("rate_limit_event") && !stream.contains("tool_use"));
    assert!(!Path::new(&f.p("rv.md.err")).exists());
    // Every account cut: exit 75 on the last, each tried once.
    f.mode("a2", "limit45");
    f.reset_calls();
    let mut r = f.request("rv2.md");
    r["next_accounts"] = json!([f.p("a2")]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert_eq!(f.calls().len(), 2);
    assert_eq!(v["account"], f.p("a2"));
    // A resume of a cut record, which holds no answer path, stays on its account (RF1 pins the move).
    f.reset_calls();
    let resume = f.resume(r, "rv3.md", "rv2.md");
    let (_, status) = f.run(&resume).unwrap();
    assert_eq!(status, 75);
    assert_eq!(f.calls().len(), 1);
}

/// A1: an implementer or a fixer cut after a tool ran goes on at the next account (CUT1), as a
/// new session.
#[test]
fn a1_implementer_cut_after_work_goes_to_the_next_account() {
    let mut f = Fake::new();
    for (role, out) in [("implementer", "im.md"), ("fixer", "fx.md")] {
        f.reset_calls();
        let mut r = f.request(out);
        r["role"] = json!(role);
        r["next_accounts"] = json!([f.p("a2")]);
        f.mode("a1", "limit45");
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{role}");
        assert_eq!(f.calls().len(), 2, "{role}");
        assert_eq!(v["account"], f.p("a2"));
        assert!(Path::new(&f.p(out)).exists());
        assert!(!f.calls()[1].contains("--resume"));
    }
}

/// (b) cut after work, (c) resume on the recorded account; "an earlier answer under the same
/// name" becomes: an existing `out` refuses and nothing of it is touched; a failed resume leaves
/// the session resumable from its own record.
#[test]
fn a2_cut_after_work_resume_and_an_existing_out_refuses() {
    let mut f = Fake::new();
    let mut r = f.request("v.md");
    r["role"] = json!("implementer");
    // (b) a cut after work, of an implementer with no next account (with one, the call goes on
    // there: see `a1_implementer_cut_after_work_goes_to_the_next_account`): exit 75, no answer,
    // a resume record.
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
    // A resume of a cut record, which holds no answer path, stays on its account (RF1 pins the move).
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
    assert_eq!(
        code(f.run(&r)),
        "invalid_input@role",
        "reviewers run on Claude Code"
    );
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

#[test]
fn p3_a_preamble_before_the_verdict_is_no_result() {
    for (runtime, role) in [
        ("claude", "reviewer"),
        ("claude", "implementer"),
        ("claude", "fixer"),
        ("codex", "implementer"),
        ("codex", "fixer"),
    ] {
        for newline in ["\n", "\r\n"] {
            let mut f = Fake::new();
            let answer = format!(
                "I have read the change.{newline}Verdict: accept{newline}{}{newline}body «ü» \t {newline}",
                "a".repeat(40)
            );
            std::fs::write(f.t.join("bin/answer"), &answer).unwrap();
            f.mode("a1", "answer");
            let mut r = if runtime == "codex" {
                f.codex("p.md")
            } else {
                f.request("p.md")
            };
            r["role"] = json!(role);
            r["expect"] = json!({"first_line": "Verdict: *"});
            let (v, status) = f.run(&r).unwrap();
            assert_eq!(
                (status, v["state"].as_str(), v["unmet"].as_str()),
                (3, Some("no_result"), Some("first_line")),
                "{v}"
            );
            assert_eq!(std::fs::read(f.p("p.md")).unwrap(), answer.as_bytes());
            assert_eq!(v["expect_met"], false);
            assert_eq!(v["answer"]["sha256"], f.sha("bin/answer"));
            assert_eq!(v, f.record("p.md"));
        }
    }
    // A later result with a preamble does not replace an earlier matching result.
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/skeleton"), "result success\nresult success\n").unwrap();
    std::fs::write(
        f.t.join("bin/answers"),
        "Verdict: accept\\nfirst\npreamble\\nVerdict: accept\\nsecond",
    )
    .unwrap();
    f.mode("a1", "skeleton");
    let mut r = f.request("p.md");
    r["role"] = json!("implementer");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(
        std::fs::read(f.p("p.md")).unwrap(),
        b"Verdict: accept\nfirst"
    );
    assert_eq!(v["results"], 2);
}

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

// ---- runtime codex: a stage on the Codex CLI (tests/fixtures/fake-codex.sh) ------------------

/// The flags that leave out the home's configuration, hooks, rules, apps, plugins, memories and
/// further agents, for a fresh and a resumed stage alike.
const CODEX_FLAGS: [&str; 21] = [
    "--json",
    "--ignore-user-config",
    "--ignore-rules",
    "--disable",
    "hooks",
    "--disable",
    "apps",
    "--disable",
    "plugins",
    "--disable",
    "plugin_discovery_prefetch",
    "--disable",
    "skill_mcp_dependency_install",
    "--disable",
    "memories",
    "--disable",
    "multi_agent",
    "-c",
    "features.multi_agent_v2.max_concurrent_threads_per_session=1",
    "--skip-git-repo-check",
    "--dangerously-bypass-approvals-and-sandbox",
];
/// One turn's usage in the fake, as the record maps it (input without the cached and the
/// cache-write part).
fn one_turn() -> Value {
    json!({"input": 18839 - 12288 - 100, "output": 5, "cache_read": 12288, "cache_creation": 100})
}
impl Fake {
    fn codex(&self, out: &str) -> Value {
        json!({"role":"implementer","runtime":"codex","account":self.p("a1"),"model":"gpt-6.1-terra",
               "effort":"high","permission_mode":"bypassPermissions","cwd":self.p("cwd"),
               "prompt":self.p("prompt.md"),"out":self.p(out)})
    }
    /// The program and arguments of the last launch after `systemd-run … --`.
    fn last_cli(&self) -> Vec<String> {
        let launch = self.launches.last().unwrap();
        let at = launch.iter().position(|a| a == "--").unwrap();
        launch[at + 1..].to_vec()
    }
    fn env_of_last(&self) -> Vec<String> {
        let env = std::fs::read_to_string(self.t.join(format!("bin/env.{}", self.count())));
        env.unwrap().lines().map(str::to_owned).collect()
    }
}
fn codex_cli(head: &[&str], tail: &[&str]) -> Vec<String> {
    let mut a: Vec<String> = head.iter().map(|s| s.to_string()).collect();
    a.extend(CODEX_FLAGS.iter().map(|s| s.to_string()));
    a.extend(tail.iter().map(|s| s.to_string()));
    a
}

#[test]
fn x1_codex_stage_is_started_headless_without_the_user_configuration() {
    let mut f = Fake::new();
    // Nothing of the caller's environment chooses another home or another login.
    f.env
        .push(("CODEX_HOME".into(), "/elsewhere/.codex".into()));
    f.env.push(("CODEX_SQLITE_HOME".into(), "/var/x".into()));
    f.env.push(("OPENAI_API_KEY".into(), "not-a-key".into()));
    let mut r = f.codex("c.md");
    r["env"] = json!({"CARGO_TARGET_DIR": "/x/target"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let launch = f.launches.last().unwrap();
    let at = launch.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        launch[..at],
        [
            "systemd-run",
            "--scope",
            "--quiet",
            "--collect",
            v["unit"]
                .as_str()
                .map(|u| format!("--unit={u}"))
                .unwrap()
                .as_str(),
            "--slice=agents.slice",
            "-p",
            "RuntimeMaxSec=5410s",
            "-p",
            "TimeoutStopSec=10s",
        ]
    );
    let cwd = f.p("cwd");
    let want = codex_cli(
        &["codex", "--no-daemon", "exec"],
        &[
            "-m",
            "gpt-6.1-terra",
            "-c",
            "model_reasoning_effort=\"high\"",
            "-C",
            &cwd,
            "-",
        ],
    );
    assert_eq!(f.last_cli(), want);
    // The prompt on stdin, byte for byte; the home is the request's account, by CODEX_HOME only.
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.1")).unwrap(),
        b"review this\n"
    );
    assert!(f.calls()[0].starts_with(&format!("{}|--no-daemon exec --json ", f.p("a1"))));
    let env = f.env_of_last();
    let names: Vec<&str> = env.iter().filter_map(|l| l.split('=').next()).collect();
    assert!(
        env.contains(&format!("CODEX_HOME={}", f.p("a1"))),
        "{env:?}"
    );
    assert!(env.contains(&"CARGO_TARGET_DIR=/x/target".to_owned()));
    for n in &names {
        assert!(
            !n.starts_with("CLAUDE_") && !n.starts_with("HERDR_") && !n.starts_with("OPENAI_"),
            "{n}"
        );
        assert!(!n.starts_with("CODEX_") || *n == "CODEX_HOME", "{n}");
    }
    assert!(!names.contains(&"FOO"));
    assert_eq!(v["runtime"], "codex");
    // `ap` writes no file of the home.
    assert_eq!(std::fs::read_dir(f.t.join("a1")).unwrap().count(), 0);
}

#[test]
fn x1_claude_stage_arguments_are_unchanged() {
    let mut f = Fake::new();
    // A Codex stage before it leaves nothing behind in the next Claude stage.
    assert_eq!(f.run(&f.codex("c.md")).unwrap().0["state"], "ok");
    for (out, runtime) in [("a.md", None), ("b.md", Some("claude"))] {
        let mut r = f.request(out);
        if let Some(rt) = runtime {
            r["runtime"] = json!(rt);
        }
        // CODEX_ and OPENAI_ names stay a Claude stage's own business, as before.
        r["env"] = json!({"OPENAI_PROBE": "1"});
        let (v, _) = f.run(&r).unwrap();
        assert_eq!(
            (v["state"].as_str(), v["runtime"].as_str()),
            (Some("ok"), Some("claude"))
        );
        let session = v["session"].as_str().unwrap();
        let settings = r#"{"autoMemoryEnabled":false,"disableAllHooks":true,"env":{"BASH_DEFAULT_TIMEOUT_MS":"2700000","BASH_MAX_TIMEOUT_MS":"2700000","CLAUDE_CODE_DISABLE_BACKGROUND_TASKS":"1"}}"#;
        let want = [
            "claude",
            "--print",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "sonnet",
            "--effort",
            "high",
            "--permission-mode",
            "auto",
            "--permission-prompts",
            "none",
            "--setting-sources",
            "user",
            "--settings",
            settings,
            "--disallowedTools",
            "Monitor",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--session-id",
            session,
        ];
        assert_eq!(f.last_cli(), want);
        let env = f.env_of_last();
        assert!(env.contains(&format!("CLAUDE_CONFIG_DIR={}", f.p("a1"))));
        assert!(!env.iter().any(|l| l.starts_with("CODEX_")), "{env:?}");
    }
}

#[test]
fn x2_refusals_of_the_codex_runtime_write_nothing() {
    let mut f = Fake::new();
    let cases: Vec<(&str, Option<Value>, &str)> = vec![
        ("role", Some(json!("reviewer")), "invalid_input@role"),
        (
            "next_accounts",
            Some(json!([f.p("a2")])),
            "invalid_input@next_accounts",
        ),
        ("permission_mode", None, "invalid_input@permission_mode"),
        (
            "permission_mode",
            Some(json!("auto")),
            "invalid_input@permission_mode",
        ),
        (
            "permission_mode",
            Some(json!("acceptEdits")),
            "invalid_input@permission_mode",
        ),
        (
            "mcp_config",
            Some(json!(["/etc/ap-test/mcp.json"])),
            "invalid_input@mcp_config",
        ),
        ("skills", Some(json!(true)), "invalid_input@skills"),
        ("skills", Some(json!(false)), "invalid_input@skills"),
        ("memory", Some(json!(false)), "invalid_input@memory"),
        (
            "integration",
            Some(json!(false)),
            "invalid_input@integration",
        ),
        (
            "env",
            Some(json!({"CODEX_HOME": "/elsewhere/.codex"})),
            "env_forbidden@env",
        ),
        (
            "env",
            Some(json!({"CODEX_SQLITE_HOME": "/x"})),
            "env_forbidden@env",
        ),
        (
            "env",
            Some(json!({"OPENAI_API_KEY": "x"})),
            "env_forbidden@env",
        ),
        (
            "env",
            Some(json!({"CLAUDE_CONFIG_DIR": "/x"})),
            "env_forbidden@env",
        ),
        ("env", Some(json!({"HOME": "/"})), "env_forbidden@env"),
        (
            "account",
            Some(json!(f.p("missing"))),
            "account_missing@account",
        ),
        ("runtime", Some(json!("grok")), "invalid_input@runtime"),
    ];
    for (member, value, want) in cases {
        let mut r = f.codex("r.md");
        match value {
            Some(v) => r[member] = v,
            None => {
                r.as_object_mut().unwrap().remove(member);
            }
        }
        assert_eq!(code(f.run(&r)), want, "{member}");
    }
    // An empty list of next accounts or of MCP files is no member that asks for anything.
    let mut r = f.codex("ok.md");
    (r["next_accounts"], r["mcp_config"]) = (json!([]), json!([]));
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    // Nothing of the refused requests exists: no file, no call, one launch (the last request).
    let mut names: Vec<String> = std::fs::read_dir(&f.t)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "a1",
            "a2",
            "bin",
            "cwd",
            "ok.md",
            "ok.md.run.json",
            "ok.md.stream.jsonl",
            "prompt.md"
        ]
    );
    assert_eq!((f.calls().len(), f.launches.len()), (1, 1));
}

#[test]
fn x3_stream_becomes_the_record() {
    let mut f = Fake::new();
    let mut r = f.codex("s.md");
    r["expect"] = json!({"first_line": "stage ok"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(v, f.record("s.md"));
    // The answer is the last agent message of the completed turn, byte for byte.
    assert_eq!(
        std::fs::read(f.p("s.md")).unwrap(),
        b"stage ok\nsecond line"
    );
    assert_eq!(v["answer"]["sha256"], f.sha("s.md"));
    assert_eq!(v["session"], "00000000-0000-7000-8000-000000000001");
    assert_eq!(v["tokens"], one_turn());
    // reasoning, the command, two messages; one turn end; no denial on this runtime.
    assert_eq!(
        (v["turns"].as_u64(), v["results"].as_u64()),
        (Some(4), Some(1))
    );
    assert_eq!(v["permission_denials"], json!([]));
    assert_eq!(
        (v["expect_met"].as_bool(), v["unmet"].as_str()),
        (Some(true), None)
    );
    assert_eq!(
        (v["model"].as_str(), v["effort"].as_str()),
        (Some("gpt-6.1-terra"), Some("high"))
    );
    // A CLI that reports no cache-write part gives cache_creation 0.
    f.mode("a1", "nowrite");
    let (v, _) = f.run(&f.codex("n.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input": 18839 - 12288, "output": 5, "cache_read": 12288, "cache_creation": 0})
    );
}

#[test]
fn x3_session_enters_the_record_at_the_thread_start() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, status) = f.run(&f.codex("w.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    let thread = "00000000-0000-7000-8000-000000000001";
    assert_eq!(
        (seen["state"].as_str(), seen["session"].as_str()),
        (Some("running"), Some(thread))
    );
    // A turn that failed (the CLI without a login) keeps the session in the record.
    assert_eq!(
        (status, v["state"].as_str(), v["session"].as_str()),
        (3, Some("failed"), Some(thread))
    );
    // A CLI that names no thread leaves the session null.
    f.clear("a1");
    let fake = f.t.join("bin/codex");
    let text = FAKE_CODEX.replace(
        "started() { e '{\"type\":\"thread.started\",\"thread_id\":\"'\"$tid\"'\"}';",
        "started() {",
    );
    std::fs::write(&fake, text).unwrap();
    let (v, _) = f.run(&f.codex("n.md")).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["session"].as_str()),
        (Some("ok"), None)
    );
}

#[test]
fn x3_a_message_of_an_unfinished_turn_is_no_answer() {
    let mut f = Fake::new();
    for (mode, out) in [
        ("unfinished", "u.md"),
        ("msgfailed", "m.md"),
        ("hang", "h.md"),
    ] {
        f.mode("a1", mode);
        let mut r = f.codex(out);
        r["timeout_seconds"] = json!(2);
        let (v, status) = f.run(&r).unwrap();
        let want = if mode == "hang" { "timeout" } else { "failed" };
        assert_eq!((status, v["state"].as_str()), (3, Some(want)), "{mode} {v}");
        assert!(
            !Path::new(&f.p(out)).exists(),
            "{mode}: the message was written"
        );
        // A failed turn is a result that met nothing; without a turn end there is none.
        let met = if mode == "msgfailed" {
            json!(false)
        } else {
            Value::Null
        };
        assert_eq!(
            (
                v["answer"].clone(),
                v["expect_met"].clone(),
                v["results"].clone()
            ),
            (Value::Null, met, json!(u64::from(mode == "msgfailed"))),
            "{mode}"
        );
    }
}

#[test]
fn x4_failed_turn_is_failed() {
    let mut f = Fake::new();
    f.mode("a1", "failed");
    let (v, status) = f.run(&f.codex("f.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")), "{v}");
    assert_eq!(
        (v["results"].as_u64(), v["answer"].clone()),
        (Some(1), Value::Null)
    );
    assert!(!Path::new(&f.p("f.md")).exists());
    // The record keeps no error text; the stream holds it.
    let record = std::fs::read_to_string(f.p("f.md.run.json")).unwrap();
    assert!(!record.contains("not supported"), "{record}");
    let stream = std::fs::read_to_string(f.p("f.md.stream.jsonl")).unwrap();
    assert!(stream.contains("not supported"));
}

#[test]
fn x4_usage_limit_text_is_cut() {
    let mut f = Fake::new();
    for (mode, out) in [("limit", "l.md"), ("limit2", "l2.md")] {
        f.mode("a1", mode);
        let (v, status) = f.run(&f.codex(out)).unwrap();
        assert_eq!(
            (status, v["state"].as_str()),
            (75, Some("cut")),
            "{mode} {v}"
        );
        assert!(!Path::new(&f.p(out)).exists());
        assert!(
            !std::fs::read_to_string(f.p(&format!("{out}.run.json")))
                .unwrap()
                .contains("usage limit")
        );
    }
}

#[test]
fn x4_error_event_alone_decides_nothing() {
    let mut f = Fake::new();
    f.mode("a1", "errors");
    let mut r = f.codex("e.md");
    r["expect"] = json!({"first_line": "stage ok"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(std::fs::read(f.p("e.md")).unwrap(), b"stage ok");
    assert_eq!(v["results"], 1);
}

#[test]
fn x4_no_turn_end_is_timeout_or_failed() {
    let mut f = Fake::new();
    f.mode("a1", "hang");
    let mut r = f.codex("t.md");
    r["timeout_seconds"] = json!(2);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("timeout"), None),
        "{v}"
    );
    f.mode("a1", "noend");
    let (v, status) = f.run(&f.codex("n.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("result_event")),
        "{v}"
    );
    assert_eq!(
        (v["results"].as_u64(), v["turns"].clone()),
        (Some(0), Value::Null)
    );
}

#[test]
fn x4_empty_answer_is_no_result() {
    let mut f = Fake::new();
    f.mode("a1", "empty");
    let (v, status) = f.run(&f.codex("y.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("answer")),
        "{v}"
    );
    assert!(!Path::new(&f.p("y.md")).exists());
    assert_eq!(v["tokens"], one_turn());
}

#[test]
fn x5_resume_continues_the_recorded_session() {
    let mut f = Fake::new();
    let (first, _) = f.run(&f.codex("r1.md")).unwrap();
    let session = first["session"].as_str().unwrap().to_owned();
    let mut r = f.resume(f.codex("x"), "r2.md", "r1.md");
    r["role"] = json!("fixer");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let cwd = f.p("cwd");
    let want = codex_cli(
        &["codex", "--no-daemon", "exec", "resume"],
        &[
            "-m",
            "gpt-6.1-terra",
            "-c",
            "model_reasoning_effort=\"high\"",
            &session,
            "-",
        ],
    );
    assert_eq!(f.last_cli(), want);
    assert!(f.calls()[1].starts_with(&format!("{}|", f.p("a1"))));
    assert_eq!(
        (v["session"].as_str(), v["cwd"].as_str()),
        (Some(session.as_str()), Some(cwd.as_str()))
    );
    assert_eq!(v["resumes"], f.p("r1.md.run.json"));
    // The CLI reports the thread's usage so far; the record holds this run's own part.
    assert_eq!(v["tokens"], one_turn());
    let mut r = f.resume(f.codex("x"), "r3.md", "r2.md");
    r["role"] = json!("fixer");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["session"].as_str(), v["tokens"].clone()),
        (Some(session.as_str()), one_turn())
    );
    // A session the CLI would read as a flag is no session.
    let mut rec = f.record("r1.md");
    rec["session"] = json!("--last");
    std::fs::write(f.p("bad.md.run.json"), rec.to_string()).unwrap();
    let mut r = f.resume(f.codex("x"), "r4.md", "bad.md");
    r["role"] = json!("fixer");
    assert_eq!(code(f.run(&r)), "resume_record_missing@resume");
}

#[test]
fn x5_resume_of_the_other_runtime_is_refused() {
    let mut f = Fake::new();
    let mut claude = f.request("c.md");
    claude["role"] = json!("implementer");
    f.run(&claude).unwrap();
    f.run(&f.codex("x.md")).unwrap();
    let mut r = f.resume(f.codex("x"), "c2.md", "c.md");
    r["role"] = json!("fixer");
    assert_eq!(code(f.run(&r)), "resume_runtime_differs@runtime");
    let mut r = f.resume(claude.clone(), "x2.md", "x.md");
    r["role"] = json!("fixer");
    assert_eq!(code(f.run(&r)), "resume_runtime_differs@runtime");
    r["runtime"] = json!("claude");
    assert_eq!(code(f.run(&r)), "resume_runtime_differs@runtime");
    assert!(
        !Path::new(&f.p("c2.md.run.json")).exists() && !Path::new(&f.p("x2.md.run.json")).exists()
    );
}

#[test]
fn x3_a_failed_write_of_the_session_at_the_thread_start_is_a_failed_write() {
    let mut f = Fake::new();
    f.mode("a1", "blockrecord");
    let (v, status) = f.run(&f.codex("b.md")).unwrap();
    // The record could not be written when the thread started: exit 5, as for any write of the
    // record after the launch; the final record is on stdout and, once writable, in the file.
    assert_eq!(status, 5, "{v}");
    assert_eq!(v["session"], "00000000-0000-7000-8000-000000000001");
    assert_eq!(f.record("b.md"), v);
}

#[test]
fn x5_tokens_of_a_resume_whose_chain_cannot_be_followed_are_null() {
    let mut f = Fake::new();
    f.run(&f.codex("c0.md")).unwrap();
    let base = f.record("c0.md");
    // `name` resumes `earlier` (a record path), with the tokens of one turn.
    let link = |f: &Fake, name: &str, earlier: &str| {
        let mut rec = base.clone();
        rec["resumes"] = json!(f.p(earlier));
        std::fs::write(f.p(&format!("{name}.run.json")), rec.to_string()).unwrap();
    };
    let null = json!({"input": null, "output": null, "cache_read": null, "cache_creation": null});
    let resume = |f: &mut Fake, out: &str, earlier: &str| {
        let mut r = f.resume(f.codex("x"), out, earlier);
        r["role"] = json!("fixer");
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{out} {v}");
        v["tokens"].clone()
    };
    // Past the bound: 256 records before the one resumed, 257 in the chain.
    let mut earlier = "c0.md.run.json".to_owned();
    for i in 1..=256 {
        link(&f, &format!("long{i}.md"), &earlier);
        earlier = format!("long{i}.md.run.json");
    }
    assert_eq!(resume(&mut f, "r1.md", "long256.md"), null);
    // A record of the chain that cannot be read.
    link(&f, "gap.md", "missing.md.run.json");
    assert_eq!(resume(&mut f, "r2.md", "gap.md"), null);
    // A chain that returns to a record already seen.
    link(&f, "loop1.md", "loop2.md.run.json");
    link(&f, "loop2.md", "loop1.md.run.json");
    assert_eq!(resume(&mut f, "r3.md", "loop2.md"), null);
    // A chain followed to its end keeps its tokens known.
    assert_ne!(resume(&mut f, "r4.md", "c0.md"), null);
}

// ---- V1: a typed answer (`answer_schema`) --------------------------------------------------

/// The object the real CLI printed for the probe's schema (verdict, head, notes).
const OBJECT: &str = r#"{"verdict":"accept","head":"0123abcd","notes":["ok"]}"#;
/// A schema file whose text is not what a parser would print back.
const SCHEMA: &str = "{\n  \"type\": \"object\",   \"required\": [\"verdict\"],\n  \"properties\": {\"verdict\": {\"enum\": [\"accept\", \"repair\"]}}\n}\n";

impl Fake {
    /// A request with the schema file `schema.json` (written with `SCHEMA`).
    fn typed(&self, out: &str) -> Value {
        std::fs::write(self.t.join("schema.json"), SCHEMA).unwrap();
        let mut r = self.request(out);
        r["answer_schema"] = json!(self.p("schema.json"));
        r
    }
    /// The fake's `typed` mode: one result event per line (see the fixture).
    fn objects(&self, lines: &[&str], texts: Option<&[&str]>) {
        self.mode("a1", "typed");
        std::fs::write(self.t.join("bin/object"), lines.join("\n")).unwrap();
        let file = self.t.join("bin/resulttext");
        match texts {
            Some(texts) => std::fs::write(file, texts.join("\n")).unwrap(),
            None => drop(std::fs::remove_file(file)),
        }
    }
    /// The CLI's arguments of the launch `n` (from 0), without the program.
    fn cli_of(&self, n: usize) -> Vec<String> {
        let launch = &self.launches[n];
        let at = launch.iter().position(|a| a == "--").unwrap();
        launch[at + 2..].to_vec()
    }
}
fn pretty(object: &str) -> String {
    let v: Value = serde_json::from_str(object).unwrap();
    format!("{}\n", serde_json::to_string_pretty(&v).unwrap())
}
fn without_schema(mut cli: Vec<String>) -> Vec<String> {
    let at = cli.iter().position(|a| a == "--json-schema").unwrap();
    cli.drain(at..at + 2);
    cli
}

/// Point 1: a value that is no such file, and the combinations, refuse before any effect.
#[test]
fn v1_p1_an_answer_schema_that_is_not_a_json_object_file_is_refused_before_any_effect() {
    let mut f = Fake::new();
    let t = f.t.clone();
    let big = format!("{{\"description\": \"{}\"}}", "x".repeat(65_536));
    assert!(big.len() > 65_536);
    let at_limit = format!("{{\"description\": \"{}\"}}", "x".repeat(65_536 - 19));
    assert_eq!(at_limit.len(), 65_536);
    std::fs::write(t.join("big.json"), big).unwrap();
    std::fs::write(t.join("limit.json"), at_limit).unwrap();
    std::fs::write(t.join("array.json"), "[1]").unwrap();
    std::fs::write(t.join("text.json"), "not json").unwrap();
    std::fs::write(t.join("empty.json"), "").unwrap();
    std::fs::write(t.join("trail.json"), "{} x").unwrap();
    std::fs::create_dir(t.join("dir.json")).unwrap();
    std::os::unix::fs::symlink(t.join("nowhere.json"), t.join("dangling.json")).unwrap();
    let bad = [
        json!(null),
        json!(7),
        json!(""),
        json!({"type": "object"}),
        json!("schema.json"),
        json!(f.p("missing.json")),
        json!(f.p("dangling.json")),
        json!(f.p("dir.json")),
        json!(f.p("big.json")),
        json!(f.p("array.json")),
        json!(f.p("text.json")),
        json!(f.p("empty.json")),
        json!(f.p("trail.json")),
    ];
    for value in bad {
        let mut r = f.request("s.md");
        r["answer_schema"] = value.clone();
        assert_eq!(code(f.run(&r)), "invalid_input@answer_schema", "{value}");
    }
    // The limit itself is admitted.
    let mut r = f.request("ok.md");
    r["answer_schema"] = json!(f.p("limit.json"));
    f.objects(&[OBJECT], None);
    assert_eq!(f.run(&r).unwrap().1, 0);
    // The member with `expect.first_line`, with or without a `file`.
    let mut r = f.typed("s.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    assert_eq!(code(f.run(&r)), "invalid_input@answer_schema");
    r["expect"] = json!({"first_line": "Verdict: *", "file": f.p("e.md")});
    assert_eq!(code(f.run(&r)), "invalid_input@answer_schema");
    // The member with the Codex runtime.
    let mut r = f.codex("s.md");
    r["answer_schema"] = json!(f.p("schema.json"));
    assert_eq!(code(f.run(&r)), "invalid_input@answer_schema");
    // `expect.file` alone is no obstacle.
    let mut r = f.typed("file.md");
    r["expect"] = json!({"file": f.p("e.md")});
    assert_eq!(f.run(&r).unwrap().0["unmet"], "file");
    // Before any effect: one launch only (the admitted ones above), and no file of a refused out.
    assert_eq!(f.launches.len(), 2);
    assert!(!t.join("s.md.run.json").exists() && !t.join("s.md").exists());
    assert!(!t.join("s.md.stream.jsonl").exists());
}

/// Point 2: without the member nothing of a call changes: the argument list has no schema flag,
/// the record no member, and a `structured_output` in the stream is not read.
#[test]
fn v1_p2_without_the_member_nothing_changes() {
    let mut f = Fake::new();
    f.objects(&[OBJECT], Some(&["unused"]));
    let (v, status) = f.run(&f.request("n.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert!(!f.cli_of(0).iter().any(|a| a == "--json-schema"));
    assert!(v.get("answer_schema").is_none());
    // The text of the `result` member, not the object, is the answer.
    assert_eq!(std::fs::read_to_string(f.p("n.md")).unwrap(), OBJECT);
    assert_ne!(
        std::fs::read_to_string(f.p("n.md")).unwrap(),
        pretty(OBJECT)
    );
    // A pattern still applies to the text, and a result without text is still no answer.
    f.objects(&["null"], None);
    let mut r = f.request("m.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["unmet"].as_str()), (3, Some("answer")), "{v}");
}

/// Point 3: the flag and the file's text, for a first run and a resume; nothing else changes; the
/// record holds the file's path and digest from the launch on.
#[test]
fn v1_p3_the_argument_list_gains_the_schema_and_the_record_names_the_file() {
    let mut f = Fake::new();
    f.objects(&[OBJECT], None);
    // A first run without and with the member.
    let (plain, _) = f.run(&f.request("p.md")).unwrap();
    let (v, status) = f.run(&f.typed("t.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let (plain_cli, cli) = (f.cli_of(0), f.cli_of(1));
    let at = cli.iter().position(|a| a == "--json-schema").unwrap();
    assert_eq!(cli[at + 1], SCHEMA, "the file's text, as it is");
    assert_eq!(cli.len(), plain_cli.len() + 2);
    // Everything else is the same but the session of the run.
    let (mut a, mut b) = (without_schema(cli.clone()), plain_cli);
    assert_eq!((a.pop().is_some(), b.pop().is_some()), (true, true));
    assert_eq!(a, b);
    assert!(cli.contains(&"--session-id".to_owned()) && !cli.contains(&"--resume".to_owned()));
    assert_eq!(v["session"].as_str(), cli.last().map(String::as_str));
    assert_ne!(v["session"], plain["session"]);
    let want = json!({"path": f.p("schema.json"), "sha256": f.sha("schema.json")});
    assert_eq!(v["answer_schema"], want);
    assert_eq!(f.record("t.md")["answer_schema"], want);
    // A resume, without and with the member: only the two arguments differ.
    f.objects(&[OBJECT], None);
    let (r1, _) = f
        .run(&f.resume(f.request("r1.md"), "r1.md", "t.md"))
        .unwrap();
    assert_eq!(r1["state"], "ok");
    let (r2, _) = f.run(&f.resume(f.typed("r2.md"), "r2.md", "t.md")).unwrap();
    assert_eq!(
        (r2["state"].as_str(), &r2["answer_schema"]),
        (Some("ok"), &want)
    );
    let n = f.launches.len();
    let (plain_cli, cli) = (f.cli_of(n - 2), f.cli_of(n - 1));
    assert!(cli.contains(&"--resume".to_owned()) && !cli.contains(&"--session-id".to_owned()));
    let at = cli.iter().position(|a| a == "--json-schema").unwrap();
    assert_eq!(cli[at + 1], SCHEMA);
    assert_eq!(without_schema(cli), plain_cli);
    assert_eq!(r2["session"], v["session"]);
}

/// The record holds `answer_schema` while the CLI is still running (the fake copies it).
#[test]
fn v1_p3_the_running_record_names_the_schema_file() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, _) = f.run(&f.typed("w.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(seen["state"], "running");
    assert_eq!(
        seen["answer_schema"],
        json!({"path": f.p("schema.json"), "sha256": f.sha("schema.json")})
    );
    assert_eq!(v["answer_schema"], seen["answer_schema"]);
}

/// Point 4: the object is the answer: pretty-printed with one newline; the digest is the file's.
#[test]
fn v1_p4_the_answer_file_holds_the_object() {
    let mut f = Fake::new();
    // The `result` text is not the object's pretty text: the object is what is written.
    f.objects(&[OBJECT], None);
    let (v, status) = f.run(&f.typed("o.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let bytes = std::fs::read_to_string(f.p("o.md")).unwrap();
    assert_eq!(bytes, pretty(OBJECT));
    assert!(bytes.ends_with("}\n") && !bytes.ends_with("\n\n") && bytes.contains("\n  \"head\""));
    assert_eq!(
        v["answer"],
        json!({"path": f.p("o.md"), "sha256": f.sha("o.md")})
    );
    assert_eq!(v["expect_met"], true);
    assert!(v["unmet"].is_null());
    // An object with no member is an object.
    f.objects(&["{}"], None);
    let (v, status) = f.run(&f.typed("e.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(std::fs::read_to_string(f.p("e.md")).unwrap(), "{}\n");
    // No temporary file is left beside the answer.
    let left = std::fs::read_dir(&f.t).unwrap().filter_map(Result::ok);
    assert!(
        left.filter(|e| e.file_name().to_string_lossy().ends_with(".ap-tmp"))
            .count()
            == 0
    );
}

/// Point 4: among several results the last one with an object is chosen, a later one without does
/// not unmake it (an implementer runs to the exit and sees them all).
#[test]
fn v1_p4_among_several_results_the_last_with_an_object_is_chosen() {
    let mut f = Fake::new();
    let (a, b) = (
        r#"{"verdict":"accept","n":1}"#,
        r#"{"verdict":"repair","n":2}"#,
    );
    let typed = |f: &Fake, out: &str| {
        let mut r = f.typed(out);
        r["role"] = json!("implementer");
        r
    };
    f.objects(&[a, b, ""], Some(&["", "", "late prose"]));
    let (v, status) = f.run(&typed(&f, "c1.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["results"].as_u64()),
        (0, Some("ok"), Some(3)),
        "{v}"
    );
    assert_eq!(std::fs::read_to_string(f.p("c1.md")).unwrap(), pretty(b));
    f.objects(&["", a], Some(&["early prose", ""]));
    let (v, status) = f.run(&typed(&f, "c2.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(std::fs::read_to_string(f.p("c2.md")).unwrap(), pretty(a));
    // No result has an object: the last one is judged.
    f.objects(&["", "null", "[1]"], Some(&["", "", "last prose"]));
    let (v, _) = f.run(&typed(&f, "c3.md")).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str()),
        (Some("no_result"), Some("schema")),
        "{v}"
    );
    assert_eq!(std::fs::read_to_string(f.p("c3.md")).unwrap(), "last prose");
}

/// Point 5: a chosen result without an object ends `no_result` with `unmet: schema`; the text is
/// saved when it is not empty.
#[test]
fn v1_p5_a_result_without_an_object_is_no_result_schema() {
    let mut f = Fake::new();
    for (n, line) in ["", "null", "[1]", "\"text\"", "7", "true"]
        .iter()
        .enumerate()
    {
        let out = format!("m{n}.md");
        f.objects(&[line], Some(&["prose of the model"]));
        let (v, status) = f.run(&f.typed(&out)).unwrap();
        assert_eq!(
            (
                status,
                v["state"].as_str(),
                v["unmet"].as_str(),
                v["expect_met"].as_bool()
            ),
            (3, Some("no_result"), Some("schema"), Some(false)),
            "{line} {v}"
        );
        assert_eq!(
            std::fs::read_to_string(f.p(&out)).unwrap(),
            "prose of the model"
        );
        let sha = f.sha(&out);
        assert_eq!(
            v["answer"],
            json!({"path": f.p(&out), "sha256": sha}),
            "{line}"
        );
        assert_eq!(f.record(&out), v);
    }
    // Without text: no answer file and no answer in the record, the same cause.
    f.objects(&[""], None);
    let (v, status) = f.run(&f.typed("none.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("schema")),
        "{v}"
    );
    assert!(v["answer"].is_null() && !f.t.join("none.md").exists());
}

/// Point 5: an error and a cut end as today, also when an object came with them.
#[test]
fn v1_p5_an_error_and_a_cut_end_as_today() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/iserror"), "").unwrap();
    f.objects(&[OBJECT], Some(&[""]));
    let (v, status) = f.run(&f.typed("err.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].is_null()),
        (3, Some("failed"), true),
        "{v}"
    );
    assert!(!f.t.join("err.md").exists());
    // A cut: an error result with the usage-limit text (no object, as the CLI printed none).
    f.objects(
        &[""],
        Some(&["You have hit your 5-hour limit · resets 6:30pm (UTC)"]),
    );
    let (v, status) = f.run(&f.typed("cut.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")), "{v}");
}

// ---- CUT1: `limit` and `attempts` of the record, and the continuation ------------------------

/// The text a launch after a launch that worked and was cut gets before the prompt's bytes.
const CONTINUATION: &str = "A session of this stage on another account was cut by a usage limit after it had begun to work. This directory may hold its work, committed or not. First read the state it left: in a Git worktree `git status`, `git log --oneline origin/main..HEAD` and `git diff HEAD`. Keep what serves the task, do not start over, and go on until the task below is done.\n\n";
/// The event as the CLI printed it in a stage that ended ok (2026-11-15): `overageStatus` is
/// `rejected` while the limit's status is `allowed`.
const ALLOWED: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1794772800,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false,"unifiedWindows":{"five_hour":{"utilization":0.3,"resetsAt":1794772800,"status":"allowed"},"seven_day":{"utilization":0.48,"resetsAt":1795244400,"status":"allowed"}}},"uuid":"u","session_id":"s"}"#;
const FINISHED: &str = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"Verdict: accept\nok","session_id":"s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}"#;

impl Fake {
    /// A stage whose stream on `account` is `lines`, then a finished answer.
    fn stream(&self, account: &str, lines: &[&str]) {
        let mut text = lines.join("\n");
        text.push('\n');
        text.push_str(FINISHED);
        std::fs::write(self.t.join(format!("bin/raw-{account}")), text).unwrap();
        self.mode(account, "raw");
    }
    /// The record's `limit` of a stage on `account` whose stream is `lines`, then a finished answer.
    fn limit_of(&mut self, out: &str, lines: &[&str]) -> Value {
        self.stream("a1", lines);
        let (v, status) = self.run(&self.request(out)).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
        assert_eq!(v["attempts"][0]["limit"], v["limit"]);
        v["limit"].clone()
    }
    /// The number of calls the fake has had.
    fn count_now(&self) -> usize {
        std::fs::read_to_string(self.t.join("bin/count")).map_or(0, |c| c.trim().parse().unwrap())
    }
    /// What the fake's `n`-th call (counted from 1) got on stdin.
    fn input(&self, n: usize) -> Vec<u8> {
        std::fs::read(self.t.join(format!("bin/prompt.{n}"))).unwrap()
    }
    fn with_next(&self, role: &str, out: &str, next: &[&str]) -> Value {
        let mut r = self.request(out);
        r["role"] = json!(role);
        r["next_accounts"] = json!(next.iter().map(|a| self.p(a)).collect::<Vec<_>>());
        r
    }
}

#[test]
fn cut1_limit_of_an_allowed_event_with_windows_ignores_overage_status() {
    let mut f = Fake::new();
    let limit = f.limit_of("l.md", &[ALLOWED]);
    assert_eq!(
        limit,
        json!({"status": "allowed", "kind": "five_hour", "resets_at": 1794772800_u64,
            "windows": {
                "five_hour": {"status": "allowed", "utilization": 0.3, "resets_at": 1794772800_u64},
                "seven_day": {"status": "allowed", "utilization": 0.48, "resets_at": 1795244400_u64}}})
    );
    // The `overageStatus` of the event is no status and no cut.
    assert!(!limit.to_string().contains("overage") && !limit.to_string().contains("rejected"));
    // The last event of the stream is the one read: its utilization is the later one.
    let later = ALLOWED.replace("0.3", "0.5");
    let limit = f.limit_of("l2.md", &[ALLOWED, &later]);
    assert_eq!(limit["windows"]["five_hour"]["utilization"], 0.5);
}

#[test]
fn cut1_limit_of_an_event_without_the_undocumented_members_and_odd_types() {
    let mut f = Fake::new();
    let bare = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning"}}"#;
    let limit = f.limit_of("b.md", &[bare]);
    assert_eq!(
        limit,
        json!({"status": "allowed_warning", "kind": null, "resets_at": null, "windows": {}})
    );
    // Members of another shape are null, never copied: a negative or fractional `resetsAt`, a
    // text `utilization`, a window that is no object, an event with no info at all.
    let odd = r#"{"type":"rate_limit_event","rate_limit_info":{"status":7,"resetsAt":-5,"rateLimitType":false,"unifiedWindows":{"a":3,"b":{"status":null,"utilization":"high","resetsAt":1.5}}}}"#;
    let limit = f.limit_of("o.md", &[odd]);
    assert_eq!(
        limit,
        json!({"status": null, "kind": null, "resets_at": null,
            "windows": {"b": {"status": null, "utilization": null, "resets_at": null}}})
    );
    let none = r#"{"type":"rate_limit_event"}"#;
    let limit = f.limit_of("n.md", &[none]);
    assert_eq!(
        limit,
        json!({"status": null, "kind": null, "resets_at": null, "windows": {}})
    );
    // At most eight windows, taken in the order of their names, objects only.
    let names: Vec<String> = (0..10)
        .rev()
        .map(|n| format!("\"w{n}\":{{\"status\":\"allowed\"}}"))
        .collect();
    let many = format!(
        r#"{{"type":"rate_limit_event","rate_limit_info":{{"status":"allowed","unifiedWindows":{{"a":1,{}}}}}}}"#,
        names.join(",")
    );
    let limit = f.limit_of("m.md", &[&many]);
    let keys: Vec<&str> = limit["windows"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7"]);
}

#[test]
fn cut1_a_rejected_event_is_kept_when_an_allowed_one_follows() {
    let mut f = Fake::new();
    let rejected = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1794780000,"rateLimitType":"five_hour"}}"#;
    let later_rejected = rejected.replace("1794780000", "1794790000");
    let limit = f.limit_of("r.md", &[ALLOWED, rejected, ALLOWED]);
    assert_eq!(
        (limit["status"].as_str(), limit["resets_at"].as_u64()),
        (Some("rejected"), Some(1794780000))
    );
    // Of several rejected events, the last.
    let limit = f.limit_of("r2.md", &[rejected, ALLOWED, &later_rejected, ALLOWED]);
    assert_eq!(limit["resets_at"], 1794790000_u64);
}

#[test]
fn cut1_a_stream_without_the_event_has_no_limit_and_attempts_has_one_entry() {
    let mut f = Fake::new();
    f.mode("a1", "file");
    let (v, status) = f.run(&f.request("z.md")).unwrap();
    assert_eq!(status, 0, "{v}");
    assert!(v["limit"].is_null());
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    let want = [
        "account",
        "session",
        "start",
        "end",
        "state",
        "worked",
        "continued",
        "limit",
    ];
    let mut keys: Vec<&str> = attempts[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut want = want.to_vec();
    want.sort_unstable();
    assert_eq!(keys, want);
    for k in ["account", "session", "start", "end", "state"] {
        assert_eq!(attempts[0][k], v[k], "{k}");
    }
    assert_eq!(attempts[0]["worked"], true);
    assert_eq!(attempts[0]["continued"], false);
    assert!(attempts[0]["limit"].is_null());
    assert_eq!(f.record("z.md"), v);
    // A launch that ran no tool says so.
    f.mode("a1", "");
    let (v, _) = f.run(&f.request("z2.md")).unwrap();
    assert_eq!(v["attempts"][0]["worked"], false);
}

#[test]
fn cut1_while_a_launch_runs_its_entry_is_last_and_running() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, _) = f.run(&f.request("w.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert!(seen["limit"].is_null());
    let attempts = seen["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["state"], "running");
    assert!(attempts[0]["end"].is_null() && attempts[0]["limit"].is_null());
    assert_eq!(
        (
            attempts[0]["account"].clone(),
            attempts[0]["session"].clone()
        ),
        (seen["account"].clone(), seen["session"].clone())
    );
    assert_eq!(v["attempts"][0]["state"], "ok");
}

#[test]
fn cut1_attempts_of_a_move_before_any_tool_ran() {
    let mut f = Fake::new();
    f.mode("a1", "limit1");
    let (v, status) = f
        .run(&f.with_next("implementer", "mv.md", &["a2"]))
        .unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        (
            attempts[0]["account"].clone(),
            attempts[1]["account"].clone()
        ),
        (json!(f.p("a1")), json!(f.p("a2")))
    );
    assert_eq!(attempts[0]["state"], "cut");
    assert_eq!(attempts[0]["worked"], false);
    assert_eq!(attempts[0]["limit"]["status"], "rejected");
    assert_eq!(attempts[0]["limit"]["kind"], "five_hour");
    assert_ne!(attempts[0]["session"], attempts[1]["session"]);
    // No tool ran before: the next launch gets the prompt alone, and the record is the last launch's.
    assert_eq!(
        (
            attempts[0]["continued"].clone(),
            attempts[1]["continued"].clone()
        ),
        (json!(false), json!(false))
    );
    assert_eq!(f.input(2), b"review this\n");
    assert_eq!(attempts[1]["state"], "ok");
    assert!(attempts[1]["limit"].is_null() && v["limit"].is_null());
    for k in ["account", "session", "start", "end", "state"] {
        assert_eq!(attempts[1][k], v[k], "{k}");
    }
    assert_eq!(f.record("mv.md"), v);
}

#[test]
fn cut1_a_launch_after_work_and_a_cut_gets_the_continuation_for_an_implementer_and_a_fixer() {
    let mut f = Fake::new();
    for (role, out) in [("implementer", "im.md"), ("fixer", "fx.md")] {
        f.mode("a1", "limit45");
        let before = f.count_now();
        let (v, status) = f.run(&f.with_next(role, out, &["a2"])).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{role} {v}");
        // The first launch got the prompt alone; the second the text, an empty line, the prompt.
        assert_eq!(f.input(before + 1), b"review this\n");
        let want = [CONTINUATION.as_bytes(), b"review this\n"].concat();
        assert_eq!(f.input(before + 2), want, "{role}");
        let attempts = v["attempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(
            (
                attempts[0]["state"].clone(),
                attempts[0]["worked"].clone(),
                attempts[0]["continued"].clone()
            ),
            (json!("cut"), json!(true), json!(false))
        );
        assert_eq!(attempts[0]["limit"]["status"], "rejected");
        assert_eq!(attempts[1]["continued"], true);
        assert_eq!(attempts[1]["account"], f.p("a2"));
        // A new session, not a resume; the record's prompt is still the file's digest.
        assert!(!f.calls().last().unwrap().contains("--resume"));
        assert_eq!(v["prompt"]["sha256"], f.sha("prompt.md"));
        assert_eq!(v["prompt"]["path"], f.p("prompt.md"));
        assert_eq!(f.record(out), v);
    }
    // The text is one line of four sentences and one empty line.
    assert_eq!(CONTINUATION.matches("\n").count(), 2);
    assert!(CONTINUATION.ends_with(".\n\n"));
    // No file of the call but the names a call writes.
    let mut names: Vec<String> = std::fs::read_dir(&f.t)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort_unstable();
    for n in &names {
        assert!(
            ["a1", "a2", "bin", "cwd", "prompt.md"].contains(&n.as_str())
                || n.starts_with("im.md")
                || n.starts_with("fx.md"),
            "{n}"
        );
    }
    assert!(
        !names
            .iter()
            .any(|n| n.ends_with(".ap-tmp") || n.contains("continu"))
    );
}

#[test]
fn cut1_a_second_cut_goes_on_with_a_third_account_and_the_text_once() {
    let mut f = Fake::new();
    std::fs::create_dir_all(f.t.join("a3")).unwrap();
    f.mode("a1", "limit45");
    f.mode("a2", "limit45");
    let (v, status) = f
        .run(&f.with_next("implementer", "t.md", &["a2", "a3"]))
        .unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(v["account"], f.p("a3"));
    let want = [CONTINUATION.as_bytes(), b"review this\n"].concat();
    assert_eq!(f.input(1), b"review this\n");
    assert_eq!((f.input(2), f.input(3)), (want.clone(), want));
    let continued: Vec<bool> = v["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["continued"].as_bool().unwrap())
        .collect();
    assert_eq!(continued, [false, true, true]);
    // A launch that is cut before any tool, after one that worked, still hands the text on.
    f.mode("a2", "limit1");
    let (v, status) = f
        .run(&f.with_next("fixer", "t2.md", &["a2", "a3"]))
        .unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(v["attempts"].as_array().unwrap().len(), 3);
    let want = [CONTINUATION.as_bytes(), b"review this\n"].concat();
    assert_eq!((f.input(5), f.input(6)), (want.clone(), want));
    // Every account cut: the last launch gets the text and the call ends cut, with no launch more.
    f.mode("a3", "limit45");
    let (v, status) = f
        .run(&f.with_next("implementer", "t3.md", &["a2", "a3"]))
        .unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert_eq!(f.count(), "9");
    assert_eq!(v["attempts"].as_array().unwrap().len(), 3);
    assert_eq!(v["limit"]["status"], "rejected");
}

#[test]
fn cut1_no_continuation_without_a_next_account() {
    let mut f = Fake::new();
    f.mode("a1", "limit45");
    let (v, status) = f.run(&f.request("s.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert_eq!(f.count(), "1");
    assert_eq!(f.input(1), b"review this\n");
    assert_eq!(v["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(v["attempts"][0]["continued"], false);
    assert_eq!(v["limit"]["status"], "rejected");
    assert_eq!(v["attempts"][0]["limit"], v["limit"]);
}

#[test]
fn cut1_no_continuation_for_a_reviewer() {
    let mut f = Fake::new();
    f.mode("a1", "limit45");
    let (v, status) = f.run(&f.with_next("reviewer", "rv.md", &["a2"])).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(f.input(2), b"review this\n");
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(
        (
            attempts[0]["worked"].clone(),
            attempts[0]["continued"].clone(),
            attempts[1]["continued"].clone()
        ),
        (json!(true), json!(false), json!(false))
    );
}

#[test]
fn cut1_a_resumed_stage_has_no_continuation_and_its_limit_says_what_cut_it() {
    let mut f = Fake::new();
    f.mode("a1", "limit45");
    f.mode("a2", "limit45");
    let r = f.with_next("implementer", "c.md", &["a2"]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert_eq!(f.count(), "2");
    // The record's account is the last launch's and the record holds no answer path: the resume
    // runs there alone, and its input is the prompt file's bytes, the text not given.
    let (v, status) = f.run(&f.resume(r, "c2.md", "c.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
    assert_eq!(f.count(), "3");
    assert_eq!(f.input(3), b"review this\n");
    assert!(f.calls()[2].starts_with(&format!("{}|", f.p("a2"))));
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["continued"], false);
    assert_eq!(v["limit"]["status"], "rejected");
    assert_eq!(v["limit"]["kind"], "five_hour");
}

#[test]
fn cut1_a_launch_that_could_not_start_is_an_entry_of_attempts() {
    let mut f = Fake::new();
    // The first launch is cut after work and removes the program: the next launch cannot start.
    f.mode("a1", "vanish");
    let (v, status) = f.run(&f.with_next("implementer", "g.md", &["a2"])).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("launch"))
    );
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["state"], "cut");
    // It got no text, because it was never started; its entry is the record's state.
    assert_eq!(
        (
            attempts[1]["state"].clone(),
            attempts[1]["worked"].clone(),
            attempts[1]["continued"].clone()
        ),
        (json!("failed"), json!(false), json!(false))
    );
    assert!(attempts[1]["end"].is_string() && attempts[1]["limit"].is_null());
    assert_eq!(attempts[1]["account"], f.p("a2"));
    assert_eq!(f.record("g.md"), v);
}

#[test]
fn cut1_a_call_whose_files_cannot_be_opened_writes_the_record_without_the_two_members() {
    let mut f = Fake::new();
    // The stream file exists after the record was claimed: the files cannot be opened.
    f.precreate = Some(f.p("h.md.stream.jsonl"));
    let (v, status) = f.run(&f.request("h.md")).unwrap();
    assert_eq!(status, 5, "{v}");
    assert_eq!(
        v,
        json!({"state": "failed", "unmet": "launch", "account": f.p("a1")})
    );
    assert_eq!(f.record("h.md"), v);
    assert_eq!(f.count_now(), 0, "no CLI call");
}

// ---- The compaction window of a Claude stage (`compact_window`) ----------------------------------

/// The `--settings` value of launch `n` (from 0), parsed.
fn settings_at(f: &Fake, n: usize) -> Value {
    let cli = f.cli_of(n);
    let at = cli.iter().position(|a| a == "--settings").unwrap();
    serde_json::from_str(&cli[at + 1]).unwrap()
}

#[test]
fn compact_window_is_an_integer_from_100000_to_1000000() {
    let mut f = Fake::new();
    let refused = [
        json!(99_999),
        json!(1_000_001),
        json!(0),
        json!(-400_000),
        json!(400_000.0),
        json!(4e5),
        json!("400000"),
        json!(null),
        json!(true),
        json!([400_000]),
    ];
    for value in refused {
        let mut r = f.request("w.md");
        r["compact_window"] = value.clone();
        assert_eq!(code(f.run(&r)), "invalid_input@compact_window", "{value}");
    }
    // A refusal comes before any effect: no record, no launch.
    assert!(!f.t.join("w.md.run.json").exists());
    assert_eq!(f.launches.len(), 0);
    // Both bounds are admitted and reach the settings as decimal strings.
    for (n, out) in [(100_000_u64, "lo.md"), (1_000_000, "hi.md")] {
        let mut r = f.request(out);
        r["compact_window"] = json!(n);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
        assert_eq!(
            settings_at(&f, f.launches.len() - 1)["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"],
            json!(n.to_string())
        );
    }
}

#[test]
fn compact_window_starts_a_claude_stage_in_its_settings_and_nothing_else() {
    let mut f = Fake::new();
    let mut r = f.request("s.md");
    r["compact_window"] = json!(400_000);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert_eq!(
        settings_at(&f, 0),
        json!({"autoMemoryEnabled": false, "disableAllHooks": true, "env": {
            "BASH_DEFAULT_TIMEOUT_MS": "2700000", "BASH_MAX_TIMEOUT_MS": "2700000",
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "400000",
            "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"}})
    );
    let with = f.cli_of(0);
    // Without the member: the settings of today, byte for byte.
    let (v, _) = f.run(&f.request("t.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    let call = f.calls().last().unwrap().clone();
    assert!(
        call.contains(
            r#"--settings {"autoMemoryEnabled":false,"disableAllHooks":true,"env":{"BASH_DEFAULT_TIMEOUT_MS":"2700000","BASH_MAX_TIMEOUT_MS":"2700000","CLAUDE_CODE_DISABLE_BACKGROUND_TASKS":"1"}} --disallowedTools"#
        ),
        "{call}"
    );
    let without = f.cli_of(1);
    // Apart from the session id (new in each launch), the arguments differ in the settings only.
    let (with, without) = (&with[..with.len() - 1], &without[..without.len() - 1]);
    assert_eq!(with.len(), without.len());
    let differ: Vec<_> = with.iter().zip(without).filter(|(a, b)| a != b).collect();
    assert_eq!(differ.len(), 1, "{differ:?}");
    assert!(
        differ[0].0.starts_with("{\"autoMemoryEnabled\""),
        "{differ:?}"
    );
}

#[test]
fn compact_window_a_resume_and_a_next_account_get_their_own_request_value() {
    let mut f = Fake::new();
    let mut r = f.request("first.md");
    r["compact_window"] = json!(400_000);
    let (first, _) = f.run(&r).unwrap();
    assert_eq!(first["state"], "ok", "{first}");
    // The resumed launch is cut before any tool ran, so it moves to the next account as a new session.
    f.mode("a1", "limit1");
    let before = f.launches.len();
    let mut r = f.with_next("reviewer", "second.md", &["a2"]);
    r["compact_window"] = json!(600_000);
    let (v, status) = f.run(&f.resume(r, "second.md", "first.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(f.launches.len() - before, 2, "{v}");
    let (resumed, moved) = (before, before + 1);
    assert!(f.cli_of(resumed).contains(&"--resume".to_owned()));
    assert!(f.cli_of(moved).contains(&"--session-id".to_owned()));
    for n in [resumed, moved] {
        assert_eq!(
            settings_at(&f, n)["env"]["CLAUDE_CODE_AUTO_COMPACT_WINDOW"],
            "600000",
            "launch {n}"
        );
    }
}

#[test]
fn compact_window_a_codex_stage_refuses_it_before_any_effect() {
    let mut f = Fake::new();
    let mut r = f.codex("c.md");
    r["compact_window"] = json!(400_000);
    assert_eq!(code(f.run(&r)), "invalid_input@compact_window");
    assert!(!f.t.join("c.md.run.json").exists());
    assert_eq!(f.launches.len(), 0);
    // A Codex stage without the member runs as before.
    let (v, _) = f.run(&f.codex("ok.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
}

#[test]
fn compact_window_the_record_names_it_or_null_for_every_runtime() {
    let mut f = Fake::new();
    let mut r = f.request("with.md");
    r["compact_window"] = json!(600_000);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["compact_window"], 600_000);
    assert_eq!(f.record("with.md")["compact_window"], 600_000);
    let (v, _) = f.run(&f.request("none.md")).unwrap();
    assert!(v["compact_window"].is_null());
    assert!(f.record("none.md")["compact_window"].is_null());
    let (v, _) = f.run(&f.codex("cx.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert!(f.record("cx.md")["compact_window"].is_null());
}

#[test]
fn compact_window_a_launch_that_could_not_start_names_it_in_its_record() {
    let mut f = Fake::new();
    // The first launch is cut after work and removes the program: the next launch cannot start.
    f.mode("a1", "vanish");
    let mut r = f.with_next("implementer", "g.md", &["a2"]);
    r["compact_window"] = json!(400_000);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("launch")),
        "{v}"
    );
    assert_eq!(v["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(v["compact_window"], 400_000);
    assert_eq!(f.record("g.md"), v);
}

#[test]
fn compact_window_a_call_whose_files_cannot_be_opened_keeps_three_members() {
    let mut f = Fake::new();
    // The stream file exists after the record was claimed: the files cannot be opened.
    f.precreate = Some(f.p("h.md.stream.jsonl"));
    let mut r = f.request("h.md");
    r["compact_window"] = json!(400_000);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 5, "{v}");
    assert_eq!(
        v,
        json!({"state": "failed", "unmet": "launch", "account": f.p("a1")})
    );
    assert_eq!(f.record("h.md"), v);
    assert_eq!(f.count_now(), 0, "no CLI call");
}

#[test]
fn cut1_a_codex_stage_has_a_null_limit_and_one_entry() {
    let mut f = Fake::new();
    let (v, status) = f.run(&f.codex("cx.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert!(v["limit"].is_null());
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["continued"], false);
    assert!(attempts[0]["limit"].is_null());
    assert_eq!(attempts[0]["session"], v["session"]);
    assert_eq!(attempts[0]["worked"], true);
}

// ---- RF1: a resumed stage cut by a usage limit goes on as a new session on the next account -----

/// What a launch after a cut launch of a resumed call reads before the prompt's bytes: one line and
/// one empty line, `p` the resumed record's prompt path and `a` its answer path.
fn moved_text(role: &str, p: &str, a: &str) -> String {
    let mut t = format!(
        "This stage was to resume an earlier session, and a usage limit cut that session on its account. You are a new session and hold nothing of what it read or wrote: before anything else read completely the prompt it was given, the file {p}, and the answer it gave, the file {a}. The task below was written for that session: where it speaks of what you did, wrote or were told, it means that session, its answer and its prompt."
    );
    if role != "reviewer" {
        t += " This directory holds its work, committed or not. Read the state it left: in a Git worktree `git status`, `git log --oneline origin/main..HEAD` and `git diff HEAD`. Keep what serves the task, do not start over, and go on until the task below is done.";
    }
    t + "\n\n"
}
const AGAIN: &[u8] = b"go on\n";

impl Fake {
    /// A first run of `role` on `account` that ends ok: the record a resume names.
    fn first(&mut self, role: &str, account: &str, out: &str) -> Value {
        let mut r = self.request(out);
        (r["role"], r["account"]) = (json!(role), json!(self.p(account)));
        let (v, status) = self.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
        v
    }
    /// The resumed call: `role`, the prompt file `again.md`, `account` and `next` as the request's.
    fn resumed(&self, role: &str, out: &str, earlier: &str, account: &str, next: &[&str]) -> Value {
        std::fs::write(self.t.join("again.md"), AGAIN).unwrap();
        let mut r = self.with_next(role, out, next);
        (r["account"], r["prompt"]) = (json!(self.p(account)), json!(self.p("again.md")));
        self.resume(r, out, earlier)
    }
    /// The accounts the fake's calls ran on, in order.
    fn accounts(&self) -> Vec<String> {
        let dir = format!("{}/", self.t.display());
        let on = |c: &str| c.split('|').next().unwrap().replace(&dir, "");
        self.calls().iter().map(|c| on(c)).collect()
    }
    /// The record file of `out`, rewritten by `change`.
    fn tamper(&self, out: &str, change: impl FnOnce(&mut Value)) {
        let mut rec = self.record(out);
        change(&mut rec);
        std::fs::write(
            self.p(&format!("{out}.run.json")),
            serde_json::to_vec(&rec).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn rf1_the_accounts_of_a_resumed_call_are_the_records_then_the_requests_without_repetition() {
    let mut f = Fake::new();
    std::fs::create_dir_all(f.t.join("a3")).unwrap();
    // The record's account is a2: first, then the request's `account` and `next_accounts` in order.
    f.first("reviewer", "a2", "o1.md");
    for a in ["a1", "a2", "a3"] {
        f.mode(a, "limit1");
    }
    f.reset_calls();
    let r = f.resumed(
        "reviewer",
        "o2.md",
        "o1.md",
        "a1",
        &["a2", "a3", "a1", "a3"],
    );
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")), "{v}");
    assert_eq!(f.accounts(), ["a2", "a1", "a3"]);
    assert_eq!(v["attempts"].as_array().unwrap().len(), 3);
    // Whatever the order of the request.
    f.reset_calls();
    let r = f.resumed("reviewer", "o3.md", "o1.md", "a3", &["a1", "a2"]);
    f.run(&r).unwrap();
    assert_eq!(f.accounts(), ["a2", "a3", "a1"]);
    f.reset_calls();
    let r = f.resumed("reviewer", "o4.md", "o1.md", "a2", &["a3", "a1"]);
    f.run(&r).unwrap();
    assert_eq!(f.accounts(), ["a2", "a3", "a1"]);
    // The record's account must still be one of the request's.
    let r = f.resumed("reviewer", "o5.md", "o1.md", "a1", &["a3"]);
    assert_eq!(code(f.run(&r)), "resume_account_differs@account");
}

#[test]
fn rf1_a_resumed_launch_cut_goes_on_as_a_new_session_with_the_text_and_the_prompt() {
    let mut f = Fake::new();
    let cases = [
        ("implementer", "fixer", "limit45"),
        ("implementer", "fixer", "limit1"),
        ("reviewer", "reviewer", "limit45"),
        ("reviewer", "reviewer", "limit1"),
    ];
    for (n, (was, role, mode)) in cases.into_iter().enumerate() {
        let (first, out) = (format!("f{n}.md"), format!("g{n}.md"));
        f.clear("a1");
        let rec = f.first(was, "a1", &first);
        let session = rec["session"].as_str().unwrap().to_owned();
        f.mode("a1", mode);
        f.reset_calls();
        let before = f.count_now();
        let (v, status) = f
            .run(&f.resumed(role, &out, &first, "a1", &["a2"]))
            .unwrap();
        let why = format!("{role} {mode} {v}");
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{why}");
        let calls = f.calls();
        assert_eq!(calls.len(), 2, "{why}");
        // The first launch resumes the record's session on its account, with the prompt file alone.
        assert!(calls[0].starts_with(&format!("{}|", f.p("a1"))), "{why}");
        assert!(calls[0].ends_with(&format!("--resume {session}")), "{why}");
        assert_eq!(f.input(before + 1), AGAIN, "{why}");
        // The next is a new session on the next account: no resume argument.
        assert!(calls[1].starts_with(&format!("{}|", f.p("a2"))), "{why}");
        assert!(!calls[1].contains("--resume"), "{why}");
        let new = calls[1].split("--session-id ").nth(1).unwrap()[..36].to_owned();
        assert_ne!(new, session, "{why}");
        // Its input: the text (the record's two paths), an empty line, the prompt file's bytes.
        let (p, a) = (f.p("prompt.md"), f.p(&first));
        assert_eq!(
            f.input(before + 2),
            [moved_text(role, &p, &a).as_bytes(), AGAIN].concat(),
            "{why}"
        );
        // The record: one entry per launch, the first with the resumed session, the last one's members.
        let attempts = v["attempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 2, "{why}");
        assert_eq!(attempts[0]["session"], session.as_str(), "{why}");
        assert_eq!(attempts[0]["state"], "cut", "{why}");
        assert_eq!(attempts[0]["worked"], mode == "limit45", "{why}");
        assert_eq!(attempts[1]["session"], new.as_str(), "{why}");
        assert_eq!(
            (&attempts[0]["continued"], &attempts[1]["continued"]),
            (&json!(false), &json!(false))
        );
        assert_eq!(
            (&v["session"], &v["account"]),
            (&json!(new), &json!(f.p("a2"))),
            "{why}"
        );
        assert_eq!(v["resumes"], f.p(&format!("{first}.run.json")), "{why}");
        assert_eq!(
            v["prompt"],
            json!({"path": f.p("again.md"), "sha256": f.sha("again.md")})
        );
        assert_eq!(f.record(&out), v, "{why}");
        assert!(std::fs::metadata(f.p(&out)).unwrap().len() > 0, "{why}");
    }
}

#[test]
fn rf1_a_second_cut_goes_on_with_a_third_account_and_the_same_input() {
    let mut f = Fake::new();
    std::fs::create_dir_all(f.t.join("a3")).unwrap();
    for (n, (was, role, mode)) in [
        ("implementer", "fixer", "limit45"),
        ("reviewer", "reviewer", "limit1"),
    ]
    .into_iter()
    .enumerate()
    {
        let (first, out) = (format!("s{n}.md"), format!("u{n}.md"));
        for a in ["a1", "a2", "a3"] {
            f.clear(a);
        }
        let session = f.first(was, "a1", &first)["session"].clone();
        f.mode("a1", mode);
        f.mode("a2", mode);
        let before = f.count_now();
        let (v, status) = f
            .run(&f.resumed(role, &out, &first, "a1", &["a2", "a3"]))
            .unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{role} {v}");
        assert_eq!(f.count_now(), before + 3);
        let want = [
            moved_text(role, &f.p("prompt.md"), &f.p(&first)).as_bytes(),
            AGAIN,
        ]
        .concat();
        assert_eq!(f.input(before + 1), AGAIN);
        assert_eq!(
            (f.input(before + 2), f.input(before + 3)),
            (want.clone(), want)
        );
        let attempts = v["attempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 3);
        let sessions: Vec<&Value> = attempts.iter().map(|a| &a["session"]).collect();
        assert_eq!(sessions[0], &session);
        assert!(
            sessions[1] != sessions[0] && sessions[2] != sessions[1] && sessions[2] != sessions[0]
        );
        assert!(attempts.iter().all(|a| a["continued"] == false));
        assert_eq!(
            (&v["account"], &v["session"]),
            (&json!(f.p("a3")), sessions[2])
        );
        // With every account cut, the last launch's record ends the call as cut.
        f.mode("a3", mode);
        let before = f.count_now();
        let r = f.resumed(role, &format!("w{n}.md"), &first, "a1", &["a2", "a3"]);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (75, Some("cut")));
        assert_eq!(f.count_now(), before + 3);
        assert_eq!(v["attempts"].as_array().unwrap().len(), 3);
    }
}

#[test]
fn rf1_no_move_without_another_account() {
    let mut f = Fake::new();
    f.first("implementer", "a1", "n1.md");
    f.mode("a1", "limit45");
    // No `next_accounts`, one that names only the record's account, and one that repeats it.
    for (n, next) in [vec![], vec!["a1"], vec!["a1", "a1"]]
        .into_iter()
        .enumerate()
    {
        let before = f.count_now();
        let r = f.resumed("fixer", &format!("n{}.md", n + 2), "n1.md", "a1", &next);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (75, Some("cut")), "{next:?}");
        assert_eq!(f.count_now(), before + 1, "{next:?}");
        assert_eq!(f.input(before + 1), AGAIN);
        assert_eq!(v["attempts"].as_array().unwrap().len(), 1);
        assert_eq!(v["limit"]["status"], "rejected");
    }
}

#[test]
fn rf1_no_move_when_the_record_lacks_a_path_or_a_path_is_unfit() {
    let mut f = Fake::new();
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Change)> = vec![
        ("no answer", Box::new(|r| r["answer"] = Value::Null)),
        (
            "answer path missing",
            Box::new(|r| r["answer"] = json!({"sha256": "x"})),
        ),
        (
            "answer path not a string",
            Box::new(|r| r["answer"]["path"] = json!(7)),
        ),
        ("no prompt", Box::new(|r| r["prompt"] = Value::Null)),
        (
            "prompt path not a string",
            Box::new(|r| r["prompt"]["path"] = json!(["/p.md"])),
        ),
        (
            "newline",
            Box::new(|r| r["prompt"]["path"] = json!("/p\n.md")),
        ),
        (
            "escape",
            Box::new(|r| r["answer"]["path"] = json!("/a\u{1b}[0m.md")),
        ),
        (
            "delete",
            Box::new(|r| r["answer"]["path"] = json!("/a\u{7f}.md")),
        ),
        ("tab", Box::new(|r| r["prompt"]["path"] = json!("/p\t.md"))),
        (
            "out named in the prompt path",
            Box::new(|r| r["prompt"]["path"] = json!("/x/zz.md")),
        ),
        (
            "out named in the answer path",
            Box::new(|r| r["answer"]["path"] = json!("/x/zz.md.old")),
        ),
    ];
    for (n, (what, change)) in cases.into_iter().enumerate() {
        f.clear("a1");
        let first = format!("m{n}.md");
        f.first("fixer", "a1", &first);
        f.tamper(&first, change);
        f.mode("a1", "limit45");
        let before = f.count_now();
        let r = f.resumed("fixer", "zz.md", &first, "a1", &["a2"]);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (75, Some("cut")), "{what}");
        assert_eq!(f.count_now(), before + 1, "{what}");
        assert_eq!(v["attempts"].as_array().unwrap().len(), 1, "{what}");
        f.reset_calls();
        f.clear("a1");
        std::fs::remove_file(f.p("zz.md.run.json")).unwrap();
        let _ = std::fs::remove_file(f.p("zz.md.stream.jsonl"));
        let _ = std::fs::remove_file(f.p("zz.md.err"));
    }
}

#[test]
fn rf1_no_move_after_a_resumed_launch_that_did_not_end_cut() {
    let mut f = Fake::new();
    f.first("fixer", "a1", "k1.md");
    let cases = [
        ("ok", "", 0, "ok"),
        ("failed", "noout", 3, "failed"),
        ("no_result", "", 3, "no_result"),
        ("timeout", "other", 3, "timeout"),
    ];
    for (n, (what, mode, want, state)) in cases.into_iter().enumerate() {
        f.clear("a1");
        if !mode.is_empty() {
            f.mode("a1", mode);
        }
        let before = f.count_now();
        let mut r = f.resumed("fixer", &format!("k{}.md", n + 2), "k1.md", "a1", &["a2"]);
        match what {
            "no_result" => r["expect"] = json!({"first_line": "Never *"}),
            "timeout" => r["timeout_seconds"] = json!(2),
            _ => (),
        }
        let (v, status) = f.run(&r).unwrap();
        assert_eq!(
            (status, v["state"].as_str()),
            (want, Some(state)),
            "{what} {v}"
        );
        assert_eq!(f.count_now(), before + 1, "{what}");
        assert_eq!(v["attempts"].as_array().unwrap().len(), 1, "{what}");
    }
}

#[test]
fn rf1_a_moved_record_is_resumed_on_its_new_session_and_account() {
    let mut f = Fake::new();
    f.first("fixer", "a1", "r1.md");
    f.mode("a1", "limit45");
    let (v2, _) = f
        .run(&f.resumed("fixer", "r2.md", "r1.md", "a1", &["a2"]))
        .unwrap();
    assert_eq!(
        (v2["state"].as_str(), v2["account"].clone()),
        (Some("ok"), json!(f.p("a2")))
    );
    // The next resume starts on a2 with the new session, never on a1 (still at its limit).
    f.reset_calls();
    f.mode("a2", "limit45");
    f.clear("a1");
    let before = f.count_now();
    let r = f.resumed("fixer", "r3.md", "r2.md", "a1", &["a2"]);
    let (v3, status) = f.run(&r).unwrap();
    assert_eq!((status, v3["state"].as_str()), (0, Some("ok")), "{v3}");
    let calls = f.calls();
    assert_eq!(f.accounts(), ["a2", "a1"]);
    assert!(calls[0].ends_with(&format!("--resume {}", v2["session"].as_str().unwrap())));
    assert!(!calls[1].contains("--resume"));
    // The paths are those of the record the request names (r2 resumed r1 and its prompt is again.md).
    let want = [
        moved_text("fixer", &f.p("again.md"), &f.p("r2.md")).as_bytes(),
        AGAIN,
    ]
    .concat();
    assert_eq!(f.input(before + 2), want);
    assert_eq!(v3["resumes"], f.p("r2.md.run.json"));
}

#[test]
fn rf1_the_text_holds_each_path_byte_for_byte_whatever_the_other_path_holds() {
    let mut f = Fake::new();
    // An absolute path may hold the literal `{A}` that a naive substitution would replace again.
    std::fs::create_dir_all(f.t.join("{A}")).unwrap();
    std::fs::write(f.t.join("{A}/p.md"), "review this\n").unwrap();
    let mut r = f.request("pa.md");
    r["prompt"] = json!(f.p("{A}/p.md"));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    f.mode("a1", "limit1");
    let before = f.count_now();
    let r = f.resumed("reviewer", "pb.md", "pa.md", "a1", &["a2"]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let want = [
        moved_text("reviewer", &f.p("{A}/p.md"), &f.p("pa.md")).as_bytes(),
        AGAIN,
    ]
    .concat();
    assert!(String::from_utf8_lossy(&want).contains(&format!("the file {}, and", f.p("{A}/p.md"))));
    assert_eq!(f.input(before + 2), want);
}

// ---- runtime muse: a stage on the Muse Code CLI (tests/fixtures/fake-muse.sh) -----------------

const MUSE_MODEL: &str = "muse-wonder-1.0";
/// A line of `muse exec --json` in the shape of the kept real ones: an envelope whose `payload`
/// names the kind.
fn m_line(payload: Value) -> Value {
    json!({"schema_version":1,"stream":{"kind":"session","id":"5c4c0346-a668-4d09-8868-8df92c97bc38"},
           "record_type":"event","durability":"durable","payload":payload})
}
/// A `task_lifecycle` line of the task kind `kind` (`tool.read_file`, `model.meta.response`).
fn m_task(kind: &str) -> Value {
    m_line(
        json!({"command_id":"c1","kind":"task_lifecycle","task_id":"t1",
        "event":{"kind":"proposed","task_id":"t1","task_kind":kind}}),
    )
}
/// The status line that names the serving model in a `producer` facet (line 6 of the observed file).
fn m_served(model: &Value) -> Value {
    m_line(
        json!({"command_id":"c1","kind":"task_lifecycle","task_id":"t1",
        "event":{"kind":"status","task_id":"t1","message":"completed meta model stream attempt 1/10",
        "details":{"phase":"stream_succeeded","facets":[
            {"attempt":1,"error_kind":"stream_succeeded","kind":"external_attempt","operation":"model.response"},
            {"kind":"producer","detail":{"kind":"provider","model":model,"provider":"meta","request_id":"r1"}}]}}}),
    )
}
fn m_usage(calls: Value, models: Value, allowance: Value) -> Value {
    let mut p = json!({"command_id":"c1","kind":"run_usage","calls":calls,"models":models,
        "cancelled_calls":0,"children_included":false});
    if !allowance.is_null() {
        p["allowance"] = allowance;
    }
    m_line(p)
}
fn m_done(text: Value) -> Value {
    m_line(
        json!({"command_id":"c1","kind":"run_terminal","terminal":"completed","text":text,"reason":null}),
    )
}
fn m_failed(reason: Value) -> Value {
    m_line(
        json!({"command_id":"c1","kind":"run_terminal","terminal":"failed","text":"","reason":reason}),
    )
}
/// The usual lines of a completed run: a tool, the usage, the end.
fn m_run(text: &str) -> Vec<Value> {
    let models =
        json!([{"model":MUSE_MODEL,"input_tokens":100,"cached_tokens":40,"output_tokens":7}]);
    vec![
        m_task("tool.read_file"),
        m_usage(json!(3), models, Value::Null),
        m_done(json!(text)),
    ]
}
const MUSE_FLAGS: [&str; 6] = [
    "--disable-reminders",
    "--session-id",
    "",
    "--workspace",
    "",
    "--prompt-file",
];
impl Fake {
    fn muse(&self, out: &str) -> Value {
        for d in ["m1/muse", "m2/muse"] {
            std::fs::create_dir_all(self.t.join(d)).unwrap();
        }
        json!({"role":"implementer","runtime":"muse","account":self.p("m1/muse"),"model":MUSE_MODEL,
               "effort":"high","permission_mode":"bypassPermissions","cwd":self.p("cwd"),
               "prompt":self.p("prompt.md"),"out":self.p(out)})
    }
    /// The lines the fake prints for the account `m1` (one object per line).
    fn lines(&self, lines: &[Value]) {
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(self.t.join("bin/lines-m1"), text).unwrap();
    }
    fn lines_then(&self, lines: &[Value], hang: bool, exit: i32) {
        self.lines(lines);
        if hang {
            std::fs::write(self.t.join("bin/hang-m1"), "").unwrap();
        }
        std::fs::write(self.t.join("bin/exit-m1"), exit.to_string()).unwrap();
    }
    /// The arguments after `muse` that a Muse stage is started with.
    fn muse_cli(&self, session: &str, prompt: &str) -> Vec<String> {
        let (cwd, mut a) = (
            self.p("cwd"),
            vec!["muse", "exec", "--json", "--model", MUSE_MODEL],
        );
        a.extend(["--reasoning-effort", "high", "--contributor-mode", "off"]);
        a.extend(MUSE_FLAGS);
        let mut a: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        for (i, v) in [(a.len() - 4, session), (a.len() - 2, &*cwd)] {
            a[i] = v.to_owned();
        }
        a.extend([prompt.to_owned(), "--yolo".to_owned()]);
        a
    }
    /// A Muse stage that ended `ok` to resume from, with its out `first`.
    fn muse_first(&mut self, first: &str) -> Value {
        self.lines(&m_run("stage ok"));
        self.run(&self.muse(first)).unwrap().0
    }
}

#[test]
fn m1_refusals_name_their_field_and_come_before_any_effect() {
    let mut f = Fake::new();
    f.lines(&m_run("stage ok"));
    let set = |k: &str, v: Value| {
        let k = k.to_owned();
        move |r: &mut Value| r[k.as_str()] = v.clone()
    };
    type Edit = Box<dyn Fn(&mut Value)>;
    let mut cases: Vec<(&str, Edit, &str)> = vec![
        (
            "runtime",
            Box::new(set("runtime", json!("grok"))),
            "invalid_input@runtime",
        ),
        (
            "runtime",
            Box::new(set("runtime", json!(["muse"]))),
            "invalid_input@runtime",
        ),
        (
            "next_accounts",
            Box::new(set("next_accounts", json!([f.p("m2/muse")]))),
            "invalid_input@next_accounts",
        ),
        (
            "mcp_config",
            Box::new(set("mcp_config", json!([f.p("prompt.md")]))),
            "invalid_input@mcp_config",
        ),
        (
            "skills",
            Box::new(set("skills", json!(true))),
            "invalid_input@skills",
        ),
        (
            "skills",
            Box::new(set("skills", json!(false))),
            "invalid_input@skills",
        ),
        (
            "memory",
            Box::new(set("memory", json!(true))),
            "invalid_input@memory",
        ),
        (
            "integration",
            Box::new(set("integration", json!(false))),
            "invalid_input@integration",
        ),
        (
            "answer_schema",
            Box::new(set("answer_schema", json!(f.p("prompt.md")))),
            "invalid_input@answer_schema",
        ),
        (
            "compact_window",
            Box::new(set("compact_window", json!(400_000))),
            "invalid_input@compact_window",
        ),
        (
            "mode",
            Box::new(set("permission_mode", json!("auto"))),
            "invalid_input@permission_mode",
        ),
        (
            "mode",
            Box::new(set("permission_mode", json!("acceptEdits"))),
            "invalid_input@permission_mode",
        ),
        (
            "mode",
            Box::new(|r| {
                r.as_object_mut().unwrap().remove("permission_mode");
            }),
            "invalid_input@permission_mode",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("a1")))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("m1")))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("m1/muse/..")))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("m3/muse")))),
            "account_missing@account",
        ),
    ];
    for name in [
        "CLAUDE_X",
        "HERDR_X",
        "ANTHROPIC_X",
        "HOME",
        "MUSE_X",
        "MUSE_NO_AUTO_UPDATE",
        "XDG_CONFIG_HOME",
    ] {
        let env = json!({ name: "1" });
        cases.push(("env", Box::new(set("env", env)), "env_forbidden@env"));
    }
    for role in ["implementer", "fixer", "reviewer"] {
        for (what, edit, want) in &cases {
            let mut r = f.muse("x.md");
            r["role"] = json!(role);
            edit(&mut r);
            assert_eq!(code(f.run(&r)), *want, "{role} {what}");
        }
    }
    assert!(f.launches.is_empty() && f.calls().is_empty());
    assert!(!Path::new(&f.p("x.md.run.json")).exists());
    // What a Muse stage does admit: empty lists, the names of another CLI, all three roles.
    for (i, role) in ["implementer", "fixer", "reviewer"].into_iter().enumerate() {
        let mut r = f.muse(&format!("ok{i}.md"));
        r["role"] = json!(role);
        r["next_accounts"] = json!([]);
        r["mcp_config"] = json!([]);
        r["env"] = json!({"CODEX_HOME": "/x", "OPENAI_API_KEY": "k", "CARGO_TARGET_DIR": "/t"});
        let (v, status) = f.run(&r).unwrap();
        assert_eq!(
            (status, v["state"].as_str(), v["runtime"].as_str()),
            (0, Some("ok"), Some("muse")),
            "{role} {v}"
        );
        assert_eq!(
            std::fs::read(f.p(&format!("ok{i}.md"))).unwrap(),
            b"stage ok"
        );
        assert_eq!(v["role"], role);
    }
}

#[test]
fn m2_a_fresh_stage_is_started_with_exactly_these_arguments_and_this_environment() {
    let mut f = Fake::new();
    f.env.push(("XDG_CONFIG_HOME".into(), "/elsewhere".into()));
    f.env.push(("MUSE_API_KEY".into(), "not-a-key".into()));
    f.lines(&m_run("stage ok"));
    let mut r = f.muse("c.md");
    r["env"] = json!({"CARGO_TARGET_DIR": "/x/target"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let launch = f.launches.last().unwrap();
    let at = launch.iter().position(|a| a == "--").unwrap();
    assert_eq!(launch[4], format!("--unit={}", v["unit"].as_str().unwrap()));
    assert_eq!(
        launch[at - 3..at],
        ["RuntimeMaxSec=5410s", "-p", "TimeoutStopSec=10s"][..]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()[..]
    );
    let session = v["session"].as_str().unwrap();
    assert_eq!(session.len(), 36);
    assert_eq!(f.last_cli(), f.muse_cli(session, &f.p("prompt.md")));
    // The prompt reaches the CLI as the path; its standard input is the null device.
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.1")).unwrap(),
        b"review this\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.t.join("bin/stdin.1"))
            .unwrap()
            .trim(),
        "/dev/null"
    );
    // The configuration directory is the account's parent, by XDG_CONFIG_HOME alone.
    let env = f.env_of_last();
    assert!(
        env.contains(&format!("XDG_CONFIG_HOME={}", f.p("m1"))),
        "{env:?}"
    );
    assert!(env.contains(&"MUSE_NO_AUTO_UPDATE=1".to_owned()));
    assert!(env.contains(&"CARGO_TARGET_DIR=/x/target".to_owned()));
    assert_eq!(
        env.iter()
            .filter(|l| l.starts_with("XDG_CONFIG_HOME="))
            .count(),
        1
    );
    for l in &env {
        let n = l.split('=').next().unwrap();
        assert!(
            !n.starts_with("CLAUDE_") && !n.starts_with("HERDR_") && !n.starts_with("CODEX_"),
            "{n}"
        );
        assert!(!n.starts_with("MUSE_") || n == "MUSE_NO_AUTO_UPDATE", "{n}");
        assert!(n != "FOO", "{n}");
    }
    assert_eq!(v["runtime"], "muse");
    // `ap run` writes no file under the account directory, nor beside it.
    assert_eq!(std::fs::read_dir(f.t.join("m1/muse")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(f.t.join("m1")).unwrap().count(), 1);
}

#[test]
fn m2_a_resumed_stage_is_the_same_launch_with_the_recorded_session() {
    let mut f = Fake::new();
    let first = f.muse_first("r1.md");
    let session = first["session"].as_str().unwrap().to_owned();
    std::fs::write(f.p("next.md"), "continue\n").unwrap();
    f.lines(&m_run("second answer"));
    let mut r = f.resume(f.muse("x"), "r2.md", "r1.md");
    r["prompt"] = json!(f.p("next.md"));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(f.last_cli(), f.muse_cli(&session, &f.p("next.md")));
    assert_eq!(v["session"], session.as_str());
    assert_eq!(v["resumes"], f.p("r1.md.run.json").as_str());
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.2")).unwrap(),
        b"continue\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.t.join("bin/stdin.2"))
            .unwrap()
            .trim(),
        "/dev/null"
    );
    let env = f.env_of_last();
    assert!(env.contains(&format!("XDG_CONFIG_HOME={}", f.p("m1"))));
    assert!(env.contains(&"MUSE_NO_AUTO_UPDATE=1".to_owned()));
    assert!(
        !env.iter().any(|l| l.starts_with("CLAUDE_CONFIG_DIR=")),
        "{env:?}"
    );
    // The record's own rules hold: the same account, and the same working directory.
    let mut r = f.resume(f.muse("x"), "r3.md", "r1.md");
    r["account"] = json!(f.p("m2/muse"));
    assert_eq!(code(f.run(&r)), "resume_account_differs@account");
    assert_eq!(f.launches.len(), 2);
}

#[test]
fn m2_the_session_is_in_the_record_before_the_launch() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.lines(&m_run("stage ok"));
    let (v, _) = f.run(&f.muse("w.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(
        (seen["state"].as_str(), seen["runtime"].as_str()),
        (Some("running"), Some("muse"))
    );
    let session = seen["session"].as_str().unwrap();
    assert_eq!(session.len(), 36);
    assert_eq!(v["session"], session);
    assert!(
        f.calls()[0].contains(&format!(" --session-id {session} ")),
        "{:?}",
        f.calls()
    );
}

#[test]
fn m3_each_role_ends_ok_with_its_answer_in_out() {
    let mut f = Fake::new();
    for role in ["implementer", "fixer", "reviewer"] {
        let answer = format!("{role} answer\nline «ü» two");
        f.lines(&m_run(&answer));
        let mut r = f.muse(&format!("{role}.md"));
        r["role"] = json!(role);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{role} {v}");
        assert_eq!(
            std::fs::read(f.p(&format!("{role}.md"))).unwrap(),
            answer.as_bytes()
        );
        assert_eq!(v, f.record(&format!("{role}.md")));
        assert_eq!(v["answer"]["sha256"], f.sha(&format!("{role}.md")));
        assert_eq!(
            (v["results"].as_u64(), v["expect_met"].as_bool()),
            (Some(1), Some(true))
        );
        assert_eq!(v["permission_denials"], json!([]));
    }
}

#[test]
fn m3_an_empty_answer_is_no_result_and_expect_judges_the_first_line() {
    let mut f = Fake::new();
    for text in [json!(""), json!(null), json!(5)] {
        f.lines(&[m_task("tool.x"), m_done(text)]);
        let (v, status) = f
            .run(&f.muse(&format!("e{}.md", f.launches.len())))
            .unwrap();
        assert_eq!(
            (status, v["state"].as_str(), v["unmet"].as_str()),
            (3, Some("no_result"), Some("answer")),
            "{v}"
        );
        assert!(v["answer"].is_null());
    }
    assert_eq!(
        std::fs::read_dir(f.p(""))
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_str()
                .unwrap()
                .ends_with(".md"))
            .count(),
        1
    );
    f.lines(&m_run("Verdict: accept\nbody"));
    let mut r = f.muse("v1.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["expect_met"].as_bool()),
        (0, Some("ok"), Some(true))
    );
    f.lines(&m_run("Looks fine\nbody"));
    let mut r = f.muse("v2.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("first_line")),
        "{v}"
    );
    assert_eq!(std::fs::read(f.p("v2.md")).unwrap(), b"Looks fine\nbody");
}

#[test]
fn m4_a_failed_run_is_failed_and_writes_no_answer() {
    let mut f = Fake::new();
    let reason =
        "model `muse-nonexistent-9.9` does not exist or you lack access [request_id=99f7e09a]";
    f.lines_then(&[m_failed(json!(reason))], false, 1);
    let (v, status) = f.run(&f.muse("f.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), None),
        "{v}"
    );
    assert_eq!(
        (v["results"].as_u64(), v["tokens"]["input"].clone()),
        (Some(1), Value::Null)
    );
    assert!(!Path::new(&f.p("f.md")).exists());
    // A reason that is no string counts as empty: failed, not cut.
    f.lines_then(&[m_failed(json!(7))], false, 1);
    let (v, status) = f.run(&f.muse("g.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")), "{v}");
    assert!(!Path::new(&f.p("g.md")).exists());
}

#[test]
fn m4_a_failed_run_with_the_usage_limit_text_is_cut_and_never_moves() {
    let mut f = Fake::new();
    f.lines_then(
        &[
            m_task("tool.x"),
            m_failed(json!("You've hit your usage limit. Try again at 6:30 PM.")),
        ],
        false,
        1,
    );
    let (v, status) = f.run(&f.muse("c.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (75, Some("cut")), "{v}");
    assert!(!Path::new(&f.p("c.md")).exists());
    // One account: no continuation and no move.
    assert_eq!(
        (f.launches.len(), v["attempts"].as_array().unwrap().len()),
        (1, 1)
    );
    f.lines_then(&[m_failed(json!("usage limit reached"))], false, 1);
    let (_, status) = f.run(&f.muse("c2.md")).unwrap();
    assert_eq!((status, f.launches.len()), (75, 2));
}

#[test]
fn m5_no_terminal_is_failed_after_the_exit_and_timeout_after_the_bound() {
    let mut f = Fake::new();
    f.lines_then(&[m_task("tool.x")], false, 0);
    let (v, status) = f.run(&f.muse("n.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("result_event")),
        "{v}"
    );
    assert_eq!(
        (v["results"].as_u64(), v["turns"].clone()),
        (Some(0), Value::Null)
    );
    f.lines_then(&[m_task("tool.x")], true, 0);
    let mut r = f.muse("t.md");
    r["timeout_seconds"] = json!(2);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("timeout"), None),
        "{v}"
    );
}

#[test]
fn m6_two_terminals_a_reviewer_ends_at_the_first_and_the_others_take_the_last_that_meets_expect() {
    let mut f = Fake::new();
    f.lines(&[
        m_task("tool.x"),
        m_done(json!("Verdict: accept\nfirst")),
        m_done(json!("Verdict: repair\nsecond")),
    ]);
    let mut r = f.muse("rv.md");
    r["role"] = json!("reviewer");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["results"].as_u64()),
        (Some("ok"), Some(1)),
        "{v}"
    );
    assert_eq!(
        std::fs::read(f.p("rv.md")).unwrap(),
        b"Verdict: accept\nfirst"
    );
    // An implementer: the last result is the answer, unless an earlier one is the last to meet expect.
    let mut r = f.muse("im1.md");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["results"].as_u64()),
        (Some("ok"), Some(2)),
        "{v}"
    );
    assert_eq!(
        std::fs::read(f.p("im1.md")).unwrap(),
        b"Verdict: repair\nsecond"
    );
    f.lines(&[
        m_task("tool.x"),
        m_done(json!("Verdict: accept\nfirst")),
        m_done(json!("no verdict")),
    ]);
    r = f.muse("im2.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["results"].as_u64()),
        (0, Some("ok"), Some(2)),
        "{v}"
    );
    assert_eq!(
        std::fs::read(f.p("im2.md")).unwrap(),
        b"Verdict: accept\nfirst"
    );
}

#[test]
fn m6_a_cli_that_outlives_its_terminal_is_timeout_with_the_answer_kept() {
    let mut f = Fake::new();
    let mut lines = m_run("Verdict: accept\nkept");
    lines.push(m_task("tool.later"));
    f.lines_then(&lines, true, 0);
    let mut r = f.muse("h.md");
    r["timeout_seconds"] = json!(2);
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["expect_met"].as_bool()),
        (3, Some("timeout"), Some(true)),
        "{v}"
    );
    assert_eq!(
        std::fs::read(f.p("h.md")).unwrap(),
        b"Verdict: accept\nkept"
    );
    // A reviewer ends at its first terminal and does not wait for the CLI.
    let mut r = f.muse("hr.md");
    r["role"] = json!("reviewer");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
}

#[test]
fn m7_tokens_and_turns_come_from_the_last_usage_line() {
    let mut f = Fake::new();
    let models = json!([
        {"model":"a","input_tokens":100,"cached_tokens":40,"output_tokens":7,"reasoning_tokens":2},
        {"model":"b","input_tokens":10,"cached_tokens":30,"output_tokens":3,"reasoning_tokens":1}]);
    let other = json!([{"model":"a","input_tokens":1,"cached_tokens":0,"output_tokens":1}]);
    f.lines(&[
        m_task("tool.x"),
        m_usage(json!(1), other, Value::Null),
        m_usage(json!(9), models, Value::Null),
        m_done(json!("ok")),
    ]);
    let (v, _) = f.run(&f.muse("t.md")).unwrap();
    // cached above input counts input 0; the sum of `input_tokens` less `cached_tokens` is 60.
    assert_eq!(
        v["tokens"],
        json!({"input":60,"output":10,"cache_read":70,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(v["turns"], 9);
    // Members that are no numbers count 0; a `calls` that is no number is null.
    let odd = json!([{"input_tokens":"x","cached_tokens":5,"output_tokens":null},
        {"input_tokens":50,"cached_tokens":"y","output_tokens":4.5}, "not an object"]);
    f.lines(&[
        m_usage(json!("many"), odd, Value::Null),
        m_done(json!("ok")),
    ]);
    let (v, _) = f.run(&f.muse("u.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input":50,"output":0,"cache_read":5,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(v["turns"], Value::Null);
    // No usage line: all five are null, a failed run included.
    for lines in [
        vec![m_task("tool.x"), m_done(json!("ok"))],
        vec![m_failed(json!("boom"))],
    ] {
        f.lines(&lines);
        let (v, _) = f
            .run(&f.muse(&format!("n{}.md", f.launches.len())))
            .unwrap();
        assert_eq!(
            v["tokens"],
            json!({"input":null,"output":null,"cache_read":null,"cache_creation":null}),
            "{v}"
        );
        assert_eq!(
            (v["turns"].clone(), v["limit"].clone()),
            (Value::Null, Value::Null)
        );
    }
}

#[test]
fn m7_a_resume_holds_its_own_runs_numbers_as_the_cli_gives_them() {
    let mut f = Fake::new();
    f.muse_first("p1.md");
    let models =
        json!([{"model":MUSE_MODEL,"input_tokens":1000,"cached_tokens":600,"output_tokens":20}]);
    f.lines(&[
        m_task("tool.x"),
        m_usage(json!(4), models, Value::Null),
        m_done(json!("again")),
    ]);
    let (v, _) = f.run(&f.resume(f.muse("x"), "p2.md", "p1.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input":400,"output":20,"cache_read":600,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(v["turns"], 4);
}

#[test]
fn m8_limit_is_made_from_the_allowance() {
    let mut f = Fake::new();
    let models = json!([]);
    let mut limit = |allowance: Value| {
        f.lines(&[
            m_usage(json!(1), models.clone(), allowance),
            m_done(json!("ok")),
        ]);
        let (v, _) = f
            .run(&f.muse(&format!("l{}.md", f.launches.len())))
            .unwrap();
        v["limit"].clone()
    };
    let both = json!({"weekly":{"resets_at_ms":1795392000999u64,"used_percent":2},
        "window":{"resets_at_ms":1794805703000u64,"used_percent":7.5},"tier":null});
    assert_eq!(
        limit(both),
        json!({"status":null,"resets_at":null,"kind":null,"windows":{
        "weekly":{"status":null,"resets_at":1795392000u64,"utilization":0.02},
        "window":{"status":null,"resets_at":1794805703u64,"utilization":0.075}}})
    );
    let one = json!({"window":{"resets_at_ms":5000,"used_percent":100}});
    assert_eq!(
        limit(one),
        json!({"status":null,"resets_at":null,"kind":null,"windows":{
        "window":{"status":null,"resets_at":5,"utilization":1.0}}})
    );
    // A member of another shape: a window that is no object is left out; a part that is no
    // number, a negative or a fractional time is null.
    let odd = json!({"weekly":"x","window":{"resets_at_ms":-5000,"used_percent":"7"}});
    assert_eq!(
        limit(odd),
        json!({"status":null,"resets_at":null,"kind":null,"windows":{
        "window":{"status":null,"resets_at":null,"utilization":null}}})
    );
    let fraction = json!({"weekly":{"resets_at_ms":1500.5,"used_percent":0},"window":{}});
    assert_eq!(
        limit(fraction),
        json!({"status":null,"resets_at":null,"kind":null,"windows":{
        "weekly":{"status":null,"resets_at":null,"utilization":0.0},
        "window":{"status":null,"resets_at":null,"utilization":null}}})
    );
    assert_eq!(
        limit(json!({"other":{}})),
        json!({"status":null,"resets_at":null,"kind":null,"windows":{}})
    );
    // No allowance, or one that is no object: null.
    assert_eq!(limit(Value::Null), Value::Null);
    assert_eq!(limit(json!(5)), Value::Null);
}

#[test]
fn m9_work_is_seen_from_a_tool_task_only() {
    let mut f = Fake::new();
    let mut worked = |kind: Value| {
        f.lines(&[
            m_line(json!({"kind":"task_lifecycle","event":{"task_kind":kind}})),
            m_done(json!("ok")),
        ]);
        let (v, _) = f
            .run(&f.muse(&format!("w{}.md", f.launches.len())))
            .unwrap();
        v["attempts"][0]["worked"].clone()
    };
    assert_eq!(worked(json!("tool.read_file")), true);
    assert_eq!(worked(json!("tool.")), true);
    for other in [
        json!("model.meta.response"),
        json!("toolbox"),
        json!("a.tool.x"),
        json!(3),
        json!(null),
    ] {
        assert_eq!(worked(other.clone()), false, "{other}");
    }
    // Nor does another kind of line with such a member.
    f.lines(&[
        m_line(json!({"kind":"run_usage","event":{"task_kind":"tool.x"}})),
        m_done(json!("ok")),
    ]);
    let (v, _) = f.run(&f.muse("w9.md")).unwrap();
    assert_eq!(v["attempts"][0]["worked"], false);
}

#[test]
fn m10_a_line_that_names_another_serving_model_stops_the_stage() {
    let mut f = Fake::new();
    let served = format!("{MUSE_MODEL}-contributor");
    // The request's model: nothing changes.
    f.lines(&[
        m_served(&json!(MUSE_MODEL)),
        m_served(&json!(null)),
        m_done(json!("fine")),
    ]);
    let (v, status) = f.run(&f.muse("a.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (0, Some("ok"), None),
        "{v}"
    );
    // Another model: failed, `unmet: model`, no answer, wherever the line stands.
    let lines = [
        vec![m_served(&json!(served)), m_done(json!("answer"))],
        vec![m_done(json!("answer")), m_served(&json!(served))],
        vec![
            m_served(&json!(MUSE_MODEL)),
            m_done(json!("answer")),
            m_served(&json!(served)),
            m_done(json!("second")),
        ],
    ];
    for (i, lines) in lines.iter().enumerate() {
        f.lines(lines);
        let out = format!("b{i}.md");
        let (v, status) = f.run(&f.muse(&out)).unwrap();
        assert_eq!(
            (status, v["state"].as_str(), v["unmet"].as_str()),
            (3, Some("failed"), Some("model")),
            "{i} {v}"
        );
        assert!(!Path::new(&f.p(&out)).exists(), "{i}");
        assert!(v["answer"].is_null());
        assert!(
            serde_json::to_string(&v).unwrap().contains(&served),
            "{i}: the record names the model"
        );
        assert_eq!(v, f.record(&out));
    }
    // It stops the stage at once: a CLI that stays alive is not waited for to the bound.
    f.lines_then(&[m_served(&json!(served))], true, 0);
    let mut r = f.muse("c.md");
    r["timeout_seconds"] = json!(600);
    let began = Instant::now();
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (v["state"].as_str(), v["unmet"].as_str()),
        (Some("failed"), Some("model")),
        "{v}"
    );
    assert!(began.elapsed().as_secs() < 20);
}

#[test]
fn m11_a_resume_across_runtimes_is_refused_in_both_directions() {
    let mut f = Fake::new();
    let muse = f.muse_first("m.md");
    assert_eq!(muse["runtime"], "muse");
    let claude = f.run(&f.request("c.md")).unwrap().0;
    assert_eq!(claude["runtime"], "claude");
    let codex = f.run(&f.codex("x.md")).unwrap().0;
    assert_eq!(codex["runtime"], "codex");
    let before = f.launches.len();
    for (earlier, request) in [
        ("m.md", f.request("x")),
        ("m.md", f.codex("x")),
        ("c.md", f.muse("x")),
        ("x.md", f.muse("x")),
    ] {
        let r = f.resume(request, "again.md", earlier);
        assert_eq!(
            code(f.run(&r)),
            "resume_runtime_differs@runtime",
            "{earlier}"
        );
    }
    // A Claude record without the member counts as `claude`.
    let mut rec = f.record("c.md");
    rec.as_object_mut().unwrap().remove("runtime");
    std::fs::write(f.p("old.md.run.json"), rec.to_string()).unwrap();
    let r = f.resume(f.muse("x"), "again.md", "old.md");
    assert_eq!(code(f.run(&r)), "resume_runtime_differs@runtime");
    let r = f.resume(f.muse("x"), "again.md", "m.md");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    assert_eq!(f.launches.len(), before + 1);
}

#[test]
fn m12_a_model_or_effort_that_looks_like_a_flag_is_refused_for_muse_only() {
    let mut f = Fake::new();
    f.lines(&m_run("stage ok"));
    for member in ["model", "effort"] {
        for value in ["--trust-workspace", "--yolo", "-h", "-"] {
            let mut r = f.muse("x.md");
            r[member] = json!(value);
            assert_eq!(
                code(f.run(&r)),
                format!("invalid_input@{member}"),
                "{member} {value}"
            );
        }
    }
    assert!(f.launches.is_empty() && !Path::new(&f.p("x.md.run.json")).exists());
    // A value with a dash inside is still a name.
    let mut r = f.muse("y.md");
    r["model"] = json!("muse-ember-1.0");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    // The other runtimes keep their rules.
    let mut r = f.request("c.md");
    r["effort"] = json!("-x");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
}

// ---- runtime agy: a stage on the Antigravity CLI (tests/fixtures/fake-agy.sh) ------------------

const AGY_MODEL: &str = "gemini-4-hydrogen-medium";
fn g_usage(input: u64, output: u64, thinking: u64, cache: u64) -> Value {
    json!({"input_tokens":input,"output_tokens":output,"thinking_tokens":thinking,
           "cache_read_tokens":cache,"total_tokens":input + output})
}
/// The line `init` in the shape of the kept real one (`@CID@` becomes the conversation id).
fn g_init() -> Value {
    json!({"event":"init","conversation_id":"@CID@","init":{"model":AGY_MODEL,
           "cwd":"/tmp/agy-evidence","tools":["run_command","view_file"],
           "permission_mode":"always-proceed"}})
}
/// A line `step_update` of the step type `kind` (`user_input`, `agent_response`, `tool`).
fn g_step(index: u64, kind: &str, state: &str) -> Value {
    let mut step =
        json!({"conversation_id":"@CID@","step_index":index,"state":state,"step_type":kind});
    if kind == "tool" {
        step["tool_name"] = json!("run_command");
    }
    json!({"event":"step_update","step_update":step})
}
fn g_result(status: &str, response: Value, usage: Value, turns: Value) -> Value {
    json!({"event":"result","result":{"conversation_id":"@CID@","status":status,"response":response,
           "duration_seconds":2.65,"num_turns":turns,"usage":usage}})
}
/// An error result in the shape of the kept real one: `error` in place of an answer.
fn g_failure(status: &str, error: Value) -> Value {
    json!({"event":"result","result":{"conversation_id":"@CID@","status":status,"response":"",
           "error":error,"duration_seconds":0,"num_turns":0,"usage":g_usage(0, 0, 0, 0)}})
}
/// The usual lines of a stage that ran a command and answered.
fn g_run(response: &str) -> Vec<Value> {
    vec![
        g_init(),
        g_step(0, "user_input", "DONE"),
        g_step(1, "tool", "DONE"),
        g_result(
            "SUCCESS",
            json!(response),
            g_usage(100, 20, 5, 40),
            json!(1),
        ),
    ]
}
/// The names under `root` (symbolic links are not followed), sorted.
fn tree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, names: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            names.push(e.path().strip_prefix(root).unwrap().display().to_string());
            if e.file_type().unwrap().is_dir() {
                walk(&e.path(), root, names);
            }
        }
    }
    let mut names = vec![];
    walk(root, root, &mut names);
    names.sort();
    names
}
impl Fake {
    /// An Antigravity home: the login token is a link to a real file, as the host's homes are made.
    fn agy_home(&self, name: &str) -> String {
        let dir = self.t.join(name).join(".gemini/antigravity-cli");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(self.t.join("token"), "not-a-token").unwrap();
        let _ =
            std::os::unix::fs::symlink(self.t.join("token"), dir.join("antigravity-oauth-token"));
        self.p(name)
    }
    fn agy(&self, out: &str) -> Value {
        json!({"role":"reviewer","runtime":"agy","account":self.agy_home("g1"),"model":AGY_MODEL,
               "effort":"medium","permission_mode":"bypassPermissions","cwd":self.p("cwd"),
               "prompt":self.p("prompt.md"),"out":self.p(out)})
    }
    /// The lines the fake prints for the home `g1` (one object per line).
    fn agy_lines(&self, lines: &[Value]) {
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(self.t.join("bin/lines-g1"), text).unwrap();
    }
    fn agy_lines_then(&self, lines: &[Value], hang: bool, exit: i32) {
        self.agy_lines(lines);
        if hang {
            std::fs::write(self.t.join("bin/hang-g1"), "").unwrap();
        }
        std::fs::write(self.t.join("bin/exit-g1"), exit.to_string()).unwrap();
    }
    /// The arguments after `agy` that a stage is started with.
    fn agy_cli(&self, prompt: &str, bound: &str, conversation: Option<&str>) -> Vec<String> {
        let mut a = vec!["agy", "--print", prompt, "--output-format", "stream-json"];
        a.extend(["--model", AGY_MODEL, "--dangerously-skip-permissions"]);
        a.extend(["--print-timeout", bound]);
        a.extend(
            conversation
                .map(|c| ["--conversation", c])
                .into_iter()
                .flatten(),
        );
        a.iter().map(|s| s.to_string()).collect()
    }
    /// The home the stage of call `n` of the fake ran in, as its record names it.
    fn agy_home_of(&self, record: &Value) -> PathBuf {
        PathBuf::from(record["home"].as_str().unwrap())
    }
    /// The names under the account `name` that are not the stages' homes.
    fn agy_account_tree(&self, name: &str) -> Vec<String> {
        let mut names = tree(&self.t.join(name));
        names.retain(|n| n != "homes" && !n.starts_with("homes/"));
        names
    }
    /// A stage that ended `ok` to resume from, with its out `first`.
    fn agy_first(&mut self, first: &str) -> Value {
        self.agy_lines(&g_run("stage ok"));
        self.run(&self.agy(first)).unwrap().0
    }
}

#[test]
fn g1_refusals_name_their_field_and_come_before_any_effect() {
    let mut f = Fake::new();
    f.agy_lines(&g_run("stage ok"));
    let bare = f.t.join("g0");
    std::fs::create_dir_all(&bare).unwrap();
    let configured = f.agy_home("g3");
    std::fs::create_dir_all(f.t.join("g3/.gemini/config")).unwrap();
    let dangling = f.t.join("g4/.gemini/antigravity-cli");
    std::fs::create_dir_all(&dangling).unwrap();
    std::os::unix::fs::symlink(f.p("nowhere"), dangling.join("antigravity-oauth-token")).unwrap();
    let other = f.agy_home("g2");
    let set = |k: &str, v: Value| {
        let k = k.to_owned();
        move |r: &mut Value| r[k.as_str()] = v.clone()
    };
    type Edit = Box<dyn Fn(&mut Value)>;
    let mut cases: Vec<(&str, Edit, &str)> = vec![
        (
            "role",
            Box::new(set("role", json!("implementer"))),
            "invalid_input@role",
        ),
        (
            "role",
            Box::new(set("role", json!("fixer"))),
            "invalid_input@role",
        ),
        (
            "role",
            Box::new(set("role", json!("builder"))),
            "invalid_input@role",
        ),
        (
            "next",
            Box::new(set("next_accounts", json!([other]))),
            "invalid_input@next_accounts",
        ),
        (
            "mcp",
            Box::new(set("mcp_config", json!([f.p("prompt.md")]))),
            "invalid_input@mcp_config",
        ),
        (
            "skills",
            Box::new(set("skills", json!(true))),
            "invalid_input@skills",
        ),
        (
            "skills",
            Box::new(set("skills", json!(false))),
            "invalid_input@skills",
        ),
        (
            "memory",
            Box::new(set("memory", json!(true))),
            "invalid_input@memory",
        ),
        (
            "integration",
            Box::new(set("integration", json!(false))),
            "invalid_input@integration",
        ),
        (
            "schema",
            Box::new(set("answer_schema", json!(f.p("prompt.md")))),
            "invalid_input@answer_schema",
        ),
        (
            "window",
            Box::new(set("compact_window", json!(400_000))),
            "invalid_input@compact_window",
        ),
        (
            "mode",
            Box::new(set("permission_mode", json!("auto"))),
            "invalid_input@permission_mode",
        ),
        (
            "mode",
            Box::new(set("permission_mode", json!("acceptEdits"))),
            "invalid_input@permission_mode",
        ),
        (
            "mode",
            Box::new(|r| {
                r.as_object_mut().unwrap().remove("permission_mode");
            }),
            "invalid_input@permission_mode",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("g9")))),
            "account_missing@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("prompt.md")))),
            "account_missing@account",
        ),
        (
            "account",
            Box::new(set("account", json!(bare.to_str().unwrap()))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(configured))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("g4")))),
            "invalid_input@account",
        ),
        (
            "account",
            Box::new(set("account", json!(f.p("a1")))),
            "invalid_input@account",
        ),
        (
            "model",
            Box::new(set("model", json!("-x"))),
            "invalid_input@model",
        ),
        (
            "model",
            Box::new(set("model", json!("--dangerously-skip-permissions"))),
            "invalid_input@model",
        ),
        (
            "effort",
            Box::new(set("effort", json!("high"))),
            "invalid_input@effort",
        ),
        (
            "effort",
            Box::new(set("effort", json!("max"))),
            "invalid_input@effort",
        ),
        (
            "effort",
            Box::new(set("effort", json!("-x"))),
            "invalid_input@effort",
        ),
        (
            "effort",
            Box::new(set("model", json!("gemini-pro-orchestrator-g1-pro"))),
            "invalid_input@effort",
        ),
    ];
    for name in [
        "CLAUDE_X",
        "HERDR_X",
        "ANTHROPIC_X",
        "HOME",
        "AGY_X",
        "AGY_CLI_DISABLE_AUTO_UPDATE",
        "GEMINI_X",
        "GEMINI_API_KEY",
    ] {
        let env = json!({ name: "1" });
        cases.push(("env", Box::new(set("env", env)), "env_forbidden@env"));
    }
    for (what, edit, want) in &cases {
        let mut r = f.agy("x.md");
        edit(&mut r);
        assert_eq!(code(f.run(&r)), *want, "{what}");
    }
    // A prompt that cannot be one argument: above 131071 bytes, with a NUL byte, or not text.
    for (i, bytes) in [vec![b'a'; 131_072], b"a\0b".to_vec(), vec![0xff, 0xfe]]
        .iter()
        .enumerate()
    {
        std::fs::write(f.p(&format!("p{i}.md")), bytes).unwrap();
        let mut r = f.agy("x.md");
        r["prompt"] = json!(f.p(&format!("p{i}.md")));
        assert_eq!(code(f.run(&r)), "invalid_input@prompt", "prompt {i}");
    }
    assert!(f.launches.is_empty() && f.calls().is_empty());
    assert!(!Path::new(&f.p("x.md.run.json")).exists());
    // What the runtime does admit: a prompt of 131071 bytes, empty lists, the names of other CLIs.
    std::fs::write(f.p("big.md"), vec![b'a'; 131_071]).unwrap();
    let mut r = f.agy("ok.md");
    r["prompt"] = json!(f.p("big.md"));
    r["next_accounts"] = json!([]);
    r["mcp_config"] = json!([]);
    r["env"] = json!({"CODEX_HOME": "/x", "MUSE_X": "1", "XDG_CONFIG_HOME": "/y", "CARGO_HOME": "/c",
                      "GH_CONFIG_DIR": "/g", "RUSTUP_HOME": "/r"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["runtime"].as_str()),
        (0, Some("ok"), Some("agy")),
        "{v}"
    );
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.1")).unwrap().len(),
        131_071
    );
    for level in ["low", "high", "xhigh"] {
        let mut r = f.agy(&format!("l{level}.md"));
        (r["model"], r["effort"]) = (json!(format!("gemini-4-hydrogen-{level}")), json!(level));
        assert_eq!(f.run(&r).unwrap().0["state"], "ok", "{level}");
    }
}

#[test]
fn g2_a_fresh_stage_is_started_with_exactly_these_arguments_and_this_environment() {
    let mut f = Fake::new();
    f.env
        .push(("AGY_CLI_DISABLE_AUTO_UPDATE".into(), "0".into()));
    f.env.push(("GEMINI_API_KEY".into(), "not-a-key".into()));
    f.agy_lines(&g_run("stage ok"));
    f.agy_home("g1");
    let before = f.agy_account_tree("g1");
    let mut r = f.agy("c.md");
    r["env"] = json!({"CARGO_HOME": "/x/cargo"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let launch = f.launches.last().unwrap();
    let at = launch.iter().position(|a| a == "--").unwrap();
    assert_eq!(launch[4], format!("--unit={}", v["unit"].as_str().unwrap()));
    assert_eq!(
        launch[at - 3..at],
        ["RuntimeMaxSec=3310s", "-p", "TimeoutStopSec=10s"]
    );
    assert_eq!(f.last_cli(), f.agy_cli("review this\n", "3300s", None));
    // The stage's own bound is the CLI's print timeout.
    let mut r = f.agy("d.md");
    r["timeout_seconds"] = json!(90);
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    assert_eq!(f.last_cli(), f.agy_cli("review this\n", "90s", None));
    // The prompt is the argument; its standard input is the null device; the cwd is the request's.
    assert_eq!(
        std::fs::read(f.t.join("bin/prompt.1")).unwrap(),
        b"review this\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.t.join("bin/stdin.1"))
            .unwrap()
            .trim(),
        "/dev/null"
    );
    assert_eq!(
        std::fs::read_to_string(f.t.join("bin/cwd.1"))
            .unwrap()
            .trim(),
        f.p("cwd")
    );
    assert!(
        f.calls()[0].contains("|--print <prompt> --output-format "),
        "{:?}",
        f.calls()
    );
    // The home is the account, replacing the caller's `HOME`, and the CLI never updates itself.
    let env: Vec<String> = std::fs::read_to_string(f.t.join("bin/env.1"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let named = |n: &str| {
        env.iter()
            .filter(|l| l.starts_with(&format!("{n}=")))
            .cloned()
            .collect::<Vec<_>>()
    };
    // `HOME` is the home the record names: a new directory under the account, never the account.
    let home = f.agy_home_of(&v);
    assert_eq!(named("HOME"), [format!("HOME={}", home.display())]);
    assert_eq!(home.parent().unwrap(), f.t.join("g1/homes"));
    assert_eq!(home.file_name().unwrap().len(), 36);
    assert_eq!(
        named("AGY_CLI_DISABLE_AUTO_UPDATE"),
        ["AGY_CLI_DISABLE_AUTO_UPDATE=1"]
    );
    assert_eq!(named("CARGO_HOME"), ["CARGO_HOME=/x/cargo"]);
    for l in &env {
        let n = l.split('=').next().unwrap();
        assert!(
            !n.starts_with("CLAUDE_") && !n.starts_with("HERDR_") && !n.starts_with("GEMINI_"),
            "{n}"
        );
        assert!(
            !n.starts_with("AGY_") || n == "AGY_CLI_DISABLE_AUTO_UPDATE",
            "{n}"
        );
        assert!(n != "FOO" && n != "XDG_CONFIG_HOME", "{n}");
    }
    assert_eq!(v["runtime"], "agy");
    // The account is a template: `ap run` writes nothing in it but the homes, and the CLI none.
    assert_eq!(f.agy_account_tree("g1"), before);
    assert!(!f.t.join("g1/.gemini/config").exists());
}

#[test]
fn g9_a_fresh_stage_gets_a_new_home_of_three_links_and_the_record_names_it() {
    let mut f = Fake::new();
    f.agy_lines(&g_run("stage ok"));
    f.agy_home("g1");
    // The account holds the two instruction files the home links to.
    std::fs::write(f.p("g1/.gemini/AGENTS.md"), "instructions").unwrap();
    std::fs::write(f.p("g1/.gemini/GEMINI.md"), "instructions").unwrap();
    let account = f.t.join("g1");
    let (a, _) = f.run(&f.agy("h1.md")).unwrap();
    let (b, _) = f.run(&f.agy("h2.md")).unwrap();
    let (ha, hb) = (f.agy_home_of(&a), f.agy_home_of(&b));
    // Two fresh stages, two homes; each is under `<account>/homes`, mode 0700, and is the record's.
    assert_ne!(ha, hb);
    for (n, h) in [(1, &ha), (2, &hb)] {
        assert_eq!(h.parent().unwrap(), account.join("homes"));
        assert_eq!(
            std::fs::read_to_string(f.t.join(format!("bin/homemode.{n}")))
                .unwrap()
                .trim(),
            "700"
        );
        // As the CLI found it: three links, with the directories that hold them, nothing else.
        let token = account.join(".gemini/antigravity-cli/antigravity-oauth-token");
        let want = format!(
            ". d \n./.gemini d \n./.gemini/AGENTS.md l {a}/.gemini/AGENTS.md\n./.gemini/GEMINI.md l {a}/.gemini/GEMINI.md\n./.gemini/antigravity-cli d \n./.gemini/antigravity-cli/antigravity-oauth-token l {t}\n",
            a = account.display(),
            t = token.display()
        );
        let seen = std::fs::read_to_string(f.t.join(format!("bin/home.{n}"))).unwrap();
        let mut got: Vec<&str> = seen.lines().collect();
        got.sort();
        let mut want: Vec<&str> = want.lines().collect();
        want.sort();
        assert_eq!(got, want, "{seen}");
        assert_eq!(
            std::fs::read_to_string(f.t.join(format!("bin/env.{n}")))
                .unwrap()
                .lines()
                .filter(|l| l.starts_with("HOME="))
                .collect::<Vec<_>>(),
            [format!("HOME={}", h.display())]
        );
    }
    // The fake wrote its `.gemini/config` in the home it was given, and so the account has none.
    assert!(ha.join(".gemini/config/config.json").is_file());
    assert!(ha.join(".gemini/config/projects").is_dir());
    assert!(!account.join(".gemini/config").exists());
    // Another request on the same account is admitted again.
    assert_eq!(f.run(&f.agy("h3.md")).unwrap().0["state"], "ok");
    // Three stages, three homes, none removed, none made outside `<account>/homes`.
    assert_eq!(std::fs::read_dir(account.join("homes")).unwrap().count(), 3);
    assert!(!f.t.join("cwd/homes").exists() && !f.t.join("cwd/.gemini").exists());
}

#[test]
fn g2_a_resumed_stage_adds_the_conversation_and_runs_in_the_records_cwd() {
    let mut f = Fake::new();
    let first = f.agy_first("r1.md");
    let session = first["session"].as_str().unwrap().to_owned();
    assert_eq!(session.len(), 36);
    std::fs::write(f.p("next.md"), "continue\n").unwrap();
    f.agy_lines(&g_run("second answer"));
    let mut r = f.resume(f.agy("x"), "r2.md", "r1.md");
    r["prompt"] = json!(f.p("next.md"));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(
        f.last_cli(),
        f.agy_cli("continue\n", "3300s", Some(&session))
    );
    assert_eq!(v["session"], session.as_str());
    assert_eq!(v["resumes"], f.p("r1.md.run.json").as_str());
    assert_eq!(
        std::fs::read_to_string(f.t.join("bin/cwd.2"))
            .unwrap()
            .trim(),
        first["cwd"].as_str().unwrap()
    );
    let env = f.env_of_last();
    // A resume runs in the home of its record (the conversation lives there), made again by no one.
    let home = f.agy_home_of(&first);
    assert!(env.contains(&format!("HOME={}", home.display())));
    assert_eq!(f.agy_home_of(&v), home);
    assert_eq!(std::fs::read_dir(f.t.join("g1/homes")).unwrap().count(), 1);
    assert!(env.contains(&"AGY_CLI_DISABLE_AUTO_UPDATE=1".to_owned()));
    // The record's own rules hold: the same account.
    let mut r = f.resume(f.agy("x"), "r3.md", "r1.md");
    r["account"] = json!(f.agy_home("g2"));
    assert_eq!(code(f.run(&r)), "resume_account_differs@account");
    assert_eq!(f.launches.len(), 2);
}

#[test]
fn g2_a_resume_refuses_a_home_that_is_absent_no_directory_or_not_under_the_account() {
    let mut f = Fake::new();
    let first = f.agy_first("r1.md");
    let home = f.agy_home_of(&first);
    let outside = f.t.join("a1");
    let record = f.p("r1.md.run.json");
    let original = std::fs::read_to_string(&record).unwrap();
    std::fs::write(f.p("file"), "x").unwrap();
    std::fs::create_dir_all(f.t.join("g1/other")).unwrap();
    let homes = f.t.join("g1/homes");
    let bad: Vec<(&str, Option<Value>)> = vec![
        ("absent", None),
        ("null", Some(Value::Null)),
        ("number", Some(json!(3))),
        ("no directory", Some(json!(f.p("file")))),
        (
            "missing",
            Some(json!(homes.join("nowhere").to_str().unwrap())),
        ),
        ("outside", Some(json!(outside.to_str().unwrap()))),
        ("the account", Some(json!(f.p("g1")))),
        ("the homes", Some(json!(homes.to_str().unwrap()))),
        ("beside", Some(json!(f.p("g1/other")))),
        ("dots", Some(json!(format!("{}/../other", homes.display())))),
    ];
    for (what, value) in bad {
        let mut rec: Value = serde_json::from_str(&original).unwrap();
        match value {
            None => drop(rec.as_object_mut().unwrap().remove("home")),
            Some(v) => rec["home"] = v,
        }
        std::fs::write(&record, rec.to_string()).unwrap();
        let r = f.resume(f.agy("x"), "again.md", "r1.md");
        assert_eq!(code(f.run(&r)), "invalid_input@resume", "{what}");
    }
    // Nothing launched, and no home was made for the refused calls.
    assert_eq!(f.launches.len(), 1);
    assert_eq!(std::fs::read_dir(&homes).unwrap().count(), 1);
    // The home as the CLI left it (a `.gemini/config` in it) is no refusal: only that stage ran there.
    std::fs::write(&record, &original).unwrap();
    assert!(home.join(".gemini/config").is_dir());
    let r = f.resume(f.agy("x"), "again.md", "r1.md");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
}

#[test]
fn g3_the_conversation_is_in_the_record_at_once_and_stays_in_every_state() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    std::fs::write(f.t.join("bin/watch-late"), f.p("w.md.run.json")).unwrap();
    f.agy_lines(&g_run("stage ok"));
    let (v, _) = f.run(&f.agy("w.md")).unwrap();
    let read = |name: &str| -> Value {
        serde_json::from_slice(&std::fs::read(f.t.join(name)).unwrap()).unwrap()
    };
    // Before the launch the session is not known; a second after the line `init`, it is there.
    let (early, late) = (read("bin/seen.json"), read("bin/seen-late.json"));
    assert_eq!(
        (early["state"].as_str(), early["session"].is_null()),
        (Some("running"), true)
    );
    assert_eq!(
        (late["state"].as_str(), late["runtime"].as_str()),
        (Some("running"), Some("agy"))
    );
    assert_eq!(late["session"].as_str().map(str::len), Some(36));
    assert_eq!(v["session"], late["session"]);
    // Every end keeps it.
    let ends: Vec<(&str, Vec<Value>, bool, Option<&str>)> = vec![
        (
            "no_result",
            vec![
                g_init(),
                g_result("SUCCESS", json!(""), g_usage(1, 1, 1, 1), json!(1)),
            ],
            false,
            None,
        ),
        (
            "failed",
            vec![g_init(), g_failure("ERROR", json!("boom"))],
            false,
            None,
        ),
        (
            "cut",
            vec![
                g_init(),
                g_failure("ERROR", json!("You've hit your usage limit.")),
            ],
            false,
            None,
        ),
        ("failed", vec![g_init()], false, Some("result_event")),
        ("timeout", vec![g_init()], true, None),
    ];
    for (i, (state, lines, hang, unmet)) in ends.into_iter().enumerate() {
        f.agy_lines_then(&lines, hang, 0);
        let mut r = f.agy(&format!("e{i}.md"));
        r["timeout_seconds"] = json!(2);
        let (v, _) = f.run(&r).unwrap();
        assert_eq!(v["state"], state, "{i} {v}");
        if let Some(unmet) = unmet {
            assert_eq!(v["unmet"], unmet, "{i} {v}");
        }
        assert_eq!(v["session"].as_str().map(str::len), Some(36), "{i} {v}");
        assert_eq!(v, f.record(&format!("e{i}.md")));
    }
    // A failure before any conversation (the real one has an empty id and no `init`): no session.
    let mut bare = g_failure("ERROR", json!("invalid model selection"));
    bare["result"]["conversation_id"] = json!("");
    f.agy_lines_then(&[bare], false, 1);
    let (v, status) = f.run(&f.agy("n.md")).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["session"].is_null()),
        (3, Some("failed"), true),
        "{v}"
    );
}

#[test]
fn g4_a_result_with_an_answer_is_ok_and_its_response_is_saved_byte_for_byte() {
    let mut f = Fake::new();
    let answer = "Verdict: accept\nlíne «ü» \"quoted\" \\ back\ttab\r\nlast line, no newline";
    f.agy_lines(&g_run(answer));
    let (v, status) = f.run(&f.agy("x.md")).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    assert_eq!(std::fs::read(f.p("x.md")).unwrap(), answer.as_bytes());
    assert_eq!(v, f.record("x.md"));
    assert_eq!(v["answer"]["sha256"], f.sha("x.md"));
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
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
        "limit",
        "attempts",
        "compact_window",
        "home",
        "policy",
        "run_object",
    ];
    want.sort_unstable();
    // The member `home` is this runtime's own; no member `served_by`: the stream names no serving model.
    assert_eq!(keys, want);
    assert_eq!(
        (
            v["role"].as_str(),
            v["model"].as_str(),
            v["effort"].as_str()
        ),
        (Some("reviewer"), Some(AGY_MODEL), Some("medium"))
    );
    assert_eq!(
        (v["results"].as_u64(), v["expect_met"].as_bool()),
        (Some(1), Some(true))
    );
    assert_eq!(
        (v["limit"].clone(), v["permission_denials"].clone()),
        (Value::Null, json!([]))
    );
    assert_eq!(v["account"], f.p("g1").as_str());
    assert!(v["home"].as_str().unwrap().starts_with(&f.p("g1/homes/")));
    // A reviewer's stage ends at the first result.
    let mut lines = g_run("first");
    lines.push(g_result(
        "SUCCESS",
        json!("second"),
        g_usage(1, 1, 1, 1),
        json!(1),
    ));
    f.agy_lines(&lines);
    let (v, _) = f.run(&f.agy("y.md")).unwrap();
    assert_eq!(
        (v["results"].as_u64(), v["state"].as_str()),
        (Some(1), Some("ok"))
    );
    assert_eq!(std::fs::read(f.p("y.md")).unwrap(), b"first");
}

#[test]
fn g4_a_line_step_update_of_the_type_tool_shows_that_the_session_did_work() {
    let mut f = Fake::new();
    for (i, (steps, worked)) in [
        (vec![g_step(1, "tool", "ACTIVE")], true),
        (vec![g_step(1, "tool", "DONE")], true),
        (
            vec![
                g_step(1, "agent_response", "DONE"),
                g_step(2, "checkpoint", "DONE"),
            ],
            false,
        ),
        (vec![], false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut lines = vec![g_init(), g_step(0, "user_input", "DONE")];
        lines.extend(steps);
        lines.push(g_result(
            "SUCCESS",
            json!("ok"),
            g_usage(1, 1, 1, 1),
            json!(1),
        ));
        f.agy_lines(&lines);
        let (v, _) = f.run(&f.agy(&format!("w{i}.md"))).unwrap();
        assert_eq!(v["attempts"][0]["worked"], worked, "{i}");
    }
}

#[test]
fn g5_an_empty_answer_is_no_result_and_expect_judges_the_first_line() {
    let mut f = Fake::new();
    for (i, response) in [json!(""), json!(null), json!(5)].into_iter().enumerate() {
        f.agy_lines(&[
            g_init(),
            g_result("SUCCESS", response, g_usage(1, 1, 1, 1), json!(1)),
        ]);
        let (v, status) = f.run(&f.agy(&format!("e{i}.md"))).unwrap();
        assert_eq!(
            (status, v["state"].as_str(), v["unmet"].as_str()),
            (3, Some("no_result"), Some("answer")),
            "{v}"
        );
        assert!(v["answer"].is_null() && !Path::new(&f.p(&format!("e{i}.md"))).exists());
    }
    f.agy_lines(&g_run("Verdict: accept\nbody"));
    let mut r = f.agy("v1.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["expect_met"].as_bool()),
        (0, Some("ok"), Some(true))
    );
    f.agy_lines(&g_run("Looks fine\nbody"));
    let mut r = f.agy("v2.md");
    r["expect"] = json!({"first_line": "Verdict: *"});
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("no_result"), Some("first_line")),
        "{v}"
    );
    assert_eq!(std::fs::read(f.p("v2.md")).unwrap(), b"Looks fine\nbody");
}

#[test]
fn g6_an_error_result_is_failed_and_one_with_the_usage_limit_text_is_cut() {
    let mut f = Fake::new();
    let reason = "invalid model selection: model gemini-4-nonexistent-low is not recognized";
    for (i, status) in [
        "ERROR",
        "CANCELED",
        "INTERRUPTED",
        "INVALID",
        "WAITING",
        "RUNNING",
    ]
    .into_iter()
    .enumerate()
    {
        f.agy_lines_then(&[g_init(), g_failure(status, json!(reason))], false, 1);
        let (v, exit) = f.run(&f.agy(&format!("f{i}.md"))).unwrap();
        assert_eq!(
            (exit, v["state"].as_str(), v["unmet"].as_str()),
            (3, Some("failed"), None),
            "{status} {v}"
        );
        assert!(!Path::new(&f.p(&format!("f{i}.md"))).exists(), "{status}");
        assert_eq!(
            (v["results"].as_u64(), v["tokens"]["input"].clone()),
            (Some(1), json!(0))
        );
    }
    // An error that is no string counts as empty: failed, not cut. A response of an error result is not read.
    let mut odd = g_failure("ERROR", json!(7));
    odd["result"]["response"] = json!("You've hit your usage limit.");
    f.agy_lines_then(&[g_init(), odd], false, 1);
    let (v, exit) = f.run(&f.agy("g.md")).unwrap();
    assert_eq!((exit, v["state"].as_str()), (3, Some("failed")), "{v}");
    for (i, text) in [
        "You've hit your usage limit. Try again at 6:30 PM.",
        "usage limit reached",
        "You have hit your weekly limit · resets Nov 21",
    ]
    .into_iter()
    .enumerate()
    {
        f.agy_lines_then(
            &[
                g_init(),
                g_step(1, "tool", "DONE"),
                g_failure("ERROR", json!(text)),
            ],
            false,
            1,
        );
        let launches = f.launches.len();
        let (v, exit) = f.run(&f.agy(&format!("c{i}.md"))).unwrap();
        assert_eq!((exit, v["state"].as_str()), (75, Some("cut")), "{text} {v}");
        assert!(!Path::new(&f.p(&format!("c{i}.md"))).exists());
        // One account: no move; the stream holds no quota.
        assert_eq!(
            (f.launches.len(), v["attempts"].as_array().unwrap().len()),
            (launches + 1, 1)
        );
        assert_eq!(v["limit"], Value::Null);
    }
    // A finished answer is never read as a cut.
    f.agy_lines(&[
        g_init(),
        g_result(
            "SUCCESS",
            json!("You've hit your usage limit."),
            g_usage(1, 1, 1, 1),
            json!(1),
        ),
    ]);
    let (v, exit) = f.run(&f.agy("h.md")).unwrap();
    assert_eq!((exit, v["state"].as_str()), (0, Some("ok")), "{v}");
}

#[test]
fn g6_no_result_is_failed_after_the_exit_and_timeout_after_the_bound() {
    let mut f = Fake::new();
    f.agy_lines_then(&[g_init(), g_step(1, "tool", "DONE")], false, 0);
    let (v, exit) = f.run(&f.agy("n.md")).unwrap();
    assert_eq!(
        (exit, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("failed"), Some("result_event")),
        "{v}"
    );
    assert_eq!(
        (
            v["results"].as_u64(),
            v["turns"].clone(),
            v["tokens"]["input"].clone()
        ),
        (Some(0), Value::Null, Value::Null)
    );
    f.agy_lines_then(&[g_init(), g_step(1, "tool", "DONE")], true, 0);
    let mut r = f.agy("t.md");
    r["timeout_seconds"] = json!(2);
    let (v, exit) = f.run(&r).unwrap();
    assert_eq!(
        (exit, v["state"].as_str(), v["unmet"].as_str()),
        (3, Some("timeout"), None),
        "{v}"
    );
}

#[test]
fn g7_tokens_are_the_usage_of_the_result_and_turns_its_num_turns() {
    let mut f = Fake::new();
    // The lines of the real fresh run (the fake's own): 15025 input, 134 output (thinking included), 10724 read.
    let (v, _) = f.run(&f.agy("real.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input":15025,"output":134,"cache_read":10724,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(
        (v["turns"].clone(), v["state"].as_str()),
        (json!(1), Some("ok"))
    );
    // Any other member that is no number counts 0; turns is null when num_turns is no number.
    let usage = json!({"input_tokens":"x","output_tokens":7,"thinking_tokens":null,"cache_read_tokens":1.5});
    f.agy_lines(&[
        g_init(),
        g_result("SUCCESS", json!("ok"), usage, json!("two")),
    ]);
    let (v, _) = f.run(&f.agy("odd.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input":0,"output":7,"cache_read":0,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(v["turns"], Value::Null);
    f.agy_lines(&[
        g_init(),
        g_result("SUCCESS", json!("ok"), Value::Null, json!(4)),
    ]);
    let (v, _) = f.run(&f.agy("none.md")).unwrap();
    assert_eq!(
        v["tokens"],
        json!({"input":0,"output":0,"cache_read":0,"cache_creation":0}),
        "{v}"
    );
    assert_eq!(v["turns"], 4);
}

#[test]
fn g7_a_resumes_tokens_are_the_results_sums_less_every_earlier_record_of_the_chain() {
    let mut f = Fake::new();
    // The CLI sums the usage over the conversation: each result holds the totals so far.
    let step = |i: u64, o: u64, t: u64, c: u64, turns: u64, text: &str| {
        vec![
            g_init(),
            g_result("SUCCESS", json!(text), g_usage(i, o, t, c), json!(turns)),
        ]
    };
    f.agy_lines(&step(100, 20, 5, 40, 1, "one"));
    let (v1, _) = f.run(&f.agy("c1.md")).unwrap();
    assert_eq!(
        v1["tokens"],
        json!({"input":100,"output":20,"cache_read":40,"cache_creation":0}),
        "{v1}"
    );
    f.agy_lines(&step(160, 50, 9, 100, 2, "two"));
    let (v2, _) = f.run(&f.resume(f.agy("x"), "c2.md", "c1.md")).unwrap();
    assert_eq!(
        v2["tokens"],
        json!({"input":60,"output":30,"cache_read":60,"cache_creation":0}),
        "{v2}"
    );
    f.agy_lines(&step(400, 90, 30, 300, 3, "three"));
    let (v3, _) = f.run(&f.resume(f.agy("x"), "c3.md", "c2.md")).unwrap();
    assert_eq!(
        v3["tokens"],
        json!({"input":240,"output":40,"cache_read":200,"cache_creation":0}),
        "{v3}"
    );
    assert_eq!(v3["turns"], 3);
    // The three records sum to the last result's totals.
    let sum = |k: &str| {
        [&v1, &v2, &v3]
            .iter()
            .map(|v| v["tokens"][k].as_u64().unwrap())
            .sum::<u64>()
    };
    assert_eq!(
        (sum("input"), sum("output"), sum("cache_read")),
        (400, 90, 300)
    );
    // An earlier record of the chain that cannot be read: the tokens are null, the answer is kept.
    std::fs::remove_file(f.p("c1.md.run.json")).unwrap();
    f.agy_lines(&step(500, 100, 30, 350, 4, "four"));
    let (v4, status) = f.run(&f.resume(f.agy("x"), "c4.md", "c3.md")).unwrap();
    assert_eq!((status, v4["state"].as_str()), (0, Some("ok")), "{v4}");
    assert_eq!(
        v4["tokens"],
        json!({"input":null,"output":null,"cache_read":null,"cache_creation":null}),
        "{v4}"
    );
    assert_eq!(std::fs::read(f.p("c4.md")).unwrap(), b"four");
}

#[test]
fn g8_a_resume_across_runtimes_is_refused_in_both_directions() {
    let mut f = Fake::new();
    let agy = f.agy_first("a.md");
    assert_eq!(agy["runtime"], "agy");
    let claude = f.run(&f.request("c.md")).unwrap().0;
    assert_eq!(claude["runtime"], "claude");
    let before = f.launches.len();
    for (earlier, request) in [
        ("a.md", f.request("x")),
        ("a.md", f.codex("x")),
        ("a.md", f.muse("x")),
        ("c.md", f.agy("x")),
    ] {
        let r = f.resume(request, "again.md", earlier);
        assert_eq!(
            code(f.run(&r)),
            "resume_runtime_differs@runtime",
            "{earlier}"
        );
    }
    // A Claude record without the member counts as `claude`.
    let mut rec = f.record("c.md");
    rec.as_object_mut().unwrap().remove("runtime");
    std::fs::write(f.p("old.md.run.json"), rec.to_string()).unwrap();
    let r = f.resume(f.agy("x"), "again.md", "old.md");
    assert_eq!(code(f.run(&r)), "resume_runtime_differs@runtime");
    let r = f.resume(f.agy("x"), "again.md", "a.md");
    assert_eq!(f.run(&r).unwrap().0["state"], "ok");
    assert_eq!(f.launches.len(), before + 1);
}

// ---- notes of earlier verifiers on `ap run`, taken by unit AGY1 -------------------------------

#[test]
fn cut1_a_resets_at_in_float_form_is_null() {
    let mut f = Fake::new();
    let float = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1794772800.0,"unifiedWindows":{"five_hour":{"status":"allowed","utilization":0.5,"resetsAt":1794772800.0}}}}"#;
    let limit = f.limit_of("fl.md", &[float]);
    assert_eq!(
        limit,
        json!({"status": "allowed", "kind": null, "resets_at": null,
            "windows": {"five_hour": {"status": "allowed", "utilization": 0.5, "resets_at": null}}})
    );
}

#[test]
fn cut1_the_record_on_disk_while_a_second_launch_runs_holds_both_attempts() {
    let mut f = Fake::new();
    f.mode("a1", "limit45");
    f.mode("a2", "watch");
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    let (v, status) = f.run(&f.with_next("implementer", "w.md", &["a2"])).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    // The file as the second launch's CLI saw it, while the launch ran.
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(
        (seen["state"].as_str(), seen["account"].clone()),
        (Some("running"), json!(f.p("a2")))
    );
    assert!(seen["end"].is_null() && seen["answer"].is_null());
    let attempts = seen["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        (
            attempts[0]["state"].as_str(),
            attempts[0]["worked"].as_bool(),
            attempts[0]["limit"]["status"].as_str()
        ),
        (Some("cut"), Some(true), Some("rejected"))
    );
    assert!(attempts[0]["end"].is_string());
    assert_eq!(
        (
            attempts[1]["state"].as_str(),
            attempts[1]["continued"].as_bool(),
            attempts[1]["account"].clone()
        ),
        (Some("running"), Some(true), json!(f.p("a2")))
    );
    assert!(attempts[1]["end"].is_null() && attempts[1]["limit"].is_null());
    assert_eq!(seen["session"], attempts[1]["session"]);
    assert_ne!(attempts[0]["session"], attempts[1]["session"]);
}

#[test]
fn rf1_an_implementer_resumed_over_an_implementers_record_moves_with_the_six_sentences() {
    let mut f = Fake::new();
    f.first("implementer", "a1", "i1.md");
    f.mode("a1", "limit45");
    f.reset_calls();
    let before = f.count_now();
    let r = f.resumed("implementer", "i2.md", "i1.md", "a1", &["a2"]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
    let (p, a) = (f.p("prompt.md"), f.p("i1.md"));
    let text = moved_text("implementer", &p, &a);
    assert_eq!(f.input(before + 2), [text.as_bytes(), AGAIN].concat());
    // One line of six sentences (three of the move, three about the directory) and an empty line.
    assert_eq!(
        (text.matches(". ").count(), text.matches('\n').count()),
        (5, 2)
    );
    assert!(text.ends_with("is done.\n\n") && text.contains("This directory holds its work"));
    assert_eq!(f.accounts(), ["a1", "a2"]);
}

// ---- The host's policy (specs/ap/policy.md) --------------------------------------------------

impl Fake {
    /// Writes `policy.json` and names it as the host's policy file.
    fn policy_is(&mut self, v: &Value) -> String {
        self.policy_text(&serde_json::to_string(v).unwrap())
    }
    fn policy_text(&mut self, text: &str) -> String {
        std::fs::write(self.p("policy.json"), text).unwrap();
        self.policy = Some(self.t.join("policy.json"));
        self.p("policy.json")
    }
    /// A policy that allows the tests' models and both accounts to the runtime `claude`.
    fn open_policy(&self) -> Value {
        json!({"models": {"allowed": ["sonnet", "haiku", "opus"], "never": []},
               "accounts": {"claude": [self.p("a1"), self.p("a2")]}})
    }
    /// Nothing started, claimed or written by a refused request on `out`.
    fn untouched(&self, out: &str) {
        assert!(self.launches.is_empty() && self.git_calls.is_empty() && self.calls().is_empty());
        for suffix in ["", ".run.json", ".stream.jsonl", ".err"] {
            assert!(
                !Path::new(&self.p(&format!("{out}{suffix}"))).exists(),
                "{suffix}"
            );
        }
    }
}

// spec: specs/ap/policy.md#R2
#[test]
fn policy_without_a_file_the_record_says_null_and_a_missing_file_is_no_policy() {
    let mut f = Fake::new();
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!((status, v["policy"].clone()), (0, Value::Null), "{v}");
    f.policy = Some(f.t.join("absent.json"));
    let (v, _) = f.run(&f.request("y.md")).unwrap();
    assert_eq!(v["policy"], Value::Null);
    assert_eq!(f.record("y.md")["policy"], Value::Null);
}

// spec: specs/ap/policy.md#R3
#[test]
fn policy_invalid_refuses_every_stage_and_names_the_member() {
    let mut f = Fake::new();
    let open = f.open_policy();
    let with = |key: &str, value: Value| {
        let mut v = open.clone();
        v[key] = value;
        v
    };
    let long: Vec<String> = (0..65).map(|i| format!("m{i}")).collect();
    let paths: Vec<String> = (0..6).map(|i| format!("/root/.claude-{i}")).collect();
    let cases = [
        (json!([1]), "file"),
        (json!({"accounts": {}}), "models"),
        (
            json!({"models": {"allowed": ["sonnet"], "never": []}}),
            "accounts",
        ),
        (with("models", json!({"allowed": ["sonnet"]})), "models"),
        (
            with(
                "models",
                json!({"allowed": ["sonnet"], "never": [], "x": 1}),
            ),
            "models",
        ),
        (
            with("models", json!({"allowed": [], "never": []})),
            "models.allowed",
        ),
        (
            with("models", json!({"allowed": long, "never": []})),
            "models.allowed",
        ),
        (
            with("models", json!({"allowed": ["a b"], "never": []})),
            "models.allowed",
        ),
        (
            with("models", json!({"allowed": ["a*"], "never": []})),
            "models.allowed",
        ),
        (
            with("models", json!({"allowed": ["a"], "never": ["*a*b*"]})),
            "models.never",
        ),
        (
            with("models", json!({"allowed": ["a"], "never": vec!["x"; 17]})),
            "models.never",
        ),
        (with("accounts", json!([])), "accounts"),
        (with("accounts", json!({"claude": []})), "accounts.claude"),
        (
            with("accounts", json!({"claude": paths})),
            "accounts.claude",
        ),
        (
            with("accounts", json!({"claude": ["relative"]})),
            "accounts.claude",
        ),
        (
            with("accounts", json!({"gemini": ["/root/.x"]})),
            "accounts.gemini",
        ),
        (
            with("accounts", json!({"claude/a b": ["/root/.x"]})),
            "accounts.claude/a b",
        ),
    ];
    for (i, (policy, member)) in cases.iter().enumerate() {
        f.policy_is(policy);
        let out = format!("c{i}.md");
        let refused = f.run(&f.request(&out)).unwrap_err();
        assert_eq!(
            (refused.code, refused.field.as_str()),
            ("policy_invalid", "policy"),
            "{policy}"
        );
        let reason = refused.detail.as_str().unwrap().to_owned();
        assert!(
            reason.starts_with(&format!("{member} ")),
            "{policy}: {reason}"
        );
        f.untouched(&out);
    }
    for text in ["", "not json", "{\"models\":"] {
        f.policy_text(text);
        assert_eq!(
            code(f.run(&f.request("t.md"))),
            "policy_invalid@policy",
            "{text}"
        );
    }
    // Unreadable: the path is a directory.
    f.policy = Some(f.t.join("cwd"));
    assert_eq!(code(f.run(&f.request("d.md"))), "policy_invalid@policy");
    // Another top-level member is not read; a runtime, a role and a model key are well formed.
    let mut ok = f.open_policy();
    ok["scheduler"] = json!({"anything": true});
    ok["accounts"]["claude/reviewer"] = json!([f.p("a1")]);
    ok["accounts"]["claude/sonnet"] = json!([f.p("a1")]);
    ok["accounts"]["muse"] = json!([f.p("a1")]);
    f.policy_is(&ok);
    assert_eq!(f.run(&f.request("ok.md")).unwrap().0["state"], "ok");
}

// spec: specs/ap/policy.md#R4
#[test]
fn policy_a_model_that_is_not_allowed_is_refused() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["models"]["allowed"] = json!(["haiku"]);
    f.policy_is(&p);
    assert_eq!(code(f.run(&f.request("x.md"))), "model_not_allowed@model");
    f.untouched("x.md");
    let mut r = f.request("y.md");
    r["model"] = json!("haiku");
    assert_eq!(f.run(&r).unwrap().1, 0);
}

// spec: specs/ap/policy.md#R5
#[test]
fn policy_a_model_that_matches_a_never_pattern_is_refused() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["models"]["allowed"] = json!(["sonnet", "claude-opus-4[1m]", "muse-spark-1.0"]);
    p["models"]["never"] = json!(["*opus*", "muse-spark-*"]);
    f.policy_is(&p);
    for (i, model) in ["claude-opus-4[1m]", "muse-spark-1.0"]
        .into_iter()
        .enumerate()
    {
        let mut r = f.request(&format!("n{i}.md"));
        r["model"] = json!(model);
        assert_eq!(code(f.run(&r)), "model_not_allowed@model", "{model}");
        f.untouched(&format!("n{i}.md"));
    }
    assert_eq!(f.run(&f.request("ok.md")).unwrap().1, 0);
}

// spec: specs/ap/policy.md#R6
#[test]
fn policy_the_role_key_comes_before_the_model_key_and_the_model_key_before_the_runtime() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["accounts"] = json!({"claude": [f.p("a1")], "claude/sonnet": [f.p("a2")]});
    f.policy_is(&p);
    // The model's list: a stage on `sonnet` takes `a2`, not the runtime's `a1`.
    assert_eq!(
        code(f.run(&f.request("m.md"))),
        "account_not_allowed@account"
    );
    let mut r = f.request("m2.md");
    r["account"] = json!(f.p("a2"));
    assert_eq!(f.run(&r).unwrap().0["account"], json!(f.p("a2")));
    // A model no key names falls to the runtime's list.
    let mut r = f.request("h.md");
    r["model"] = json!("haiku");
    let mut p = f.open_policy();
    p["accounts"] = json!({"claude": [f.p("a1")], "claude/sonnet": [f.p("a2")]});
    f.policy_is(&p);
    assert_eq!(f.run(&r).unwrap().0["account"], json!(f.p("a1")));
    // The role's list comes before the model's.
    p["accounts"]["claude/reviewer"] = json!([f.p("a1")]);
    f.policy_is(&p);
    assert_eq!(
        f.run(&f.request("rv.md")).unwrap().0["account"],
        json!(f.p("a1"))
    );
    let mut r = f.request("rv2.md");
    r["account"] = json!(f.p("a2"));
    assert_eq!(code(f.run(&r)), "account_not_allowed@account");
    // Another runtime's key does not stand in for the stage's runtime.
    let mut p = f.open_policy();
    p["accounts"] = json!({"codex": [f.p("a1")]});
    f.policy_is(&p);
    assert_eq!(
        code(f.run(&f.request("cx.md"))),
        "account_not_allowed@account"
    );
}

// spec: specs/ap/policy.md#R7
#[test]
fn policy_a_stage_with_no_key_has_no_account() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["accounts"] = json!({"claude/implementer": [f.p("a1")], "claude/opus": [f.p("a1")]});
    f.policy_is(&p);
    assert_eq!(
        code(f.run(&f.request("x.md"))),
        "account_not_allowed@account"
    );
    f.untouched("x.md");
    p["accounts"] = json!({});
    f.policy_is(&p);
    assert_eq!(
        code(f.run(&f.request("y.md"))),
        "account_not_allowed@account"
    );
}

// spec: specs/ap/policy.md#R8
#[test]
fn policy_the_account_and_each_next_account_must_be_in_the_list() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["accounts"] = json!({"claude": [format!("{}/", f.p("a1"))]});
    f.policy_is(&p);
    // A trailing `/` makes no difference, in the list or in the request.
    let (v, _) = f.run(&f.request("x.md")).unwrap();
    assert_eq!(v["account"], json!(f.p("a1")));
    let mut r = f.request("y.md");
    r["account"] = json!(format!("{}/", f.p("a1")));
    assert_eq!(f.run(&r).unwrap().1, 0);
    f.reset_calls();
    f.launches.clear();
    let mut r = f.request("z.md");
    r["next_accounts"] = json!([f.p("a2")]);
    assert_eq!(code(f.run(&r)), "account_not_allowed@next_accounts");
    f.untouched("z.md");
    let mut r = f.request("w.md");
    r["account"] = json!(f.p("a2"));
    r["next_accounts"] = json!([f.p("a1")]);
    assert_eq!(code(f.run(&r)), "account_not_allowed@account");
}

// spec: specs/ap/policy.md#R9
#[test]
fn policy_a_resumed_call_meets_the_rule_with_the_account_of_its_record() {
    let mut f = Fake::new();
    f.mode("a1", "limit45");
    f.run(&f.request("v.md")).unwrap();
    f.clear("a1");
    f.reset_calls();
    f.launches.clear();
    f.git_calls.clear();
    let mut p = f.open_policy();
    p["accounts"] = json!({"claude": [f.p("a2")]});
    f.policy_is(&p);
    // The record names `a1`, which a resumed call must keep among its accounts.
    let mut r = f.resume(f.request("v2.md"), "v2.md", "v.md");
    r["account"] = json!(f.p("a2"));
    r["next_accounts"] = json!([f.p("a1")]);
    assert_eq!(code(f.run(&r)), "account_not_allowed@next_accounts");
    f.untouched("v2.md");
    p["accounts"] = json!({"claude": [f.p("a1"), f.p("a2")]});
    f.policy_is(&p);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
}

// spec: specs/ap/policy.md#R11
#[test]
fn policy_the_record_names_the_file_as_it_was_read() {
    let mut f = Fake::new();
    let path = f.policy_is(&f.open_policy());
    let sha = f.sha("policy.json");
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(v["policy"], json!({"path": path, "sha256": sha}));
    assert_eq!(f.record("x.md")["policy"], v["policy"]);
    // A file that holds another text names another digest.
    let mut p = f.open_policy();
    p["scheduler"] = json!(1);
    f.policy_is(&p);
    let (v, _) = f.run(&f.request("y.md")).unwrap();
    assert_ne!(v["policy"]["sha256"], json!(sha));
    assert_eq!(v["policy"]["sha256"], json!(f.sha("policy.json")));
}

// spec: specs/ap/quota.md#R14
#[test]
fn policy_the_member_quota_is_not_read_by_a_stage() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["quota"] = json!({"slice": "agents.slice", "busy_limit": 85});
    f.policy_is(&p);
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!(status, 0, "{v}");
    // Not even a `quota` that `ap quota` refuses changes an admission, and a refusal stays what it was.
    p["quota"] = json!({"slice": "a-b.slice", "other": 1});
    p["models"]["allowed"] = json!(["haiku"]);
    f.policy_is(&p);
    assert_eq!(code(f.run(&f.request("y.md"))), "model_not_allowed@model");
    p["quota"] = json!("not an object");
    p["models"]["allowed"] = json!(["sonnet"]);
    f.policy_is(&p);
    assert_eq!(f.run(&f.request("z.md")).unwrap().1, 0);
}

// spec: specs/ap/policy.md#R1
/// The built binary with the default path replaced only through `AP_POLICY_FILE`: a host file at
/// `/etc/agent-platform/policy.json` cannot change these answers. The policy is the last check of
/// admission, so a request it admits goes on to its launch: here `PATH` holds no `systemd-run`, so
/// the launch fails and the record says so, with nothing started.
#[test]
fn policy_the_built_binary_reads_the_file_the_variable_names() {
    if !Path::new("/run/systemd/system").is_dir() {
        eprintln!("skipped: systemd is not this host's init");
        return;
    }
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let f = Fake::new();
    std::fs::create_dir(f.t.join("empty")).unwrap();
    let ask = |file: &Path, out: &str| {
        let mut r = f.request(out);
        r["account"] = json!(f.p("a1"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
            .arg("run")
            .env_clear()
            .env("PATH", f.t.join("empty"))
            .env("AP_POLICY_FILE", file)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(&mut child.stdin.take().unwrap(), r.to_string().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let (error, record) = (out.stderr, out.stdout);
        let body: Value =
            serde_json::from_slice(if record.is_empty() { &error } else { &record }).unwrap();
        (out.status.code(), body)
    };
    let refused = |answer: (Option<i32>, Value)| {
        (
            answer.0,
            answer.1["error"]["code"].as_str().unwrap_or("").to_owned(),
        )
    };
    let write = |name: &str, v: &Value| {
        let path = f.t.join(name);
        std::fs::write(&path, v.to_string()).unwrap();
        path
    };
    let mut p = f.open_policy();
    p["models"]["allowed"] = json!(["haiku"]);
    let deny = write("deny.json", &p);
    assert_eq!(
        refused(ask(&deny, "d.md")),
        (Some(2), "model_not_allowed".into())
    );
    assert_eq!(
        refused(ask(&write("bad.json", &json!({})), "b.md")),
        (Some(2), "policy_invalid".into())
    );
    let mut odd = f.t.as_os_str().as_bytes().to_vec();
    odd.extend_from_slice(b"/\xff-policy.json");
    let odd = PathBuf::from(std::ffi::OsString::from_vec(odd));
    // No file at a path that is not text: no policy. A file there: invalid.
    let (status, record) = ask(&odd, "n2.md");
    assert_eq!(
        (status, record["state"].as_str(), record["policy"].clone()),
        (Some(3), Some("failed"), Value::Null)
    );
    std::fs::write(&odd, "{}").unwrap();
    assert_eq!(
        refused(ask(&odd, "o.md")),
        (Some(2), "policy_invalid".into())
    );
    for out in ["d.md", "b.md", "o.md"] {
        assert!(!f.t.join(format!("{out}.run.json")).exists());
    }
    // Admitted: the launch fails, and the record names the file that was read.
    let open = write("open.json", &f.open_policy());
    let (status, record) = ask(&open, "a.md");
    assert_eq!(
        (status, record["state"].as_str(), record["unmet"].as_str()),
        (Some(3), Some("failed"), Some("launch"))
    );
    assert_eq!(record["policy"]["path"], json!(open.to_str().unwrap()));
    // A file that does not exist is no policy, whatever this host holds at the default path.
    let (status, record) = ask(&f.t.join("absent.json"), "n.md");
    assert_eq!(
        (status, record["state"].as_str(), record["policy"].clone()),
        (Some(3), Some("failed"), Value::Null)
    );
}

// spec: specs/ap/policy.md#R10
#[test]
fn policy_refusals_come_after_every_refusal_of_the_request_and_nothing_starts_before_one() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["models"]["allowed"] = json!(["haiku"]);
    f.policy_is(&p);
    // Every member of the request is read first, `skills` and `memory` among them.
    for (key, value) in [
        ("skills", json!("yes")),
        ("memory", json!("yes")),
        ("bind", json!("-bad")),
    ] {
        let mut r = f.request("m.md");
        r[key] = value;
        assert_eq!(code(f.run(&r)), format!("invalid_input@{key}"));
    }
    let mut r = f.request("a.md");
    r["prompt"] = json!(f.p("none.md"));
    assert_eq!(code(f.run(&r)), "prompt_unreadable@prompt");
    f.policy_text("not json");
    let mut r = f.request("c.md");
    r["cwd"] = json!(f.p("no-cwd"));
    assert_eq!(code(f.run(&r)), "cwd_missing@cwd");
    // Then the policy, with nothing claimed, asked of Git or of `systemctl`, or launched after it.
    let mut r = f.request("d.md");
    r["role"] = json!("implementer");
    assert_eq!(code(f.run(&r)), "policy_invalid@policy");
    f.policy_is(&p);
    f.git_calls.clear();
    assert_eq!(code(f.run(&r)), "model_not_allowed@model");
    assert!(f.launches.is_empty() && f.calls().is_empty());
    for suffix in ["", ".run.json", ".stream.jsonl", ".err"] {
        assert!(
            !Path::new(&f.p(&format!("d.md{suffix}"))).exists(),
            "{suffix}"
        );
    }
}

// spec: specs/ap/policy.md#R10
#[test]
fn policy_refusals_come_after_the_refusals_that_ask_the_host() {
    let mut f = Fake::new();
    let mut p = f.open_policy();
    p["models"]["allowed"] = json!(["haiku"]);
    f.policy_is(&p);
    let mut bound = f.request("b.md");
    bound["bind"] = json!("ap-policy-test.service");
    let mut work = f.request("w.md");
    work["role"] = json!("implementer");
    f.rev_parse = Some((false, b"fatal: broken".to_vec()));
    assert_eq!(code(f.run(&work)), "worktree_unreadable@cwd");
    f.rev_parse = None;
    assert_eq!(code(f.run(&bound)), "bind_inactive@bind");
    f.units = false;
    assert_eq!(code(f.run(&f.request("s.md"))), "systemd_absent@host");
    f.units = true;
    // A dirty worktree, and a bind of the wrong shape after it: the order is the one without a policy.
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(f.p("cwd"))
            .status();
        assert!(status.unwrap().success());
    };
    git(&["init", "-q"]);
    std::fs::write(f.p("cwd/dirty"), "x").unwrap();
    let mut dirty = f.request("g.md");
    dirty["role"] = json!("implementer");
    dirty["bind"] = json!("-bad");
    assert_eq!(code(f.run(&dirty)), "worktree_dirty@cwd");
    f.policy = None;
    assert_eq!(code(f.run(&dirty)), "worktree_dirty@cwd");
    f.policy_is(&p);
    // A resume whose unit still lives.
    f.policy = None;
    f.mode("a1", "limit45");
    f.run(&f.request("v.md")).unwrap();
    f.clear("a1");
    let unit = f.record("v.md")["unit"].as_str().unwrap().to_owned();
    f.alive.insert(unit);
    f.policy_is(&p);
    assert_eq!(
        code(f.run(&f.resume(f.request("x"), "v2.md", "v.md"))),
        "run_still_active@resume"
    );
}

// spec: specs/ap/policy.md#R2
// spec: specs/ap/policy.md#R3
#[test]
fn policy_a_path_that_is_not_utf8_is_no_policy_without_a_file_and_invalid_with_one() {
    use std::os::unix::ffi::OsStringExt;
    let mut f = Fake::new();
    let mut name = f.t.as_os_str().to_owned().into_vec();
    name.extend_from_slice(b"/\xff-policy.json");
    let odd = PathBuf::from(std::ffi::OsString::from_vec(name));
    f.policy = Some(odd.clone());
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!((status, v["policy"].clone()), (0, Value::Null), "{v}");
    std::fs::write(&odd, "{}").unwrap();
    let refused = f.run(&f.request("y.md")).unwrap_err();
    assert_eq!(
        (refused.code, refused.field.as_str()),
        ("policy_invalid", "policy")
    );
    assert!(refused.detail.as_str().unwrap().starts_with("file "));
}

// ---- A stage that belongs to a unit or an intent (specs/ap/run-object.md) --------------------------

impl Fake {
    /// Gives the fake store with these objects; its path is an invented directory of the test.
    fn store_with(&mut self, exist: &[&str]) -> String {
        self.ont = Some(FakeOnt::with(exist));
        self.p("scratch-store")
    }
    /// A reviewer request for the unit `u-1` on `out`, its calls to the store naming the fake store.
    fn for_unit(&mut self, out: &str) -> Value {
        let store = self.store_with(&["unit:u-1", "intent:i-1"]);
        let mut r = self.request(out);
        r["unit"] = json!("u-1");
        r["store"] = json!(store);
        r
    }
    fn store(&self) -> &FakeOnt {
        self.ont.as_ref().unwrap()
    }
    fn way(&mut self, action: &str, ways: Vec<Way>) {
        self.ont
            .as_mut()
            .unwrap()
            .script
            .insert(action.into(), ways);
    }
    /// The environment of the last launch as a map.
    fn stage_env(&self) -> BTreeMap<String, String> {
        let lines = self.env_of_last().into_iter();
        lines
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
            .collect()
    }
    fn rid(&self, out: &str, end: &str) -> String {
        let sha = format!("{:x}", sha2::Sha256::digest(self.p(out).as_bytes()));
        format!("run-{}-{end}", &sha[..32])
    }
}

// spec: specs/ap/run-object.md#R1
#[test]
fn run_object_r1_the_members_are_checked_before_any_effect() {
    let mut f = Fake::new();
    let store = f.p("scratch-store");
    let cases = [
        (
            json!({"unit": "u-1", "intent": "i-1"}),
            "invalid_input@intent",
        ),
        (json!({"store": store}), "invalid_input@store"),
        (
            json!({"unit": "u-1", "store": "relative/store"}),
            "invalid_input@store",
        ),
        (json!({"unit": "u 1"}), "invalid_input@unit"),
        (json!({"unit": ""}), "invalid_input@unit"),
        (json!({"intent": "i".repeat(65)}), "invalid_input@intent"),
        (json!({"unit": 7}), "invalid_input@unit"),
        (json!({"git_author": "true"}), "invalid_input@git_author"),
        (json!({"ont_role": "fixer"}), "invalid_input@ont_role"),
        (json!({"ont_role": "lead"}), "invalid_input@ont_role"),
        (
            json!({"ont_role": "verifier", "role": "implementer"}),
            "invalid_input@ont_role",
        ),
    ];
    for (extra, want) in cases {
        let mut r = f.request("x.md");
        for (k, v) in extra.as_object().unwrap() {
            r[k] = v.clone();
        }
        assert_eq!(code(f.run(&r)), want, "{extra}");
        f.untouched("x.md");
    }
    // Accepted: a unit with a store, and `git_author` false as absence.
    let mut r = f.for_unit("y.md");
    r["git_author"] = json!(false);
    assert_eq!(f.run(&r).unwrap().1, 0);
    assert!(f.stage_env().keys().all(|k| !k.starts_with("GIT_")));
}

// spec: specs/ap/run-object.md#R2
#[test]
fn run_object_r2_env_may_not_name_the_actor_or_the_author() {
    let mut f = Fake::new();
    for name in [
        "ONT_ROLE",
        "ONT_AGENT",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
    ] {
        let mut r = f.request("x.md");
        r["env"] = json!({name: "lead"});
        assert_eq!(code(f.run(&r)), "env_forbidden@env", "{name}");
        f.untouched("x.md");
    }
    // Another name of the same families stays a request's own business.
    let mut r = f.request("y.md");
    r["env"] = json!({"ONT_PROBE": "1", "GIT_COMMITTER_NAME": "c"});
    assert_eq!(f.run(&r).unwrap().1, 0);
}

// spec: specs/ap/run-object.md#R3
#[test]
fn run_object_r3_every_stage_gets_its_role_and_never_lead() {
    let mut f = Fake::new();
    // A caller that is a Lead: its variables never reach the stage.
    f.env.push(("ONT_ROLE".into(), "lead".into()));
    f.env.push(("ONT_AGENT".into(), "someone".into()));
    f.env.push(("GIT_AUTHOR_NAME".into(), "caller".into()));
    for (role, ont_role, want) in [
        ("implementer", None, "implementer"),
        ("fixer", None, "fixer"),
        ("reviewer", None, "reviewer"),
        ("reviewer", Some("verifier"), "verifier"),
    ] {
        let mut r = f.request("x.md");
        r["role"] = json!(role);
        if let Some(o) = ont_role {
            r["ont_role"] = json!(o);
        }
        let _ = std::fs::remove_file(f.p("x.md.run.json"));
        for suffix in ["", ".stream.jsonl", ".err"] {
            let _ = std::fs::remove_file(f.p(&format!("x.md{suffix}")));
        }
        assert_eq!(f.run(&r).unwrap().1, 0);
        let env = f.stage_env();
        assert_eq!(env["ONT_ROLE"], want);
        assert!(
            !env.contains_key("GIT_AUTHOR_NAME"),
            "the caller's author is not inherited"
        );
        assert_eq!(env.keys().filter(|k| k.starts_with("ONT_")).count(), 2);
    }
}

// spec: specs/ap/run-object.md#R4
#[test]
fn run_object_r4_the_agent_is_a_form_the_engine_accepts() {
    let mut f = Fake::new();
    let long = format!("{}[1m]", "m".repeat(34));
    for (model, want) in [
        ("sonnet", "claude:sonnet".to_owned()),
        ("opus[1m]", "claude:opus-1m-".to_owned()),
        ("gpt-6.1_x", "claude:gpt-6.1_x".to_owned()),
        (long.as_str(), format!("claude:{}-1m-", "m".repeat(34))),
    ] {
        let mut r = f.request(&format!("{}.md", model.len()));
        r["model"] = json!(model);
        assert_eq!(f.run(&r).unwrap().1, 0, "{model}");
        let agent = f.stage_env()["ONT_AGENT"].clone();
        assert_eq!(agent, want);
        assert!((1..=64).contains(&agent.len()));
        assert!(
            agent
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
        );
    }
    // Another runtime: its own name before the colon.
    let r = f.codex("c.md");
    assert_eq!(f.run(&r).unwrap().1, 0);
    assert_eq!(f.stage_env()["ONT_AGENT"], "codex:gpt-6.1-terra");
}

// spec: specs/ap/run-object.md#R5
#[test]
fn run_object_r5_git_author_names_the_stage_and_leaves_the_committer() {
    let mut f = Fake::new();
    let mut r = f.request("a.md");
    r["model"] = json!("opus[1m]");
    r["git_author"] = json!(true);
    assert_eq!(f.run(&r).unwrap().1, 0);
    let env = f.stage_env();
    assert_eq!(env["GIT_AUTHOR_NAME"], "reviewer opus[1m]");
    assert_eq!(env["GIT_AUTHOR_EMAIL"], "reviewer.opus-1m-@agents.invalid");
    assert!(env.keys().all(|k| !k.starts_with("GIT_COMMITTER")));
    // The finer role names the author too.
    let mut r = f.request("b.md");
    r["ont_role"] = json!("verifier");
    r["git_author"] = json!(true);
    assert_eq!(f.run(&r).unwrap().1, 0);
    let env = f.stage_env();
    assert_eq!(env["GIT_AUTHOR_NAME"], "verifier sonnet");
    assert_eq!(env["GIT_AUTHOR_EMAIL"], "verifier.sonnet@agents.invalid");
    // Without the member nothing of Git's environment is set.
    assert_eq!(f.run(&f.request("c.md")).unwrap().1, 0);
    assert!(f.stage_env().keys().all(|k| !k.starts_with("GIT_")));
}

// spec: specs/ap/run-object.md#R6
#[test]
fn run_object_r6_a_unit_or_an_intent_the_store_lacks_refuses_before_any_effect() {
    let mut f = Fake::new();
    let store = f.store_with(&["unit:u-1"]);
    for (member, key, want) in [
        ("unit", "u-9", "unknown_unit@unit"),
        ("intent", "i-9", "unknown_intent@intent"),
    ] {
        let mut r = f.request("x.md");
        r[member] = json!(key);
        r["store"] = json!(store);
        assert_eq!(code(f.run(&r)), want);
        assert!(f.launches.is_empty() && f.calls().is_empty());
        assert!(!Path::new(&f.p("x.md.run.json")).exists());
        assert_eq!(f.store().bodies("start_run").len(), 0);
    }
    let read = &f.store().calls[0];
    assert_eq!(read.0, ["get", "--store", store.as_str()]);
    assert_eq!(read.1["id"]["native_key"], "u-9");
    // Any other answer or failure of the read does not refuse: the stage goes on.
    for (n, way) in [
        Way::Refuse("store_not_found"),
        Way::Lost,
        Way::Refuse("store_busy"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut r = f.request(&format!("y{n}.md"));
        r["unit"] = json!("u-9");
        f.way("get", vec![way]);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{v}");
        assert_eq!(v["run_object"]["start"], "applied");
    }
}

fn object_id(kind: &str, key: &str) -> Value {
    json!({"object_type": kind, "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}

// spec: specs/ap/run-object.md#R7
#[test]
fn run_object_r7_the_start_comes_after_admission_and_before_the_first_launch() {
    let mut f = Fake::new();
    let r = f.for_unit("a.md");
    let store = f.p("scratch-store");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(
        f.events,
        ["ont get", "ont start_run", "launch", "ont close_run"]
    );
    let calls = &f.store().calls;
    assert_eq!(
        calls[1].0,
        [
            "action",
            "apply",
            "--client",
            "ap",
            "--store",
            store.as_str()
        ]
    );
    let start = &calls[1].1;
    assert_eq!(
        (start["action_type"].as_str(), start["agent"].as_str()),
        (Some("start_run"), Some("ap-run"))
    );
    assert_eq!(start["request_id"], f.rid("a.md", "start"));
    assert_eq!(start["role"], "reviewer");
    assert_eq!(
        start["parameters"],
        json!({"unit": object_id("unit", "u-1"), "role": "reviewer", "runtime": "claude", "model": "sonnet",
            "effort": "high", "account": "a1", "session": v["attempts"][0]["session"],
            "record": f.p("a.md.run.json"), "stream": f.p("a.md.stream.jsonl")})
    );
    // The finer role is the object's and the actor's; an intent takes the place of a unit.
    let mut r = f.for_unit("b.md");
    r["ont_role"] = json!("verifier");
    r["intent"] = json!("i-1");
    r.as_object_mut().unwrap().remove("unit");
    assert_eq!(f.run(&r).unwrap().1, 0);
    let start = &f.store().bodies("start_run")[0];
    assert_eq!(
        (start["role"].clone(), start["parameters"]["role"].clone()),
        (json!("verifier"), json!("verifier"))
    );
    assert!(start["parameters"].get("unit").is_none());
    assert_eq!(start["parameters"]["intent"], object_id("intent", "i-1"));
    // A resume names its session before the launch; a Codex stage's id is its own, given at the close.
    let mut r = f.for_unit("e.md");
    r["unit"] = json!("u-1");
    let earlier = f.run(&f.request("p.md")).unwrap().0;
    let r = f.resume(r, "e.md", "p.md");
    assert_eq!(f.run(&r).unwrap().1, 0);
    assert_eq!(
        f.store().bodies("start_run")[0]["parameters"]["session"],
        earlier["session"]
    );
    let mut r = f.codex("c.md");
    r["unit"] = json!("u-1");
    f.store_with(&["unit:u-1"]);
    assert_eq!(f.run(&r).unwrap().1, 0);
    assert!(
        f.store().bodies("start_run")[0]["parameters"]
            .get("session")
            .is_none()
    );
    // A path that is no `file_path` of the schema is left out, not sent.
    let mut r = f.for_unit("with space.md");
    r["unit"] = json!("u-1");
    assert_eq!(f.run(&r).unwrap().1, 0);
    let parameters = &f.store().bodies("start_run")[0]["parameters"];
    assert!(
        parameters.get("record").is_none() && parameters.get("stream").is_none(),
        "{parameters}"
    );
    // A move to another account applies nothing.
    let mut f = Fake::new();
    let mut r = f.for_unit("m.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "limit1");
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["attempts"].as_array().unwrap().len(), 2, "{v}");
    assert_eq!(f.store().bodies("start_run").len(), 1);
    assert_eq!(
        f.store().bodies("start_run")[0]["parameters"]["account"],
        "a1"
    );
    // The policy step is the last check of admission: a stage it refuses touches no store.
    let mut f = Fake::new();
    let r = f.for_unit("n.md");
    f.policy_is(&json!({"models": {"allowed": ["haiku"], "never": []}, "accounts": {"claude": [f.p("a1")]}}));
    assert_eq!(code(f.run(&r)), "model_not_allowed@model");
    assert!(f.store().calls.is_empty() && f.launches.is_empty());
}

// spec: specs/ap/run-object.md#R8
#[test]
fn run_object_r8_the_record_holds_the_key_before_the_launch() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let r = f.for_unit("w.md");
    let (v, _) = f.run(&r).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(seen["state"], "running");
    assert_eq!(
        seen["run_object"],
        json!({"key": "run-1", "start": "applied", "close": null})
    );
    assert_eq!(
        v["run_object"],
        json!({"key": "run-1", "start": "applied", "close": "applied"})
    );
    assert_eq!(f.record("w.md"), v);
    // Without a unit and an intent: null, before the launch and after the end.
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("n.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, _) = f.run(&f.request("n.md")).unwrap();
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(
        (seen["run_object"].clone(), v["run_object"].clone()),
        (Value::Null, Value::Null)
    );
    assert_eq!(f.record("n.md"), v);
}

// spec: specs/ap/run-object.md#R9
#[test]
fn run_object_r9_the_close_takes_the_end_state_the_tokens_and_the_last_session() {
    let mut f = Fake::new();
    let r = f.for_unit("a.md");
    let (v, _) = f.run(&r).unwrap();
    let close = &f.store().calls[2];
    assert_eq!(close.0[..4], ["action", "apply", "--client", "ap"]);
    assert_eq!(close.1["request_id"], f.rid("a.md", "close"));
    assert_eq!(
        (
            close.1["action_type"].as_str(),
            close.1["agent"].as_str(),
            close.1["role"].as_str()
        ),
        (Some("close_run"), Some("ap-run"), Some("reviewer"))
    );
    assert_eq!(
        close.1["parameters"],
        json!({"run": object_id("run", "run-1"), "state": "ok", "session": v["session"],
            "input_tokens": 2, "cache_read_tokens": 9064, "cache_write_tokens": 14047, "output_tokens": 4})
    );
    // A stage that moved is one object and the close holds the last account's session.
    let mut f = Fake::new();
    let mut r = f.for_unit("m.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "limit1");
    let (v, _) = f.run(&r).unwrap();
    assert_ne!(v["attempts"][0]["session"], v["attempts"][1]["session"]);
    let close = &f.store().bodies("close_run");
    assert_eq!((close.len(), f.store().bodies("start_run").len()), (1, 1));
    assert_eq!(
        close[0]["parameters"]["session"],
        v["attempts"][1]["session"]
    );
    assert_eq!(close[0]["parameters"]["session"], v["session"]);
    // A stage that failed is closed with that state; a count the record holds as null is left out.
    let mut f = Fake::new();
    f.mode("a1", "noout");
    let r = f.for_unit("f.md");
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")), "{v}");
    let close = &f.store().bodies("close_run")[0]["parameters"];
    assert_eq!(close["state"], "failed");
    assert!(close.get("input_tokens").is_none() && close.get("limit").is_none());
    // The limit reading: compact JSON when it fits in 300 bytes, else the window of the highest
    // utilization, else nothing.
    let mut f = Fake::new();
    f.stream("a1", &[ALLOWED]);
    let r = f.for_unit("l.md");
    let (v, _) = f.run(&r).unwrap();
    let sent = f.store().bodies("close_run")[0]["parameters"]["limit"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(serde_json::from_str::<Value>(&sent).unwrap(), v["limit"]);
    assert!(sent.len() <= 300 && !sent.contains(' '));
    let event = |status: &str| {
        let windows: serde_json::Map<String, Value> = (1..=8)
            .map(|n| (format!("window_{n}_of_the_unified_limit"), json!({"utilization": n as f64 / 10.0, "resetsAt": 1794772800, "status": status})))
            .collect();
        json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "resetsAt": 1794772800,
            "rateLimitType": "five_hour", "unifiedWindows": windows}}).to_string()
    };
    for (n, (status, kept)) in [("allowed", true), (&"x".repeat(400), false)]
        .into_iter()
        .enumerate()
    {
        let mut f = Fake::new();
        f.stream("a1", &[&event(status)]);
        let r = f.for_unit(&format!("w{n}.md"));
        let (v, status) = f.run(&r).unwrap();
        assert_eq!(status, 0, "{v}");
        assert!(v["limit"].to_string().len() > 300);
        let sent = f.store().bodies("close_run")[0]["parameters"]["limit"].clone();
        match kept {
            true => {
                let sent: Value = serde_json::from_str(sent.as_str().unwrap()).unwrap();
                assert_eq!(
                    sent,
                    json!({"window": "window_8_of_the_unified_limit", "utilization": 0.8,
                    "status": "allowed", "resets_at": 1794772800_u64})
                );
            }
            false => assert!(sent.is_null()),
        }
    }
}

// spec: specs/ap/run-object.md#R10
#[test]
fn run_object_r10_the_store_never_stops_changes_or_delays_a_stage_beyond_its_bounds() {
    let mut f = Fake::new();
    let r = f.for_unit("ref.md");
    let (plain, plain_status) = f.run(&f.request("plain.md")).unwrap();
    let (with, status) = f.run(&r).unwrap();
    assert_eq!(
        (status, with["state"].clone()),
        (plain_status, plain["state"].clone())
    );
    assert_eq!(
        std::fs::read(f.p("ref.md")).unwrap(),
        std::fs::read(f.p("plain.md")).unwrap()
    );
    // Refused start, refused close, and a store that cannot be reached at all.
    // (name, ways of the read, of the start, of the close, the record's `run_object`)
    type Case = (&'static str, Vec<Way>, Vec<Way>, Vec<Way>, Value);
    let cases: [Case; 3] = [
        (
            "start",
            vec![],
            vec![Way::Refuse("store_busy")],
            vec![],
            json!({"key": null, "start": "store_busy", "close": null}),
        ),
        (
            "close",
            vec![],
            vec![],
            vec![Way::Refuse("store_busy")],
            json!({"key": "run-1", "start": "applied", "close": "store_busy"}),
        ),
        (
            "lost",
            vec![Way::Lost],
            vec![Way::Lost, Way::Lost],
            vec![],
            json!({"key": null, "start": "process_failed", "close": null}),
        ),
    ];
    for (name, read, start, close, want) in cases {
        let mut f = Fake::new();
        let r = f.for_unit(&format!("{name}.md"));
        f.way("get", read);
        f.way("start_run", start);
        f.way("close_run", close);
        let (v, status) = f.run(&r).unwrap();
        assert_eq!((status, v["state"].as_str()), (0, Some("ok")), "{name} {v}");
        assert_eq!(v["run_object"], want, "{name}");
        // One attempt for each call, each bound to 15 seconds; a busy answer is not asked again.
        assert!(f.store().calls.iter().all(|c| c.2 == 15_000), "{name}");
        assert_eq!(
            f.store().bodies("start_run").len(),
            if name == "lost" { 2 } else { 1 },
            "{name}"
        );
        assert_eq!(
            f.store().bodies("close_run").len(),
            usize::from(name == "close"),
            "{name}"
        );
        let before: Vec<_> = f.events.iter().take_while(|e| *e != "launch").collect();
        assert!(
            before.len() <= 2,
            "at most the read and the start before the launch: {before:?}"
        );
        assert_eq!(f.events.iter().filter(|e| *e == "launch").count(), 1);
    }
    // A stage that ends badly is judged by itself.
    let mut f = Fake::new();
    f.mode("a1", "noout");
    let r = f.for_unit("bad.md");
    f.way("close_run", vec![Way::Lost]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, v["state"].as_str()), (3, Some("failed")), "{v}");
    assert_eq!(v["run_object"]["close"], "process_failed");
    assert_eq!(f.record("bad.md"), v);
}

// spec: specs/ap/run-object.md#R11
#[test]
fn run_object_r11_a_start_without_an_answer_is_repeated_after_the_end_with_the_same_content() {
    // The first start was not applied: the repeat applies it, late; the stage moved on meanwhile, so
    // the final record's account and session are not the first call's.
    let mut f = Fake::new();
    let mut r = f.for_unit("a.md");
    r["next_accounts"] = json!([f.p("a2")]);
    f.mode("a1", "limit1");
    f.way("start_run", vec![Way::Lost]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(
        f.events,
        [
            "ont get",
            "ont start_run",
            "launch",
            "launch",
            "ont start_run",
            "ont close_run"
        ]
    );
    let starts = f.store().bodies("start_run");
    assert_eq!(starts[0], starts[1]);
    assert_eq!(starts[0]["parameters"]["account"], "a1");
    assert_eq!(
        starts[0]["parameters"]["session"],
        v["attempts"][0]["session"]
    );
    assert_eq!(
        v["run_object"],
        json!({"key": "run-1", "start": "applied_late", "close": "applied"})
    );
    // The first start was applied and its answer lost: the repeat is `already_applied`.
    let mut f = Fake::new();
    let r = f.for_unit("b.md");
    f.way("start_run", vec![Way::Silent]);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        v["run_object"],
        json!({"key": "run-1", "start": "already_applied", "close": "applied"})
    );
    assert_eq!(f.store().runs.len(), 1);
    // A refusal is an answer: nothing is repeated. A repeat that fails leaves the first code.
    let mut f = Fake::new();
    let r = f.for_unit("c.md");
    f.way("start_run", vec![Way::Refuse("unknown_unit")]);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (
            f.store().bodies("start_run").len(),
            v["run_object"]["start"].clone()
        ),
        (1, json!("unknown_unit"))
    );
}

// spec: specs/ap/run-object.md#R12
#[test]
fn run_object_r12_a_second_call_on_the_same_out_is_refused_before_the_store() {
    let mut f = Fake::new();
    let r = f.for_unit("o.md");
    let (first, _) = f.run(&r).unwrap();
    let (calls, applied, runs) = (
        f.store().calls.len(),
        f.store().applied.clone(),
        f.store().runs.clone(),
    );
    assert_eq!(code(f.run(&r)), "out_exists@out");
    assert_eq!(f.store().calls.len(), calls);
    assert_eq!((&f.store().applied, &f.store().runs), (&applied, &runs));
    assert_eq!(f.record("o.md"), first);
}

// spec: specs/ap/run-object.md#R13
#[test]
fn run_object_r13_a_request_without_the_members_never_reaches_the_store() {
    let mut f = Fake::new();
    let (v, status) = f.run(&f.request("x.md")).unwrap();
    assert_eq!((status, v["run_object"].clone()), (0, Value::Null));
    assert_eq!(f.events, ["launch"]);
    // The fallback record keeps its three members and nothing is started.
    let mut f = Fake::new();
    let r = f.for_unit("h.md");
    f.precreate = Some(f.p("h.md.stream.jsonl"));
    let (v, status) = f.run(&r).unwrap();
    assert_eq!(status, 5, "{v}");
    assert_eq!(
        v,
        json!({"state": "failed", "unmet": "launch", "account": f.p("a1")})
    );
    assert_eq!(f.record("h.md"), v);
    assert!(f.store().bodies("start_run").is_empty() && f.store().bodies("close_run").is_empty());
}

// ---- A reviewer stage with limits (specs/ap/run-limits.md) ------------------------------------

const FAKE_LIMITS: &str = include_str!("fixtures/run_limits/fake-claude.sh");
impl Fake {
    /// A reviewer request on `out` whose `limits` name the scratch directory `<t>/scratch`.
    fn limited(&self, out: &str) -> Value {
        let mut r = self.request(out);
        r["limits"] = json!({"scratch": self.p("scratch")});
        r
    }
    /// The fake of `fixtures/run_limits` in place of the CLI.
    fn limits_fake(&self) {
        std::fs::write(self.t.join("bin/claude"), FAKE_LIMITS).unwrap();
    }
    /// What a launch on `account` runs between `systemd-run … --` and the CLI.
    fn bwrap(&self, account: &str) -> Vec<String> {
        let (home, scratch, cwd) = (self.p(account), self.p("scratch"), self.p("cwd"));
        [
            BWRAP,
            "--unshare-pid",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--ro-bind",
            "/proc/sys",
            "/proc/sys",
            "--bind",
            &home,
            &home,
            "--bind",
            &scratch,
            &scratch,
            "--cap-drop",
            "ALL",
            "--chdir",
            &cwd,
            "--",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    /// The words of launch `n` after `systemd-run … --`.
    fn after_unit(&self, n: usize) -> Vec<String> {
        let launch = &self.launches[n];
        let at = launch.iter().position(|a| a == "--").unwrap();
        launch[at + 1..].to_vec()
    }
    fn text(&self, rel: &str) -> String {
        std::fs::read_to_string(self.t.join(rel)).unwrap()
    }
}

// spec: specs/ap/run-limits.md#R1
#[test]
fn limits_r1_only_a_claude_reviewer_with_one_scratch_apart_from_its_paths() {
    let mut f = Fake::new();
    let (v, status) = f.run(&f.limited("x.md")).unwrap();
    assert_eq!((status, &v["state"]), (0, &json!("ok")), "{v}");
    for d in ["o", "pr"] {
        std::fs::create_dir(f.t.join(d)).unwrap();
    }
    std::fs::copy(f.p("prompt.md"), f.p("pr/prompt.md")).unwrap();
    let scratch = |r: &mut Value, p: Value| r["limits"] = json!({ "scratch": p });
    type Change<'a> = Box<dyn Fn(&mut Value) + 'a>;
    let cases: Vec<(&str, Change)> = vec![
        (
            "implementer",
            Box::new(|r| r["role"] = json!("implementer")),
        ),
        ("fixer", Box::new(|r| r["role"] = json!("fixer"))),
        (
            "muse",
            Box::new(|r| {
                r["runtime"] = json!("muse");
                r["permission_mode"] = json!("bypassPermissions");
            }),
        ),
        ("no object", Box::new(|r| r["limits"] = json!("/tmp/x"))),
        ("empty", Box::new(|r| r["limits"] = json!({}))),
        (
            "other member",
            Box::new(|r| r["limits"]["writable"] = json!(["/tmp"])),
        ),
        ("relative", Box::new(|r| scratch(r, json!("scratch")))),
        ("not text", Box::new(|r| scratch(r, json!(7)))),
        (
            "..",
            Box::new(|r| scratch(r, json!(format!("{}/../s", r["cwd"].as_str().unwrap())))),
        ),
        (
            "in account",
            Box::new(|r| scratch(r, json!(format!("{}/s", r["account"].as_str().unwrap())))),
        ),
        (
            "in next_accounts",
            Box::new(|r| {
                let a2 = r["account"].as_str().unwrap().replace("/a1", "/a2");
                r["next_accounts"] = json!([a2.clone() + "/"]);
                scratch(r, json!(format!("{a2}/x/s")));
            }),
        ),
        (
            "in cwd",
            Box::new(|r| scratch(r, json!(format!("{}//s", r["cwd"].as_str().unwrap())))),
        ),
        ("is cwd", Box::new(|r| scratch(r, r["cwd"].clone()))),
        (
            "holds out",
            Box::new(|r| {
                let o = r["out"].as_str().unwrap().replace("/y.md", "/o");
                r["out"] = json!(format!("{o}/y.md"));
                scratch(r, json!(o));
            }),
        ),
        (
            "holds prompt",
            Box::new(|r| {
                let p = r["prompt"].as_str().unwrap().replace("/prompt.md", "/pr");
                r["prompt"] = json!(format!("{p}/prompt.md"));
                scratch(r, json!(p));
            }),
        ),
        ("holds all", Box::new(|r| scratch(r, json!("/")))),
        (
            "expect.file",
            Box::new(|r| {
                r["expect"] = json!({"file": r["cwd"].as_str().unwrap().to_owned() + "/a"})
            }),
        ),
    ];
    for (name, change) in cases {
        let mut r = f.limited("y.md");
        change(&mut r);
        assert_eq!(code(f.run(&r)), "invalid_input@limits", "{name}");
    }
    // Every refusal came before the probe and made nothing.
    assert_eq!((f.probes.len(), f.launches.len()), (1, 1));
    assert!(!Path::new(&f.p("y.md.run.json")).exists() && !Path::new(&f.p("scratch")).exists());
}

// spec: specs/ap/run-limits.md#R2
#[test]
fn limits_r2_an_existing_scratch_or_a_parent_that_is_no_directory_refuses_before_the_policy() {
    use std::os::unix::fs::symlink;
    let mut f = Fake::new();
    // A policy that refuses the request's model: these refusals come first.
    let barred = json!({"models": {"allowed": ["haiku"], "never": []},
                        "accounts": {"claude": [f.p("a1")]}});
    f.policy_is(&barred);
    let s = f.p("scratch");
    std::fs::create_dir(&s).unwrap();
    std::fs::write(f.t.join("scratch/keep"), "k").unwrap();
    assert_eq!(code(f.run(&f.limited("x.md"))), "scratch_exists@limits");
    assert_eq!(f.text("scratch/keep"), "k");
    std::fs::remove_dir_all(&s).unwrap();
    symlink(f.p("nowhere"), &s).unwrap();
    assert_eq!(code(f.run(&f.limited("x.md"))), "scratch_exists@limits");
    std::fs::remove_file(&s).unwrap();
    std::fs::write(&s, "").unwrap();
    assert_eq!(code(f.run(&f.limited("x.md"))), "scratch_exists@limits");
    std::fs::remove_file(&s).unwrap();
    symlink(f.p("cwd"), f.p("link")).unwrap();
    for parent in ["none", "prompt.md", "link"] {
        let mut r = f.limited("x.md");
        r["limits"]["scratch"] = json!(f.p(&format!("{parent}/s")));
        assert_eq!(code(f.run(&r)), "scratch_parent_missing@limits", "{parent}");
    }
    f.untouched("x.md");
    assert!(f.probes.is_empty() && !Path::new(&f.p("cwd/s")).exists());
    // Nothing in the way: the policy's refusal, after the probe.
    assert_eq!(code(f.run(&f.limited("x.md"))), "model_not_allowed@model");
    assert_eq!(f.events, ["probe"]);
    assert!(!Path::new(&s).exists());
    f.untouched("x.md");
}

// spec: specs/ap/run-limits.md#R3
#[test]
fn limits_r3_a_bwrap_that_does_not_answer_refuses_before_any_effect() {
    let mut f = Fake::new();
    f.bwrap = false;
    let barred = json!({"models": {"allowed": ["haiku"], "never": []},
                        "accounts": {"claude": [f.p("a1")]}});
    f.policy_is(&barred);
    assert_eq!(code(f.run(&f.limited("x.md"))), "limits_unavailable@limits");
    assert_eq!(f.probes, [(vec!["--version".to_owned()], 10_000)]);
    f.untouched("x.md");
    assert!(!Path::new(&f.p("scratch")).exists());
    // An answering `bwrap` admits the stage, probed once before its launch.
    (f.bwrap, f.policy) = (true, None);
    f.events.clear();
    let (v, status) = f.run(&f.limited("x.md")).unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(f.events, ["probe", "launch"]);
}

// spec: specs/ap/run-limits.md#R4
#[test]
fn limits_r4_the_scratch_directory_is_made_once_with_mode_0700_tmp_and_target() {
    let mut f = Fake::new();
    f.limits_fake();
    std::fs::write(f.t.join("bin/cut-a1"), "").unwrap();
    let mut r = f.limited("x.md");
    r["next_accounts"] = json!([f.p("a2")]);
    let (v, status) = f.run(&r).unwrap();
    assert_eq!((status, &v["account"]), (0, &json!(f.p("a2"))), "{v}");
    assert_eq!(v["attempts"].as_array().unwrap().len(), 2);
    // The second launch finds what the first left: one directory for the call.
    assert_eq!(f.text("bin/scratch.1"), "700\ntarget\ntmp\n");
    assert_eq!(f.text("bin/scratch.2"), "700\nmark.1\ntarget\ntmp\n");
    assert!(!Path::new(&f.p("scratch")).exists());
}

// spec: specs/ap/run-limits.md#R4
// spec: specs/ap/run-limits.md#R8
#[test]
fn limits_r8_a_directory_another_call_made_is_never_removed() {
    let mut f = Fake::new();
    // It appears after the request's checks and before this call's `mkdir`.
    f.precreate_dir = Some(f.p("scratch"));
    let (v, status) = f.run(&f.limited("x.md")).unwrap();
    assert_eq!(
        (status, &v["state"], &v["unmet"]),
        (3, &json!("failed"), &json!("launch")),
        "{v}"
    );
    assert!(f.launches.is_empty() && f.calls().is_empty());
    assert_eq!(v["limits"]["scratch"], f.p("scratch"));
    assert!(Path::new(&f.p("scratch")).is_dir());
    assert!(!Path::new(&f.p("scratch/tmp")).exists());
}

// spec: specs/ap/run-limits.md#R5
#[test]
fn limits_r5_bwrap_comes_after_the_unit_and_binds_the_account_of_each_launch() {
    let mut f = Fake::new();
    f.limits_fake();
    std::fs::write(f.t.join("bin/cut-a1"), "").unwrap();
    let mut r = f.limited("x.md");
    r["next_accounts"] = json!([f.p("a2")]);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    for (n, account) in [(0, "a1"), (1, "a2")] {
        let after = f.after_unit(n);
        let want = f.bwrap(account);
        assert_eq!(after[..want.len()], want[..], "{after:?}");
        assert_eq!(after[want.len()], "claude");
        assert_eq!(after.iter().filter(|a| *a == BWRAP).count(), 1);
    }
    assert_eq!(f.accounts(), ["a1", "a2"]);
    // A resumed call binds the account its record names, where it starts.
    std::fs::remove_file(f.t.join("bin/cut-a1")).unwrap();
    let mut r = f.resume(f.limited("y.md"), "y.md", "x.md");
    r["next_accounts"] = json!([f.p("a2")]);
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(
        (&v["state"], &v["account"]),
        (&json!("ok"), &json!(f.p("a2")))
    );
    assert_eq!(f.after_unit(2)[..f.bwrap("a2").len()], f.bwrap("a2")[..]);
}

// spec: specs/ap/run-limits.md#R6
#[test]
fn limits_r6_tmpdir_and_cargo_target_dir_name_the_scratch_directory() {
    let mut f = Fake::new();
    f.env.push(("TMPDIR".into(), "/caller/tmp".into()));
    let mut r = f.limited("x.md");
    r["env"] = json!({"TMPDIR": "/request/tmp", "CARGO_TARGET_DIR": "/request/target", "X_Y": "1"});
    let (v, _) = f.run(&r).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    let env = f.text("bin/env.1");
    let lines = |name: &str| -> Vec<String> {
        env.lines()
            .filter(|l| l.starts_with(&format!("{name}=")))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(lines("TMPDIR"), [format!("TMPDIR={}/tmp", f.p("scratch"))]);
    assert_eq!(
        lines("CARGO_TARGET_DIR"),
        [format!("CARGO_TARGET_DIR={}/target", f.p("scratch"))]
    );
    assert_eq!(lines("X_Y"), ["X_Y=1"]);
    // Replaced in the list the launch is given, not only by a later value of the name.
    let given = &f.launch_envs[0];
    for name in ["TMPDIR", "CARGO_TARGET_DIR"] {
        assert_eq!(given.iter().filter(|(k, _)| k == name).count(), 1, "{name}");
    }
    assert_eq!(
        lines("CLAUDE_CONFIG_DIR"),
        [format!("CLAUDE_CONFIG_DIR={}", f.p("a1"))]
    );
}

// spec: specs/ap/run-limits.md#R7
#[test]
fn limits_r7_the_record_names_the_limits_before_the_launch_and_at_the_end() {
    let mut f = Fake::new();
    std::fs::write(f.t.join("bin/watch"), f.p("w.md.run.json")).unwrap();
    f.mode("a1", "watch");
    let (v, _) = f.run(&f.limited("w.md")).unwrap();
    let s = f.p("scratch");
    let want = json!({"scratch": s, "writable": [f.p("a1"), s], "bwrap": BWRAP});
    assert_eq!((&v["state"], &v["limits"]), (&json!("ok"), &want), "{v}");
    assert_eq!(f.record("w.md"), v);
    // What a call killed during its stage leaves: the record names the scratch directory.
    let seen: Value =
        serde_json::from_slice(&std::fs::read(f.t.join("bin/seen.json")).unwrap()).unwrap();
    assert_eq!(
        (&seen["state"], &seen["limits"]),
        (&json!("running"), &want)
    );
    // The member is the only key a request without `limits` does not have.
    f.clear("a1");
    let (plain, _) = f.run(&f.request("p.md")).unwrap();
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    let mut with = keys(&plain);
    with.push("limits".into());
    with.sort();
    assert_eq!(keys(&v), with);
}

// spec: specs/ap/run-limits.md#R8
#[test]
fn limits_r8_the_call_that_made_the_directory_removes_it_and_follows_no_link() {
    let mut f = Fake::new();
    f.limits_fake();
    std::fs::create_dir(f.t.join("outside")).unwrap();
    std::fs::write(f.t.join("outside/keep"), "k").unwrap();
    // At any end: a stage that is cut on its only account.
    std::fs::write(f.t.join("bin/cut-a1"), "").unwrap();
    let (v, _) = f.run(&f.limited("c.md")).unwrap();
    assert_eq!(v["state"], "cut", "{v}");
    assert!(!Path::new(&f.p("scratch")).exists());
    std::fs::remove_file(f.t.join("bin/cut-a1")).unwrap();
    // A link inside it that points outside: the link goes, what it points to stays.
    let then = format!("ln -s {} \"$S/link\"\nmkdir \"$S/tmp/d\"\n", f.p("outside"));
    std::fs::write(f.t.join("bin/then"), then).unwrap();
    let (v, _) = f.run(&f.limited("x.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert!(std::fs::symlink_metadata(f.p("scratch")).is_err());
    assert_eq!(f.text("outside/keep"), "k");
    // The path made a link to another directory: not a directory, so nothing is removed.
    let then = format!(
        "mv \"$S\" {}\nln -s {} \"$S\"\n",
        f.p("moved"),
        f.p("outside")
    );
    std::fs::write(f.t.join("bin/then"), then).unwrap();
    let (v, _) = f.run(&f.limited("y.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert!(std::fs::symlink_metadata(f.p("scratch")).is_ok_and(|m| m.is_symlink()));
    assert_eq!(f.text("outside/keep"), "k");
    assert!(Path::new(&f.p("moved/mark.3")).exists());
}

// spec: specs/ap/run-limits.md#R9
#[test]
fn limits_r9_a_request_without_limits_runs_as_before() {
    let mut f = Fake::new();
    f.env.push(("TMPDIR".into(), "/caller/tmp".into()));
    let (v, _) = f.run(&f.request("x.md")).unwrap();
    assert_eq!(v["state"], "ok", "{v}");
    assert!(f.probes.is_empty());
    assert_eq!(f.after_unit(0)[0], "claude");
    assert!(!f.launches[0].iter().any(|a| a.contains("bwrap")));
    assert!(v.get("limits").is_none() && f.record("x.md").get("limits").is_none());
    let env = f.text("bin/env.1");
    assert!(env.lines().any(|l| l == "TMPDIR=/caller/tmp"), "{env}");
    assert!(!env.contains("CARGO_TARGET_DIR="));
}
