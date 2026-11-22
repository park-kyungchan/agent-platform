//! `ap resource` and the functions `capacity` and `gate` (`specs/ap/resource.md`) against a fake host: the fake
//! answers `ont` (`search`, `get`, `action apply`, `index`, `log`, `render`) from objects it holds, reads `/proc`
//! from invented texts, and puts a stand-in `tailscale` (a fake PC, as the one of `tests/check.rs`) first on the
//! `PATH` its environment gives. Each test writes under a temporary directory of its own; no test reaches the host's
//! store, policy file, checks log, resource directory or the real `tailscale`. The tests marked `#[ignore]` run the
//! real engine in a scratch store (`./check` runs them with `--ignored` and `ONT_BIN`).
use ap::host::{Answer, Failure, Reply, System};
use ap::resource::{self, Options};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::ErrorKind,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// 2027-01-15T08:00:00Z.
const NOW: u64 = 1_800_000_000;
const DAY: u64 = 86_400;
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A new empty directory under the temporary directory, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ap-resource-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(std::fs::canonicalize(&p).unwrap())
    }
    fn at(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn path(&self, name: &str) -> String {
        self.at(name).to_str().unwrap().to_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The fake PC of `tests/check.rs`, cut to the two calls of the pass. ping: answers unless `down` exists. ssh: notes
/// the text it was given; the probe reads the fake PC's loadavg and meminfo and 4 CPUs; with `garbage` it answers a
/// line no reader takes.
const TAILSCALE: &str = r#"#!/bin/sh
pc=@PC@
case $1 in
ping)
  echo ping >> "$pc/calls"
  if [ -e "$pc/down" ]; then exit 1; fi
  exit 0 ;;
ssh)
  shift; dest=$1; shift; cmd=$*
  printf 'ssh %s %s\n' "$dest" "$cmd" >> "$pc/calls"
  case $cmd in *ap-resource-probe*) ;; *) exit 2 ;; esac
  if [ -e "$pc/garbage" ]; then echo "no reading here"; exit 0; fi
  cmd=$(printf '%s' "$cmd" | sed "s#/proc/loadavg#$pc/loadavg#; s#/proc/meminfo#$pc/meminfo#; s#(nproc)#(echo 4)#")
  env -i PATH=/usr/bin:/bin sh -c "$cmd"; exit ;;
esac
exit 2
"#;

fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type": kind, "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}
fn utc(secs: u64) -> String {
    ap::watch::utc(secs)
}
fn reply(success: bool, v: Value) -> Answer {
    let bytes = serde_json::to_vec(&v).unwrap();
    Ok(Reply {
        success,
        bytes: bytes.clone(),
        stdout: bytes,
    })
}
fn broken(program: &str) -> Answer {
    Err(Failure {
        code: "process_failed",
        field: program.into(),
        detail: Value::Null,
    })
}
fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Number(a), Value::Number(b)) => a.as_f64()?.partial_cmp(&b.as_f64()?),
        _ => None,
    }
}
/// The filters the verbs send, as the engine reads them.
fn matches(f: &Map<String, Value>, w: &Value) -> bool {
    if let Some(all) = w.get("all") {
        return all.as_array().unwrap().iter().all(|x| matches(f, x));
    }
    let (op, a) = w.as_object().unwrap().iter().next().unwrap();
    let field = f
        .get(a[0].as_str().unwrap())
        .cloned()
        .unwrap_or(Value::Null);
    match op.as_str() {
        "eq" => field == a[1],
        "in" => a[1].as_array().unwrap().contains(&field),
        "ge" => compare(&field, &a[1]).is_some_and(|o| o.is_ge()),
        "gt" => compare(&field, &a[1]).is_some_and(|o| o.is_gt()),
        other => panic!("filter {other}"),
    }
}

/// The fake host.
struct Fake {
    dir: Scratch,
    policy: Option<PathBuf>,
    files: BTreeMap<String, Vec<u8>>,
    clock: u64,
    clock_moves: bool,
    pauses: u32,
    objects: BTreeMap<(String, String), Map<String, Value>>,
    /// `<command> <kind or action>` that fail: `search unit`, `get host`, `apply observe_host`, `index`, `log`.
    fail: BTreeSet<String>,
    unqualified: BTreeSet<String>,
    index: Value,
    logs: BTreeMap<String, Vec<Value>>,
    views: BTreeMap<(String, String), Vec<String>>,
    calls: Vec<(String, Value)>,
}
impl Fake {
    fn new(policy: Option<Value>) -> Self {
        let dir = Scratch::new();
        let policy = policy.map(|p| {
            let file = dir.at("policy.json");
            std::fs::write(&file, p.to_string()).unwrap();
            file
        });
        std::fs::create_dir_all(dir.at("bin")).unwrap();
        std::fs::create_dir_all(dir.at("pc")).unwrap();
        let ts = dir.at("bin/tailscale");
        std::fs::write(&ts, TAILSCALE.replace("@PC@", &dir.path("pc"))).unwrap();
        std::fs::set_permissions(&ts, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.at("pc/loadavg"), "1.00 0.80 0.70 2/300 4242\n").unwrap();
        pc_memory(&dir, 8_388_608);
        let mut files = BTreeMap::new();
        files.insert(
            "/proc/stat".into(),
            b"cpu  10 20 30\ncpu0 1 2 3\ncpu1 4 5 6\nintr 7\n".to_vec(),
        );
        files.insert(
            "/proc/loadavg".into(),
            b"0.52 0.40 0.30 1/200 999\n".to_vec(),
        );
        files.insert(
            "/proc/meminfo".into(),
            b"MemTotal:        4000000 kB\nMemFree:          100000 kB\nMemAvailable:    3145728 kB\n".to_vec(),
        );
        files.insert(
            "/proc/sys/kernel/hostname".into(),
            b"fixture-vps\n".to_vec(),
        );
        let index = json!({"status": "ok", "result": {"sources": [
            {"source": "host_samples", "status": "ok", "appended": {"initial": 1}, "renewed": 0, "unchanged": 0},
            {"source": "checks", "status": "ok", "appended": {"initial": 0}, "renewed": 0, "unchanged": 0}]}});
        Fake {
            dir,
            policy,
            files,
            clock: NOW,
            clock_moves: true,
            pauses: 0,
            objects: BTreeMap::new(),
            fail: BTreeSet::new(),
            unqualified: BTreeSet::new(),
            index,
            logs: BTreeMap::new(),
            views: BTreeMap::new(),
            calls: vec![],
        }
    }
    fn resource(&self) -> PathBuf {
        self.dir.at("resource")
    }
    fn options(&self) -> Options {
        Options {
            directory: Some(self.resource().to_str().unwrap().to_owned()),
            store: Some(self.dir.path("store.sqlite")),
        }
    }
    fn put(&mut self, kind: &str, key: &str, fields: Value) {
        self.objects.insert(
            (kind.into(), key.into()),
            fields.as_object().unwrap().clone(),
        );
    }
    fn fields(&self, kind: &str, key: &str) -> Option<&Map<String, Value>> {
        self.objects.get(&(kind.to_owned(), key.to_owned()))
    }
    /// The action requests applied, in order.
    fn applied(&self) -> Vec<&Value> {
        self.calls
            .iter()
            .filter(|(c, _)| c == "action")
            .map(|(_, b)| b)
            .collect()
    }
    fn applied_of(&self, action: &str) -> Vec<&Value> {
        self.applied()
            .into_iter()
            .filter(|b| b["action_type"] == action)
            .collect()
    }
    fn pc_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.at("pc/calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn pass(&mut self) -> (Value, bool) {
        let o = self.options();
        resource::pass(self, &o).unwrap()
    }
    fn samples(&self, t: u64) -> Vec<Value> {
        let day: String = utc(t)[..10].chars().filter(|c| *c != '-').collect();
        lines(&self.resource().join(format!("{day}.samples.jsonl")))
    }
    /// The sample line of `host` written at `t`.
    fn sample(&self, t: u64, host: &str) -> Value {
        let key = format!("{host}@{t}");
        self.samples(t)
            .into_iter()
            .find(|l| l["key"] == key)
            .unwrap()
    }
    fn heads(&self, kind: &str, filter: Option<&Value>) -> Vec<Value> {
        self.objects
            .iter()
            .filter(|((k, _), f)| k == kind && filter.is_none_or(|w| matches(f, w)))
            .map(|((k, key), f)| {
                let fields: Map<String, Value> = f
                    .iter()
                    .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
                    .collect();
                json!({"id": oid(k, key), "head": {"fields": fields}})
            })
            .collect()
    }
    fn apply(&mut self, body: &Value) -> Answer {
        let action = body["action_type"].as_str().unwrap();
        let p = body["parameters"].as_object().unwrap().clone();
        let now = utc(self.clock);
        let (kind, member) = match action {
            "declare_host" | "observe_host" => ("host", "host"),
            "observe_account" => ("account", "account"),
            other => panic!("the pass applied {other}"),
        };
        let key = p[member]["native_key"].as_str().unwrap().to_owned();
        let exists = self.fields(kind, &key).is_some();
        if action == "observe_host" && !exists {
            return reply(
                false,
                json!({"status": "refused", "code": "submission_criteria", "failed": ["unknown_host"]}),
            );
        }
        let entry = self.objects.entry((kind.into(), key)).or_default();
        for (k, v) in p.iter().filter(|(k, v)| *k != member && !v.is_null()) {
            entry.insert(k.clone(), v.clone());
        }
        let stamp = match action {
            "declare_host" => "declared_at",
            _ => "observed_at",
        };
        entry.insert(stamp.into(), json!(now));
        reply(
            true,
            json!({"status": "ok", "result": {"outcome": "applied", "edits": []}}),
        )
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _: u64) -> Answer {
        assert_eq!(program, "ont", "{args:?}");
        let n = args.iter().position(|a| a == "--store").unwrap();
        assert_eq!(args[n + 1], self.dir.path("store.sqlite"));
        let command = args[0].clone();
        self.calls.push((command.clone(), input.clone()));
        let what = match command.as_str() {
            "search" => format!("search {}", input["object_type"].as_str().unwrap()),
            "get" => format!("get {}", input["id"]["object_type"].as_str().unwrap()),
            "action" => format!("apply {}", input["action_type"].as_str().unwrap()),
            other => other.to_owned(),
        };
        if self.fail.contains(&what) {
            return broken("ont");
        }
        match command.as_str() {
            "search" => {
                let kind = input["object_type"].as_str().unwrap();
                let all = self.heads(kind, input.get("where"));
                let from: usize = input["continuation"]
                    .as_str()
                    .map_or(0, |c| c.parse().unwrap());
                let limit = input["limit"].as_u64().unwrap() as usize;
                let page: Vec<Value> = all.iter().skip(from).take(limit).cloned().collect();
                let next = (from + limit < all.len()).then(|| (from + limit).to_string());
                // As the engine: a run without `ended_at` makes a page whose filter names it unqualified.
                let running = kind == "run"
                    && self
                        .objects
                        .iter()
                        .any(|((k, _), f)| k == "run" && !f.contains_key("ended_at"));
                let qualified = !running && !self.unqualified.contains(kind);
                reply(
                    true,
                    json!({"status": "ok", "now": self.clock, "result": {"basis": {"qualified": qualified},
                        "objects": page, "continuation": next}}),
                )
            }
            "get" => {
                let id = &input["id"];
                let (kind, key) = (
                    id["object_type"].as_str().unwrap(),
                    id["native_key"].as_str().unwrap(),
                );
                match self.fields(kind, key) {
                    Some(f) => {
                        let fields: Map<String, Value> = f
                            .iter()
                            .map(|(n, v)| (n.clone(), json!({"kind": "value", "value": v})))
                            .collect();
                        reply(
                            true,
                            json!({"status": "ok", "now": self.clock, "result": {"objects": [{"id": id, "head": "r1",
                                "observations": [{"revision": "r1", "fields": fields}]}]}}),
                        )
                    }
                    None => reply(false, json!({"status": "refused", "code": "unavailable"})),
                }
            }
            "action" => {
                assert_eq!(&args[..4], ["action", "apply", "--client", "ap"]);
                assert_eq!(input["agent"], "ap-resource");
                assert!(input.get("role").is_none());
                self.apply(input)
            }
            "index" => {
                assert_eq!(
                    &args[..5],
                    ["index", "--source", "host_samples", "--source", "checks"]
                );
                let ok = self.index["status"] == "ok";
                reply(ok, self.index.clone())
            }
            "log" => {
                let rows = self
                    .logs
                    .get(input["action_type"].as_str().unwrap())
                    .cloned()
                    .unwrap_or_default();
                reply(
                    true,
                    json!({"status": "ok", "now": self.clock, "result": {"rows": rows, "withheld": 0, "continuation": null}}),
                )
            }
            "render" => {
                let view = input["view"].as_str().unwrap().to_owned();
                let unit = input["args"]["unit"].as_str().unwrap_or("").to_owned();
                let keys = self.views.get(&(view, unit)).cloned().unwrap_or_default();
                let mut md = "| Key | Status |\n| --- | --- |\n".to_owned();
                for k in &keys {
                    md.push_str(&format!("| {k} | x |\n"));
                }
                reply(
                    true,
                    json!({"status": "ok", "rows": keys.len(), "truncated": false, "markdown": md}),
                )
            }
            other => panic!("ont {other}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if let Some(b) = self.files.get(path) {
            return Ok(b.clone());
        }
        if path.starts_with("/proc/") {
            return Err(ErrorKind::NotFound.into());
        }
        if !std::fs::metadata(path)?.is_file() {
            return Err(ErrorKind::InvalidInput.into());
        }
        std::fs::read(path)
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.clock)
    }
    fn pause(&mut self, d: Duration) {
        // The clock moves one second for every ten pauses of a tenth of a second.
        assert_eq!(d, Duration::from_millis(100));
        if self.clock_moves {
            self.pauses += 1;
            if self.pauses.is_multiple_of(10) {
                self.clock += 1;
            }
        }
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![
            (
                "PATH".into(),
                format!("{}:/usr/bin:/bin", self.dir.path("bin")),
            ),
            ("HOME".into(), self.dir.path("pc")),
            ("AP_CHECKS_LOG".into(), self.dir.path("checks.jsonl")),
        ]
    }
}

fn pc_memory(dir: &Scratch, kb: u64) {
    std::fs::write(
        dir.at("pc/meminfo"),
        format!(
            "MemTotal:       32000000 kB\nMemFree:          100000 kB\nMemAvailable:   {kb} kB\n"
        ),
    )
    .unwrap();
}
fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn os(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}
fn with_pc() -> Value {
    json!({"hosts": {"pc": {"ssh": "fixture@fixture-pc", "work": "/fixture/work", "busy_load": 0.5, "min_available_mb": 2048}},
        "work": {"slots": 2}})
}
fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn ap(args: &[&str], env: &[(&str, &str)], stdin: &str) -> std::process::Output {
    use std::io::Write;
    let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
    c.args(args)
        .env_remove("ONT_FN_NAME")
        .env_remove("ONT_FN_STORE")
        .env_remove("AP_POLICY_FILE")
        .env_remove("AP_CHECKS_LOG")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn error_of(out: &std::process::Output) -> Value {
    serde_json::from_slice::<Value>(&out.stderr).unwrap()["error"].clone()
}
/// A line of the checks log as `ap check` writes it (`specs/ap/check.md` R15).
fn check_line(host: &str, dir: &str, end: u64, seconds: f64, exit: i64) -> Value {
    json!({"start": utc(end - 60), "end": utc(end), "seconds": seconds, "host": host,
        "reason": if host == "pc" { Value::Null } else { json!("pc_busy") }, "dir": dir, "repository": "fixture-owner/widgets",
        "head": "5".repeat(40), "command": ["cargo", "test", "--locked"], "exit": exit, "pc_cpus": 4, "pc_load": 0.2,
        "pc_wsl_available_mb": 9000, "pc_windows_available_mb": null, "pc_available_mb": 9000, "pc_min_wsl_available_mb": null})
}

// spec: specs/ap/resource.md#R1
#[test]
fn r1_the_arguments_are_refused_before_any_effect() {
    let code = |argv: &[&str]| resource::options(&os(argv)).map_err(|e| (e.code, e.field));
    for argv in [
        &["--bogus"][..],
        &["--directory"],
        &["--directory", "/a", "--directory", "/b", "--store", "/s"],
        &["--store", "/s", "--store", "/t", "--directory", "/a"],
        &["index"],
    ] {
        assert_eq!(
            code(argv).unwrap_err(),
            ("usage", "args".into()),
            "{argv:?}"
        );
    }
    assert_eq!(
        code(&["--directory", "relative", "--store", "/s"]).unwrap_err(),
        ("invalid_input", "directory".into())
    );
    assert_eq!(
        code(&["--directory", "/a", "--store", "s"]).unwrap_err(),
        ("invalid_input", "store".into())
    );
    assert_eq!(
        code(&["--store", "/s"]).unwrap_err(),
        ("invalid_input", "directory".into())
    );
    assert_eq!(
        code(&[
            "--directory",
            "/var/lib/agent-platform/resource/",
            "--store",
            "/s"
        ])
        .unwrap_err(),
        ("invalid_input", "directory".into())
    );
    assert_eq!(
        code(&["--directory", "/a"]).unwrap_err(),
        ("invalid_input", "directory".into())
    );
    let o = resource::options(&os(&["--store", "/s", "--directory", "/a"])).unwrap();
    assert_eq!(
        (o.directory.as_deref(), o.store.as_deref()),
        (Some("/a"), Some("/s"))
    );
    let o = resource::options(&[]).unwrap();
    assert_eq!((o.directory, o.store), (None, None));
}

// spec: specs/ap/resource.md#R1
#[test]
fn r1_the_command_prints_its_usage_and_refuses_a_policy_before_any_effect() {
    let out = ap(&["resource", "--help"], &[], "");
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .starts_with("Usage: ap resource [--directory <dir>] [--store <path>]")
    );
    let out = ap(&["resource", "--help", "--store", "/s"], &[], "");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        error_of(&out),
        json!({"code": "usage", "field": "args", "detail": null})
    );
    let dir = Scratch::new();
    let target = dir.path("resource");
    let store = dir.path("store.sqlite");
    let out = ap(&["resource", "--directory", &target], &[], "");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["field"], "directory");
    for (name, policy) in [
        ("not-json", "{".to_owned()),
        ("not-object", "[]".to_owned()),
        ("hosts", json!({"hosts": {"laptop": {}}}).to_string()),
        ("work", json!({"work": {"slots": 99}}).to_string()),
        (
            "accounts",
            json!({"accounts": {"claude": "/a"}}).to_string(),
        ),
        (
            "accounts-relative",
            json!({"accounts": {"claude": ["a/b"]}}).to_string(),
        ),
    ] {
        let file = dir.at(&format!("{name}.json"));
        std::fs::write(&file, policy).unwrap();
        let out = ap(
            &["resource", "--directory", &target, "--store", &store],
            &[
                ("AP_POLICY_FILE", file.to_str().unwrap()),
                ("PATH", "/nonexistent"),
            ],
            "",
        );
        assert_eq!(out.status.code(), Some(2), "{name}");
        let e = error_of(&out);
        assert_eq!(
            (e["code"].as_str(), e["field"].as_str()),
            (Some("policy_invalid"), Some("policy")),
            "{name}"
        );
        assert!(
            out.stdout.is_empty() && !Path::new(&target).exists(),
            "{name}"
        );
    }
}

// spec: specs/ap/resource.md#R1
#[test]
fn r1_without_a_policy_the_pass_covers_local_alone() {
    let mut f = Fake::new(None);
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    let hosts: Vec<&str> = v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["host"].as_str().unwrap())
        .collect();
    assert_eq!(hosts, ["local"]);
    assert_eq!(v["accounts"], json!([]));
    assert!(f.pc_calls().is_empty());
    // No work member: three slots.
    assert_eq!(f.samples(NOW)[0]["slots"], 3);
}

// spec: specs/ap/resource.md#R2
#[test]
fn r2_the_directory_is_0700_and_every_file_of_the_pass_0600() {
    let mut f = Fake::new(None);
    std::fs::create_dir(f.resource()).unwrap();
    std::fs::set_permissions(f.resource(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        f.dir.at("checks.jsonl"),
        format!("{}\n", check_line("pc", "/w", NOW - 10, 1.0, 0)),
    )
    .unwrap();
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    assert_eq!(mode(&f.resource()), 0o700);
    let names: BTreeSet<String> = std::fs::read_dir(f.resource())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from([
            "resource.lock".into(),
            "20270115.samples.jsonl".into(),
            "log.checks.jsonl".into()
        ])
    );
    for n in &names {
        assert_eq!(mode(&f.resource().join(n)), 0o600, "{n}");
    }
    // Absent, it is made with mode 0700.
    let mut g = Fake::new(None);
    g.pass();
    assert_eq!(mode(&g.resource()), 0o700);
}

// spec: specs/ap/resource.md#R2
#[test]
fn r2_a_path_that_is_no_directory_is_refused() {
    let mut f = Fake::new(None);
    std::fs::create_dir(f.dir.at("elsewhere")).unwrap();
    std::os::unix::fs::symlink(f.dir.at("elsewhere"), f.resource()).unwrap();
    let o = f.options();
    let e = resource::pass(&mut f, &o).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "directory"));
    assert!(f.calls.is_empty());
    let mut g = Fake::new(None);
    std::fs::write(g.resource(), "a file").unwrap();
    let o = g.options();
    assert_eq!(
        resource::pass(&mut g, &o).unwrap_err().code,
        "invalid_input"
    );
}

// spec: specs/ap/resource.md#R2
#[test]
fn r2_a_lock_another_pass_holds_skips_the_pass() {
    let mut f = Fake::new(Some(with_pc()));
    std::fs::create_dir(f.resource()).unwrap();
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(f.resource().join("resource.lock"))
        .unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let (v, ok) = f.pass();
    assert_eq!((v, ok), (json!({"skipped": "pass_running"}), true));
    assert!(f.calls.is_empty() && f.pc_calls().is_empty());
    assert!(resource::samples_files(&f.resource()).is_empty());
    drop(held);
    assert!(f.pass().1);
}

// spec: specs/ap/resource.md#R3
#[test]
fn r3_this_host_reads_its_cpus_load_memory_and_the_units() {
    let mut f = Fake::new(Some(with_pc()));
    f.put("unit", "u-running", json!({"state": "running"}));
    f.put(
        "unit",
        "u-over",
        json!({"state": "ready", "waits_on": "cpu over 80%"}),
    );
    f.put(
        "unit",
        "u-outside",
        json!({"state": "ready", "waits_on": "cpu outside"}),
    );
    f.put(
        "unit",
        "u-slots",
        json!({"state": "ready", "waits_on": "slots 2/2"}),
    );
    f.put("unit", "u-merged", json!({"state": "merged"}));
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    let local = &f.samples(NOW)[0];
    assert_eq!(
        local,
        &json!({"type": "host_sample", "key": format!("local@{NOW}"), "observed_at": NOW, "at": "2027-01-15T08:00:00Z",
            "host": "local", "answers": true, "free": true, "cpus": 2, "load_x100": 52, "available_mb": 3072,
            "slots": 2, "units_running": 1, "units_ready": 3, "cpu_waits": 2})
    );
    let searches: Vec<&Value> = f
        .calls
        .iter()
        .filter(|(c, b)| c == "search" && b["object_type"] == "unit")
        .map(|(_, b)| b)
        .collect();
    assert_eq!(searches.len(), 1);
    assert_eq!(
        searches[0]["where"],
        json!({"all": [{"in": ["state", ["ready", "running"]]}]})
    );
    // The slots full: local is not free.
    f.put("unit", "u-running-2", json!({"state": "running"}));
    f.clock += 60;
    f.pass();
    assert_eq!(f.sample(NOW + 60, "local")["free"], false);
}

// spec: specs/ap/resource.md#R3
#[test]
fn r3_a_file_of_this_host_that_cannot_be_read_ends_the_pass_with_nothing_written() {
    for (path, text) in [
        ("/proc/meminfo", None),
        ("/proc/meminfo", Some("MemTotal: 1 kB\n")),
        ("/proc/stat", Some("intr 1\n")),
        ("/proc/loadavg", Some("load\n")),
    ] {
        let mut f = Fake::new(Some(with_pc()));
        match text {
            None => {
                f.files.remove(path);
            }
            Some(t) => {
                f.files.insert(path.into(), t.as_bytes().to_vec());
            }
        }
        let o = f.options();
        let e = resource::pass(&mut f, &o).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("reading_failed", path));
        assert!(resource::samples_files(&f.resource()).is_empty() && f.applied().is_empty());
        assert!(!f.resource().join("log.checks.jsonl").exists());
    }
}

// spec: specs/ap/resource.md#R3
#[test]
fn r3_a_failed_search_of_the_units_leaves_the_line_without_counts() {
    let mut f = Fake::new(None);
    f.fail.insert("search unit".into());
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(v["failed"], json!(["units"]));
    let local = &f.samples(NOW)[0];
    assert_eq!(
        (local["answers"].clone(), local["free"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        (local["cpus"].clone(), local["available_mb"].clone()),
        (json!(2), json!(3072))
    );
    for k in ["slots", "units_running", "units_ready", "cpu_waits"] {
        assert!(local.get(k).is_none(), "{k}");
    }
    let observed = f.applied_of("observe_host");
    assert_eq!(observed[0]["parameters"]["free"], false);
    assert_eq!(observed[0]["parameters"]["cpus"], 2);
}

// spec: specs/ap/resource.md#R4
#[test]
fn r4_the_pc_answers_its_probe_and_is_free_by_the_rule_of_check() {
    let mut f = Fake::new(Some(with_pc()));
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    let pc = &f.samples(NOW)[1];
    assert_eq!(
        pc,
        &json!({"type": "host_sample", "key": format!("pc@{NOW}"), "observed_at": NOW, "at": utc(NOW), "host": "pc",
            "answers": true, "free": true, "cpus": 4, "load_x100": 100, "available_mb": 8192})
    );
    let calls = f.pc_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0], "ping");
    assert!(
        calls[1].starts_with("ssh fixture@fixture-pc bash -c ")
            && calls[1].contains("ap-resource-probe")
    );
    // The probe reads three numbers and starts no Windows program and no engine.
    for word in ["powershell", "ont ", "ENGINE", ".exe", "HEAD"] {
        assert!(!calls[1].contains(word), "{word}");
    }
    // The load at busy_load times the CPUs: answers, not free; the memory below the bound: the same.
    std::fs::write(f.dir.at("pc/loadavg"), "2.00 1.0 1.0 1/1 1\n").unwrap();
    f.clock += 300;
    f.pass();
    let pc = &f.sample(NOW + 300, "pc");
    assert_eq!(
        (
            pc["answers"].clone(),
            pc["free"].clone(),
            pc["load_x100"].clone()
        ),
        (json!(true), json!(false), json!(200))
    );
    std::fs::write(f.dir.at("pc/loadavg"), "0.10 1.0 1.0 1/1 1\n").unwrap();
    pc_memory(&f.dir, 2_047 * 1024);
    f.clock += 300;
    f.pass();
    let pc = &f.sample(NOW + 600, "pc");
    assert_eq!(
        (pc["free"].clone(), pc["available_mb"].clone()),
        (json!(false), json!(2047))
    );
}

// spec: specs/ap/resource.md#R4
#[test]
fn r4_the_pc_silent_or_answering_an_unreadable_line_answers_false_with_no_reading() {
    for marker in ["down", "garbage"] {
        let mut f = Fake::new(Some(with_pc()));
        std::fs::write(f.dir.at(&format!("pc/{marker}")), "").unwrap();
        let (v, ok) = f.pass();
        assert!(ok, "{v}");
        let pc = &f.samples(NOW)[1];
        assert_eq!(
            pc,
            &json!({"type": "host_sample", "key": format!("pc@{NOW}"), "observed_at": NOW, "at": utc(NOW),
                "host": "pc", "answers": false, "free": false}),
            "{marker}"
        );
        let calls = f.pc_calls();
        assert_eq!(
            calls.len(),
            if marker == "down" { 1 } else { 2 },
            "{calls:?}"
        );
        let observed = f.applied_of("observe_host");
        let p = &observed[1]["parameters"];
        assert_eq!(
            p,
            &json!({"host": oid("host", "pc"), "answers": false, "free": false}),
            "{marker}"
        );
    }
}

// spec: specs/ap/resource.md#R5
#[test]
fn r5_a_host_is_declared_once_and_observed_every_pass() {
    let mut f = Fake::new(Some(with_pc()));
    f.put(
        "host",
        "pc-user",
        json!({"node": "fixture-pc-user", "machine": "pc", "declared_at": utc(NOW - 99)}),
    );
    let (v, _) = f.pass();
    let declared = f.applied_of("declare_host");
    assert_eq!(declared.len(), 2);
    assert_eq!(
        declared[0]["parameters"],
        json!({"host": oid("host", "local"), "node": "fixture-vps", "machine": "local"})
    );
    assert_eq!(
        declared[1]["parameters"],
        json!({"host": oid("host", "pc"), "node": "fixture-pc", "machine": "pc"})
    );
    assert_eq!(v["hosts"][0]["declared"], true);
    let observed = f.applied_of("observe_host");
    assert_eq!(
        observed[0]["parameters"],
        json!({"host": oid("host", "local"), "answers": true, "free": true, "seen_at": utc(NOW), "cpus": 2,
            "load_x100": 52, "available_mb": 3072})
    );
    assert_eq!(f.fields("host", "pc").unwrap()["seen_at"], utc(NOW));
    // A second pass declares nothing; the user's own node of the PC is never declared or observed.
    f.calls.clear();
    f.clock += 300;
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    assert!(f.applied_of("declare_host").is_empty());
    assert_eq!(f.applied_of("observe_host").len(), 2);
    assert_eq!(
        (
            v["hosts"][0]["declared"].clone(),
            v["hosts"][1]["declared"].clone()
        ),
        (json!(false), json!(false))
    );
    assert!(
        f.calls
            .iter()
            .all(|(_, b)| !b.to_string().contains("pc-user"))
    );
    assert_eq!(f.fields("host", "pc-user").unwrap().len(), 3);
}

// spec: specs/ap/resource.md#R5
#[test]
fn r5_a_failed_read_or_apply_is_named_and_the_pass_goes_on() {
    let mut f = Fake::new(Some(with_pc()));
    f.fail.insert("get host".into());
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(
        v["failed"],
        json!([
            "declare_host:local",
            "observe_host:local",
            "declare_host:pc",
            "observe_host:pc"
        ])
    );
    assert_eq!(f.samples(NOW).len(), 2);
    let mut f = Fake::new(Some(with_pc()));
    f.fail.insert("apply observe_host".into());
    let (v, _) = f.pass();
    assert_eq!(
        v["failed"],
        json!(["observe_host:local", "observe_host:pc"])
    );
    let mut f = Fake::new(None);
    f.fail.insert("apply declare_host".into());
    let (v, _) = f.pass();
    assert_eq!(
        v["failed"],
        json!(["declare_host:local", "observe_host:local"])
    );
    assert_eq!(v["hosts"][0]["declared"], false);
}

// spec: specs/ap/resource.md#R6
#[test]
fn r6_lines_are_written_whether_or_not_the_store_answers() {
    let mut f = Fake::new(Some(with_pc()));
    for what in ["search unit", "get host", "apply observe_host", "index"] {
        f.fail.insert(what.into());
    }
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(
        v["failed"],
        json!([
            "units",
            "declare_host:local",
            "observe_host:local",
            "declare_host:pc",
            "observe_host:pc",
            "index"
        ])
    );
    let lines = f.samples(NOW);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        (lines[0]["host"].clone(), lines[1]["host"].clone()),
        (json!("local"), json!("pc"))
    );
    assert_eq!(v["sources"], json!({"host_samples": null, "checks": null}));
}

// spec: specs/ap/resource.md#R6
#[test]
fn r6_two_passes_in_one_second_write_no_key_twice() {
    let mut f = Fake::new(Some(with_pc()));
    f.pass();
    // The second pass waits for a later second and takes it as its time.
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    assert_eq!(v["at"], utc(NOW + 1));
    let keys: Vec<String> = f
        .samples(NOW)
        .iter()
        .map(|l| l["key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        keys,
        [
            format!("local@{NOW}"),
            format!("pc@{NOW}"),
            format!("local@{}", NOW + 1),
            format!("pc@{}", NOW + 1)
        ]
    );
    // A clock that does not move within 2 seconds: nothing is appended and the step is named.
    f.clock_moves = false;
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(v["failed"], json!(["samples"]));
    assert_eq!(f.samples(NOW).len(), 4);
}

// spec: specs/ap/resource.md#R6
#[test]
fn r6_a_samples_file_more_than_14_days_old_is_removed_and_nothing_else() {
    let mut f = Fake::new(None);
    std::fs::create_dir(f.resource()).unwrap();
    let name = |days: u64| {
        let d: String = utc(NOW - days * DAY)[..10]
            .chars()
            .filter(|c| *c != '-')
            .collect();
        format!("{d}.samples.jsonl")
    };
    for n in [
        name(15),
        name(14),
        name(40),
        "notes.samples.jsonl".into(),
        "20261399.samples.jsonl".into(),
        "keep.txt".into(),
    ] {
        std::fs::write(f.resource().join(n), "").unwrap();
    }
    std::fs::create_dir(f.resource().join(name(20))).unwrap();
    let (v, _) = f.pass();
    let mut removed = vec![name(40), name(15)];
    removed.sort();
    assert_eq!(v["removed"], json!(removed));
    for n in [
        name(14),
        "notes.samples.jsonl".into(),
        "20261399.samples.jsonl".into(),
        "keep.txt".into(),
        name(20),
    ] {
        assert!(f.resource().join(&n).exists(), "{n}");
    }
}

// spec: specs/ap/resource.md#R7
#[test]
fn r7_each_kind_of_line_the_schema_does_not_take_is_skipped() {
    let ok = check_line("pc", "/fixture/wt/a", NOW - 100, 2.0006, 0);
    let with = |k: &str, v: Value| {
        let mut l = ok.clone();
        l[k] = v;
        l.to_string()
    };
    let without = |k: &str| {
        let mut l = ok.clone();
        l.as_object_mut().unwrap().remove(k);
        l.to_string()
    };
    let skipped = [
        "not json".to_owned(),
        "[1, 2]".to_owned(),
        String::new(),
        with("end", json!("2027-01-15 07:00:00")),
        with("end", json!(utc(NOW + 1))),
        with("end", json!(utc(NOW - 30 * DAY - 1))),
        with("host", json!("laptop")),
        with("repository", json!(7)),
        without("head"),
        with("dir", Value::Null),
        with("command", json!("cargo test")),
        with("command", json!(["cargo", 1])),
        with("seconds", json!(-1)),
        with("seconds", json!(1e20)),
        with("seconds", json!("2")),
        with("exit", json!(1.5)),
        with("start", json!("yesterday")),
        with("reason", json!("r".repeat(41))),
        with("reason", json!(3)),
        with("dir", json!("d".repeat(70_000))),
    ];
    let mut log = String::new();
    log.push_str(&ok.to_string());
    log.push('\n');
    for l in &skipped {
        log.push_str(l);
        log.push('\n');
    }
    // A line equal in bytes to an earlier one is taken once, and is not skipped.
    log.push_str(&ok.to_string());
    log.push('\n');
    let kept_edges = [
        with("end", json!(utc(NOW - 30 * DAY))),
        with("start", Value::Null),
        without("reason"),
        with("reason", json!("r".repeat(40))),
        with("seconds", json!(0)),
    ];
    for l in &kept_edges {
        log.push_str(l);
        log.push('\n');
    }
    let (records, n) = resource::check_records(log.as_bytes(), NOW);
    assert_eq!(n, skipped.len() as u64);
    assert_eq!(records.len(), 1 + kept_edges.len());
    let line = ok.to_string();
    assert_eq!(
        records[0],
        json!({"type": "check", "key": &hex(line.as_bytes())[..32], "observed_at": NOW - 100, "host": "pc",
            "repository": "fixture-owner/widgets", "head": "5".repeat(40), "dir": "/fixture/wt/a",
            "command": "cargo test --locked", "end": utc(NOW - 100), "exit": 0, "milliseconds": 2001,
            "start": utc(NOW - 160)})
    );
    assert!(
        records[1..]
            .iter()
            .all(|r| r.get("start").is_none() || r["start"].is_string())
    );
    assert_eq!(records[4]["reason"], "r".repeat(40));
    assert_eq!(records[5]["milliseconds"], 0);
}

// spec: specs/ap/resource.md#R7
#[test]
fn r7_the_command_is_cut_at_a_character_boundary_and_the_newest_50000_are_kept() {
    let mut l = check_line("local", "/w", NOW - 5, 1.0, 1);
    l["command"] = json!(["é".repeat(600)]);
    let (records, _) = resource::check_records(l.to_string().as_bytes(), NOW);
    let command = records[0]["command"].as_str().unwrap();
    assert_eq!((command.len(), command.chars().count()), (1000, 500));
    assert_eq!(records[0]["reason"], "pc_busy");
    // One line older than the rest is the one dropped past 50,000; the order of the log stays.
    let mut log = String::new();
    for i in 0..resource::CHECKS_KEPT {
        log.push_str(&check_line("local", &format!("/w/{i}"), NOW - 1_000, 1.0, 0).to_string());
        log.push('\n');
        if i == 10 {
            log.push_str(&check_line("local", "/w/oldest", NOW - 2_000, 1.0, 0).to_string());
            log.push('\n');
        }
    }
    let (records, skipped) = resource::check_records(log.as_bytes(), NOW);
    assert_eq!((records.len(), skipped), (resource::CHECKS_KEPT, 0));
    assert!(records.iter().all(|r| r["dir"] != "/w/oldest"));
    assert_eq!(
        (records[0]["dir"].clone(), records[49_999]["dir"].clone()),
        (json!("/w/0"), json!("/w/49999"))
    );
}

// spec: specs/ap/resource.md#R7
#[test]
fn r7_the_pass_writes_the_records_and_never_the_log() {
    let mut f = Fake::new(None);
    let log = format!(
        "{}\nnot json\n{}\n",
        check_line("pc", "/w", NOW - 10, 1.0, 0),
        check_line("local", "/w", NOW - 9, 2.0, 0)
    );
    std::fs::write(f.dir.at("checks.jsonl"), &log).unwrap();
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    assert_eq!(v["checks"], json!({"records": 2, "skipped": 1}));
    assert_eq!(lines(&f.resource().join("log.checks.jsonl")).len(), 2);
    assert_eq!(
        std::fs::read_to_string(f.dir.at("checks.jsonl")).unwrap(),
        log
    );
    // No log is no check: the file is written empty, so its records are retracted.
    std::fs::remove_file(f.dir.at("checks.jsonl")).unwrap();
    f.clock += 300;
    let (v, ok) = f.pass();
    assert!(ok);
    assert_eq!(v["checks"], json!({"records": 0, "skipped": 0}));
    assert_eq!(
        std::fs::read(f.resource().join("log.checks.jsonl")).unwrap(),
        b""
    );
}

// spec: specs/ap/resource.md#R7
#[test]
fn r7_a_log_that_cannot_be_read_keeps_the_previous_records() {
    let mut f = Fake::new(None);
    std::fs::write(
        f.dir.at("checks.jsonl"),
        format!("{}\n", check_line("pc", "/w", NOW - 10, 1.0, 0)),
    )
    .unwrap();
    f.pass();
    std::fs::remove_file(f.dir.at("checks.jsonl")).unwrap();
    std::fs::create_dir(f.dir.at("checks.jsonl")).unwrap();
    f.clock += 300;
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(v["failed"], json!(["checks"]));
    assert_eq!(v["checks"], json!({"records": null, "skipped": null}));
    assert_eq!(lines(&f.resource().join("log.checks.jsonl")).len(), 1);
}

fn with_accounts() -> Value {
    json!({"accounts": {"codex": ["/x/acct-c"], "claude/writer": ["/y/acct-c/"], "claude": ["/z/acct-a", "/z/again/acct-a"],
        "muse": ["/"]}})
}

// spec: specs/ap/resource.md#R8
#[test]
fn r8_each_account_of_the_policy_is_made_with_its_runtime() {
    let mut f = Fake::new(Some(with_accounts()));
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    assert_eq!(
        v["accounts"],
        json!([{"account": "acct-a", "observed": true}, {"account": "acct-c", "observed": true}])
    );
    let observed = f.applied_of("observe_account");
    assert_eq!(
        observed[0]["parameters"],
        json!({"account": oid("account", "acct-a"), "runtime": "claude"})
    );
    // "claude/writer" is before "codex" in byte order.
    assert_eq!(
        observed[1]["parameters"],
        json!({"account": oid("account", "acct-c"), "runtime": "claude"})
    );
    let searches: Vec<&Value> = f
        .calls
        .iter()
        .filter(|(c, b)| c == "search" && b["object_type"] == "run")
        .map(|(_, b)| b)
        .collect();
    assert_eq!(
        searches[0]["where"],
        json!({"all": [{"eq": ["account", "acct-a"]}, {"gt": ["ended_at", utc(NOW - 7 * DAY)]}]})
    );
    // A second pass with nothing new applies nothing.
    f.calls.clear();
    f.clock += 300;
    let (v, _) = f.pass();
    assert_eq!(
        v["accounts"],
        json!([{"account": "acct-a", "observed": false}, {"account": "acct-c", "observed": false}])
    );
    assert!(f.applied_of("observe_account").is_empty());
}

// spec: specs/ap/resource.md#R8
#[test]
fn r8_the_newest_run_with_a_reading_is_observed_with_a_running_run_in_a_page_that_is_not_qualified()
{
    let mut f = Fake::new(Some(json!({"accounts": {"claude": ["/z/acct-a"]}})));
    f.put("account", "acct-a", json!({"runtime": "claude", "limit": "{\"utilization\": 0.1}", "limit_at": utc(NOW - 3_000)}));
    let windows = r#"{"windows": {"five_hour": {"utilization": 0.4, "status": "allowed"}}}"#;
    f.put(
        "run",
        "r-windows",
        json!({"account": "acct-a", "ended_at": utc(NOW - 600), "limit": windows}),
    );
    f.put(
        "run",
        "r-text",
        json!({"account": "acct-a", "ended_at": utc(NOW - 60), "limit": "five-hour 12%"}),
    );
    f.put("run", "r-old", json!({"account": "acct-a", "ended_at": utc(NOW - 4_000), "limit": "{\"utilization\": 0.9}"}));
    f.put(
        "run",
        "r-other",
        json!({"account": "acct-b", "ended_at": utc(NOW - 30), "limit": "{\"utilization\": 0.9}"}),
    );
    f.put(
        "run",
        "r-running",
        json!({"account": "acct-a", "status": "running"}),
    );
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    let observed = f.applied_of("observe_account");
    assert_eq!(
        observed[0]["parameters"],
        json!({"account": oid("account", "acct-a"), "runtime": "claude", "limit": windows, "limit_at": utc(NOW - 600),
            "limit_run": oid("run", "r-windows")})
    );
    assert_eq!(
        f.calls
            .iter()
            .find(|(c, b)| c == "search" && b["object_type"] == "run")
            .unwrap()
            .1["where"]["all"][1],
        json!({"gt": ["ended_at", utc(NOW - 3_000)]})
    );
    let mut f = Fake::new(Some(json!({"accounts": {"claude": ["/z/acct-a"]}})));
    f.fail.insert("search run".into());
    let (v, _) = f.pass();
    assert_eq!(v["failed"], json!(["observe_account:acct-a"]));
    assert_eq!(
        v["accounts"],
        json!([{"account": "acct-a", "observed": false}])
    );
    let mut f = Fake::new(Some(json!({"accounts": {"claude": ["/z/acct-a"]}})));
    f.fail.insert("get account".into());
    assert_eq!(f.pass().0["failed"], json!(["observe_account:acct-a"]));
    let mut f = Fake::new(Some(json!({"accounts": {"claude": ["/z/acct-a"]}})));
    f.fail.insert("apply observe_account".into());
    assert_eq!(f.pass().0["failed"], json!(["observe_account:acct-a"]));
}

// spec: specs/ap/resource.md#R9
#[test]
fn r9_the_answer_line_and_the_index() {
    let mut f = Fake::new(Some(with_pc()));
    let (v, ok) = f.pass();
    assert!(ok, "{v}");
    let members: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        [
            "accounts",
            "at",
            "checks",
            "directory",
            "failed",
            "hosts",
            "removed",
            "sources"
        ]
    );
    assert_eq!(v["directory"], f.resource().to_str().unwrap());
    assert_eq!(
        v["hosts"][1],
        json!({"key": format!("pc@{NOW}"), "observed_at": NOW, "at": utc(NOW), "host": "pc", "answers": true, "free": true,
            "cpus": 4, "load_x100": 100, "available_mb": 8192, "declared": true})
    );
    assert_eq!(
        v["sources"]["host_samples"],
        json!({"status": "ok", "code": null, "appended": {"initial": 1}, "renewed": 0, "unchanged": 0})
    );
    // A source that did not hold.
    f.index = json!({"status": "source_failed", "result": {"sources": [
        {"source": "host_samples", "status": "ok", "appended": {}, "renewed": 0, "unchanged": 2},
        {"source": "checks", "status": "source_failed", "code": "duplicate_key", "appended": {}, "renewed": 0, "unchanged": 0}]}});
    f.clock += 300;
    let (v, ok) = f.pass();
    assert!(!ok);
    assert_eq!(v["failed"], json!(["index"]));
    assert_eq!(v["sources"]["checks"]["code"], "duplicate_key");
    let indexes = f.calls.iter().filter(|(c, _)| c == "index").count();
    assert_eq!(indexes, 2);
}

// spec: specs/ap/resource.md#R10
#[test]
fn r10_the_pass_applies_only_its_three_actions_and_writes_only_its_directory() {
    let mut f = Fake::new(Some(
        json!({"hosts": with_pc()["hosts"], "accounts": {"claude": ["/z/acct-a"]}}),
    ));
    let log = format!("{}\n", check_line("pc", "/w", NOW - 10, 1.0, 0));
    std::fs::write(f.dir.at("checks.jsonl"), &log).unwrap();
    let policy = std::fs::read(f.dir.at("policy.json")).unwrap();
    f.pass();
    let actions: BTreeSet<&str> = f
        .applied()
        .iter()
        .map(|b| b["action_type"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        BTreeSet::from(["declare_host", "observe_host", "observe_account"])
    );
    let commands: BTreeSet<&str> = f.calls.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(
        commands,
        BTreeSet::from(["action", "get", "index", "search"])
    );
    let entries: BTreeSet<String> = std::fs::read_dir(&f.dir.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        entries,
        BTreeSet::from([
            "bin".into(),
            "checks.jsonl".into(),
            "pc".into(),
            "policy.json".into(),
            "resource".into()
        ])
    );
    assert_eq!(
        std::fs::read_to_string(f.dir.at("checks.jsonl")).unwrap(),
        log
    );
    assert_eq!(std::fs::read(f.dir.at("policy.json")).unwrap(), policy);
    // Every request id is the pass's own.
    assert!(f.applied().iter().all(|b| {
        b["request_id"]
            .as_str()
            .unwrap()
            .starts_with(&format!("resource-{NOW}-"))
    }));
    assert!(
        f.applied()
            .iter()
            .all(|b| b["session"] == format!("resource-{NOW}"))
    );
}

// spec: specs/ap/resource.md#R11
#[test]
fn r11_a_parameter_the_function_does_not_declare_or_of_another_form_is_refused() {
    let mut f = Fake::new(None);
    let store = f.dir.path("store.sqlite");
    let field = |r: Result<Value, Failure>| r.map_err(|e| (e.code, e.field)).unwrap_err();
    assert_eq!(
        field(resource::capacity(&mut f, &store, &json!({"x": 1}))),
        ("invalid_input", "x".into())
    );
    assert_eq!(
        field(resource::capacity(&mut f, &store, &json!([]))),
        ("invalid_input", "parameters".into())
    );
    for (p, member) in [
        (json!({}), "since"),
        (json!({"since": "2027-01-15"}), "since"),
        (json!({"since": "2027-02-30T00:00:00Z"}), "since"),
        (json!({"since": 7}), "since"),
        (json!({"since": utc(NOW), "intent": ""}), "intent"),
        (
            json!({"since": utc(NOW), "intent": "i".repeat(513)}),
            "intent",
        ),
        (json!({"since": utc(NOW), "intent": 3}), "intent"),
        (json!({"since": utc(NOW), "until": utc(NOW)}), "until"),
    ] {
        assert_eq!(
            field(resource::gate(&mut f, &store, &p)),
            ("invalid_input", member.into()),
            "{p}"
        );
    }
    assert!(f.calls.is_empty());
    // A named intent the store does not have.
    assert_eq!(
        field(resource::gate(
            &mut f,
            &store,
            &json!({"since": utc(NOW), "intent": "absent"})
        )),
        ("unknown_intent", "intent".into())
    );
    // A store that cannot be read is a failure of the store, not a refusal of the input.
    let mut f = Fake::new(None);
    let store = f.dir.path("store.sqlite");
    f.fail.insert("search host".into());
    assert_eq!(
        resource::capacity(&mut f, &store, &json!({}))
            .unwrap_err()
            .code,
        "process_failed"
    );
    // Every page is read, qualified or not.
    let mut f = Fake::new(None);
    let store = f.dir.path("store.sqlite");
    f.put(
        "host",
        "local",
        json!({"node": "n", "machine": "local", "declared_at": utc(NOW)}),
    );
    f.unqualified.extend([
        "host".to_owned(),
        "host_sample".into(),
        "check".into(),
        "run".into(),
    ]);
    assert_eq!(
        resource::capacity(&mut f, &store, &json!({})).unwrap()["hosts"][0]["host"],
        "local"
    );
    assert!(resource::gate(&mut f, &store, &json!({"since": utc(NOW - DAY)})).is_ok());
}

// spec: specs/ap/resource.md#R11
#[test]
fn r11_the_functions_exit_2_on_a_refusal_and_3_on_a_store_they_cannot_read() {
    let dir = Scratch::new();
    std::fs::create_dir(dir.at("bin")).unwrap();
    let ont = dir.at("bin/ont");
    std::fs::write(&ont, "#!/bin/sh\necho 'the engine is down' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&ont, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", dir.path("bin"));
    let store = dir.path("store.sqlite");
    let env = |name: &'static str| {
        [
            ("ONT_FN_NAME", name),
            ("ONT_FN_STORE", store.as_str()),
            ("PATH", path.as_str()),
        ]
    };
    let out = ap(&[], &env("capacity"), r#"{"x": 1}"#);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["field"], "x");
    let out = ap(&[], &env("gate"), r#"{"since": "now"}"#);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["field"], "since");
    let out = ap(&[], &env("gate"), "not json");
    assert_eq!(out.status.code(), Some(2));
    for (name, input) in [
        ("capacity", "{}"),
        ("gate", r#"{"since": "2027-01-15T00:00:00Z"}"#),
    ] {
        let out = ap(&[], &env(name), input);
        assert_eq!(out.status.code(), Some(3), "{name}");
        assert!(out.stdout.is_empty());
        assert!(error_of(&out)["code"].is_string());
    }
}

fn host_fields(
    machine: &str,
    observed: Option<u64>,
    seen: u64,
    answers: bool,
    free: bool,
    cpus: u64,
) -> Value {
    let mut f = json!({"node": format!("node-{machine}"), "machine": machine, "declared_at": utc(NOW - DAY)});
    if let Some(o) = observed {
        f["observed_at"] = json!(utc(o));
        f["answers"] = json!(answers);
        f["free"] = json!(free);
        f["seen_at"] = json!(utc(seen));
        f["cpus"] = json!(cpus);
        f["load_x100"] = json!(cpus * 10);
        f["available_mb"] = json!(cpus * 1000);
    }
    f
}

// spec: specs/ap/resource.md#R12
#[test]
fn r12_capacity_counts_a_machine_once_and_says_what_can_start() {
    let mut f = Fake::new(None);
    let store = f.dir.path("store.sqlite");
    f.put(
        "host",
        "local",
        host_fields("local", Some(NOW - 60), NOW - 60, true, true, 2),
    );
    // Two hosts of the machine pc: the one that answered last gives the readings; one stale host does not count.
    f.put(
        "host",
        "pc",
        host_fields("pc", Some(NOW - 30), NOW - 30, true, false, 22),
    );
    f.put(
        "host",
        "pc-user",
        host_fields("pc", Some(NOW - 10), NOW - 10, true, true, 24),
    );
    f.put(
        "host",
        "pc-stale",
        host_fields("pc", Some(NOW - 601), NOW - 5, true, true, 99),
    );
    // A host of its own machine that is stale, one that does not answer, and one never observed.
    f.put(
        "host",
        "spare",
        host_fields("spare", Some(NOW - 900), NOW - 900, true, true, 8),
    );
    f.put(
        "host",
        "silent",
        host_fields("silent", Some(NOW - 60), NOW - 900, false, false, 8),
    );
    f.put("host", "new", host_fields("new", None, 0, false, false, 0));
    f.put(
        "account",
        "acct-a",
        json!({"runtime": "claude", "limit": "{\"utilization\": 0.4}", "limit_at": utc(NOW - 99),
        "allowance": "ignored while a run reports one", "allowance_at": utc(NOW - 9)}),
    );
    f.put(
        "account",
        "acct-b",
        json!({"runtime": "codex", "limit": "five-hour 12%", "limit_at": utc(NOW - 98)}),
    );
    f.put(
        "account",
        "acct-c",
        json!({"allowance": "Five hours a day.", "allowance_at": utc(NOW - 97)}),
    );
    f.put("account", "acct-d", json!({"runtime": "agy"}));
    let v = resource::capacity(&mut f, &store, &json!({})).unwrap();
    assert_eq!(v["at"], utc(NOW));
    let hosts: Vec<(String, Value, Value)> = v["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            (
                h["host"].as_str().unwrap().to_owned(),
                h["answers"].clone(),
                h["stale"].clone(),
            )
        })
        .collect();
    assert_eq!(
        hosts,
        [
            ("local".into(), json!(true), json!(false)),
            ("new".into(), Value::Null, json!(false)),
            ("pc".into(), json!(true), json!(false)),
            ("pc-stale".into(), json!(true), json!(true)),
            ("pc-user".into(), json!(true), json!(false)),
            ("silent".into(), json!(false), json!(false)),
            ("spare".into(), json!(true), json!(true)),
        ]
    );
    let h = &v["hosts"][4];
    let members: Vec<&str> = h.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        members,
        [
            "answers",
            "available_mb",
            "cpus",
            "free",
            "host",
            "load_x100",
            "machine",
            "node",
            "observed_at",
            "seen_at",
            "stale"
        ]
    );
    assert_eq!(
        v["machines"],
        json!([
            {"machine": "local", "hosts": ["local"], "up": true, "cpus": 2, "load_x100": 20, "available_mb": 2000, "free": true},
            {"machine": "new", "hosts": ["new"], "up": false, "cpus": null, "load_x100": null, "available_mb": null, "free": false},
            {"machine": "pc", "hosts": ["pc", "pc-stale", "pc-user"], "up": true, "cpus": 24, "load_x100": 240, "available_mb": 24000, "free": true},
            {"machine": "silent", "hosts": ["silent"], "up": false, "cpus": null, "load_x100": null, "available_mb": null, "free": false},
            {"machine": "spare", "hosts": ["spare"], "up": false, "cpus": null, "load_x100": null, "available_mb": null, "free": false}
        ])
    );
    assert_eq!(v["can_start"], json!(["local", "pc"]));
    assert_eq!(
        v["accounts"],
        json!([
            {"account": "acct-a", "runtime": "claude", "allowance": {"utilization": 0.4}, "source": "run", "at": utc(NOW - 99)},
            {"account": "acct-b", "runtime": "codex", "allowance": "five-hour 12%", "source": "run", "at": utc(NOW - 98)},
            {"account": "acct-c", "runtime": null, "allowance": "Five hours a day.", "source": "stated", "at": utc(NOW - 97)},
            {"account": "acct-d", "runtime": "agy", "allowance": null, "source": null, "at": null}
        ])
    );
    // The latest answering host of a machine is not free: the machine is up and not free.
    f.put(
        "host",
        "pc-user",
        host_fields("pc", Some(NOW - 10), NOW - 10, true, false, 24),
    );
    f.put(
        "host",
        "pc",
        host_fields("pc", Some(NOW - 30), NOW - 30, true, true, 22),
    );
    let v = resource::capacity(&mut f, &store, &json!({})).unwrap();
    assert_eq!(
        (
            v["machines"][2]["up"].clone(),
            v["machines"][2]["free"].clone()
        ),
        (json!(true), json!(false))
    );
    assert_eq!(v["can_start"], json!(["local"]));
}

/// `n` samples of `local` from `since` on, each made by `each(i)` over the base fields.
fn samples(f: &mut Fake, since: u64, n: usize, each: impl Fn(usize, &mut Value)) {
    for i in 0..n {
        let t = since + 300 * i as u64;
        let mut s = json!({"host": oid("host", "local"), "at": utc(t), "answers": true, "free": true, "cpus": 2,
            "load_x100": 100, "available_mb": 3000, "slots": 3, "units_running": 1, "units_ready": 1, "cpu_waits": 0});
        each(i, &mut s);
        f.put("host_sample", &format!("local@{t}"), s);
    }
}
fn gate_rows(f: &mut Fake, since: u64) -> Vec<(String, String, String)> {
    let store = f.dir.path("store.sqlite");
    let v = resource::gate(f, &store, &json!({"since": utc(since)})).unwrap();
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["gate"].as_str().unwrap().to_owned(),
                r["verdict"].as_str().unwrap().to_owned(),
                r["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}
fn row<'a>(rows: &'a [(String, String, String)], gate: &str) -> Option<(&'a str, &'a str)> {
    rows.iter()
        .find(|r| r.0 == gate)
        .map(|r| (r.1.as_str(), r.2.as_str()))
}

// spec: specs/ap/resource.md#R13
#[test]
fn r13_fewer_than_12_samples_of_this_host_since_the_time_are_not_enough() {
    let mut f = Fake::new(None);
    let since = NOW - DAY;
    samples(&mut f, since, 11, |_, _| {});
    // Before the time, and of another host: not counted.
    samples(&mut f, since - 10 * DAY, 5, |_, _| {});
    f.put(
        "host_sample",
        "pc@1",
        json!({"host": oid("host", "pc"), "at": utc(since + 1), "answers": true, "free": true}),
    );
    let store = f.dir.path("store.sqlite");
    let v = resource::gate(&mut f, &store, &json!({"since": utc(since)})).unwrap();
    assert_eq!(
        (v["since"].clone(), v["at"].clone(), v["samples"].clone()),
        (json!(utc(since)), json!(utc(NOW)), json!(11))
    );
    assert_eq!(
        v["rows"][0],
        json!({"gate": "G1-G3", "verdict": "N.A.", "text": format!("G1-G3 need >= 12 samples since {} (have 11)", utc(since))})
    );
    let gates: Vec<&str> = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["gate"].as_str().unwrap())
        .collect();
    assert_eq!(gates, ["G1-G3", "G4", "G6", "G7"]);
    assert_eq!(
        resource::row_line(&v["rows"][0]),
        format!(
            "N.A.   G1-G3 need >= 12 samples since {} (have 11)",
            utc(since)
        )
    );
}

// spec: specs/ap/resource.md#R13
#[test]
fn r13_g1_g2_g3_on_both_sides_of_their_bounds_decided_before_rounding() {
    let since = NOW - DAY;
    // At the bounds: memory 2500, load per CPU 1.50 at the 90th percentile, 1 in 10 with a wait.
    let mut f = Fake::new(None);
    samples(&mut f, since, 20, |i, s| {
        s["available_mb"] = json!(if i == 3 { 2_500 } else { 4_000 });
        s["load_x100"] = json!(if i >= 17 { 300 } else { 100 + i as u64 });
        s["cpu_waits"] = json!(u64::from(i % 10 == 0));
        s["units_running"] = json!(if i < 5 { 3 } else { 1 });
        s["units_ready"] = json!(if (5..9).contains(&i) { 0 } else { 2 });
    });
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G1"),
        Some(("PASS", "G1 free memory min 2500 MB >= 2500"))
    );
    assert_eq!(
        row(&rows, "G2"),
        Some(("PASS", "G2 load1 p90 per CPU 1.50 <= 1.50"))
    );
    assert_eq!(
        row(&rows, "G3"),
        Some(("PASS", "G3 samples with a cpu wait 10% <= 10%"))
    );
    assert_eq!(
        row(&rows, "INFO"),
        Some((
            "INFO",
            "INFO slots full in 25% of samples; a slot idle with nothing ready in 20%"
        ))
    );
    // Over each bound, two of them by a value that rounds onto it.
    let mut f = Fake::new(None);
    samples(&mut f, since, 125, |i, s| {
        s["available_mb"] = json!(if i == 3 { 2_499 } else { 4_000 });
        s["load_x100"] = json!(if i >= 100 { 301 } else { 100 });
        s["cpu_waits"] = json!(u64::from(i < 13));
    });
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G1"),
        Some(("FAIL", "G1 free memory min 2499 MB >= 2500"))
    );
    assert_eq!(
        row(&rows, "G2"),
        Some(("FAIL", "G2 load1 p90 per CPU 1.50 <= 1.50"))
    );
    assert_eq!(
        row(&rows, "G3"),
        Some(("FAIL", "G3 samples with a cpu wait 10% <= 10%"))
    );
    let line = resource::row_line(&json!({"gate": "G1", "verdict": "FAIL", "text": "G1 x"}));
    assert_eq!(line, "FAIL   G1 x");
}

// spec: specs/ap/resource.md#R13
#[test]
fn r13_samples_without_counts_leave_g3_not_applicable_and_no_info() {
    let mut f = Fake::new(None);
    let since = NOW - DAY;
    samples(&mut f, since, 12, |_, s| {
        for k in ["slots", "units_running", "units_ready", "cpu_waits"] {
            s.as_object_mut().unwrap().remove(k);
        }
    });
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G3"),
        Some(("N.A.", "G3 needs a sample with counts"))
    );
    assert_eq!(row(&rows, "INFO"), None);
    let gates: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(gates, ["G1", "G2", "G3", "G4", "G6", "G7"]);
}

/// A merged unit for the indicators: its log rows and its one packet review closed with `verdict`.
fn merged(f: &mut Fake, key: &str, merged_at: u64, verdict: &str, stops: &[&str]) {
    f.put("unit", key, json!({"state": "merged", "class": "plain"}));
    let unit = json!({"native_key": key});
    for (action, at) in [("start_unit", merged_at - 3_600), ("merge_unit", merged_at)] {
        f.logs.entry(action.into()).or_default().insert(
            0,
            json!({"submitted_at": utc(at), "parameters": {"unit": unit}}),
        );
    }
    for cause in stops {
        f.logs
            .entry("stop_unit".into())
            .or_default()
            .insert(0, json!({"submitted_at": utc(merged_at - 60), "parameters": {"unit": unit, "cause": cause}}));
    }
    let review = format!("review-{key}");
    f.put("review", &review, json!({"kind": "packet", "status": "closed", "verdict": verdict, "closed_at": utc(merged_at - 30)}));
    f.views
        .insert(("reviews_of_unit".into(), key.into()), vec![review]);
}

// spec: specs/ap/resource.md#R14
#[test]
fn r14_g4_and_g5_come_from_the_indicators() {
    let since = NOW - DAY;
    let mut f = Fake::new(None);
    merged(&mut f, "b-1", since - 100, "accept", &[]);
    merged(&mut f, "b-2", since - 200, "accept", &[]);
    merged(&mut f, "a-1", since + 100, "accept", &["check_failed"]);
    merged(
        &mut f,
        "a-2",
        since + 200,
        "repair",
        &["check_failed", "other"],
    );
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G4"),
        Some((
            "FAIL",
            "G4 first-accept share 0.50 vs 1.00 before (2 / 2 units), drop <= 0.15"
        ))
    );
    assert_eq!(
        row(&rows, "G5"),
        Some((
            "INFO",
            "G5 units merged in the window: 2; check_failed stops: 2"
        ))
    );
    // No drop: PASS.
    let mut f = Fake::new(None);
    merged(&mut f, "b-1", since - 100, "repair", &[]);
    merged(&mut f, "a-1", since + 100, "accept", &[]);
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G4"),
        Some((
            "PASS",
            "G4 first-accept share 1.00 vs 0.00 before (1 / 1 units), drop <= 0.15"
        ))
    );
    assert_eq!(
        row(&rows, "G5"),
        Some((
            "INFO",
            "G5 units merged in the window: 1; check_failed stops: 0"
        ))
    );
    // Without units before the time, or after it.
    let mut f = Fake::new(None);
    merged(&mut f, "a-1", since + 100, "accept", &[]);
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G4"),
        Some((
            "N.A.",
            format!(
                "G4 first-accept share needs merged units after {}",
                utc(since)
            )
            .as_str()
        ))
    );
    assert!(row(&rows, "G5").is_some());
    let mut f = Fake::new(None);
    merged(&mut f, "b-1", since - 100, "accept", &[]);
    let rows = gate_rows(&mut f, since);
    assert_eq!(row(&rows, "G4").unwrap().0, "N.A.");
    assert_eq!(row(&rows, "G5"), None);
}

// spec: specs/ap/resource.md#R15
#[test]
fn r15_g6_counts_the_refused_runs_and_the_highest_utilization() {
    let since = NOW - DAY;
    let run = |f: &mut Fake, key: &str, ended: u64, limit: &str| {
        f.put(
            "run",
            key,
            json!({"account": "acct-a", "ended_at": utc(ended), "limit": limit}),
        );
    };
    let mut f = Fake::new(None);
    run(
        &mut f,
        "r1",
        since + 1,
        r#"{"utilization": 0.8, "status": "allowed"}"#,
    );
    run(
        &mut f,
        "r2",
        since + 2,
        r#"{"windows": {"five_hour": {"utilization": 0.5, "status": "allowed_warning"}}}"#,
    );
    run(
        &mut f,
        "r3",
        since - 1,
        r#"{"utilization": 0.99, "status": "rejected"}"#,
    );
    run(&mut f, "r4", since + 3, "five-hour 99% rejected");
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G6"),
        Some((
            "PASS",
            "G6 account rate limit: refused/limited runs 0 == 0, utilization 0.80 <= 0.80"
        ))
    );
    run(&mut f, "r5", since + 4, r#"{"utilization": 0.804}"#);
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G6"),
        Some((
            "FAIL",
            "G6 account rate limit: refused/limited runs 0 == 0, utilization 0.80 <= 0.80"
        ))
    );
    let mut f = Fake::new(None);
    run(
        &mut f,
        "r1",
        since + 1,
        r#"{"windows": {"a": {"utilization": 0.1, "status": "rejected"}, "b": {"status": "allowed"}}}"#,
    );
    run(
        &mut f,
        "r2",
        since + 2,
        r#"{"utilization": 0.2, "status": "limited", "windows": {"a": {"status": "rejected"}}}"#,
    );
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G6"),
        Some((
            "FAIL",
            "G6 account rate limit: refused/limited runs 2 == 0, utilization 0.20 <= 0.80"
        ))
    );
}

// spec: specs/ap/resource.md#R16
#[test]
fn r16_g7_compares_the_median_seconds_of_a_worktree_on_both_hosts() {
    let since = NOW - DAY;
    let check = |f: &mut Fake, key: &str, host: &str, dir: &str, ms: u64, exit: i64, end: u64| {
        f.put(
            "check",
            key,
            json!({"host": oid("host", host), "dir": dir, "command": "./check", "milliseconds": ms,
            "exit": exit, "end": utc(end), "repository": "o/r", "head": "h"}),
        );
    };
    let mut f = Fake::new(None);
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G7"),
        Some((
            "N.A.",
            format!(
                "G7 needs a worktree checked on both hosts since {}",
                utc(since)
            )
            .as_str()
        ))
    );
    // Pair a: PC median 2000 over 5000 here, 0.40; pair b: 3000 over 5000, 0.60; their median 0.50.
    check(&mut f, "1", "pc", "/a", 1_000, 0, since + 1);
    check(&mut f, "2", "pc", "/a", 3_000, 0, since + 2);
    check(&mut f, "3", "local", "/a", 5_000, 0, since + 3);
    check(&mut f, "4", "pc", "/b", 3_000, 0, since + 4);
    check(&mut f, "5", "local", "/b", 5_000, 0, since + 5);
    // Not read: a failed check, one before the time, and a worktree of one host.
    check(&mut f, "6", "pc", "/b", 99_000, 1, since + 6);
    check(&mut f, "7", "local", "/b", 1, 0, since - 1);
    check(&mut f, "8", "pc", "/c", 1, 0, since + 8);
    let rows = gate_rows(&mut f, since);
    assert_eq!(
        row(&rows, "G7"),
        Some((
            "PASS",
            "G7 check seconds on the PC / on this host: median ratio 0.50 over 2 worktrees <= 0.50"
        ))
    );
    // A ratio that rounds onto the bound fails.
    check(&mut f, "9", "pc", "/a", 1_040, 0, since + 9);
    check(&mut f, "10", "pc", "/a", 3_000, 0, since + 10);
    let rows = gate_rows(&mut f, since);
    // Pair a: PC median 2020 over 5000, 0.404; pair b 0.60; the median 0.502.
    assert_eq!(
        row(&rows, "G7"),
        Some((
            "FAIL",
            "G7 check seconds on the PC / on this host: median ratio 0.50 over 2 worktrees <= 0.50"
        ))
    );
}

// spec: specs/ap/resource.md#R17
#[test]
fn r17_the_service_and_the_timer() {
    let service = include_str!("../../../deploy/ap-resource.service");
    let lines: Vec<&str> = service.lines().collect();
    for want in [
        "Type=oneshot",
        "ExecStart=/usr/local/bin/ap resource",
        "TimeoutStartSec=180",
        "Environment=HOME=/root",
        "ProtectSystem=full",
        "NoNewPrivileges=yes",
        "SystemCallArchitectures=native",
    ] {
        assert_eq!(lines.iter().filter(|l| **l == want).count(), 1, "{want}");
    }
    let watch = include_str!("../../../deploy/ap-watch.service");
    for l in watch
        .lines()
        .filter(|l| l.starts_with("Environment=HOME=") || l.starts_with("Environment=PATH="))
    {
        assert!(lines.contains(&l), "{l}");
    }
    assert!(!lines.iter().any(|l| l.starts_with("Slice=")));
    assert!(lines.iter().any(|l| l.starts_with("# No Slice=")));
    let timer: Vec<&str> = include_str!("../../../deploy/ap-resource.timer")
        .lines()
        .collect();
    for want in [
        "OnCalendar=*:2/5",
        "AccuracySec=10s",
        "WantedBy=timers.target",
    ] {
        assert!(timer.contains(&want), "{want}");
    }
}

// spec: specs/ap/check.md#R21
#[test]
fn check_r21_every_record_line_of_ap_check_becomes_a_check_record() {
    let line = check_line("local", "/fixture/wt/b", NOW - 3_600, 12.3456, 3).to_string();
    let (records, skipped) = resource::check_records(format!("{line}\n").as_bytes(), NOW);
    assert_eq!(skipped, 0);
    assert_eq!(
        records,
        [
            json!({"type": "check", "key": &hex(line.as_bytes())[..32], "observed_at": NOW - 3_600, "host": "local",
            "repository": "fixture-owner/widgets", "head": "5".repeat(40), "dir": "/fixture/wt/b",
            "command": "cargo test --locked", "end": utc(NOW - 3_600), "exit": 3, "milliseconds": 12_346,
            "start": utc(NOW - 3_660), "reason": "pc_busy"})
        ]
    );
}

// spec: specs/ap/run-object.md#R15
#[test]
fn run_object_r15_start_run_and_start_install_take_a_host_that_ap_run_does_not_give() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
    assert!(schema["version"].as_u64().unwrap() >= 17);
    let reference = |kind: &str| json!({"type": {"ref": kind}, "required": false});
    let start_run = &schema["action_types"]["start_run"];
    assert_eq!(start_run["parameters"]["host"], reference("host"));
    assert_eq!(start_run["parameters"]["uses"], reference("account"));
    assert_eq!(start_run["rules"][0]["set"]["host"], "$host");
    assert_eq!(start_run["rules"][0]["set"]["uses"], "$uses");
    let start_install = &schema["action_types"]["start_install"];
    assert_eq!(start_install["parameters"]["host"], reference("host"));
    assert_eq!(start_install["rules"][0]["set"]["host"], "$host");
    let run = &schema["object_types"]["run"]["properties"];
    assert_eq!(
        (run["host"].clone(), run["uses"].clone()),
        (reference("host"), reference("account"))
    );
    assert_eq!(
        schema["object_types"]["install"]["properties"]["host"],
        reference("host")
    );
    for action in ["start_run", "start_install"] {
        let criteria = schema["action_types"][action]["submission_criteria"].to_string();
        assert!(
            !criteria.contains("$host") && !criteria.contains("$uses"),
            "{action}"
        );
    }
    // `ap run` builds its start without them.
    let run_rs = include_str!("../src/run.rs");
    assert!(!run_rs.contains("\"uses\""));
}

/// The real engine: a scratch store of the repository's schema whose two record sources read a scratch directory and
/// whose functions' program is a copy of the built `ap`.
struct Bed {
    root: PathBuf,
    ont: PathBuf,
    path: String,
}
impl Drop for Bed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Bed {
    fn new() -> Self {
        let ont = PathBuf::from(std::env::var("ONT_BIN").expect("ONT_BIN names the engine"));
        let base = std::env::temp_dir().join(format!(
            "ap-resource-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let root = std::fs::canonicalize(&base).unwrap();
        for d in ["bin", "pc"] {
            std::fs::create_dir(root.join(d)).unwrap();
        }
        let ap = root.join("bin/ap");
        std::fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
        std::fs::set_permissions(&ap, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ts = root.join("bin/tailscale");
        std::fs::write(
            &ts,
            TAILSCALE.replace("@PC@", root.join("pc").to_str().unwrap()),
        )
        .unwrap();
        std::fs::set_permissions(&ts, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("pc/loadavg"), "0.40 0.3 0.2 1/1 1\n").unwrap();
        std::fs::write(root.join("pc/meminfo"), "MemAvailable:   8388608 kB\n").unwrap();
        let mut schema: Value =
            serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
        for (name, suffix) in [
            ("host_samples", "*.samples.jsonl"),
            ("checks", "*.checks.jsonl"),
        ] {
            assert_eq!(
                schema["sources"][name]["glob"],
                format!("{}/{suffix}", resource::DIRECTORY)
            );
            schema["sources"][name]["glob"] =
                json!(format!("{}/resource/{suffix}", root.display()));
        }
        for name in ["capacity", "gate"] {
            assert_eq!(schema["functions"][name]["program"], "/usr/local/bin/ap");
            schema["functions"][name]["program"] = json!(ap.to_str().unwrap());
        }
        std::fs::write(root.join("schema.json"), schema.to_string()).unwrap();
        let path = format!(
            "{}:{}:/usr/bin:/bin",
            root.join("bin").display(),
            ont.parent().unwrap().display()
        );
        let bed = Self { root, ont, path };
        let (code, out) = bed.ont(
            &["schema", "install", bed.at("schema.json").to_str().unwrap()],
            "",
        );
        assert_eq!(code, 0, "{out}");
        bed
    }
    fn at(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn store(&self) -> String {
        self.at("store.sqlite").to_str().unwrap().to_owned()
    }
    fn ont(&self, args: &[&str], stdin: &str) -> (i32, Value) {
        use std::io::Write;
        let mut child = Command::new(&self.ont)
            .arg("--store")
            .arg(self.store())
            .args(args)
            .env("PATH", &self.path)
            .env_remove("ONT_ROLE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "ont {args:?}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code().unwrap_or(-1), v)
    }
    fn apply(&self, action: &str, rid: &str, parameters: Value) -> Value {
        let body = json!({"action_type": action, "request_id": rid, "parameters": parameters});
        let (code, v) = self.ont(&["action", "apply"], &body.to_string());
        assert_eq!(code, 0, "{v}");
        v
    }
    fn pass(&self) -> Value {
        let out = Command::new(self.at("bin/ap"))
            .args([
                "resource",
                "--directory",
                self.at("resource").to_str().unwrap(),
                "--store",
                &self.store(),
            ])
            .env("PATH", &self.path)
            .env("AP_POLICY_FILE", self.at("policy.json"))
            .env("AP_CHECKS_LOG", self.at("checks.jsonl"))
            .env_remove("ONT_ROLE")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let v: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{v} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        v
    }
    fn function(&self, name: &str, parameters: Value) -> Value {
        let (code, v) = self.ont(
            &["fn", name],
            &json!({"parameters": parameters}).to_string(),
        );
        assert_eq!(code, 0, "{v}");
        v["answer"].clone()
    }
}

// spec: specs/ap/resource.md#R9
#[test]
#[ignore = "runs the real engine: ./check runs it with --ignored and ONT_BIN"]
fn engine_two_passes_into_a_store_with_a_running_run_then_both_functions() {
    let bed = Bed::new();
    std::fs::write(
        bed.at("policy.json"),
        json!({"hosts": {"pc": {"ssh": "fixture@fixture-pc", "work": "/fixture/work", "busy_load": 0.9}},
            "work": {"slots": 3}, "accounts": {"claude": ["/fixture/accounts/acct-a"]}})
        .to_string(),
    )
    .unwrap();
    // A run that ended with a reading of its limit, and one still running, both of the account.
    let started = bed.apply(
        "start_run",
        "fixture-run-1",
        json!({"role": "implementer", "runtime": "claude", "model": "m", "account": "acct-a"}),
    );
    let run = started["result"]["edits"][0]["id"].clone();
    let limit = r#"{"utilization":0.5,"status":"allowed"}"#;
    bed.apply(
        "close_run",
        "fixture-run-1-close",
        json!({"run": run, "state": "done", "limit": limit}),
    );
    bed.apply(
        "start_run",
        "fixture-run-2",
        json!({"role": "implementer", "runtime": "claude", "model": "m", "account": "acct-a"}),
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let log = [
        check_line("pc", "/fixture/wt/a", now - 120, 30.0, 0),
        check_line("local", "/fixture/wt/a", now - 60, 100.0, 0),
    ]
    .iter()
    .map(|l| format!("{l}\n"))
    .collect::<String>();
    std::fs::write(bed.at("checks.jsonl"), log).unwrap();
    let first = bed.pass();
    assert_eq!(first["failed"], json!([]), "{first}");
    assert_eq!(
        (
            first["hosts"][0]["declared"].clone(),
            first["hosts"][1]["declared"].clone()
        ),
        (json!(true), json!(true))
    );
    assert_eq!(first["hosts"][1]["answers"], true);
    assert_eq!(
        first["accounts"],
        json!([{"account": "acct-a", "observed": true}])
    );
    assert_eq!(first["checks"], json!({"records": 2, "skipped": 0}));
    assert_eq!(first["sources"]["host_samples"]["status"], "ok");
    // The PC goes down: the next pass says so, and capacity with it.
    std::fs::write(bed.at("pc/down"), "").unwrap();
    let second = bed.pass();
    assert_eq!(second["failed"], json!([]), "{second}");
    assert_eq!(
        (
            second["hosts"][0]["declared"].clone(),
            second["hosts"][1]["declared"].clone()
        ),
        (json!(false), json!(false))
    );
    assert_eq!(second["hosts"][1]["answers"], false);
    assert_eq!(
        second["accounts"],
        json!([{"account": "acct-a", "observed": false}])
    );
    let capacity = bed.function("capacity", json!({}));
    let hosts: Vec<(&str, &Value)> = capacity["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["host"].as_str().unwrap(), &h["answers"]))
        .collect();
    assert_eq!(
        hosts,
        [("local", &json!(true)), ("pc", &json!(false))],
        "{capacity}"
    );
    assert_eq!(capacity["machines"][1]["up"], false);
    assert_eq!(capacity["can_start"], json!(["local"]));
    assert_eq!(
        capacity["accounts"][0]["allowance"],
        json!({"utilization": 0.5, "status": "allowed"})
    );
    assert_eq!(capacity["accounts"][0]["source"], "run");
    let gate = bed.function("gate", json!({"since": utc(now - 3_600)}));
    assert_eq!(gate["samples"], 2, "{gate}");
    let rows: Vec<String> = gate["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(resource::row_line)
        .collect();
    assert_eq!(
        rows[0],
        format!(
            "N.A.   G1-G3 need >= 12 samples since {} (have 2)",
            utc(now - 3_600)
        )
    );
    assert!(
        rows.contains(
            &"PASS   G6 account rate limit: refused/limited runs 0 == 0, utilization 0.50 <= 0.80"
                .to_owned()
        ),
        "{rows:?}"
    );
    assert!(rows.contains(&"PASS   G7 check seconds on the PC / on this host: median ratio 0.30 over 1 worktrees <= 0.50".to_owned()), "{rows:?}");
}
