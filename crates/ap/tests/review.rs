use ap::{
    review::{DIFF_OPTIONS, execute},
    work::{Answer, Reply, System},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
};

const TASK: &str = "/home/palantirkc/work/example";
const PROMPT: &str = "/home/palantirkc/work/example/prompt.md";
const REPORT: &str = "/home/palantirkc/work/example/report.md";
const WT: &str = "/home/palantirkc/work/example/wt/repo";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const VERDICT: &[u8] = b"Verdict: accept\nEvidence\n";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(kind: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":if kind == "task" {"example"} else {kind}})
}
struct Step {
    program: &'static str,
    argv: Vec<String>,
    body: Value,
    bytes: Vec<u8>,
    success: bool,
}
fn raw(program: &'static str, argv: &str, body: Value, bytes: &[u8]) -> Step {
    Step {
        program,
        argv: argv.split_whitespace().map(str::to_owned).collect(),
        body,
        bytes: bytes.to_vec(),
        success: true,
    }
}
fn step(program: &'static str, argv: &str, body: Value, reply: Value) -> Step {
    raw(program, argv, body, &serde_json::to_vec(&reply).unwrap())
}
struct Script {
    steps: VecDeque<Step>,
    files: BTreeMap<String, Vec<u8>>,
    cwd: PathBuf,
    calls: Vec<(String, Vec<String>, Value)>,
}
impl Script {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: steps.into(),
            files: BTreeMap::from([
                (PROMPT.into(), b"Review this head\n".to_vec()),
                (REPORT.into(), VERDICT.to_vec()),
            ]),
            cwd: TASK.into(),
            calls: vec![],
        }
    }
    fn finish(&self) {
        assert!(self.steps.is_empty(), "expected call omitted");
    }
}
impl System for Script {
    fn call(&mut self, program: &str, argv: &[String], body: &Value, ms: u64) -> Answer {
        self.calls
            .push((program.into(), argv.to_vec(), body.clone()));
        let expected = self
            .steps
            .pop_front()
            .expect("unexpected call after refusal");
        assert_eq!(
            (program, argv, body),
            (expected.program, expected.argv.as_slice(), &expected.body),
            "call sequence/arguments/body"
        );
        assert_eq!(ms, 30_000);
        Ok(Reply {
            success: expected.success,
            stdout: expected.bytes.clone(),
            bytes: expected.bytes,
        })
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
    fn cwd(&self) -> std::io::Result<PathBuf> {
        Ok(self.cwd.clone())
    }
}
fn get(kind: &str, fields: Value) -> Step {
    let fields: serde_json::Map<String, Value> = fields
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), json!({"kind":"value","value":v})))
        .collect();
    step(
        "ont",
        "get",
        json!({"id":id(kind)}),
        json!({"status":"ok","result":{"objects":[{"id":id(kind),"head":"current","observations":[{"revision":"old","change":"update","fields":{"status":{"kind":"value","value":"running"}}},{"revision":"current","change":"update","fields":fields}]}]}}),
    )
}
fn pr(head: &str, state: &str, base: &str) -> Step {
    step(
        "gh",
        "pr view 12 --repo owner/repo --json state,baseRefName,headRefOid",
        Value::Null,
        json!({"state":state,"baseRefName":base,"headRefOid":head}),
    )
}
fn status(state: &str, description: &str) -> Step {
    let mut s = step(
        "gh",
        &format!(
            "api --method POST repos/owner/repo/statuses/{HEAD} -f state={state} -f context=lead-review -f placeholder"
        ),
        Value::Null,
        json!({"state":state}),
    );
    *s.argv.last_mut().unwrap() = format!("description={description}");
    s
}
fn start_request() -> Value {
    json!({"task_dir":TASK,"kind":"packet","head":HEAD,"base":BASE,"repository":"owner/repo","pull_request":12,"runs":[id("worker_run")],"model":"model","effort":"high","prompt":PROMPT})
}
fn apply(action: &str, request: String, parameters: Value) -> Step {
    step(
        "ont",
        "action apply --client lead",
        json!({"action_type":action,"request_id":request,"parameters":parameters}),
        json!({"status":"ok","result":{"outcome":"applied","edits":if action == "start_review" {
            json!([{"change":"initial","id":id("review")}])
        } else {
            json!([{"change":"update","id":id("review"),"revision":"close-action#0"},{"change":"update","id":id("review"),"revision":"close-action#1"}])
        }}}),
    )
}
fn start_apply() -> Step {
    let prompt_hash = hash(b"Review this head\n");
    apply(
        "start_review",
        format!(
            "rv-start-{}",
            &hash(format!("example\npacket\n{HEAD}\n{prompt_hash}").as_bytes())[..40]
        ),
        json!({"task":id("task"),"kind":"packet","head":HEAD,"base":BASE,"repository":"owner/repo","pull_request":12,"runs":[id("worker_run")],"model":"model","effort":"high","prompt":PROMPT,"prompt_sha256":prompt_hash}),
    )
}
fn review() -> Value {
    json!({"task":id("task"),"status":"closed","verdict":"accept","kind":"packet","repository":"owner/repo","pull_request":12,"head":HEAD,"base":BASE,"report":REPORT,"report_sha256":hash(VERDICT),"runs":[id("worker_run")]})
}
fn run() -> Value {
    json!({"task":id("task"),"write_set":["a/b.rs","docs/**"]})
}
fn accept_request() -> Value {
    json!({"review":id("review"),"worktree":WT})
}
fn git(argv: &str, bytes: &[u8]) -> Step {
    raw(
        "git",
        &format!("--no-replace-objects -C {WT} {argv}"),
        Value::Null,
        bytes,
    )
}
fn accept_steps(f: Value, run: Value, diff: &str) -> Vec<Step> {
    let bytes: Vec<_> = diff
        .lines()
        .flat_map(|path| path.bytes().chain([0]))
        .collect();
    accept_steps_raw(f, run, &bytes)
}
fn accept_steps_raw(f: Value, run: Value, diff: &[u8]) -> Vec<Step> {
    vec![
        get("review", f),
        get("worker_run", run),
        pr(HEAD, "OPEN", "main"),
        git("fetch origin main", b""),
        git(
            &format!("merge-base origin/main {HEAD}"),
            format!("{BASE}\n").as_bytes(),
        ),
        git(
            &format!(
                "diff --no-renames --ignore-submodules=none --no-relative --name-only -z {BASE} {HEAD}"
            ),
            diff,
        ),
    ]
}
fn refusal(s: &mut Script, verb: &str, r: &Value, code: &str, field: &str) -> Value {
    let e = execute(s, verb, r).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), (code, field));
    s.finish();
    assert!(!s.calls.iter().any(|(p, a, _)| (p == "ont"
        && a.starts_with(&["action".into(), "apply".into()]))
        || (p == "gh" && a.first().is_some_and(|s| s == "api"))));
    e.detail
}
#[test]
fn start_exact_calls_and_store() {
    let mut s = Script::new(vec![
        pr(HEAD, "OPEN", "main"),
        get("worker_run", run()),
        start_apply(),
    ]);
    let (v, code) = execute(&mut s, "start", &start_request()).unwrap();
    assert_eq!(
        (v, code),
        (
            json!({"status":"started","outcome":"applied","review":id("review"),"prompt_sha256":hash(b"Review this head\n")}),
            0
        )
    );
    s.finish();
}
#[test]
fn close_exact_calls_and_store() {
    for (verdict, bytes) in [
        ("accept", VERDICT),
        ("repair", b"Verdict: repair\nDetails\n".as_slice()),
    ] {
        let h = hash(bytes);
        let a = apply(
            "close_review",
            format!(
                "rv-close-{}",
                &hash(format!("review\n{h}").as_bytes())[..40]
            ),
            json!({"review":id("review"),"verdict":verdict,"report":REPORT,"report_sha256":h,"account":"b","session":"session"}),
        );
        let mut s = Script::new(vec![a]);
        s.files.insert(REPORT.into(), bytes.to_vec());
        assert_eq!(
            execute(
                &mut s,
                "close",
                &json!({"review":id("review"),"report":REPORT,"account":"b","session":"session"})
            )
            .unwrap(),
            (
                json!({"status":"closed","verdict":verdict,"report_sha256":h}),
                0
            )
        );
        s.finish();
    }
}
#[test]
fn abandoned_exact_calls_and_store() {
    let a = apply(
        "close_review",
        format!("rv-close-{}", &hash(b"review\nabandoned")[..40]),
        json!({"review":id("review"),"verdict":"abandoned"}),
    );
    let mut s = Script::new(vec![a]);
    assert_eq!(
        execute(
            &mut s,
            "close",
            &json!({"review":id("review"),"verdict":"abandoned"})
        )
        .unwrap(),
        (
            json!({"status":"closed","verdict":"abandoned","report_sha256":null}),
            0
        )
    );
    s.finish();
}
#[test]
fn accept_exact_calls_and_repeated_post() {
    let mut steps = accept_steps(review(), run(), "a/b.rs\ndocs/guide.md\n");
    steps.push(status("success", "review review accept"));
    let mut s = Script::new(steps);
    assert_eq!(
        execute(&mut s, "accept", &accept_request()).unwrap(),
        (json!({"status":"accepted","head":HEAD,"paths":2}), 0)
    );
    s.finish();
    s.steps = accept_steps(review(), run(), "a/b.rs\ndocs/guide.md\n").into();
    s.steps.push_back(status("success", "review review accept"));
    execute(&mut s, "accept", &accept_request()).unwrap();
    s.finish();
}
#[test]
fn revoke_exact_calls_and_status() {
    let mut s = Script::new(vec![
        get("review", review()),
        status("failure", "revoked: wrong report"),
    ]);
    assert_eq!(
        execute(
            &mut s,
            "revoke",
            &json!({"review":id("review"),"reason":"wrong report"})
        )
        .unwrap(),
        (json!({"status":"revoked","head":HEAD}), 0)
    );
    s.finish();
}
#[test]
fn prompt_unreadable() {
    for path in [PROMPT, "/tmp/prompt", "/home/palantirkc/work/../prompt"] {
        let mut s = Script::new(vec![]);
        s.files.clear();
        let mut r = start_request();
        r["prompt"] = json!(path);
        refusal(&mut s, "start", &r, "review_prompt_unreadable", "prompt");
    }
}
#[test]
fn start_head_differs() {
    for (head, state) in [(BASE, "OPEN"), (HEAD, "CLOSED")] {
        let mut s = Script::new(vec![pr(head, state, "main")]);
        assert_eq!(
            refusal(
                &mut s,
                "start",
                &start_request(),
                "review_head_differs",
                "head"
            ),
            head
        );
    }
}
#[test]
fn run_unknown_or_other_task() {
    let mut s = Script::new(vec![
        pr(HEAD, "OPEN", "main"),
        step(
            "ont",
            "get",
            json!({"id":id("worker_run")}),
            json!({"status":"ok","result":{"objects":[]}}),
        ),
    ]);
    refusal(
        &mut s,
        "start",
        &start_request(),
        "review_run_unknown",
        "runs",
    );
    let mut f = run();
    f["task"]["native_key"] = json!("other");
    let mut s = Script::new(vec![pr(HEAD, "OPEN", "main"), get("worker_run", f)]);
    refusal(
        &mut s,
        "start",
        &start_request(),
        "review_run_of_other_task",
        "runs",
    );
}
#[test]
fn invalid_input_before_calls() {
    let mut cases = vec![
        ("start", json!([]), "stdin"),
        ("close", json!({"review":id("task")}), "review"),
    ];
    for key in ["task_dir", "prompt", "report", "worktree", "store"] {
        let mut r = start_request();
        r[key] = json!("relative");
        cases.push(("start", r, key));
    }
    for runs in [
        json!([id("worker_run"), id("worker_run")]),
        json!([id("task")]),
        json!("bad"),
    ] {
        let mut r = start_request();
        r["runs"] = runs;
        cases.push(("start", r, "runs"));
    }
    let mut r = start_request();
    r.as_object_mut().unwrap().remove("repository");
    cases.push(("start", r, "repository"));
    for verdict in ["accept", "repair", "unreadable"] {
        cases.push((
            "close",
            json!({"review":id("review"),"verdict":verdict,"report":REPORT}),
            "verdict",
        ));
    }
    cases.push((
        "close",
        json!({"review":id("review"),"verdict":"abandoned","report":REPORT}),
        "report",
    ));
    for reason in ["".into(), " ".into(), "x".repeat(101), "é".repeat(51)] {
        cases.push((
            "revoke",
            json!({"review":id("review"),"reason":reason}),
            "reason",
        ));
    }
    for (verb, r, field) in cases {
        refusal(&mut Script::new(vec![]), verb, &r, "invalid_input", field);
    }
}
#[test]
fn report_unreadable() {
    for (path, bytes) in [
        (REPORT, None),
        ("/tmp/report", Some(VERDICT.to_vec())),
        (REPORT, Some(vec![b'x'; 1_048_577])),
    ] {
        let mut s = Script::new(vec![]);
        s.files.clear();
        if let Some(bytes) = bytes {
            s.files.insert(path.into(), bytes);
        }
        refusal(
            &mut s,
            "close",
            &json!({"review":id("review"),"report":path}),
            "review_report_unreadable",
            "report",
        );
    }
}
#[test]
fn exact_first_line_verdict() {
    for bytes in [
        b"Verdict: accepted\n".as_slice(),
        b"Verdict: accept extra\n",
        b"Verdict: repair_extra\n",
        b"\nVerdict: accept\n",
        b"Verdict: abandoned\n",
        b"Verdict: accept\r\n",
    ] {
        let mut s = Script::new(vec![]);
        s.files.insert(REPORT.into(), bytes.to_vec());
        refusal(
            &mut s,
            "close",
            &json!({"review":id("review"),"report":REPORT}),
            "review_report_lacks_verdict",
            "report",
        );
    }
}
#[test]
fn not_accepting_first_member() {
    for key in [
        "status",
        "verdict",
        "kind",
        "repository",
        "pull_request",
        "head",
        "base",
        "report",
        "report_sha256",
    ] {
        let mut f = review();
        f[key] = Value::Null;
        let mut s = Script::new(vec![get("review", f)]);
        assert_eq!(
            refusal(
                &mut s,
                "accept",
                &accept_request(),
                "review_not_accepting",
                "review"
            ),
            key
        );
    }
    for (key, value) in [
        ("status", "running"),
        ("verdict", "repair"),
        ("verdict", "abandoned"),
        ("kind", "design"),
        ("kind", "integration"),
    ] {
        let mut f = review();
        f[key] = json!(value);
        refusal(
            &mut Script::new(vec![get("review", f)]),
            "accept",
            &accept_request(),
            "review_not_accepting",
            "review",
        );
    }
}
#[test]
fn report_changed() {
    for bytes in [None, Some(b"Verdict: accept\nChanged\n".to_vec())] {
        let mut s = Script::new(vec![get("review", review())]);
        s.files.remove(REPORT);
        if let Some(bytes) = bytes {
            s.files.insert(REPORT.into(), bytes);
        }
        refusal(
            &mut s,
            "accept",
            &accept_request(),
            "review_report_changed",
            "report",
        );
    }
}
#[test]
fn runs_required_and_write_set_required() {
    for runs in [Value::Null, json!([]), json!("bad")] {
        let mut f = review();
        f["runs"] = runs;
        refusal(
            &mut Script::new(vec![get("review", f)]),
            "accept",
            &accept_request(),
            "review_runs_required",
            "runs",
        );
    }
    for patterns in [Value::Null, json!([]), json!([""])] {
        let mut f = run();
        f["write_set"] = patterns;
        assert_eq!(
            refusal(
                &mut Script::new(vec![get("review", review()), get("worker_run", f)]),
                "accept",
                &accept_request(),
                "review_run_lacks_write_set",
                "runs"
            ),
            "worker_run"
        );
    }
}
#[test]
fn accept_head_differs() {
    for (head, state, base) in [
        (BASE, "OPEN", "main"),
        (HEAD, "CLOSED", "main"),
        (HEAD, "OPEN", "release"),
    ] {
        let mut s = Script::new(vec![
            get("review", review()),
            get("worker_run", run()),
            pr(head, state, base),
        ]);
        refusal(
            &mut s,
            "accept",
            &accept_request(),
            "review_head_differs",
            "head",
        );
    }
}
#[test]
fn base_differs() {
    let mut steps = accept_steps(review(), run(), "");
    steps.truncate(5);
    steps[4].bytes = format!("{HEAD}\n").into_bytes();
    assert_eq!(
        refusal(
            &mut Script::new(steps),
            "accept",
            &accept_request(),
            "review_base_differs",
            "base"
        ),
        HEAD
    );
}
#[test]
fn path_match_table_and_empty_diff() {
    for (pattern, path, ok) in [
        ("a/b.rs", "a/b.rs", true),
        ("a/**", "a/b/c.rs", true),
        ("a/**", "a", false),
        ("a/**", "ab/c.rs", false),
        ("a/b.rs", "a/b.rs.bak", false),
        ("a/**", "", true),
    ] {
        let mut f = run();
        f["write_set"] = json!([pattern]);
        let diff = if path.is_empty() {
            String::new()
        } else {
            format!("{path}\n")
        };
        let mut steps = accept_steps(review(), f, &diff);
        if ok {
            steps.push(status("success", "review review accept"));
            let mut s = Script::new(steps);
            assert_eq!(
                execute(&mut s, "accept", &accept_request()).unwrap().0["paths"],
                usize::from(!path.is_empty())
            );
            s.finish();
        } else {
            assert_eq!(
                refusal(
                    &mut Script::new(steps),
                    "accept",
                    &accept_request(),
                    "review_path_outside_write_set",
                    "paths"
                ),
                json!({"paths":[path],"count":1})
            );
        }
    }
}
#[test]
fn accept_reads_nul_separated_paths() {
    let paths = ["dir/x.md", "dir/é.md", "dir/a\nb.md"];
    for terminated in [true, false] {
        let mut bytes = paths.join("\0").into_bytes();
        if terminated {
            bytes.push(0);
        }
        for (pattern, ok) in [("dir/**", true), ("dir/x.md", false)] {
            let mut f = run();
            f["write_set"] = json!([pattern]);
            let mut steps = accept_steps_raw(review(), f, &bytes);
            if ok {
                steps.push(status("success", "review review accept"));
                let mut s = Script::new(steps);
                assert_eq!(
                    execute(&mut s, "accept", &accept_request()).unwrap().0["paths"],
                    3
                );
                s.finish();
            } else {
                assert_eq!(
                    refusal(
                        &mut Script::new(steps),
                        "accept",
                        &accept_request(),
                        "review_path_outside_write_set",
                        "paths"
                    ),
                    json!({"paths":paths[1..],"count":2})
                );
            }
        }
    }
    let mut steps = accept_steps_raw(review(), run(), b"");
    steps.push(status("success", "review review accept"));
    let mut s = Script::new(steps);
    assert_eq!(
        execute(&mut s, "accept", &accept_request()).unwrap().0["paths"],
        0
    );
    s.finish();
}
#[test]
fn accept_refuses_non_utf8_path() {
    let mut f = run();
    f["write_set"] = json!(["dir/**"]);
    refusal(
        &mut Script::new(accept_steps_raw(review(), f, b"dir/\xff.md\0")),
        "accept",
        &accept_request(),
        "process_failed",
        "git",
    );
}
#[test]
fn outside_paths_capped_at_ten() {
    let names: Vec<_> = (0..12).map(|i| format!("outside/{i}")).collect();
    let d = refusal(
        &mut Script::new(accept_steps(review(), run(), &names.join("\n"))),
        "accept",
        &accept_request(),
        "review_path_outside_write_set",
        "paths",
    );
    assert_eq!(d, json!({"paths":names[..10],"count":12}));
}
#[test]
fn caller_worktree_refused_before_status() {
    for cwd in [
        WT,
        "/home/palantirkc/work/example/wt/another",
        "/home/palantirkc/work/example/wt",
    ] {
        let mut s = Script::new(accept_steps(review(), run(), "a/b.rs\n"));
        s.cwd = cwd.into();
        refusal(
            &mut s,
            "accept",
            &accept_request(),
            "review_accept_from_worktree",
            "cwd",
        );
    }
}
#[test]
fn revoke_missing_repository_or_head() {
    for key in ["repository", "head"] {
        let mut f = review();
        f[key] = Value::Null;
        assert_eq!(
            refusal(
                &mut Script::new(vec![get("review", f)]),
                "revoke",
                &json!({"review":id("review"),"reason":"wrong"}),
                "review_not_accepting",
                "review"
            ),
            key
        );
    }
}
#[test]
fn store_refusal_preserves_codes() {
    let mut a = start_apply();
    a.bytes=serde_json::to_vec(&json!({"status":"refused","result":{"criteria":[{"passed":false,"code":"duplicate_running_review"}]}})).unwrap();
    a.success = false;
    let mut s = Script::new(vec![pr(HEAD, "OPEN", "main"), get("worker_run", run()), a]);
    let e = execute(&mut s, "start", &start_request()).unwrap_err();
    assert_eq!(
        (e.code, e.field.as_str(), e.detail),
        (
            "store_refused",
            "apply",
            json!({"codes":["duplicate_running_review"]})
        )
    );
    s.finish();
    assert_eq!(
        s.calls
            .iter()
            .filter(|(p, a, _)| p == "ont" && a[0] == "action")
            .count(),
        1
    );
}
#[test]
fn optional_store_forwarded_everywhere() {
    let mut r = start_request();
    r["store"] = json!("/home/palantirkc/work/example/store.sqlite");
    let mut steps = vec![
        pr(HEAD, "OPEN", "main"),
        get("worker_run", run()),
        start_apply(),
    ];
    for s in &mut steps {
        if s.program == "ont" {
            s.argv
                .extend(["--store".into(), r["store"].as_str().unwrap().into()]);
        }
    }
    let mut s = Script::new(steps);
    execute(&mut s, "start", &r).unwrap();
    s.finish();
}
#[test]
fn malformed_current_review_refused() {
    let mut g = get("review", review());
    let mut v: Value = serde_json::from_slice(&g.bytes).unwrap();
    v["result"]["objects"][0]["observations"][1]["change"] = json!("retraction");
    g.bytes = serde_json::to_vec(&v).unwrap();
    refusal(
        &mut Script::new(vec![g]),
        "accept",
        &accept_request(),
        "store_review_unreadable",
        "review",
    );
}

#[test]
fn process_failure_and_unreadable_store_stop() {
    let mut steps = accept_steps(review(), run(), "a/b.rs\n");
    steps.truncate(4);
    steps[3].success = false;
    refusal(
        &mut Script::new(steps),
        "accept",
        &accept_request(),
        "process_failed",
        "git",
    );
    let mut a = start_apply();
    a.bytes = b"not JSON".to_vec();
    let mut s = Script::new(vec![pr(HEAD, "OPEN", "main"), get("worker_run", run()), a]);
    let e = execute(&mut s, "start", &start_request()).unwrap_err();
    assert_eq!(
        (e.code, e.field.as_str()),
        ("store_answer_unreadable", "answer")
    );
    s.finish();
}
#[test]
fn usage_and_cli_envelope() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    refusal(
        &mut Script::new(vec![]),
        "unknown",
        &json!({}),
        "usage",
        "args",
    );
    let help = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["review", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert_eq!(
        help.stdout,
        b"Usage: ap review <start|close|accept|revoke> < request.json\n"
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(["review", "close"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap(),
        json!({"error":{"code":"invalid_input","field":"review","detail":null}})
    );
}
#[test]
fn integrated_review_union_of_runs() {
    let mut f = review();
    f["kind"] = json!("integrated");
    let mut second = id("worker_run");
    second["native_key"] = json!("second");
    f["runs"] = json!([id("worker_run"), second]);
    let mut g = get(
        "worker_run",
        json!({"task":id("task"),"write_set":["other/**"]}),
    );
    g.body["id"] = second.clone();
    let mut response: Value = serde_json::from_slice(&g.bytes).unwrap();
    response["result"]["objects"][0]["id"] = second;
    g.bytes = serde_json::to_vec(&response).unwrap();
    let mut steps = accept_steps(f, run(), "a/b.rs\nother/c.rs\n");
    steps.insert(2, g);
    steps.push(status("success", "review review accept"));
    let mut s = Script::new(steps);
    assert_eq!(
        execute(&mut s, "accept", &accept_request()).unwrap().0["paths"],
        2
    );
    s.finish();
}

#[test]
fn accept_argv_disables_rename_detection() {
    let steps = accept_steps(review(), run(), "a/b.rs\noutside/old.rs\n");
    assert_eq!(
        steps[3].argv,
        ["--no-replace-objects", "-C", WT, "fetch", "origin", "main"]
    );
    assert_eq!(
        steps[4].argv,
        [
            "--no-replace-objects",
            "-C",
            WT,
            "merge-base",
            "origin/main",
            HEAD
        ]
    );
    assert_eq!(
        steps[5].argv,
        [
            "--no-replace-objects",
            "-C",
            WT,
            "diff",
            "--no-renames",
            "--ignore-submodules=none",
            "--no-relative",
            "--name-only",
            "-z",
            BASE,
            HEAD
        ]
    );
    assert_eq!(
        refusal(
            &mut Script::new(steps),
            "accept",
            &accept_request(),
            "review_path_outside_write_set",
            "paths"
        ),
        json!({"paths":["outside/old.rs"],"count":1})
    );
}
#[test]
fn accept_run_of_other_task() {
    let mut f = run();
    f["task"]["native_key"] = json!("other");
    assert_eq!(
        refusal(
            &mut Script::new(vec![get("review", review()), get("worker_run", f)]),
            "accept",
            &accept_request(),
            "review_run_of_other_task",
            "runs"
        ),
        "worker_run"
    );
}
fn repeated_start_steps(status: &str) -> Vec<Step> {
    let mut a = start_apply();
    let mut response: Value = serde_json::from_slice(&a.bytes).unwrap();
    response["result"]["outcome"] = json!("already_applied");
    a.bytes = serde_json::to_vec(&response).unwrap();
    vec![
        pr(HEAD, "OPEN", "main"),
        get("worker_run", run()),
        a,
        get("review", json!({"status":status})),
    ]
}
#[test]
fn start_repeated_while_running_is_idempotent() {
    let mut s = Script::new(repeated_start_steps("running"));
    assert_eq!(
        execute(&mut s, "start", &start_request()).unwrap(),
        (
            json!({"status":"started","outcome":"already_applied","review":id("review"),"prompt_sha256":hash(b"Review this head\n")}),
            0
        )
    );
    s.finish();
}
#[test]
fn start_repeated_after_close_is_refused() {
    let mut s = Script::new(repeated_start_steps("closed"));
    let e = execute(&mut s, "start", &start_request()).unwrap_err();
    assert_eq!(
        (e.code, e.field.as_str(), e.detail),
        ("review_already_closed", "review", id("review"))
    );
    s.finish();
}
#[test]
fn start_attempt_changes_request_id() {
    let first = start_apply().body["request_id"].clone();
    let mut ids = vec![first];
    for attempt in [2, 3, 99] {
        let mut r = start_request();
        r["attempt"] = json!(attempt);
        let mut a = start_apply();
        let prompt_hash = hash(b"Review this head\n");
        a.body["request_id"] = json!(format!(
            "rv-start-{}",
            &hash(format!("example\npacket\n{HEAD}\n{prompt_hash}\n{attempt}").as_bytes())[..40]
        ));
        assert!(!ids.contains(&a.body["request_id"]));
        ids.push(a.body["request_id"].clone());
        let mut s = Script::new(vec![pr(HEAD, "OPEN", "main"), get("worker_run", run()), a]);
        assert_eq!(
            execute(&mut s, "start", &r).unwrap().0["outcome"],
            "applied"
        );
        s.finish();
    }
    for attempt in [
        json!(1),
        json!(0),
        json!(100),
        json!("2"),
        json!(2.5),
        Value::Null,
        json!(-2),
    ] {
        let mut r = start_request();
        r["attempt"] = attempt;
        refusal(
            &mut Script::new(vec![]),
            "start",
            &r,
            "invalid_input",
            "attempt",
        );
    }
}

#[test]
fn real_git_replace_cannot_hide_changed_paths() {
    use std::{fs, process::Command, time::SystemTime};
    let root = PathBuf::from(std::env::var_os("AP_S03_TEST_ROOT").unwrap()).join(format!(
        "review-git-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _cleanup = Cleanup(root.clone());
    let git = |argv: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(argv)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(output.status.success(), "{argv:?}: {:?}", output.stderr);
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    };
    git(&["init", "--quiet"]);
    git(&["commit", "--quiet", "--allow-empty", "-m", "base"]);
    let base = git(&["rev-parse", "HEAD"]);
    fs::write(root.join("real.rs"), "real\n").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/é.md"), "unicode\n").unwrap();
    git(&["add", "real.rs", "dir/é.md"]);
    git(&["commit", "--quiet", "-m", "real"]);
    let head = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "--quiet", "--detach", &base]);
    fs::write(root.join("fake.rs"), "fake\n").unwrap();
    git(&["add", "fake.rs"]);
    git(&["commit", "--quiet", "-m", "fake"]);
    let fake = git(&["rev-parse", "HEAD"]);
    git(&["replace", &head, &fake]);
    let mut diff = vec!["diff"];
    diff.extend(DIFF_OPTIONS);
    diff.extend([base.as_str(), head.as_str()]);
    assert_eq!(git(&diff), "fake.rs\0");
    let mut protected = vec!["--no-replace-objects"];
    protected.extend(diff);
    assert_eq!(git(&protected), "dir/é.md\0real.rs\0");
    let quoted: Vec<_> = protected
        .iter()
        .copied()
        .filter(|arg| *arg != "-z")
        .collect();
    assert_eq!(git(&quoted), "\"dir/\\303\\251.md\"\nreal.rs");
}
fn revoke_apply(reason: &str) -> Step {
    apply(
        "revoke_review",
        format!("rv-revoke-{}", &hash(b"review")[..40]),
        json!({"review":id("review"),"reason":reason}),
    )
}
fn revoked(reason: &str) -> Value {
    let mut f = review();
    (f["status"], f["revoked_at"], f["revoke_reason"]) = (
        json!("revoked"),
        json!("2026-11-14T00:00:00Z"),
        json!(reason),
    );
    f
}
/// A closed review: the read, then `revoke_review`, then the post; the answer is as before.
#[test]
fn d8_revoke_applies_before_post() {
    let mut s = Script::new(vec![
        get("review", review()),
        revoke_apply("wrong report"),
        status("failure", "revoked: wrong report"),
    ]);
    let r = json!({"review":id("review"),"reason":"wrong report"});
    assert_eq!(
        execute(&mut s, "revoke", &r).unwrap(),
        (json!({"status":"revoked","head":HEAD}), 0)
    );
    s.finish();
    let order: Vec<_> = s
        .calls
        .iter()
        .map(|(p, a, _)| (p.as_str(), a[0].as_str()))
        .collect();
    assert_eq!(order, [("ont", "get"), ("ont", "action"), ("gh", "api")]);
    // The request id is the same for every revoke of that review, whatever the reason.
    let mut s = Script::new(vec![
        get("review", review()),
        revoke_apply("another reason"),
        status("failure", "revoked: another reason"),
    ]);
    let r = json!({"review":id("review"),"reason":"another reason"});
    execute(&mut s, "revoke", &r).unwrap();
    s.finish();
}
/// A store that refuses, or answers what cannot be read, ends the revoke with a failure of the
/// store; nothing is posted.
#[test]
fn d8_revoke_store_refusal_posts_nothing() {
    let refusal =
        json!({"status":"refused","code":"submission_criteria","failed":["review_not_closed"]});
    for (bytes, code) in [
        (serde_json::to_vec(&refusal).unwrap(), "store_refused"),
        (b"{}".to_vec(), "store_answer_unreadable"),
    ] {
        let mut a = revoke_apply("wrong report");
        (a.bytes, a.success) = (bytes, false);
        let mut s = Script::new(vec![get("review", review()), a]);
        let r = json!({"review":id("review"),"reason":"wrong report"});
        let e = execute(&mut s, "revoke", &r).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), (code, "apply"));
        s.finish();
        assert!(!s.calls.iter().any(|(p, _, _)| p == "gh"));
    }
}
/// A review already revoked: nothing is applied and the failure is posted again. A post that fails
/// after the apply leaves exactly this, and the repeated command posts it.
#[test]
fn d8_revoke_of_revoked_posts_again() {
    let mut failed = status("failure", "revoked: first");
    failed.success = false;
    let mut s = Script::new(vec![get("review", review()), revoke_apply("first"), failed]);
    let r = json!({"review":id("review"),"reason":"first"});
    let e = execute(&mut s, "revoke", &r).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("process_failed", "gh"));
    s.finish();
    let mut s = Script::new(vec![
        get("review", revoked("first")),
        status("failure", "revoked: again"),
    ]);
    let r = json!({"review":id("review"),"reason":"again"});
    assert_eq!(
        execute(&mut s, "revoke", &r).unwrap(),
        (json!({"status":"revoked","head":HEAD}), 0)
    );
    s.finish();
    assert!(
        !s.calls
            .iter()
            .any(|(p, a, _)| p == "ont" && a[0] == "action")
    );
    assert_eq!(s.calls.iter().filter(|(p, _, _)| p == "gh").count(), 1);
}
