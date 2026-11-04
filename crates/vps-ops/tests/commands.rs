use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};
use vps_ops::{
    Result,
    adapters::{
        self,
        ssh::{self, HermesTarget},
    },
    cli, domain,
    host::{hostctl, release_update},
    runner::{CommandOutput, CommandSpec, Runner},
    update::{
        checkpoints, release, retry,
        workflow::{self, BackendIdentity, Checkpoint, DesktopIdentity, Operations},
    },
    util,
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let base = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
        let p = util::resolve(&base)
            .expect("absolute lexical fixture directory")
            .join("test-work")
            .join(util::id());
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(name: &str) -> Value {
    util::json(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("{name}.json")),
    )
    .unwrap()
}
fn ok(s: impl Into<String>) -> CommandOutput {
    let stdout = s.into();
    let n = stdout.len();
    CommandOutput {
        exit_code: 0,
        stdout,
        stderr: String::new(),
        reason: "completed".into(),
        created: true,
        reaped: true,
        term_sent: false,
        kill_sent: false,
        signal_failed: false,
        stdout_bytes_observed: n,
        stderr_bytes_observed: 0,
        stdout_complete: true,
        stderr_complete: true,
        transport_exit_code: Some(0),
        transport_signal: None,
    }
}
#[derive(Default)]
struct Fake {
    outputs: VecDeque<CommandOutput>,
    calls: Vec<CommandSpec>,
}
impl Fake {
    fn new(v: Vec<CommandOutput>) -> Self {
        Self {
            outputs: v.into(),
            calls: vec![],
        }
    }
}
impl Runner for Fake {
    fn command_available(&mut self, _program: &str) -> Result<bool> {
        Ok(true)
    }
    fn run(&mut self, s: CommandSpec) -> Result<CommandOutput> {
        self.calls.push(s);
        self.outputs
            .pop_front()
            .ok_or_else(|| "Unexpected external command in fixture".into())
    }
    fn pause(&mut self, _ms: u64) {}
}
fn setup(d: &Path) -> (Value, HermesTarget) {
    fs::create_dir_all(d.join("architecture")).unwrap();
    let f = fixture("domain");
    let mut t = fixture("backend")[0]["target"].clone();
    t["hostId"] = f["operation"]["hostId"].clone();
    util::save(&d.join("architecture/targets.json"), &t).unwrap();
    (f, serde_json::from_value(t).unwrap())
}
#[test]
fn cli_doctor_inventory_plan_and_verify_injected() {
    let d = Temp::new();
    let (f, _) = setup(&d.0);
    let mut observation = f["observation"].clone();
    let now = util::now();
    observation["observedAtUtc"] = json!(now);
    for r in observation["runtimes"].as_array_mut().unwrap() {
        r["native"]["status"]["answered_at"] = json!(util::seconds(&now));
    }
    for command in ["doctor", "inventory", "plan"] {
        let mut r = Fake::new(vec![ok(observation.to_string())]);
        let args = if command == "plan" {
            vec!["plan".into(), "restart".into()]
        } else {
            vec![command.into()]
        };
        let out = cli::dispatch(&mut r, &args, &d.0).unwrap();
        assert_eq!(out.code, 0, "{}", out.stderr);
        let v: Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(r.calls.len(), 1);
        assert_eq!(r.calls[0].program, "ssh");
        assert!(
            r.calls[0]
                .args
                .contains(&"StrictHostKeyChecking=yes".into())
        );
        if command == "doctor" {
            assert_eq!(v["controlPlanePassed"], true)
        }
        if command == "inventory" {
            assert_eq!(v["inventory"], observation["inventory"]);
            assert_eq!(v["ports"], observation["ports"])
        }
        if command == "plan" {
            assert_eq!(v["executed"], false);
            let request: Value = util::json(v["proposedOperation"].as_str().unwrap()).unwrap();
            domain::validate_operation(&request).unwrap();
            assert_eq!(request["status"], "proposed");
        }
    }
    let file = d.0.join("verify-input.json");
    let mut r = Fake::default();
    util::save(&file, &observation).unwrap();
    let out = cli::dispatch(
        &mut r,
        &["verify".into(), file.to_string_lossy().into()],
        &d.0,
    )
    .unwrap();
    assert_eq!(out.code, 0);
    util::save(
        &file,
        &json!({"observation":null,"properties":{"observation":observation.clone()}}),
    )
    .unwrap();
    assert_eq!(
        cli::dispatch(
            &mut r,
            &["verify".into(), file.to_string_lossy().into()],
            &d.0
        )
        .unwrap()
        .code,
        0
    );
    observation["runtimes"][1]["native"]["adapters"][0]["credentialRejected"] = json!(true);
    util::save(&file, &observation).unwrap();
    let out = cli::dispatch(
        &mut r,
        &["verify".into(), file.to_string_lossy().into()],
        &d.0,
    )
    .unwrap();
    assert_eq!(out.code, 3);
    observation["observedAtUtc"] = json!("2020-01-01T00:00:00Z");
    util::save(&file, &observation).unwrap();
    assert_eq!(
        cli::dispatch(
            &mut r,
            &["verify".into(), file.to_string_lossy().into()],
            &d.0
        )
        .unwrap()
        .code,
        2
    );
    assert!(r.calls.is_empty());
}
#[test]
fn apply_idempotency_and_uncertain_result_never_replay() {
    let d = Temp::new();
    let (f, t) = setup(&d.0);
    let req = f["operation"].clone();
    let run = d.0.join("run");
    let mut r = Fake::new(vec![ok(
        "{\"operation\":\"native-gateway-restart\",\"unchangedLockInode\":true}",
    )]);
    let result = adapters::lifecycle::apply(&mut r, &t, &req, &run, &d.0).unwrap();
    assert_eq!(result["status"], "executed");
    assert_eq!(
        adapters::lifecycle::apply(&mut r, &t, &req, &run, &d.0).unwrap()["replayed"],
        true
    );
    assert_eq!(r.calls.len(), 1);
    let mut changed = req.clone();
    changed["reason"] = json!("different request");
    assert!(adapters::lifecycle::apply(&mut r, &t, &changed, &run, &d.0).is_err());
    let mut uncertain = req.clone();
    uncertain["idempotencyKey"] = json!("uncertain");
    let mut fail = ok("sensitive output discarded");
    fail.exit_code = 124;
    fail.reason = "deadline-exceeded".into();
    fail.term_sent = true;
    fail.stdout_complete = false;
    let mut r = Fake::new(vec![fail]);
    assert!(adapters::lifecycle::apply(&mut r, &t, &uncertain, &run, &d.0).is_err());
    assert!(adapters::lifecycle::apply(&mut r, &t, &uncertain, &run, &d.0).is_err());
    assert_eq!(r.calls.len(), 1);
    let receipt: Value = util::json(run.join("native-operation-execution.json")).unwrap();
    assert_eq!(receipt["stdout"], "");
    assert_eq!(receipt["remoteOutcome"], "uncertain");
    assert_eq!(receipt["automaticReplayPermitted"], false);
}
#[test]
fn apply_cli_exact_confirmation() {
    let d = Temp::new();
    let (f, _) = setup(&d.0);
    let file = d.0.join("request.json");
    util::save(&file, &f["operation"]).unwrap();
    let mut r = Fake::default();
    let args = vec![
        "apply".into(),
        file.display().to_string(),
        "--confirm-exact-target".into(),
        "wrong".into(),
    ];
    assert_eq!(cli::dispatch(&mut r, &args, &d.0).unwrap().code, 1);
    assert!(r.calls.is_empty());
}
#[test]
fn ssh_spawn_nonzero_timeout_and_limit_contracts() {
    let d = Temp::new();
    let (_, t) = setup(&d.0);
    for why in [
        "spawn-failed",
        "nonzero-exit",
        "deadline-exceeded",
        "stdout-limit-exceeded",
        "stderr-limit-exceeded",
        "stream-read-failed",
    ] {
        let mut o = ok("must discard");
        o.created = why != "spawn-failed";
        o.reason = why.into();
        o.exit_code = if why == "deadline-exceeded" { 124 } else { 125 };
        o.stderr = "must also discard".into();
        let mut r = Fake::new(vec![o]);
        let receipt = ssh::execute(
            &mut r,
            &t,
            why,
            "print('fixture')",
            &d.0,
            Default::default(),
        )
        .unwrap();
        assert_eq!(receipt["stdout"], "");
        assert!(!receipt.to_string().contains("must discard"));
        assert_eq!(
            receipt["remoteOutcome"],
            if why == "spawn-failed" {
                "not-started"
            } else {
                "uncertain"
            }
        );
        assert_eq!(receipt["automaticReplayPermitted"], false);
        let saved: Value = util::json(d.0.join(format!("{why}-execution.json"))).unwrap();
        assert_eq!(saved, receipt);
    }
}
#[test]
fn checkpoint_immutable_history_is_authoritative() {
    let d = Temp::new();
    let c = &fixture("workflow")[1];
    let state: Checkpoint = serde_json::from_value({
        let mut s = c["saves"][0].clone();
        s["createdAtUtc"] = json!("2026-10-08T00:00:00Z");
        s["updatedAtUtc"] = s["createdAtUtc"].clone();
        s
    })
    .unwrap();
    checkpoints::save(&d.0, &state).unwrap();
    assert!(checkpoints::save(&d.0, &state).is_err());
    fs::write(d.0.join("state.json"), b"invalid").unwrap();
    assert_eq!(checkpoints::load(&d.0).unwrap().revision, state.revision);
}
struct WorkflowOps {
    fixture: Value,
    calls: Vec<String>,
    saves: Vec<Value>,
}
impl Operations for WorkflowOps {
    fn inspect_backend(&mut self) -> Result<BackendIdentity> {
        self.calls.push("inspect-backend".into());
        Ok(serde_json::from_value(self.fixture["current"].clone())?)
    }
    fn resolve_main(&mut self) -> Result<String> {
        self.calls.push("resolve-main".into());
        Ok("c".repeat(40))
    }
    fn inspect_desktop(&mut self) -> Result<DesktopIdentity> {
        self.calls.push("inspect-desktop".into());
        let key = if self
            .calls
            .iter()
            .filter(|s| *s == "inspect-desktop")
            .count()
            > 1
        {
            "done"
        } else {
            "desk"
        };
        Ok(serde_json::from_value(self.fixture[key].clone())?)
    }
    fn update_desktop(&mut self) -> Result<DesktopIdentity> {
        self.calls.push("update-desktop".into());
        Ok(serde_json::from_value(self.fixture["done"].clone())?)
    }
    fn recover_desktop(&mut self) -> Result<DesktopIdentity> {
        self.calls.push("recover-desktop".into());
        Ok(serde_json::from_value(self.fixture["done"].clone())?)
    }
    fn verify_existing_desktop(&mut self) -> Result<DesktopIdentity> {
        self.calls.push("verify-existing-desktop".into());
        Ok(serde_json::from_value(self.fixture["desk"].clone())?)
    }
    fn prepare_backend(&mut self, sha: &str) -> Result<Value> {
        self.calls.push(format!("prepare:{sha}"));
        Ok(json!({"sha":sha}))
    }
    fn deploy_backend(&mut self, _: &Value) -> Result<()> {
        self.calls.push("deploy".into());
        Ok(())
    }
    fn verify_backend(&mut self, sha: &str) -> Result<BackendIdentity> {
        self.calls.push(format!("verify:{sha}"));
        let mut v = self.fixture["current"].clone();
        v["sha"] = json!(sha);
        v["verified"] = json!(self.fixture["scenario"] != "failed-verify");
        Ok(serde_json::from_value(v)?)
    }
    fn save(&mut self, s: &Checkpoint) -> Result<()> {
        let mut v = serde_json::to_value(s)?;
        v["createdAtUtc"] = json!("@time");
        v["updatedAtUtc"] = json!("@time");
        self.saves.push(v);
        Ok(())
    }
}
#[test]
fn workflow_command_order_and_saved_state_golden() {
    for c in fixture("workflow").as_array().unwrap() {
        let req = &c["request"];
        let mut o = WorkflowOps {
            fixture: c.clone(),
            calls: vec![],
            saves: vec![],
        };
        let request = workflow::Request {
            id: req["id"].as_str().unwrap().into(),
            mode: serde_json::from_value(req["mode"].clone()).unwrap(),
            apply: req["apply"].as_bool().unwrap(),
            backend_sha: None,
            expected_release_sha: req["expectedReleaseSha"].as_str().map(str::to_owned),
            resume: req
                .get("resume")
                .map(|v| serde_json::from_value(v.clone()).unwrap()),
        };
        let result = workflow::run(request, &mut o);
        match result {
            Ok(s) => {
                let mut v = serde_json::to_value(s).unwrap();
                v["createdAtUtc"] = json!("@time");
                v["updatedAtUtc"] = json!("@time");
                assert_eq!(v, c["result"], "{}", c["scenario"]);
            }
            Err(e) => assert_eq!(e.to_string(), c["error"].as_str().unwrap()),
        }
        assert_eq!(json!(o.calls), c["calls"], "{}", c["scenario"]);
        assert_eq!(json!(o.saves), c["saves"], "{}", c["scenario"]);
    }
}
#[test]
fn release_pinning_no_network_and_resume_scope() {
    let d = Temp::new();
    let sha = "a".repeat(40);
    let mut r = Fake::new(vec![ok(format!("{sha}\trefs/tags/v0.22.0\n"))]);
    let t = release::resolve(&mut r, &d.0, "fixture-run", "backend-only", "v0.22.0").unwrap();
    assert_eq!(t["sourceSha"], sha);
    assert_eq!(release::load(&d.0).unwrap(), Some(t.clone()));
    assert_eq!(r.calls.len(), 1);
    assert_eq!(
        release::resolve(&mut r, &d.0, "fixture-run", "backend-only", "v0.22.0").unwrap(),
        t
    );
    assert!(
        release::validate_resume(Some(&t), "backend-only", Some("v0.22.1"), None, None).is_err()
    );
    let mut t = t;
    t["resolution"]["stdout"] = json!("changed");
    util::save(&d.0.join("release-target.json"), &t).unwrap();
    assert!(release::load(&d.0).is_err());
}
#[test]
fn hostctl_command_output_goldens() {
    let mut r = Fake::new(vec![
        ok("up 3 days\n"),
        ok("memory rows\n"),
        ok("disk rows\n"),
    ]);
    let o = hostctl::dispatch(&mut r, "resources").unwrap();
    assert_eq!(
        o.stdout,
        "=== RESOURCES ===\nup 3 days\nmemory rows\ndisk rows\n"
    );
    assert_eq!(o.code, 0);
    let mut r = Fake::new(vec![ok("names images status ports\n"), ok("stats\n")]);
    assert_eq!(
        hostctl::dispatch(&mut r, "docker.status").unwrap().stdout,
        "=== DOCKER STATUS ===\nnames images status ports\nstats\n"
    );
    let mut r = Fake::default();
    let o = hostctl::dispatch(&mut r, "timers").unwrap();
    assert_eq!(o.code, 64);
    assert_eq!(o.stderr, "DENIED: unsupported hostctl command\n");
    assert!(r.calls.is_empty());
    let mut r = Fake::new(vec![
        ok("passwordauthentication no\nport 22\nlistenaddress 100.64.0.1\nother x\n"),
        ok("LISTEN 100.64.0.1:22\nLISTEN :80\n"),
    ]);
    assert_eq!(
        hostctl::dispatch(&mut r, "ssh.status").unwrap().stdout,
        "=== SSH STATUS ===\nlistenaddress 100.64.0.1\npasswordauthentication no\nport 22\nLISTEN 100.64.0.1:22\n"
    );
}
#[test]
fn hostctl_inventory_and_retired_k3s_exit_preserved() {
    let mut missing = ok("");
    missing.exit_code = 127;
    let mut r = Fake::new(vec![
        ok("fixture\n"),
        ok("Linux fixture\n"),
        ok("ID=fixture\nVERSION_ID=1\n"),
        ok("up 1 day\n"),
        ok("8\n"),
        ok("Mem: 16G 2G 10G 0 4G 14G\n"),
        ok("Filesystem Type Size Used Avail Use% Mounted\n/dev/vda1 ext4 100G 20G 80G 20% /\n"),
        ok("active\n"),
        ok("active\n"),
        ok("inactive\n"),
        ok("container rows\n"),
        missing,
    ]);
    let o = hostctl::dispatch(&mut r, "inventory").unwrap();
    assert_eq!(o.code, 127);
    assert_eq!(
        o.stdout,
        "=== HOST INVENTORY ===\nhostname=fixture\nkernel=Linux fixture\nos=fixture 1\nuptime=up 1 day\ncpu=8\nmemory=16G total, 14G available\ndisk=100G total, 80G used, 20G available\n--- SERVICES ---\nssh.socket=active\ndocker=active\nk3s=inactive\n--- DOCKER ---\ncontainer rows\n--- K3S NODE ---\n"
    );
}
#[test]
fn legacy_release_update_injected_success_and_rollback() {
    let d = Temp::new();
    let paths = release_update::ReleasePaths {
        project: d.0.clone(),
        lock: d.0.join("lock"),
    };
    let file = d.0.join("docker-compose.yml");
    let source = "services:\n  hermes-agent:\n    image: fixture:old\n";
    fs::write(&file, source).unwrap();
    fs::write(d.0.join(".env"), "FIXTURE=placeholder\n").unwrap();
    let digest = "nousresearch/hermes-agent@sha256:fixture";
    let mut r = Fake::new(vec![
        ok("{\"tag_name\":\"v0.22.0\"}"),
        ok("pull rows\n"),
        ok(format!("{digest}\n")),
        ok(""),
        ok("up rows\n"),
        ok(""),
        ok("fixture-container\n"),
        ok(format!("status=running health=healthy image={digest}\n")),
    ]);
    assert_eq!(
        release_update::run_with_uid(&mut r, &paths, 0).unwrap(),
        format!(
            "Latest published release: v0.22.0\npull rows\nImmutable image: {digest}\nup rows\nstatus=running health=healthy image={digest}\nHermes latest release update completed successfully: v0.22.0\n"
        )
    );
    assert!(fs::read_to_string(&file).unwrap().contains(digest));
    fs::write(&file, source).unwrap();
    let mut fail = ok("");
    fail.exit_code = 1;
    let mut r = Fake::new(vec![
        ok("{\"tag_name\":\"v0.22.0\"}"),
        ok(""),
        ok(digest),
        ok(""),
        fail,
        ok(""),
    ]);
    assert!(release_update::run_with_uid(&mut r, &paths, 0).is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), source);
    assert!(
        r.calls
            .iter()
            .all(|s| s.program == "docker" || s.program == "curl")
    );
}
#[test]
fn retries_only_admit_proven_pre_effect_results() {
    let d = Temp::new();
    let id = "fixture-native-operation";
    let v = json!({"schemaVersion":1,"operationId":id,"executionBoundary":"Native mutation not executed","status":"blocked","canonicalSha":null,"after":null,"receipt":null,"verification":null,"startedAtUtc":"2026-10-08T00:00:00Z","completedAtUtc":"2026-10-08T00:00:01Z","before":{"targetId":"fixture"}});
    util::record(&d.0.join(format!("desktop-update-result-{id}.json")), &v).unwrap();
    assert_eq!(retry::preflight(&d.0, "fixture").unwrap()["eligible"], true);
    util::record(
        &d.0.join(format!("desktop-native-request-{id}.json")),
        &json!({"schemaVersion":1}),
    )
    .unwrap();
    assert_eq!(
        retry::preflight(&d.0, "fixture").unwrap()["eligible"],
        false
    );
    assert!(retry::rejected(&d.0, "fixture").unwrap()["candidate"].is_null());
}
#[test]
fn update_cli_plan_verify_and_overlay_all_use_fixture_runner() {
    let d = Temp::new();
    fs::create_dir_all(d.0.join("architecture")).unwrap();
    let f = fixture("backend")[0].clone();
    util::save(&d.0.join("architecture/targets.json"), &f["target"]).unwrap();
    util::save(
        &d.0.join("architecture/backend-update-policy.json"),
        &f["policy"],
    )
    .unwrap();
    let sha = f["sha"].as_str().unwrap();
    let mut snapshot = f["input"].clone();
    snapshot["observedAtUtc"] = json!(util::now());
    let mut r = Fake::new(vec![ok(snapshot.to_string())]);
    let out = cli::dispatch(
        &mut r,
        &[
            "update".into(),
            "backend-only".into(),
            "--sha".into(),
            sha.into(),
        ],
        &d.0,
    )
    .unwrap();
    assert_eq!(out.code, 0, "{}", out.stderr);
    let v: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["status"], "planned");
    assert_eq!(v["sourceSha"], sha);
    assert_eq!(v["desktopRunningAlignment"], "not-requested");
    assert_eq!(r.calls.len(), 1);
    assert!(!d.0.join("architecture/desktop-update-target.json").exists());
    let mut r = Fake::new(vec![ok(snapshot.to_string())]);
    let out = cli::dispatch(
        &mut r,
        &["update".into(), "verify".into(), "--sha".into(), sha.into()],
        &d.0,
    )
    .unwrap();
    assert_eq!(out.code, 0, "{}", out.stderr);
    let v: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["mode"], "read-only-verification");
    assert_eq!(v["passed"], true);
    let patch = d.0.join("fixture.patch");
    fs::write(&patch, "synthetic patch\n").unwrap();
    let hash = util::hash(fs::read(&patch).unwrap());
    let path = format!(
        "{}/overlays/{hash}.patch",
        f["policy"]["updaterRoot"].as_str().unwrap()
    );
    let mut r = Fake::new(vec![ok(
        json!({"path":path,"overlay_sha256":hash,"source_sha":sha}).to_string(),
    )]);
    let out = cli::dispatch(
        &mut r,
        &[
            "update".into(),
            "stage-overlay".into(),
            "--sha".into(),
            sha.into(),
            "--patch".into(),
            patch.display().to_string(),
            "--apply".into(),
            "--authorization".into(),
            "fixture instruction".into(),
        ],
        &d.0,
    )
    .unwrap();
    assert_eq!(out.code, 0, "{}", out.stderr);
    let v: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["policyChanged"], false);
    assert_eq!(v["overlay"]["sha256"], hash);
    assert_eq!(r.calls.len(), 1);
}
struct CacheReaders {
    names: Vec<String>,
    bytes: std::collections::BTreeMap<String, Vec<u8>>,
    lists: usize,
    reads: Vec<String>,
    audit: Option<Value>,
}
impl adapters::desktop::ReceiptReaders for CacheReaders {
    fn list(&mut self) -> Result<Vec<String>> {
        self.lists += 1;
        Ok(self.names.clone())
    }
    fn read(&mut self, p: &str) -> Result<Option<Vec<u8>>> {
        self.reads.push(p.into());
        Ok(self.bytes.get(p).cloned())
    }
    fn audit(&mut self) -> Result<Option<Value>> {
        Ok(self.audit.clone())
    }
}
#[test]
fn desktop_receipt_poller_rereads_bytes_and_audits_after_settlement() {
    let f = fixture("desktop")[0].clone();
    let mut raw = f["raw"].clone();
    raw["finished_at"] = Value::Null;
    let correlation = raw["correlation_id"].as_str().unwrap().to_owned();
    let readers = CacheReaders {
        names: vec!["/fixture/other.json".into(), "/fixture/match.json".into()],
        bytes: std::collections::BTreeMap::from([
            (
                "/fixture/other.json".into(),
                serde_json::to_vec(&json!({"correlation_id":"other"})).unwrap(),
            ),
            (
                "/fixture/match.json".into(),
                serde_json::to_vec(&raw).unwrap(),
            ),
        ]),
        lists: 0,
        reads: vec![],
        audit: Some(f["normalized"].clone()),
    };
    let mut p = adapters::desktop::ReceiptPoller::new(readers);
    assert!(p.poll(&correlation, false).unwrap().is_none());
    assert_eq!(p.readers.lists, 1);
    p.readers.bytes.insert(
        "/fixture/match.json".into(),
        serde_json::to_vec(&f["raw"]).unwrap(),
    );
    let receipt = p.poll(&correlation, false).unwrap().unwrap();
    assert_eq!(receipt["postSha"], f["normalized"]["postSha"]);
    assert_eq!(p.readers.lists, 1);
    assert_eq!(p.readers.reads.last().unwrap(), "/fixture/match.json");
    assert_eq!(
        p.poll(&correlation, true).unwrap(),
        Some(f["normalized"].clone())
    );
}
#[test]
fn native_process_deadline_and_stream_bounds_only_reap_owned_child() {
    use vps_ops::runner::ProcessRunner;
    let mut runner = ProcessRunner;
    let mut spec = CommandSpec::new(
        "python3",
        vec![
            "-I".into(),
            "-c".into(),
            "import sys;sys.stdout.write('x'*4096);sys.stdout.flush()".into(),
        ],
    );
    spec.max_stdout_bytes = 64;
    let o = runner.run(spec).unwrap();
    assert_eq!(o.reason, "stdout-limit-exceeded");
    assert_eq!(o.exit_code, 125);
    assert!(o.reaped);
    let mut spec = CommandSpec::new(
        "python3",
        vec![
            "-I".into(),
            "-c".into(),
            "import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(5)".into(),
        ],
    );
    spec.timeout_ms = 200;
    spec.termination_grace_ms = 20;
    spec.reap_timeout_ms = 1000;
    let start = std::time::Instant::now();
    let o = runner.run(spec).unwrap();
    assert_eq!(o.reason, "deadline-exceeded");
    assert_eq!(o.exit_code, 124);
    assert!(o.reaped);
    assert!(o.term_sent);
    assert!(o.kill_sent);
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}
#[test]
fn retained_native_sources_parse_without_execution() {
    use vps_ops::runner::ProcessRunner;
    let mut r = ProcessRunner;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for p in [
        root.join("scripts/hermes-main-update"),
        root.join("deployments/herdr-network/firewall-policy.sh"),
        root.join("deployments/herdr-network/sshd-policy.sh"),
    ] {
        let o = r
            .run(CommandSpec::new(
                "bash",
                vec!["-n".into(), p.display().to_string()],
            ))
            .unwrap();
        assert_eq!(o.exit_code, 0, "{}", p.display())
    }
    let source = "import ast,pathlib,sys\nfor p in pathlib.Path(sys.argv[1]).glob('*.py'): ast.parse(p.read_text())\n";
    let o = r
        .run(CommandSpec::new(
            "python3",
            vec![
                "-I".into(),
                "-c".into(),
                source.into(),
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("protocol")
                    .display()
                    .to_string(),
            ],
        ))
        .unwrap();
    assert_eq!(o.exit_code, 0);
}
#[test]
fn installed_basename_aliases_preserve_denial_and_usage() {
    let d = Temp::new();
    for (alias, args, code, expected) in [
        (
            "hermes-hostctl",
            vec!["unsupported"],
            64,
            "DENIED: unsupported hostctl command\n",
        ),
        (
            "hermes-main-report",
            vec![],
            1,
            "usage: hermes-main-report REPO OLD_SHA NEW_SHA OUTPUT\n",
        ),
    ] {
        let p = d.0.join(alias);
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_vps-ops"), &p).unwrap();
        let o = std::process::Command::new(p)
            .args(args)
            .env("VPS_OPS_ROOT", &d.0)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(code));
        assert_eq!(String::from_utf8(o.stderr).unwrap(), expected);
        assert!(o.stdout.is_empty());
    }
    let p = d.0.join("hermes-canary-control");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_vps-ops"), &p).unwrap();
    let o = std::process::Command::new(p)
        .arg("restart")
        .env("VPS_OPS_ROOT", &d.0)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["reason"], "UNSUPPORTED_INGRESS");
    assert_eq!(v["invocation_attempted"], false);
    assert!(!d.0.join("runs").exists());
}

#[test]
fn hostctl_preserves_child_stderr_and_explicit_shell_suppression() {
    let mut first = ok("partial docker rows\n");
    first.exit_code = 23;
    first.stderr = "docker diagnostic\n".into();
    let mut runner = Fake::new(vec![first]);
    let output = hostctl::dispatch(&mut runner, "docker.status").unwrap();
    assert_eq!(output.code, 23);
    assert_eq!(
        output.stdout,
        "=== DOCKER STATUS ===\npartial docker rows\n"
    );
    assert_eq!(output.stderr, "docker diagnostic\n");
    assert_eq!(runner.calls.len(), 1);

    let mut uptime = ok("uptime rows\n");
    uptime.stderr = "uptime warning\n".into();
    let mut free = ok("memory rows\n");
    free.stderr = "free warning\n".into();
    let mut df = ok("disk rows\n");
    df.exit_code = 1;
    df.stderr = "suppressed df diagnostic\n".into();
    let mut runner = Fake::new(vec![uptime, free, df]);
    let output = hostctl::dispatch(&mut runner, "resources").unwrap();
    assert_eq!(output.code, 0);
    assert_eq!(output.stderr, "uptime warning\nfree warning\n");
    assert_eq!(
        output.stdout,
        "=== RESOURCES ===\nuptime rows\nmemory rows\ndisk rows\n"
    );
}

#[test]
fn hostctl_ssh_pipefail_prints_sorted_matches_before_failure() {
    for (stdout, expected, code) in [
        (
            "port 22\naddressfamily inet\nother value\n",
            "addressfamily inet\nport 22\n",
            7,
        ),
        ("other value\n", "", 1),
    ] {
        let mut sshd = ok(stdout);
        sshd.exit_code = 7;
        sshd.stderr = "sshd diagnostic\n".into();
        let mut runner = Fake::new(vec![sshd]);
        let output = hostctl::dispatch(&mut runner, "ssh.status").unwrap();
        assert_eq!(output.code, code);
        assert_eq!(output.stdout, format!("=== SSH STATUS ===\n{expected}"));
        assert_eq!(output.stderr, "sshd diagnostic\n");
        assert_eq!(runner.calls.len(), 1);
    }
}

#[test]
fn rejected_launch_evidence_is_bounded_regular_and_lexically_resolved() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let real = temp.0.join("real");
    fs::create_dir(&real).unwrap();
    // Exercise dot segments even when the target directory is already absolute and normalized.
    let alias = temp.0.join("real/../alias");
    symlink(&real, &alias).unwrap();
    // Node resolve() collapses '..' without resolving the directory symlink.
    let directory = alias.join("unused/../.");
    let resolved = util::resolve(&directory).unwrap();
    assert_eq!(resolved, util::resolve(&alias).unwrap());
    assert!(
        fs::symlink_metadata(&resolved)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let f = &fixture("rejected-proof")[0];
    let id = f["request"]["operationId"].as_str().unwrap();
    let request = real.join(format!("desktop-native-request-{id}.json"));
    let result_name = format!("desktop-update-result-{id}.json");
    let prior = util::bytes(&f["prior"]).unwrap();
    fs::write(real.join(&result_name), &prior).unwrap();
    let mut proof = f["proof"].clone();
    proof["failedResult"] = json!({"path":resolved.join(&result_name)});
    proof["failedNativeResultSha256"] = json!(util::hash(&prior));
    util::save(&real.join("native-launch-rejection-proof.json"), &proof).unwrap();
    let mut bytes = serde_json::to_vec(&f["request"]).unwrap();
    bytes.resize(2 * 1024 * 1024, b' ');
    fs::write(&request, &bytes).unwrap();
    // The inspector must normalize its input while keeping the symlink's lexical identity.
    let nominated = retry::rejected(&alias, "fixture").unwrap();
    assert_eq!(nominated["candidate"]["operationId"], id);
    assert_eq!(
        nominated["candidate"]["proofRef"],
        json!(resolved.join("native-launch-rejection-proof.json"))
    );
    bytes.push(b' ');
    fs::write(&request, &bytes).unwrap();
    assert!(retry::rejected(&resolved, "fixture").unwrap()["candidate"].is_null());
    bytes.pop();
    let outside = temp.0.join("request-copy.json");
    fs::write(&outside, &bytes).unwrap();
    fs::remove_file(&request).unwrap();
    symlink(&outside, &request).unwrap();
    assert!(retry::rejected(&resolved, "fixture").unwrap()["candidate"].is_null());
    fs::remove_file(&request).unwrap();
    fs::create_dir(&request).unwrap();
    assert!(retry::rejected(&resolved, "fixture").unwrap()["candidate"].is_null());
}

struct ReleaseFixtureRunner {
    inner: Fake,
    missing: Option<&'static str>,
    probes: Vec<String>,
    validation_failure: Option<&'static str>,
    project: PathBuf,
}
impl Runner for ReleaseFixtureRunner {
    fn command_available(&mut self, program: &str) -> Result<bool> {
        self.probes.push(program.into());
        Ok(self.missing != Some(program))
    }
    fn run(&mut self, spec: CommandSpec) -> Result<CommandOutput> {
        if spec.args.ends_with(&["config".into(), "--quiet".into()]) {
            match self.validation_failure {
                Some("runner") => return Err("fixture validation runner failure".into()),
                Some("backup") => {
                    let backups = self.project.join(".update-backups");
                    fs::remove_dir(&backups)?;
                    fs::write(backups, "fixture prevents backup copy")?;
                }
                _ => {}
            }
        }
        self.inner.run(spec)
    }
    fn pause(&mut self, _ms: u64) {}
}
fn release_fixture(project: &Path) -> release_update::ReleasePaths {
    fs::write(
        project.join("docker-compose.yml"),
        "services:\n  hermes-agent:\n    image: fixture:old\n",
    )
    .unwrap();
    fs::write(project.join(".env"), "FIXTURE=placeholder\n").unwrap();
    release_update::ReleasePaths {
        project: project.to_owned(),
        lock: project.join("lock"),
    }
}
#[test]
fn release_update_prechecks_keep_original_messages_and_order() {
    for (index, program) in ["curl", "docker", "python3"].into_iter().enumerate() {
        let temp = Temp::new();
        let paths = release_fixture(&temp.0);
        let mut runner = ReleaseFixtureRunner {
            inner: Fake::default(),
            missing: Some(program),
            probes: vec![],
            validation_failure: None,
            project: temp.0.clone(),
        };
        let error = release_update::run_with_uid(&mut runner, &paths, 0).unwrap_err();
        assert_eq!(error.to_string(), format!("{program} is required."));
        assert_eq!(runner.probes, ["curl", "docker", "python3"][..=index]);
        assert!(runner.inner.calls.is_empty());
        assert!(!paths.lock.exists());
        assert!(!temp.0.join(".update-backups").exists());
    }
}
#[test]
fn release_update_cleans_staged_compose_on_every_failure_boundary() {
    for boundary in ["validation", "runner", "backup", "deploy"] {
        let temp = Temp::new();
        let paths = release_fixture(&temp.0);
        let original = fs::read(temp.0.join("docker-compose.yml")).unwrap();
        let mut failure = ok("");
        failure.exit_code = 1;
        let mut outputs = vec![
            ok("{\"tag_name\":\"v0.22.0\"}"),
            ok(""),
            ok("nousresearch/hermes-agent@sha256:fixture"),
        ];
        outputs.push(if boundary == "validation" {
            failure.clone()
        } else {
            ok("")
        });
        if boundary == "deploy" {
            outputs.extend([failure, ok("")]);
        }
        let mut runner = ReleaseFixtureRunner {
            inner: Fake::new(outputs),
            missing: None,
            probes: vec![],
            validation_failure: Some(boundary),
            project: temp.0.clone(),
        };
        let error = release_update::run_with_uid(&mut runner, &paths, 0).unwrap_err();
        match boundary {
            "validation" => assert_eq!(
                error.to_string(),
                "the generated Compose configuration is invalid."
            ),
            "runner" => assert_eq!(error.to_string(), "fixture validation runner failure"),
            "backup" => assert!(error.to_string().contains("Not a directory")),
            "deploy" => assert!(error.to_string().contains("rolled back")),
            _ => unreachable!(),
        }
        assert_eq!(
            fs::read(temp.0.join("docker-compose.yml")).unwrap(),
            original
        );
        assert!(
            util::names(&temp.0)
                .unwrap()
                .iter()
                .all(|name| !name.starts_with("docker-compose.yml.new.")),
            "candidate leaked at {boundary}"
        );
    }
}
