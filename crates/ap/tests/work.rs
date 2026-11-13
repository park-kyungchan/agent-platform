use ap::work::{Answer, Failure, Host, Reply, System, execute};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    collections::{BTreeMap, VecDeque},
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const PACKET: &str = "/home/palantirkc/work/example/TASK.md";
const REPORT: &str = "/home/palantirkc/work/example/REPORT.md";
const BYTES: &[u8] =
    b"Callers searched:\nBroken tests:\nControls:\nWrite set: crates/ap/src/work.rs, ontology/**\n";
#[derive(Clone)]
enum Response {
    Json(Value),
    Unreadable,
    Raw(Vec<u8>),
    Stderr(Vec<u8>),
    Failed,
}
struct Step {
    program: &'static str,
    argv: Vec<String>,
    body: Option<Value>,
    response: Response,
    success: bool,
    ms: u64,
    elapsed: u64,
}
fn step(program: &'static str, argv: &str, v: Value) -> Step {
    Step {
        program,
        argv: argv.split_whitespace().map(String::from).collect(),
        body: None,
        response: Response::Json(v),
        success: true,
        ms: 5_000,
        elapsed: 0,
    }
}
struct Script {
    steps: VecDeque<Step>,
    files: BTreeMap<String, Vec<u8>>,
    clock: Cell<Instant>,
    exists: bool,
    read_error: Option<std::io::ErrorKind>,
}
impl Script {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: steps.into(),
            files: BTreeMap::from([(PACKET.into(), BYTES.to_vec())]),
            clock: Cell::new(Instant::now()),
            exists: false,
            read_error: None,
        }
    }
    fn finish(&self) {
        assert!(self.steps.is_empty(), "expected native call omitted");
    }
}
impl System for Script {
    fn call(&mut self, program: &str, argv: &[String], body: &Value, ms: u64) -> Answer {
        let expected = self.steps.pop_front().expect("unexpected native call");
        assert_eq!(
            (program, argv),
            (expected.program, expected.argv.as_slice()),
            "native call order/arguments"
        );
        assert_eq!(ms, expected.ms, "native timeout cap");
        if let Some(v) = expected.body {
            assert_eq!(*body, v, "native JSON body");
        }
        self.clock
            .set(self.clock.get() + Duration::from_millis(expected.elapsed));
        let stderr_only = matches!(expected.response, Response::Stderr(_));
        let bytes = match expected.response {
            Response::Json(v) => serde_json::to_vec(&v).unwrap(),
            Response::Unreadable => b"not json".to_vec(),
            Response::Raw(bytes) | Response::Stderr(bytes) => bytes,
            Response::Failed => {
                return Err(Failure {
                    code: "process_failed",
                    field: program.into(),
                    detail: Value::Null,
                });
            }
        };
        Ok(Reply {
            success: expected.success,
            stdout: if stderr_only { vec![] } else { bytes.clone() },
            bytes,
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if let Some(e) = self.read_error {
            return Err(e.into());
        }
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
    fn exists(&self, _: &str) -> std::io::Result<bool> {
        Ok(self.exists)
    }
    fn now(&self) -> Instant {
        self.clock.get()
    }
    fn pause(&mut self, d: Duration) {
        self.clock.set(self.clock.get() + d);
    }
}
fn id(kind: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"fixture","account":"test"},"native_key":kind})
}
fn row(kind: &str) -> Value {
    json!({"status":"ok","result":{"objects":[{"id":id(kind)}],"continuation":null,"basis":{"qualified":true}}})
}
fn hash(bytes: &[u8], n: usize) -> String {
    format!("{:x}", Sha256::digest(bytes))[..n].into()
}
const BINDING: &str = r#"ap-work/1:["w1:p2","w1:t2"]"#;
fn codex_fixture(after: bool) -> Value {
    serde_json::from_str(if after {
        include_str!("fixtures/work-codex-after.json")
    } else {
        include_str!("fixtures/work-codex-before.json")
    })
    .unwrap()
}
fn run_row(status: &str, result: &str) -> Value {
    json!({"status":"ok","result":{"objects":[{"id":id("worker_run"),"head":"current","observations":[
        {"revision":"old","fields":{"session":{"kind":"value","value":"obsolete-session"},"status":{"kind":"value","value":"running"}}},
        {"revision":"current","change":"update","fields":{"name":{"kind":"value","value":"worker"},"status":{"kind":"value","value":status},"session":{"kind":"value","value":BINDING},"result":{"kind":"value","value":result}}}
    ]}]}})
}
fn start_setup() -> (Value, Vec<Step>) {
    let r = json!({"name":"worker","packet":PACKET,"report":REPORT,"task_dir":"/home/palantirkc/work/example","thread":"P12","layer":"harness","tier":"bounded","cwd":"/example","workspace":"w1","model":"fixture-model","effort":"medium","store":"/scratch/store"});
    let mut prompt = step(
        "herdr",
        "agent prompt w1:p2 --wait --until working --until blocked --timeout 30000",
        json!({"result":{}}),
    );
    prompt.argv.insert(
        3,
        format!("Read the file {PACKET} and do exactly what it says. Never open a question dialog and never wait for an answer: a point that only the Lead can decide goes into your report, and you continue with every step that does not depend on it."),
    );
    prompt.ms = 30_000;
    let mut steps = vec![
        step("ont", "search --store /scratch/store", row("thread")),
        step("ont", "search --store /scratch/store", row("plan")),
        step(
            "ont",
            "action validate --store /scratch/store",
            json!({"status":"ok","result":{"valid":true}}),
        ),
        step("herdr", "agent list", json!({"result":{"agents":[]}})),
        step(
            "herdr",
            "tab create --workspace w1 --cwd /example --label worker --no-focus",
            json!({"result":{"tab":{"tab_id":"w1:t2"},"root_pane":{"pane_id":"w1:p2"}}}),
        ),
        step(
            "herdr",
            "agent start worker --kind codex --pane w1:p2 --timeout 90000 -- --no-daemon -C /example -m fixture-model -c model_reasoning_effort=medium --dangerously-bypass-approvals-and-sandbox",
            json!({"result":{}}),
        ),
        step("herdr", "agent list", {
            let mut v = codex_fixture(false);
            v["result"]["agents"].as_array_mut().unwrap().insert(0,
                    json!({"name":"other","pane_id":"w1:p9","tab_id":"w1:t9","agent_status":"idle","agent_session":{"value":"wrong-session"}}));
            v
        }),
        step(
            "ont",
            "action apply --client lead --store /scratch/store",
            json!({"status":"ok","result":{"outcome":"applied","edits":[{"change":"initial","id":id("worker_run")}]}}),
        ),
        prompt,
    ];
    let task = json!({"object_type":"task","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"example"});
    steps[0].body = Some(
        json!({"object_type":"thread","where":{"all":[{"eq":["task",task]},{"eq":["key","P12"]}]},"limit":200}),
    );
    steps[1].body =
        Some(json!({"object_type":"plan","where":{"eq":["thread",id("thread")]},"limit":200}));
    let mut body = json!({"action_type":"start_worker_run","parameters":{"task":task,"name":"worker","packet":{"object_type":"packet","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"example/TASK.md"},"runtime":"codex","thread":id("thread"),"plan":id("plan"),"tier":"bounded","layer":"harness"}});
    steps[2].body = Some(body.clone());
    body["parameters"]["session"] = json!(BINDING);
    body["request_id"] = json!(format!("start-worker-{}", hash(BYTES, 12)));
    steps[7].body = Some(body);
    steps[5].ms = 90_000;
    for i in [0, 1, 2, 7] {
        steps[i].ms = 30_000;
    }
    (r, steps)
}
fn check_error(mut s: Script, verb: &str, r: &Value, code: &str) {
    assert_eq!(execute(&mut s, verb, r).unwrap_err().code, code);
    s.finish();
}
#[test]
fn start_local_refusals() {
    for name in ["", "-bad", "UPPER", "bad_name", "1worker", &"a".repeat(33)] {
        let (mut r, _) = start_setup();
        r["name"] = json!(name);
        check_error(Script::new(vec![]), "start", &r, "name_invalid");
    }
    for (bytes, exists, code) in [
        (None, false, "packet_missing"),
        (Some(vec![]), false, "packet_missing"),
        (Some(BYTES.to_vec()), true, "report_exists"),
    ] {
        let (r, _) = start_setup();
        let mut s = Script::new(vec![]);
        s.files.clear();
        s.exists = exists;
        if let Some(b) = bytes {
            s.files.insert(PACKET.into(), b);
        }
        check_error(s, "start", &r, code);
    }
    for (dirty, bytes, code) in [
        ("dirty", BYTES, "worktree_dirty"),
        (
            "",
            b"Controls:\nWrite set: src\n".as_ref(),
            "packet_lacks_callers_or_broken_tests_line",
        ),
        (
            "",
            b"Callers searched:\nBroken tests:\nWrite set: src\n".as_ref(),
            "packet_lacks_controls_line",
        ),
    ] {
        let (mut r, _) = start_setup();
        r["worktree"] = json!("/tree");
        let mut git = step("git", "-C /tree status --porcelain", Value::Null);
        git.response = Response::Raw(dirty.as_bytes().to_vec());
        let mut s = Script::new(vec![git]);
        s.files.insert(PACKET.into(), bytes.to_vec());
        check_error(s, "start", &r, code);
    }
}
#[test]
fn start_answer_refusals_and_controls() {
    let cases = vec![
        (0, Response::Failed, "process_failed"),
        (
            0,
            Response::Json(json!({"status":"error","code":"search_refused"})),
            "store_search_refused",
        ),
        (0, Response::Unreadable, "store_answer_unreadable"),
        (
            0,
            Response::Json(
                json!({"status":"ok","result":{"objects":[],"basis":{"qualified":true}}}),
            ),
            "thread_not_found",
        ),
        (
            1,
            Response::Json(
                json!({"status":"ok","result":{"objects":[],"basis":{"qualified":true}}}),
            ),
            "plan_not_found",
        ),
        (2, Response::Unreadable, "store_answer_unreadable"),
        (
            2,
            Response::Json(
                json!({"status":"ok","result":{"valid":false,"criteria":[{"passed":false,"code":"plan_over_limit"}]}}),
            ),
            "dispatch_stopped_by_store",
        ),
        (
            2,
            Response::Json(json!({"status":"error","code":"denied"})),
            "dispatch_stopped_by_store",
        ),
        (
            2,
            Response::Json(json!({"status":"ok"})),
            "dispatch_stopped_by_store",
        ),
        (
            3,
            Response::Json(
                json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent_status":"idle","agent_session":{"value":"fixture-session"}}]}}),
            ),
            "agent_exists",
        ),
        (3, Response::Unreadable, "herdr_answer_unreadable"),
        (3, Response::Failed, "herdr_answer_unreadable"),
        (4, Response::Unreadable, "tab_create_failed"),
        (4, Response::Failed, "tab_create_failed"),
        (
            4,
            Response::Json(json!({"result":{"tab":{"tab_id":"w1:t2"}}})),
            "tab_create_failed",
        ),
        (5, Response::Failed, "agent_start_failed"),
        (6, Response::Unreadable, "agent_not_ready"),
        (6, Response::Failed, "agent_not_ready"),
        (7, Response::Unreadable, "store_start_uncertain"),
        (7, Response::Failed, "store_start_uncertain"),
    ];
    for (at, response, code) in cases {
        let (r, mut steps) = start_setup();
        steps.truncate(at + 1);
        steps[at].response = response;
        check_error(Script::new(steps), "start", &r, code);
    }
    for v in [
        json!({}),
        Value::Null,
        json!({"status":"ok"}),
        json!({"result":{"outcome":"applied","edits":[{"change":"initial","id":id("worker_run")}]}}),
        json!({"status":"ok","result":{"outcome":"applied","edits":[{"change":"initial","id":null}]}}),
        json!({"status":"ok","result":{"outcome":"applied","edits":[{"change":"initial","id":{"native_key":"partial"}}]}}),
    ] {
        let (r, mut steps) = start_setup();
        steps.truncate(8);
        steps[7].response = Response::Json(v);
        check_error(Script::new(steps), "start", &r, "store_start_uncertain");
    }
    for cleanup_fails in [false, true] {
        let (r, mut steps) = start_setup();
        steps.truncate(8);
        steps[7].response = Response::Json(
            json!({"status":"error","code":"denied","failed":["criterion",{"not_a_code":"fixture"}]}),
        );
        let mut close = step("herdr", "tab close w1:t2", json!({"result":{}}));
        if cleanup_fails {
            close.response = Response::Failed;
        }
        steps.push(close);
        let mut s = Script::new(steps);
        let e = execute(&mut s, "start", &r).unwrap_err();
        assert_eq!(e.code, "store_start_refused_after_gate");
        assert_eq!(e.detail["codes"], json!(["criterion", "denied"]));
        assert_eq!(e.detail["tab_closed"], !cleanup_fails);
        assert_eq!(e.detail["tab"], "w1:t2");
        s.finish();
    }
}
#[test]
fn start_success_prompt_control_and_deadline() {
    for failed in [false, true] {
        let (r, mut steps) = start_setup();
        if failed {
            steps[8].response = Response::Failed;
        }
        let mut s = Script::new(steps);
        let (v, status) = execute(&mut s, "start", &r).unwrap();
        assert_eq!(status, 0);
        assert_eq!(v["run"], id("worker_run"));
        assert_eq!(v["prompt_target"], "w1:p2");
        assert_eq!(v["prompt"], if failed { "uncertain" } else { "sent" });
        assert!(v.get("started_at").is_none());
        s.finish();
    }
    let (r, mut steps) = start_setup();
    steps.truncate(7);
    steps[6].elapsed = 60_000;
    check_error(Script::new(steps), "start", &r, "agent_not_ready");
    let (r, mut steps) = start_setup();
    steps.truncate(7);
    steps[6].response = Response::Json(json!({"result":{"agents":[]}}));
    steps[6].elapsed = 59_000;
    let mut final_read = step(
        "herdr",
        "agent list",
        json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent_status":"idle","agent_session":{"value":"late"}}]}}),
    );
    final_read.ms = 750;
    final_read.elapsed = 750;
    steps.push(final_read);
    check_error(Script::new(steps), "start", &r, "agent_not_ready");
}
#[test]
fn pagination_and_ambiguity() {
    for kind in ["thread", "plan"] {
        let at = usize::from(kind == "plan");
        let (r, mut steps) = start_setup();
        let mut next = step("ont", "search --store /scratch/store", row(kind));
        next.ms = 30_000;
        let mut q = steps[at].body.clone().unwrap();
        q["continuation"] = json!("next");
        next.body = Some(q);
        steps[at].response = Response::Json(
            json!({"status":"ok","result":{"objects":[],"continuation":"next","basis":{"qualified":true}}}),
        );
        steps.insert(at + 1, next);
        let mut s = Script::new(steps);
        execute(&mut s, "start", &r).unwrap();
        s.finish();
        let (r, mut steps) = start_setup();
        steps.truncate(at + 1);
        steps[at].response = Response::Json(
            json!({"status":"ok","result":{"objects":[{"id":id(kind)},{"id":id(kind)}],"basis":{"qualified":true}}}),
        );
        check_error(
            Script::new(steps),
            "start",
            &r,
            if kind == "thread" {
                "thread_ambiguous"
            } else {
                "plan_ambiguous"
            },
        );
    }
}
fn close_setup() -> (Value, Vec<Step>) {
    let r = json!({"run":id("worker_run"),"name":"worker","tab":"w1:t2","result":"finished","report":REPORT,"store":"/scratch/store"});
    let mut steps = vec![
        step("ont", "get --store /scratch/store", run_row("running", "")),
        step(
            "herdr",
            "tab get w1:t2",
            json!({"result":{"tab":{"tab_id":"w1:t2","label":"worker","pane_count":1}}}),
        ),
        step(
            "herdr",
            "pane get w1:p2",
            json!({"result":{"pane":{"pane_id":"w1:p2","tab_id":"w1:t2"}}}),
        ),
        step("herdr", "agent list", {
            let mut v = codex_fixture(true);
            v["result"]["agents"][0]["agent_status"] = json!("idle");
            v
        }),
        step(
            "ont",
            "action apply --client lead --store /scratch/store",
            json!({"status":"ok","result":{"outcome":"applied","edits":[{"change":"update","id":id("worker_run")}]}}),
        ),
        step(
            "ont",
            "get --store /scratch/store",
            run_row("closed", "finished; session=fixture-session"),
        ),
        step("herdr", "tab close w1:t2", json!({"result":{}})),
    ];
    for i in [0, 4, 5] {
        steps[i].ms = 30_000;
    }
    for i in [0, 5] {
        steps[i].body = Some(json!({"id":id("worker_run")}));
    }
    steps[4].body = Some(
        json!({"action_type":"close_worker_run","request_id":format!("close-{}",hash(b"worker_run",32)),"parameters":{"worker_run":id("worker_run"),"result":"finished; session=fixture-session","report":{"object_type":"report","namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":"example/REPORT.md"}}}),
    );
    (r, steps)
}
#[test]
fn close_refusals_and_controls() {
    for (at, response, code) in [
        (1, Response::Unreadable, "herdr_answer_unreadable"),
        (1, Response::Failed, "herdr_answer_unreadable"),
        (
            1,
            Response::Json(
                json!({"result":{"tab":{"tab_id":"w1:t2","label":"other","pane_count":1}}}),
            ),
            "tab_not_owned",
        ),
        (
            1,
            Response::Json(
                json!({"result":{"tab":{"tab_id":"w1:t2","label":"worker-old","pane_count":1}}}),
            ),
            "tab_not_owned",
        ),
        (3, Response::Unreadable, "agent_status_unreadable"),
        (3, Response::Failed, "agent_status_unreadable"),
        (
            3,
            Response::Json(json!({"result":{"agents":[]}})),
            "agent_not_listed",
        ),
        (
            3,
            Response::Json(
                json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent_session":{"value":"fixture-session"},"agent_status":"working"}]}}),
            ),
            "agent_still_working",
        ),
        (
            3,
            Response::Json(
                json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent_session":{"value":"fixture-session"},"agent_status":"unknown"}]}}),
            ),
            "agent_status_unknown",
        ),
        (
            4,
            Response::Json(json!({"status":"error","code":"denied"})),
            "store_close_refused",
        ),
        (
            4,
            Response::Json(json!({"status":"ok","code":"denied"})),
            "store_close_refused",
        ),
        (4, Response::Json(json!({})), "store_close_uncertain"),
        (4, Response::Json(Value::Null), "store_close_uncertain"),
        (4, Response::Failed, "store_close_uncertain"),
        (4, Response::Unreadable, "store_close_uncertain"),
    ] {
        let (r, mut steps) = close_setup();
        steps.truncate(at + 1);
        steps[at].response = response;
        check_error(Script::new(steps), "close", &r, code);
    }
    for (key, v) in [
        ("run", Value::Null),
        ("result", json!("x".repeat(601))),
        ("report", json!("relative")),
    ] {
        let (mut r, mut steps) = close_setup();
        r[key] = v;
        if key == "result" {
            steps.truncate(4);
        } else {
            steps.clear();
        }
        check_error(Script::new(steps), "close", &r, "invalid_input");
    }
}
#[test]
fn close_success_and_cleanup_failure() {
    for status in ["idle", "done", "blocked", "gone"] {
        for failed in [false, true] {
            let (mut r, mut steps) = close_setup();
            steps[3].response = Response::Json(if status == "gone" {
                json!({"result":{"agents":[]}})
            } else {
                json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent_session":{"value":"fixture-session"},"agent_status":status}]}})
            });
            r["agent_gone"] = json!(status == "gone");
            if status == "gone" {
                let result = "finished; session=unavailable (agent_gone)";
                steps[4].body.as_mut().unwrap()["parameters"]["result"] = json!(result);
                steps[5].response = Response::Json(run_row("closed", result));
            }
            if failed {
                steps[6].response = Response::Failed;
            }
            let mut s = Script::new(steps);
            let (v, exit) = execute(&mut s, "close", &r).unwrap();
            assert_eq!(
                v,
                json!({"status":"closed","outcome":"applied","tab_closed":!failed})
            );
            assert_eq!(exit, if failed { 3 } else { 0 });
            s.finish();
        }
    }
}
#[test]
fn wait_checks_report_after_every_answer() {
    for response in [
        Response::Failed,
        Response::Unreadable,
        Response::Json(json!({"result":{"agent":{"agent_status":"idle"}}})),
        Response::Json(json!({"result":{"event":{"data":{"agent_status":"idle"}}}})),
    ] {
        for present in [false, true] {
            let mut call = step(
                "herdr",
                "agent wait worker --until idle --until done --until blocked --timeout 1000",
                json!({}),
            );
            call.ms = 1000;
            call.response = response.clone();
            let mut s = Script::new(vec![call]);
            if present {
                s.files.insert(REPORT.into(), b"report".to_vec());
            }
            let (v, exit) = execute(
                &mut s,
                "wait",
                &json!({"name":"worker","report":REPORT,"timeout_seconds":1}),
            )
            .unwrap();
            assert_eq!(exit, if present { 0 } else { 3 });
            assert_eq!(
                v["agent_status"],
                if matches!(response, Response::Json(_)) {
                    json!("idle")
                } else {
                    Value::Null
                }
            );
            if present {
                assert_eq!(v["sha256"], hash(b"report", 64));
                assert_eq!(v["bytes"], 6);
            } else {
                assert_eq!(v["status"], "report_missing");
            }
            assert!(v.get("pane_tail").is_none());
            s.finish();
        }
    }
    let mut call = step(
        "herdr",
        "agent wait worker --until idle --until done --until blocked --timeout 1000",
        json!({}),
    );
    call.ms = 1000;
    let mut s = Script::new(vec![call]);
    s.read_error = Some(std::io::ErrorKind::PermissionDenied);
    check_error(
        s,
        "wait",
        &json!({"name":"worker","report":REPORT,"timeout_seconds":1}),
        "io_error",
    );
    check_error(
        Script::new(vec![]),
        "wait",
        &json!({"name":"worker","report":REPORT,"timeout_seconds":-1}),
        "invalid_input",
    );
}
#[test]
fn cli_and_host_boundaries() {
    for input in [b"{} {}".to_vec(), b"[]".to_vec(), vec![b' '; 1_048_577]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
            .args(["work", "start"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        let v: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert!(v["error"]["code"].is_string());
    }
    assert!(
        Host.call("/missing-ap-fixture", &[], &Value::Null, 100)
            .is_err()
    );
    let out = Host
        .call(
            "sh",
            &[
                "-c".into(),
                "printf '{\"code\":\"fixture\"}' >&2; exit 1".into(),
            ],
            &Value::Null,
            1000,
        )
        .unwrap();
    assert!(!out.success);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.bytes).unwrap()["code"],
        "fixture"
    );
    assert!(Host.call("sleep", &["2".into()], &Value::Null, 10).is_err());
}

#[test]
fn input_and_native_exit_refusals() {
    let (r, _) = start_setup();
    for (key, value) in [
        ("cwd", json!("relative")),
        ("model", Value::Null),
        ("continue_dirty", json!("true")),
    ] {
        let mut r = r.clone();
        r[key] = value;
        check_error(Script::new(vec![]), "start", &r, "invalid_input");
    }
    check_error(Script::new(vec![]), "unknown", &json!({}), "usage");
    for (at, code) in [
        (3, "herdr_answer_unreadable"),
        (4, "tab_create_failed"),
        (5, "agent_start_failed"),
    ] {
        let (r, mut steps) = start_setup();
        steps.truncate(at + 1);
        steps[at].success = false;
        check_error(Script::new(steps), "start", &r, code);
    }
    let (mut r, _) = start_setup();
    r["worktree"] = json!("/tree");
    let mut git = step("git", "-C /tree status --porcelain", json!({}));
    git.success = false;
    check_error(Script::new(vec![git]), "start", &r, "io_error");
    let (r, mut steps) = close_setup();
    steps.truncate(2);
    steps[1].success = false;
    check_error(Script::new(steps), "close", &r, "herdr_answer_unreadable");
    let (mut r, steps) = close_setup();
    r["report"] = json!("/outside/REPORT.md");
    check_error(
        Script::new(steps.into_iter().take(4).collect()),
        "close",
        &r,
        "invalid_input",
    );
}
#[test]
fn clean_and_continued_worktree_success() {
    for dirty in [false, true] {
        let (mut r, mut steps) = start_setup();
        r["worktree"] = json!("/tree");
        for i in [2, 7] {
            steps[i].body.as_mut().unwrap()["parameters"]["write_set"] =
                json!(["crates/ap/src/work.rs", "ontology/**"]);
        }
        r["continue_dirty"] = json!(dirty);
        let mut git = step("git", "-C /tree status --porcelain", Value::Null);
        git.response = Response::Raw(if dirty {
            b" M fixture".to_vec()
        } else {
            vec![]
        });
        steps.insert(0, git);
        let mut head = step("git", "-C /tree rev-parse HEAD", Value::Null);
        head.response = Response::Raw(b"fixture-head\n".to_vec());
        steps.insert(1, head);
        let mut s = Script::new(steps);
        assert_eq!(
            execute(&mut s, "start", &r).unwrap().0["worktree_head"],
            "fixture-head"
        );
        s.finish();
    }
}

#[test]
fn empty_report_is_missing() {
    let mut call = step(
        "herdr",
        "agent wait worker --until idle --until done --until blocked --timeout 1000",
        json!({"result":{"agent":{"agent_status":"idle"}}}),
    );
    call.ms = 1000;
    let mut s = Script::new(vec![call]);
    s.files.insert(REPORT.into(), vec![]);
    let (v, exit) = execute(
        &mut s,
        "wait",
        &json!({"name":"worker","report":REPORT,"timeout_seconds":1}),
    )
    .unwrap();
    assert_eq!(v, json!({"status":"report_missing","agent_status":"idle"}));
    assert_eq!(exit, 3);
    s.finish();
}
#[test]
fn search_requires_qualification_and_bounded_pages() {
    for kind in ["thread", "plan"] {
        let (r, mut steps) = start_setup();
        let at = usize::from(kind == "plan");
        steps.truncate(at + 1);
        let mut answer = row(kind);
        answer["result"]["basis"] = json!({"qualified":false});
        steps[at].response = Response::Json(answer);
        check_error(Script::new(steps), "start", &r, "store_search_unqualified");
        let (r, mut steps) = start_setup();
        let body = steps[at].body.clone().unwrap();
        steps.truncate(at);
        for page in 0..20 {
            let mut call = step(
                "ont",
                "search --store /scratch/store",
                json!({"status":"ok","result":{"objects":[],"continuation":"next","basis":{"qualified":true}}}),
            );
            call.ms = 30_000;
            let mut q = body.clone();
            if page > 0 {
                q["continuation"] = json!("next");
            }
            call.body = Some(q);
            steps.push(call);
        }
        check_error(Script::new(steps), "start", &r, "store_search_unbounded");
    }
}
#[test]
fn wait_uses_supplied_pane_and_null_for_absent_status() {
    let mut call = step(
        "herdr",
        "agent wait w1:p2 --until idle --until done --until blocked --timeout 1000",
        json!({"result":{"agent":{}}}),
    );
    call.ms = 1000;
    let mut s = Script::new(vec![call]);
    s.files.insert(REPORT.into(), b"report".to_vec());
    let (v, exit) = execute(
        &mut s,
        "wait",
        &json!({"name":"worker","pane":"w1:p2","report":REPORT,"timeout_seconds":1}),
    )
    .unwrap();
    assert_eq!(exit, 0);
    assert_eq!(v["agent_status"], Value::Null);
    s.finish();
}
#[test]
fn worktree_head_failure_and_stderr_status() {
    for failed in [false, true] {
        let (mut r, mut steps) = start_setup();
        r["worktree"] = json!("/tree");
        for i in [2, 7] {
            steps[i].body.as_mut().unwrap()["parameters"]["write_set"] =
                json!(["crates/ap/src/work.rs", "ontology/**"]);
        }
        let mut status = step("git", "-C /tree status --porcelain", Value::Null);
        status.response = Response::Stderr(b"fixture diagnostic".to_vec());
        let mut head = step("git", "-C /tree rev-parse HEAD", Value::Null);
        head.response = if failed {
            Response::Failed
        } else {
            Response::Raw(b"fixture-head\n".to_vec())
        };
        steps.insert(0, status);
        steps.insert(1, head);
        if failed {
            steps.truncate(2);
            check_error(Script::new(steps), "start", &r, "io_error");
        } else {
            let mut s = Script::new(steps);
            let (v, exit) = execute(&mut s, "start", &r).unwrap();
            assert_eq!(exit, 0);
            assert_eq!(v["worktree_head"], "fixture-head");
            s.finish();
        }
    }
    let out = Host
        .call(
            "sh",
            &["-c".into(), "printf diagnostic >&2".into()],
            &Value::Null,
            1000,
        )
        .unwrap();
    assert!(out.success);
    assert!(out.stdout.is_empty());
    assert_eq!(out.bytes, b"diagnostic");
}
#[test]
fn work_help_names_commands_without_changing_global_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["work", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("ap work <start|wait|close>")
    );
}
#[test]
fn close_binds_tab_session_and_single_pane() {
    let (r, steps) = close_setup();
    let mut s = Script::new(steps);
    assert_eq!(execute(&mut s, "close", &r).unwrap().0["status"], "closed");
    s.finish();
    for count in [json!(2), Value::Null] {
        let (r, mut steps) = close_setup();
        steps.truncate(2);
        steps[1].response = Response::Json(
            json!({"result":{"tab":{"tab_id":"w1:t2","label":"worker","pane_count":count}}}),
        );
        check_error(Script::new(steps), "close", &r, "tab_not_owned");
    }
    for (key, value, gone) in [
        ("tab_id", json!("w1:t9"), false),
        ("pane_id", json!("w1:p9"), false),
        ("name", json!("worker"), true),
        ("name", json!("replacement"), true),
    ] {
        let (mut r, mut steps) = close_setup();
        r["agent_gone"] = json!(gone);
        steps.truncate(4);
        if let Response::Json(v) = &mut steps[3].response {
            v["result"]["agents"][0][key] = value;
        }
        check_error(Script::new(steps), "close", &r, "tab_not_owned");
    }
}
#[test]
fn start_binds_session_to_created_pane() {
    let (r, mut steps) = start_setup();
    steps.truncate(7);
    if let Response::Json(v) = &mut steps[6].response {
        v["result"]["agents"][1]["pane_id"] = json!("w1:p9");
    }
    let mut s = Script::new(steps);
    let e = execute(&mut s, "start", &r).unwrap_err();
    assert_eq!(e.code, "agent_identity_conflict");
    assert_eq!(e.detail, json!({"tab":"w1:t2","pane":"w1:p2"}));
    s.finish();
    for session in [json!({"value":""}), json!("raw-session")] {
        let (r, mut steps) = start_setup();
        let invalid = step(
            "herdr",
            "agent list",
            json!({"result":{"agents":[{"name":"worker","pane_id":"w1:p2","tab_id":"w1:t2","agent":"codex","interactive_ready":true,"agent_status":"idle","agent_session":session}]}}),
        );
        steps[6] = invalid;
        let mut s = Script::new(steps);
        let (v, exit) = execute(&mut s, "start", &r).unwrap();
        assert_eq!(exit, 0);
        assert_eq!(v["session"], Value::Null);
        assert_eq!(v["binding"], BINDING);
        s.finish();
    }
}
fn unacknowledged_replies() -> Vec<(Response, bool)> {
    vec![
        (Response::Unreadable, true),
        (Response::Json(Value::Null), true),
        (Response::Json(json!([])), true),
        (Response::Json(json!({})), true),
        (Response::Json(json!({"result":null})), true),
        (Response::Json(json!({"result":[]})), true),
        (Response::Json(json!({"result":{},"error":null})), true),
        (Response::Stderr(b"{\"result\":{}}".to_vec()), true),
        (Response::Json(json!({"result":{}})), false),
    ]
}
#[test]
fn start_requires_acknowledgement() {
    for (response, success) in unacknowledged_replies() {
        let (r, mut steps) = start_setup();
        steps.truncate(6);
        steps[5].response = response;
        steps[5].success = success;
        check_error(Script::new(steps), "start", &r, "agent_start_failed");
    }
}
#[test]
fn prompt_requires_acknowledgement() {
    for (response, success) in unacknowledged_replies() {
        let (r, mut steps) = start_setup();
        steps[8].response = response;
        steps[8].success = success;
        let mut s = Script::new(steps);
        let (v, exit) = execute(&mut s, "start", &r).unwrap();
        assert_eq!(exit, 0);
        assert_eq!(v["prompt"], "uncertain");
        s.finish();
    }
}
#[test]
fn tab_close_requires_acknowledgement() {
    for (response, success) in unacknowledged_replies() {
        let (r, mut steps) = close_setup();
        steps[6].response = response;
        steps[6].success = success;
        let mut s = Script::new(steps);
        let (v, exit) = execute(&mut s, "close", &r).unwrap();
        assert_eq!(exit, 3);
        assert_eq!(v["tab_closed"], false);
        s.finish();
    }
}
#[test]
fn refusal_cleanup_uses_created_id_and_acknowledgement() {
    for (response, success) in unacknowledged_replies()
        .into_iter()
        .chain([(Response::Json(json!({"result":{}})), true)])
    {
        let acknowledged =
            matches!(&response, Response::Json(v) if v == &json!({"result":{}})) && success;
        let (r, mut steps) = start_setup();
        steps.truncate(8);
        steps[7].response = Response::Json(json!({"status":"error","code":"denied"}));
        let mut close = step("herdr", "tab close w1:t2", Value::Null);
        close.response = response;
        close.success = success;
        steps.push(close);
        let mut s = Script::new(steps);
        let e = execute(&mut s, "start", &r).unwrap_err();
        assert_eq!(e.code, "store_start_refused_after_gate");
        assert_eq!(e.detail["tab_closed"], acknowledged);
        s.finish();
    }
}
#[test]
fn names_accept_native_boundaries() {
    for name in ["a".to_string(), format!("a{}", "9-".repeat(15)) + "z"] {
        let (mut r, mut steps) = start_setup();
        r["name"] = json!(name);
        steps.truncate(1);
        steps[0].response = Response::Unreadable;
        check_error(Script::new(steps), "start", &r, "store_answer_unreadable");
    }
}
#[test]
fn every_page_requires_exact_qualification_before_rows() {
    for kind in ["thread", "plan"] {
        let at = usize::from(kind == "plan");
        for qualified in [
            None,
            Some(Value::Null),
            Some(json!(false)),
            Some(json!("true")),
            Some(json!(1)),
        ] {
            let (r, mut steps) = start_setup();
            let mut page = row(kind);
            page["result"]["objects"] = json!([{"id":id(kind)},{"id":id(kind)}]);
            page["result"].as_object_mut().unwrap().remove("basis");
            if let Some(value) = qualified {
                page["result"]["basis"] = json!({"qualified":value});
            }
            steps[at].response = Response::Json(page);
            steps.truncate(at + 1);
            check_error(Script::new(steps), "start", &r, "store_search_unqualified");
        }
        let (r, mut steps) = start_setup();
        let mut next = step(
            "ont",
            "search --store /scratch/store",
            json!({"status":"ok","result":{"objects":[],"basis":{"qualified":false}}}),
        );
        next.ms = 30_000;
        let mut query = steps[at].body.clone().unwrap();
        query["continuation"] = json!("next");
        next.body = Some(query);
        steps[at].response = Response::Json(
            json!({"status":"ok","result":{"objects":[],"continuation":"next","basis":{"qualified":true}}}),
        );
        steps.truncate(at + 1);
        steps.push(next);
        check_error(Script::new(steps), "start", &r, "store_search_unqualified");
    }
}
#[test]
fn wait_rejects_present_non_string_pane() {
    for pane in [Value::Null, json!(1), json!([]), json!({}), json!(true)] {
        check_error(
            Script::new(vec![]),
            "wait",
            &json!({"name":"worker","report":REPORT,"timeout_seconds":1,"pane":pane}),
            "invalid_input",
        );
    }
}
#[test]
fn nonzero_rev_parse_is_io_error() {
    let (mut r, _) = start_setup();
    r["worktree"] = json!("/tree");
    let mut status = step("git", "-C /tree status --porcelain", Value::Null);
    status.response = Response::Raw(vec![]);
    let mut head = step("git", "-C /tree rev-parse HEAD", Value::Null);
    head.response = Response::Raw(vec![]);
    head.success = false;
    check_error(Script::new(vec![status, head]), "start", &r, "io_error");
}

#[test]
fn codex_capture_lifecycle_records_before_prompt_and_session_at_close() {
    let (r, steps) = start_setup();
    let mut s = Script::new(steps);
    let (started, status) = execute(&mut s, "start", &r).unwrap();
    assert_eq!(status, 0);
    assert_eq!(started["run"], id("worker_run"));
    assert_eq!(started["session"], Value::Null);
    assert_eq!(started["binding"], BINDING);
    assert_eq!(started["pane"], "w1:p2");
    assert_eq!(started["tab"], "w1:t2");
    assert_eq!(started["prompt"], "sent");
    s.finish(); // The script requires apply before exactly one prompt, with no later poll.
    let (mut r, steps) = close_setup();
    r["run"] = started["run"].clone();
    s.steps = steps.into();
    assert_eq!(execute(&mut s, "close", &r).unwrap().0["status"], "closed");
    s.finish(); // The close body and readback include the session from the after fixture.
}
#[test]
fn start_requires_one_initial_worker_run_before_prompt() {
    for edits in [
        json!([]),
        json!([{"change":"initial","id":id("plan")}]),
        json!([{"change":"update","id":id("worker_run")}]),
        json!([{"change":"initial","id":id("worker_run")},{"change":"initial","id":id("worker_run")}]),
    ] {
        let (r, mut steps) = start_setup();
        steps.truncate(8);
        steps[7].response =
            Response::Json(json!({"status":"ok","result":{"outcome":"applied","edits":edits}}));
        check_error(Script::new(steps), "start", &r, "store_start_uncertain");
    }
    let (r, mut steps) = start_setup();
    steps.truncate(8);
    steps[7].success = false;
    check_error(Script::new(steps), "start", &r, "store_start_uncertain");
}
#[test]
fn start_requires_created_tab_kind_and_interactive_readiness() {
    for (key, value) in [
        ("tab_id", json!("w1:t9")),
        ("agent", json!("claude")),
        ("interactive_ready", json!(false)),
        ("interactive_ready", Value::Null),
    ] {
        let (r, mut steps) = start_setup();
        steps.truncate(7);
        if let Response::Json(v) = &mut steps[6].response {
            v["result"]["agents"][1][key] = value;
        }
        steps[6].elapsed = 59_750;
        check_error(Script::new(steps), "start", &r, "agent_not_ready");
    }
    let (r, mut steps) = start_setup();
    steps[6].response = Response::Json(codex_fixture(true));
    let mut s = Script::new(steps);
    let (v, _) = execute(&mut s, "start", &r).unwrap();
    assert_eq!(v["session"], "fixture-session");
    assert_eq!(v["binding"], BINDING); // Still stores the binding when a session is known.
    s.finish();
}
#[test]
fn close_rejects_unbound_and_noncanonical_bindings() {
    for binding in [
        Value::Null,
        json!("01abcdef-fixture-id"),
        json!("unknown"),
        json!(""),
        json!(r#"ap-work/1:["w1:p2","w1:t2","extra"]"#),
        json!(r#"ap-work/1:["w1:p2", "w1:t2"]"#),
        json!(r#"ap-work/1:["","w1:t2"]"#),
        json!(r#"ap-work/1:["w1:p2",2]"#),
        json!(format!("ap-work/1:{}", json!(["x".repeat(200), "w1:t2"]))),
    ] {
        for gone in [false, true] {
            let (mut r, mut steps) = close_setup();
            r["agent_gone"] = json!(gone);
            steps.truncate(1);
            if let Response::Json(v) = &mut steps[0].response {
                v["result"]["objects"][0]["observations"][1]["fields"]["session"]["value"] =
                    binding.clone();
            }
            check_error(Script::new(steps), "close", &r, "run_binding_missing");
        }
    }
    let (r, mut steps) = close_setup();
    steps.truncate(1);
    if let Response::Json(v) = &mut steps[0].response {
        v["result"]["objects"][0]["observations"][1]["fields"]
            .as_object_mut()
            .unwrap()
            .remove("session");
    }
    check_error(Script::new(steps), "close", &r, "run_binding_missing");
}
#[test]
fn close_checks_exact_run_and_sole_bound_pane_even_when_gone() {
    let (r, mut steps) = close_setup();
    steps.truncate(1);
    steps[0].response = Response::Json(run_row("closed", "finished"));
    check_error(Script::new(steps), "close", &r, "worker_run_not_running");
    for gone in [false, true] {
        let (mut r, mut steps) = close_setup();
        r["agent_gone"] = json!(gone);
        steps.truncate(3);
        steps[2].response =
            Response::Json(json!({"result":{"pane":{"pane_id":"w1:p2","tab_id":"w1:t9"}}}));
        check_error(Script::new(steps), "close", &r, "tab_not_owned");
    }
    for key in ["name", "tab"] {
        let (mut r, mut steps) = close_setup();
        r[key] = json!("different");
        steps.truncate(1);
        check_error(Script::new(steps), "close", &r, "tab_not_owned");
    }
    let (r, mut steps) = close_setup();
    steps.truncate(1);
    if let Response::Json(v) = &mut steps[0].response {
        v["result"]["objects"][0]["id"]["native_key"] = json!("another-run");
    }
    check_error(Script::new(steps), "close", &r, "store_run_unreadable");
}
#[test]
fn close_known_session_must_match_live_before_write() {
    let (mut r, mut steps) = close_setup();
    r["session"] = json!("another-session");
    steps.truncate(4);
    check_error(Script::new(steps), "close", &r, "session_mismatch");
    for known in [json!("fixture-session"), Value::Null] {
        let (mut r, steps) = close_setup();
        r["session"] = known;
        let mut s = Script::new(steps);
        assert_eq!(execute(&mut s, "close", &r).unwrap().0["status"], "closed");
        s.finish();
    }
    for session in [Value::Null, json!({"value":""}), json!("raw-session")] {
        let (r, mut steps) = close_setup();
        steps.truncate(4);
        if let Response::Json(v) = &mut steps[3].response {
            v["result"]["agents"][0]["agent_session"] = session;
        }
        check_error(Script::new(steps), "close", &r, "session_not_reported");
    }
}
#[test]
fn close_requires_one_updated_run_and_closed_readback_before_tab_close() {
    for edits in [
        json!([]),
        json!([{"change":"initial","id":id("worker_run")}]),
        json!([{"change":"update","id":id("plan")}]),
        json!([{"change":"update","id":id("worker_run")},{"change":"update","id":id("worker_run")}]),
        json!([{"change":"update","id":{"object_type":"worker_run","namespace":{"instance":"fixture","account":"test"},"native_key":"other"}}]),
    ] {
        let (r, mut steps) = close_setup();
        steps.truncate(5);
        steps[4].response =
            Response::Json(json!({"status":"ok","result":{"outcome":"applied","edits":edits}}));
        check_error(Script::new(steps), "close", &r, "store_close_uncertain");
    }
    for (key, value) in [
        ("status", "running"),
        ("result", "different"),
        ("session", "unknown"),
        ("name", "other"),
    ] {
        let (r, mut steps) = close_setup();
        steps.truncate(6);
        if let Response::Json(v) = &mut steps[5].response {
            v["result"]["objects"][0]["observations"][1]["fields"][key]["value"] = json!(value);
        }
        check_error(Script::new(steps), "close", &r, "store_close_uncertain");
    }
}
#[test]
fn close_result_byte_limit_includes_observed_session() {
    for fits in [false, true] {
        let (mut r, mut steps) = close_setup();
        let result = format!("{}{}", "é".repeat(287), if fits { "x" } else { "xx" });
        let stored = format!("{result}; session=fixture-session");
        assert_eq!(stored.len(), if fits { 600 } else { 601 });
        r["result"] = json!(result);
        if fits {
            steps[4].body.as_mut().unwrap()["parameters"]["result"] = json!(stored);
            steps[5].response = Response::Json(run_row("closed", &stored));
            let mut s = Script::new(steps);
            assert_eq!(execute(&mut s, "close", &r).unwrap().0["status"], "closed");
            s.finish();
        } else {
            steps.truncate(4);
            check_error(Script::new(steps), "close", &r, "invalid_input");
        }
    }
}
#[test]
fn unreadable_run_or_pane_never_advances_close() {
    for at in [0, 2, 5] {
        for response in [
            Response::Failed,
            Response::Unreadable,
            Response::Json(json!({"error":{"code":"fixture"}})),
        ] {
            let (r, mut steps) = close_setup();
            steps.truncate(at + 1);
            steps[at].response = response;
            check_error(
                Script::new(steps),
                "close",
                &r,
                match at {
                    0 => "store_run_unreadable",
                    2 => "herdr_answer_unreadable",
                    _ => "store_close_uncertain",
                },
            );
        }
    }
    for at in [0, 2, 4, 5] {
        let (r, mut steps) = close_setup();
        steps.truncate(at + 1);
        steps[at].success = false;
        check_error(
            Script::new(steps),
            "close",
            &r,
            match at {
                0 => "store_run_unreadable",
                2 => "herdr_answer_unreadable",
                _ => "store_close_uncertain",
            },
        );
    }
    for at in [0, 5] {
        for head in [Value::Null, json!("missing")] {
            let (r, mut steps) = close_setup();
            steps.truncate(at + 1);
            if let Response::Json(v) = &mut steps[at].response {
                v["result"]["objects"][0]["head"] = head;
            }
            check_error(
                Script::new(steps),
                "close",
                &r,
                if at == 0 {
                    "store_run_unreadable"
                } else {
                    "store_close_uncertain"
                },
            );
        }
    }
}

#[test]
fn start_requires_fresh_apply_before_prompt() {
    for outcome in [
        Some(json!("already_applied")),
        None,
        Some(json!("unknown")),
        Some(Value::Null),
    ] {
        let (r, mut steps) = start_setup();
        steps.truncate(8);
        if let Response::Json(v) = &mut steps[7].response {
            v["result"].as_object_mut().unwrap().remove("outcome");
            if let Some(outcome) = outcome {
                v["result"]["outcome"] = outcome;
            }
        }
        check_error(Script::new(steps), "start", &r, "store_start_uncertain");
    }
}
#[test]
fn agent_list_error_key_refuses_even_with_valid_agents() {
    for at in [3, 6] {
        let (r, mut steps) = start_setup();
        steps.truncate(at + 1);
        if let Response::Json(v) = &mut steps[at].response {
            v["error"] = Value::Null;
        }
        check_error(
            Script::new(steps),
            "start",
            &r,
            if at == 3 {
                "herdr_answer_unreadable"
            } else {
                "agent_not_ready"
            },
        );
    }
}
#[test]
fn tab_create_error_key_refuses_even_with_valid_ids() {
    let (r, mut steps) = start_setup();
    steps.truncate(5);
    if let Response::Json(v) = &mut steps[4].response {
        v["error"] = Value::Null;
    }
    check_error(Script::new(steps), "start", &r, "tab_create_failed");
}
#[test]
fn close_replayed_apply_requires_closed_matching_readback() {
    for mismatch in [
        None,
        Some(("status", "running")),
        Some(("result", "another result")),
    ] {
        let (r, mut steps) = close_setup();
        if let Response::Json(v) = &mut steps[4].response {
            v["result"]["outcome"] = json!("already_applied");
        }
        if let Some((key, value)) = mismatch {
            steps.truncate(6);
            if let Response::Json(v) = &mut steps[5].response {
                v["result"]["objects"][0]["observations"][1]["fields"][key]["value"] = json!(value);
            }
            check_error(Script::new(steps), "close", &r, "store_close_uncertain");
        } else {
            let mut s = Script::new(steps);
            let (v, status) = execute(&mut s, "close", &r).unwrap();
            assert_eq!(status, 0);
            assert_eq!(
                v,
                json!({"status":"closed","outcome":"already_applied","tab_closed":true})
            );
            s.finish();
        }
    }
}

fn worktree_steps() -> Vec<Step> {
    let mut status = step("git", "-C /tree status --porcelain", Value::Null);
    status.response = Response::Raw(vec![]);
    let mut head = step("git", "-C /tree rev-parse HEAD", Value::Null);
    head.response = Response::Raw(b"fixture-head\n".to_vec());
    vec![status, head]
}
#[test]
fn write_set_line_required() {
    let (mut r, _) = start_setup();
    r["worktree"] = json!("/tree");
    let mut steps = worktree_steps();
    steps.truncate(1);
    let mut s = Script::new(steps);
    s.files.insert(
        PACKET.into(),
        b"Callers searched:\nBroken tests:\nControls:\n".to_vec(),
    );
    let e = execute(&mut s, "start", &r).unwrap_err();
    assert_eq!(e.code, "packet_lacks_write_set_line");
    assert_eq!(e.field, "packet");
    s.finish(); // No store call occurred.
}
#[test]
fn write_set_invalid_members() {
    for line in [
        "/absolute".into(),
        "a/../b".into(),
        "a\\b".into(),
        "src/*.rs".into(),
        "a,,b".into(),
        vec!["a"; 65].join(","),
        "".into(),
        "a/./b".into(),
        "a//b".into(),
        "a?".into(),
        "a[".into(),
        "a/".into(),
        "a".repeat(301),
        "a".repeat(298),
        format!("{}/**", "a".repeat(298)),
        "src/\u{00e9}.rs".into(),
        "a:b".into(),
        "a b".into(),
        "`a/b.rs`".into(),
        "a,a".into(),
        "a\nWrite set: b".into(),
    ] {
        let (mut r, _) = start_setup();
        r["worktree"] = json!("/tree");
        let mut steps = worktree_steps();
        steps.truncate(1);
        let mut s = Script::new(steps);
        s.files.insert(
            PACKET.into(),
            format!("Callers searched:\nBroken tests:\nControls:\nWrite set: {line}\n")
                .into_bytes(),
        );
        check_error(s, "start", &r, "write_set_invalid");
    }
}
#[test]
fn write_set_sent_in_order() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
    assert_eq!(
        schema["value_types"]["write_pattern"]["pattern"],
        r"^[A-Za-z0-9._][A-Za-z0-9._/+@-]{0,296}(/\*\*)?$"
    );
    for members in [
        vec!["crates/ap/src/work.rs".to_owned(), "ontology/**".to_owned()],
        vec!["a".repeat(297)],
        vec![format!("{}/**", "a".repeat(297))],
    ] {
        for member in &members {
            let path = member.strip_suffix("/**").unwrap_or(member);
            assert!((1..=297).contains(&path.len()));
            assert!(
                path.as_bytes()[0].is_ascii_alphanumeric() || b"._".contains(&path.as_bytes()[0])
            );
            assert!(
                path.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._/+@-".contains(&c))
            );
        }
        let packet_bytes = format!(
            "Callers searched:\nBroken tests:\nControls:\nWrite set: {}\n",
            members.join(", ")
        )
        .into_bytes();
        let (mut r, mut steps) = start_setup();
        r["worktree"] = json!("/tree");
        for i in [2, 7] {
            steps[i].body.as_mut().unwrap()["parameters"]["write_set"] = json!(members);
        }
        steps[7].body.as_mut().unwrap()["request_id"] =
            json!(format!("start-worker-{}", hash(&packet_bytes, 12)));
        let mut calls = worktree_steps();
        calls.extend(steps);
        let mut s = Script::new(calls);
        s.files.insert(PACKET.into(), packet_bytes);
        assert_eq!(execute(&mut s, "start", &r).unwrap().0["status"], "started");
        s.finish(); // Exact JSON body assertions inspect validate and apply.
    }
}
#[test]
fn no_worktree_sends_no_write_set() {
    let (r, steps) = start_setup();
    assert!(
        steps[7].body.as_ref().unwrap()["parameters"]
            .get("write_set")
            .is_none()
    );
    let mut s = Script::new(steps);
    assert_eq!(execute(&mut s, "start", &r).unwrap().0["status"], "started");
    s.finish();
}
