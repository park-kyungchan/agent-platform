//! `ap unit < request.json` against fakes: the case call (README "One case") in a copy made from a
//! clone of a real Git origin (a bare repository in a temporary directory), its stage on the fake
//! `claude` of tests/fixtures/fake-claude.sh in mode `script` (its account's script runs in the
//! stage's cwd and its stdout is the answer), `systemctl` and the unit launcher replaced as in
//! tests/run.rs; the refusal of a request with `op`; and the forms and verbs that reach no verb
//! (`specs/ap/verbs.md` R4). The work root is the temporary directory's `work/` through
//! `System::work_root`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
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
const IMPLEMENTER_PROMPT: &str = "Implement the page.\n";

struct U {
    t: PathBuf,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    /// `gh` and `ont` calls, in order: a case call makes none.
    calls: Vec<(String, Vec<String>, Value)>,
    launches: usize,
    clock: Duration,
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
        for d in ["bin", "a1", "home", "work/task", "judge"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        write_exec(&t.join("bin/claude"), FAKE);
        std::fs::write(t.join("bin/mode-a1"), "script").unwrap();
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
        }
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    /// A request of the form of the removed unit call, which a member `op` still meets first.
    fn request(&self) -> Value {
        json!({"repository":"o/r","clone":self.p("clone"),"worktree":self.p("work/task/wt/repo"),
            "branch":"ap/unit-test","task_key":"store-task","page":self.p("work/task/TASK.md"),
            "out":self.p("work/task/u1"),"acceptance":self.p("judge/accept.sh"),
            "implementer":{"account":self.p("a1"),"model":"opus","effort":"high","prompt":self.p("judge/implementer.md")},
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
    fn accept_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.t.join("accept-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
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
    fn directory(&self, path: &str) -> std::io::Result<PathBuf> {
        if path == "/root/.claude" {
            return Ok(path.into());
        }
        Host.directory(path)
    }
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        match program {
            // A case call starts no `gh` and no `ont`: each call is recorded and fails.
            "gh" | "ont" => {
                self.calls
                    .push((program.into(), args.to_vec(), input.clone()));
                Ok(Reply {
                    success: false,
                    bytes: vec![],
                    stdout: vec![],
                })
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
            _ => Host.call(program, args, input, ms),
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
        assert!(["claude", "codex", "muse", "agy"].contains(&argv[at + 1].as_str()));
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

// ---- I1 a request of the unit object -----------------------------------------------------------

// spec: specs/ap/unit.md#R1
#[test]
fn i1_a_request_with_the_member_op_names_the_op_as_the_first_argument() {
    const SAID: &str = "ap unit <op> < request.json: the op is the first argument";
    let mut u = U::new();
    // Before any other check: a request of the unit object, a unit call's and a case call's.
    let object = json!({"op": "amend", "request_id": "m", "agent": "lead", "session": "s",
        "unit": "u-one"});
    let mut call = u.request();
    call["op"] = json!("add");
    let mut case = json!({"case": u.p("cases/tc-1.json"), "op": "add"});
    case["agent"] = json!("lead");
    for r in [object.clone(), call, case] {
        let e = u.run(&r).expect_err("refused");
        assert_eq!(
            (e.code, e.field.as_str(), &e.detail),
            ("usage", "op", &json!(SAID)),
            "{r}"
        );
    }
    assert_eq!((u.launches, u.calls.len()), (0, 0));
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
        .write_all(object.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let e: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        (
            &e["error"]["code"],
            &e["error"]["field"],
            &e["error"]["detail"]
        ),
        (&json!("usage"), &json!("op"), &json!(SAID))
    );
}

// ---- What reaches no verb ----------------------------------------------------------------------

/// `ap` with `args`, `input` on standard input: exit, stdout, stderr.
fn ap(args: &[&str], input: &[u8]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // A form that reads no standard input may exit before the write: a broken pipe is no failure.
    let _ = child.stdin.take().unwrap().write_all(input);
    let out = child.wait_with_output().unwrap();
    (out.status.code(), out.stdout, out.stderr)
}
const UNKNOWN: &[u8] = b"{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}\n";

// spec: specs/ap/unit.md#R1
// spec: specs/ap/verbs.md#R4
#[test]
fn r1_a_request_with_neither_case_nor_op_or_no_json_is_refused_as_no_verb() {
    for input in [
        &br#"{"bogus":1}"#[..],
        br"{}",
        br"[]",
        br#""case""#,
        b"not json",
        b"",
    ] {
        let said = String::from_utf8_lossy(input);
        assert_eq!(
            ap(&["unit"], input),
            (Some(2), vec![], UNKNOWN.to_vec()),
            "{said}"
        );
    }
    // A request with `case` is a case call, refused in the form of `ap run`.
    let (code, stdout, stderr) = ap(&["unit"], br#"{"case":"cases/tc-1.json"}"#);
    assert_eq!((code, stdout.is_empty()), (Some(2), true));
    let e: Value = serde_json::from_slice(&stderr).unwrap();
    assert_eq!(
        e,
        json!({"error": {"code": "invalid_input", "field": "case", "detail": null}})
    );
}

// spec: specs/ap/verbs.md#R4
#[test]
fn r4_the_removed_verbs_and_the_page_forms_of_unit_reach_no_verb() {
    let page = "/srv/task/PAGE.md";
    for args in [
        &["unit", page][..],
        &["unit", "--compose-only", page][..],
        &["unit", "--compose-only"][..],
        &["unit", "no-such-word"][..],
        &["accept", page][..],
        &["accept", "--help"][..],
        &["backlog", "task"][..],
        &["backlog", "--help"][..],
        &["row"][..],
        &["row", "--help"][..],
        &["state", "/srv/task"][..],
        &["state", "--help"][..],
    ] {
        assert_eq!(
            ap(args, br#"{"case":"/x.json"}"#),
            (Some(2), vec![], UNKNOWN.to_vec()),
            "ap {args:?}"
        );
    }
    let help = String::from_utf8(ap(&["--help"], b"").1).unwrap();
    for verb in ["accept", "backlog", "row", "state"] {
        assert!(!help.contains(&format!("  {verb}: ")), "{verb}: {help}");
    }
    assert!(
        !help.contains("<page>") && !help.contains("compose-only"),
        "{help}"
    );
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
