//! `ap deliver` against a fake host (README "Delivery"). The fake `System` is a small model of the
//! world the command reads: the clone (a first-parent chain of merges, each with a second parent
//! and a tree), the pair (the commit `ap --version` of the installed file names and the commit
//! whose content the tree holds), the store (reviews and install attempts, searched by `all` of
//! `eq`), `df`, `flock`, and the installers, which change the pair as the case says and print
//! what the case says from a real child. Every call is recorded in order. No test builds,
//! installs, fetches or touches the host's store; the lock, the backup directories, the reports
//! and the state directory are files of a temporary directory.
use ap::deliver::execute;
use ap::work::{Answer, Failure, Reply, System};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const TARGET: &str = "/usr/local/bin/ap";
const STARTED: &str = "2026-11-14T10:00:00Z";
const TARBALL: &str = "agents-before-20261114T100001Z-7.tar";

/// A commit of the chain: `c(0)` is installed, `c(k)` is the tip.
fn c(n: u32) -> String {
    format!("{n:040}")
}
/// The second parent of `c(n)`: the head a review accepted.
fn h(n: u32) -> String {
    format!("{:040}", 100 + n)
}
fn tree(n: u32) -> String {
    format!("{:040}", 200 + n)
}
/// A commit the clone knows that is not on the chain.
fn other() -> String {
    format!("{:040}", 999)
}
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type":kind,"namespace":{"instance":"vps://palantirkc","account":"root"},"native_key":key})
}
fn target() -> Value {
    oid("installed_artifact", TARGET)
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
fn content(commit: &str, file: &str) -> Vec<u8> {
    format!("{file} at {commit}\n").into_bytes()
}
fn binary(commit: &str) -> Vec<u8> {
    format!("ap binary of {commit}").into_bytes()
}
/// A tar archive (ustar) of the content tree of `label` (None: a half-swapped tree), as
/// install-content.sh keeps one of the tree it replaces.
fn tar(label: Option<&str>) -> Vec<u8> {
    let mut out = vec![];
    for f in ["AGENTS.md", "skills/a/SKILL.md"] {
        let data = label.map_or(b"half swapped".to_vec(), |l| content(l, f));
        let mut h = [0u8; 512];
        h[..f.len()].copy_from_slice(f.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[156] = b'0';
        h[257..265].copy_from_slice(b"ustar\x0000");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend(h);
        out.extend(&data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend([0u8; 1024]);
    out
}
fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

struct Commit {
    id: String,
    parents: Vec<String>,
    tree: String,
}
/// What one installer does: the commit it leaves the file or the tree at (None: unchanged), its
/// exit and what it prints.
#[derive(Clone)]
struct Inst {
    file: Option<Option<String>>,
    tree: Option<Option<String>>,
    exit: i32,
    out: String,
    err: String,
}
enum Start {
    Json(Value),
    Fail,
}

struct W {
    t: PathBuf,
    /// Every program, in order: (program, arguments, JSON body, or for a launch its cwd and env).
    calls: Vec<(String, Vec<String>, Value)>,
    chain: Vec<Commit>,
    heads: BTreeMap<String, String>,
    file: Option<String>,
    tree: Option<String>,
    bytes: Vec<u8>,
    reviews: Vec<Value>,
    installs: Vec<Value>,
    schema: u64,
    tip_schema: u64,
    ont: String,
    pin: String,
    free: u64,
    build_busy: bool,
    fetch_fails: bool,
    start: Option<Start>,
    close_fails: bool,
    index_fails: bool,
    inst: BTreeMap<&'static str, Inst>,
    /// For each launch: whether the lock's descriptor was open in this process without
    /// close-on-exec, so that the child inherits it.
    lock_inherited: Vec<bool>,
    /// A second call, started while this one runs (at its `df`).
    nested: Option<Box<W>>,
    nested_result: Option<(Value, u8)>,
    /// The host lock (`AP_DELIVER_LOCK`); two fakes of one test may share it.
    lock: String,
    /// The tip's `content/` is the installed commit's: both have the same `content/SHA256SUMS`.
    same_content: bool,
    /// The clone's `origin/main` at its first read, when it is not the tip (a dead call's clone).
    origin_once: Option<String>,
    /// The clone cannot answer: `git show` and `rev-parse` fail.
    lost: bool,
    /// `INVOCATION_ID` of the environment.
    invocation: Option<String>,
}

impl W {
    /// Two merges after the installed commit: c(0) <- c(1) <- c(2), each accepted.
    fn new() -> Self {
        Self::with(2)
    }
    fn with(k: u32) -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-deliver-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in ["bin", "backup", "agents", "content-backup", "reports"] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        let mut chain = vec![Commit {
            id: c(0),
            parents: vec![],
            tree: tree(0),
        }];
        let mut heads = BTreeMap::new();
        for n in 1..=k {
            chain.push(Commit {
                id: c(n),
                parents: vec![c(n - 1), h(n)],
                tree: tree(n),
            });
            heads.insert(h(n), tree(n));
        }
        let tip = c(k);
        let installed = c(0);
        let mut w = Self {
            t,
            calls: vec![],
            chain,
            heads,
            file: Some(installed.clone()),
            tree: Some(installed.clone()),
            bytes: binary(&installed),
            reviews: vec![],
            installs: vec![],
            schema: 8,
            tip_schema: 8,
            ont: "e".repeat(40),
            pin: "e".repeat(40),
            free: 50_000,
            build_busy: false,
            fetch_fails: false,
            start: None,
            close_fails: false,
            index_fails: false,
            inst: BTreeMap::new(),
            lock_inherited: vec![],
            nested: None,
            nested_result: None,
            lock: String::new(),
            same_content: false,
            origin_once: None,
            lost: false,
            invocation: None,
        };
        w.lock = w.p("deliver.lock");
        for n in 1..=k {
            w.review(n, "o/r", "accept", "closed");
        }
        // Noise every search meets: a running review without a verdict, a repair, and an accepting
        // review of another repository, all of a head of the chain.
        w.review_row(json!({"head": h(1), "kind": "packet", "status": "running"}));
        w.review(1, "o/r", "repair", "closed");
        w.review(k, "x/other", "accept", "closed");
        let tarball = w.p(&format!("content-backup/{TARBALL}"));
        w.inst = BTreeMap::from([
            ("ap", installer(Some(&tip), None, 0, "", "")),
            (
                "content",
                installer(
                    None,
                    Some(&tip),
                    0,
                    &format!("installed {tip} files=2 paths=2 backup={tarball}\n"),
                    "",
                ),
            ),
            (
                "ap-rollback",
                installer(
                    Some(&installed),
                    None,
                    0,
                    &format!("ap 0.1.0 {installed}\n"),
                    "",
                ),
            ),
            (
                "content-rollback",
                installer(None, Some(&installed), 0, "rolled back\n", ""),
            ),
        ]);
        w
    }
}
fn installer(file: Option<&str>, tree: Option<&str>, exit: i32, out: &str, err: &str) -> Inst {
    Inst {
        file: file.map(|f| Some(f.to_owned())),
        tree: tree.map(|t| Some(t.to_owned())),
        exit,
        out: out.into(),
        err: err.into(),
    }
}
impl W {
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    fn tip(&self) -> String {
        self.chain.last().unwrap().id.clone()
    }
    fn tarball(&self) -> String {
        self.p(&format!("content-backup/{TARBALL}"))
    }
    /// A closed (or other `status`) review of `h(n)` with a report file whose digest it names.
    fn review(&mut self, n: u32, repository: &str, verdict: &str, status: &str) -> Value {
        let key = format!("rv-{}", self.reviews.len() + 1);
        let report = self.p(&format!("reports/{key}.md"));
        let bytes = format!("Verdict: {verdict}\n{}\n", h(n));
        std::fs::write(&report, &bytes).unwrap();
        let row = json!({"id": oid("review", &key), "kind": "packet", "head": h(n), "status": status,
            "verdict": verdict, "repository": repository, "report": report,
            "report_sha256": sha(bytes.as_bytes()), "closed_at": format!("2026-11-13T00:00:{:02}Z", self.reviews.len())});
        self.reviews.push(row.clone());
        row["id"].clone()
    }
    fn review_row(&mut self, mut row: Value) {
        row["id"] = oid("review", &format!("rv-{}", self.reviews.len() + 1));
        self.reviews.push(row);
    }
    /// The id of the accepting review of `h(n)` in `o/r` that the fixture made first.
    fn accepting(&self, n: u32) -> Value {
        self.reviews
            .iter()
            .find(|r| r["head"] == h(n) && r["verdict"] == "accept" && r["repository"] == "o/r")
            .unwrap()["id"]
            .clone()
    }
    fn request(&self) -> Value {
        json!({"repository":"o/r","url":"https://example.invalid/o/r.git","state":self.p("state"),
            "build_lock":self.p("build.lock"),"by":"session-1","reason":"after a merge"})
    }
    fn run(&mut self) -> (Value, u8) {
        let r = self.request();
        execute(self, &r)
    }
    fn run_with(&mut self, r: &Value) -> (Value, u8) {
        execute(self, r)
    }
    /// The timer's request: none of `by`, `reason`, `past`, `rollback`.
    fn timer(&self) -> Value {
        let mut r = self.request();
        let o = r.as_object_mut().unwrap();
        o.remove("by");
        o.remove("reason");
        r
    }
    /// A Lead's rollback request.
    fn rollback(&self) -> Value {
        let mut r = self.request();
        r["rollback"] = json!(true);
        r
    }
    /// A running attempt of the target that a dead call left.
    fn dead_attempt(&mut self, commit: &str, from: &str, from_sha256: &str) -> Value {
        let id = oid("install", "act-dead");
        self.installs.push(
            json!({"id": id, "target": target(), "kind": "forward", "repository": "o/r",
            "commit": commit, "attempt": 1, "from": from, "from_sha256": from_sha256, "by": "dead",
            "status": "running", "started_at": STARTED, "running_in": target()}),
        );
        id
    }
    /// A closed attempt of the target, older than every attempt a call starts and than a dead
    /// call's; each one newer than the one before.
    fn closed(&mut self, kind: &str, commit: &str, from: &str, outcome: &str, by: &str) -> Value {
        let n = self.installs.len();
        let id = oid("install", &format!("act-closed-{n}"));
        self.installs.push(
            json!({"id": id, "target": target(), "kind": kind, "repository": "o/r", "commit": commit,
            "attempt": 1, "from": from, "from_sha256": sha(&binary(from)), "by": by, "status": "closed",
            "started_at": format!("2026-11-14T09:{n:02}:00Z"), "outcome": outcome,
            "closed_at": format!("2026-11-14T09:{n:02}:30Z")}),
        );
        id
    }
    fn launched(&self) -> Vec<String> {
        self.calls
            .iter()
            .filter(|(p, _, _)| p == "bash")
            .map(|(_, a, _)| {
                let script = if a[0].ends_with("install-ap.sh") {
                    "ap"
                } else {
                    "content"
                };
                if a[1] == "rollback" {
                    format!("{script}-rollback")
                } else {
                    script.into()
                }
            })
            .collect()
    }
    fn applies(&self, action: &str) -> Vec<Value> {
        self.calls
            .iter()
            .filter(|(p, a, b)| {
                p == "ont" && a.first().is_some_and(|x| x == "action") && b["action_type"] == action
            })
            .map(|(_, _, b)| b.clone())
            .collect()
    }
    fn searches(&self) -> Vec<Value> {
        self.calls
            .iter()
            .filter(|(p, a, _)| p == "ont" && a.first().is_some_and(|x| x == "search"))
            .map(|(_, _, b)| b.clone())
            .collect()
    }
    fn last(&self) -> Option<Value> {
        let bytes = std::fs::read(self.p("state/last.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
    fn ok(v: Value) -> Answer {
        let stdout = serde_json::to_vec(&v).unwrap();
        Ok(Reply {
            success: v["status"] == "ok",
            bytes: stdout.clone(),
            stdout,
        })
    }
    fn text(success: bool, text: &str) -> Answer {
        Ok(Reply {
            success,
            bytes: text.as_bytes().to_vec(),
            stdout: text.as_bytes().to_vec(),
        })
    }
    /// The commit whose files `content/` of `commit` holds.
    fn label(&self, commit: &str) -> String {
        if self.same_content && commit == self.tip() {
            c(0)
        } else {
            commit.to_owned()
        }
    }
    fn sums(&self, commit: &str) -> String {
        let commit = &self.label(commit);
        format!(
            "{}  AGENTS.md\n{}  skills/a/SKILL.md\n",
            sha(&content(commit, "AGENTS.md")),
            sha(&content(commit, "skills/a/SKILL.md"))
        )
    }
    fn git(&mut self, a: &[String]) -> Answer {
        let clone = self.p("state/r");
        if a[0] == "init" {
            assert_eq!(a, args(&["init", "--quiet", &clone]));
            return Self::text(true, "");
        }
        assert_eq!(&a[..2], ["-C", clone.as_str()], "{a:?}");
        let rest: Vec<&str> = a[2..].iter().map(String::as_str).collect();
        match rest.as_slice() {
            ["fetch", ..] => Self::text(!self.fetch_fails, ""),
            ["show" | "rev-parse", ..] if self.lost => Self::text(false, ""),
            ["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"] => {
                let origin = self.origin_once.take().unwrap_or_else(|| self.tip());
                Self::text(!self.fetch_fails, &format!("{origin}\n"))
            }
            ["checkout", "--quiet", "--force", "--detach", _] => Self::text(true, ""),
            ["show", spec] => {
                let (commit, path) = spec.split_once(':').unwrap();
                match path {
                    "content/SHA256SUMS" => Self::text(true, &self.sums(commit)),
                    "ontology/ENGINE-PIN" => Self::text(true, &format!("{}\n", self.pin)),
                    "ontology/work.schema.json" => Self::text(
                        true,
                        &json!({"format": 1, "version": self.tip_schema}).to_string(),
                    ),
                    _ => panic!("unexpected show {spec}"),
                }
            }
            ["rev-list", "--first-parent", "--parents", range] => {
                let (from, to) = range.split_once("..").unwrap();
                assert_eq!(to, self.tip());
                let mut lines = vec![];
                for commit in self.chain.iter().rev() {
                    if commit.id == from {
                        break;
                    }
                    let mut words = vec![commit.id.clone()];
                    words.extend(commit.parents.iter().cloned());
                    lines.push(words.join(" "));
                }
                Self::text(true, &format!("{}\n", lines.join("\n")))
            }
            ["rev-parse", own, second] => {
                let commit = own.strip_suffix("^{tree}").unwrap();
                assert_eq!(*second, format!("{commit}^2^{{tree}}"));
                let c = self.chain.iter().find(|c| c.id == commit).unwrap();
                match c.parents.get(1).and_then(|p| self.heads.get(p)) {
                    Some(t) => Self::text(true, &format!("{}\n{t}\n", c.tree)),
                    None => Self::text(false, ""),
                }
            }
            _ => panic!("unexpected git call {a:?}"),
        }
    }
    fn store(&mut self, a: &[String], body: &Value) -> Answer {
        let schema = json!({"work": {"sha256": "f".repeat(64), "version": self.schema}});
        match a[0].as_str() {
            "--version" => Self::text(true, &format!("ont 0.1.0 {}\n", self.ont)),
            "index" => {
                assert_eq!(a, args(&["index", "--source", "installed_binaries"]));
                let status = if self.index_fails { "refused" } else { "ok" };
                Self::ok(
                    json!({"status": status, "result": {"sources": [{"source": "installed_binaries", "status": status}]}, "schema": schema}),
                )
            }
            "search" => {
                let kind = body["object_type"].as_str().unwrap();
                let (rows, allowed): (&Vec<Value>, &[&str]) = match kind {
                    "review" => (&self.reviews, &["kind", "head", "status"]),
                    "install" => (&self.installs, &["target", "status", "commit", "kind"]),
                    _ => panic!("unexpected search {body}"),
                };
                let all = body["where"]["all"].as_array().unwrap();
                // The engine answers `qualified: false` to a filter on an optional property as soon
                // as one scanned object lacks it.
                let qualified = all
                    .iter()
                    .all(|f| allowed.contains(&f["eq"][0].as_str().unwrap()));
                let objects: Vec<Value> = rows
                    .iter()
                    .filter(|r| {
                        all.iter()
                            .all(|f| r[f["eq"][0].as_str().unwrap()] == f["eq"][1])
                    })
                    .map(|r| {
                        let fields: serde_json::Map<String, Value> = r
                            .as_object()
                            .unwrap()
                            .iter()
                            .filter(|(k, _)| *k != "id")
                            .map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
                            .collect();
                        json!({"id": r["id"], "head": {"fields": fields}})
                    })
                    .collect();
                Self::ok(
                    json!({"status": "ok", "result": {"basis": {"qualified": qualified, "unresolved_excluded": 0},
                    "objects": objects, "continuation": null, "kind": "object_set"}, "schema": schema}),
                )
            }
            "action" => {
                assert_eq!(a, args(&["action", "apply", "--client", "ap-deliver"]));
                assert_eq!(body["agent"], "ap-deliver");
                let rid = body["request_id"].as_str().unwrap().to_owned();
                let p = body["parameters"].clone();
                if body["action_type"] == "start_install" {
                    match self.start.take() {
                        Some(Start::Json(v)) => return Self::ok(v),
                        Some(Start::Fail) => {
                            return Err(Failure {
                                code: "process_failed",
                                field: "ont".into(),
                                detail: Value::Null,
                            });
                        }
                        None => (),
                    }
                    let id = oid("install", &format!("act-{rid}"));
                    if self.installs.iter().any(|i| i["id"] == id) {
                        return Self::ok(
                            json!({"status": "ok", "result": {"outcome": "already_applied", "edits": [{"change": "initial", "id": id}]}}),
                        );
                    }
                    if self.installs.iter().any(|i| i["status"] == "running") {
                        return Self::ok(
                            json!({"status": "refused", "code": "submission_criteria", "failed": ["install_running"]}),
                        );
                    }
                    let mut row = p.clone();
                    let started = format!("2026-11-14T11:00:{:02}Z", self.installs.len());
                    (row["id"], row["status"], row["started_at"]) =
                        (id.clone(), json!("running"), json!(started));
                    row["running_in"] = p["target"].clone();
                    self.installs.push(row);
                    return Self::ok(
                        json!({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "initial", "id": id}]}, "schema": schema}),
                    );
                }
                assert_eq!(body["action_type"], "close_install");
                if self.close_fails {
                    return Err(Failure {
                        code: "process_failed",
                        field: "ont".into(),
                        detail: Value::Null,
                    });
                }
                let row = self
                    .installs
                    .iter_mut()
                    .find(|i| i["id"] == p["install"])
                    .unwrap();
                if row["status"] != "running" {
                    return Self::ok(
                        json!({"status": "refused", "code": "submission_criteria", "failed": ["install_not_running"]}),
                    );
                }
                for (k, v) in p.as_object().unwrap() {
                    if k != "install" {
                        row[k] = v.clone();
                    }
                }
                row["status"] = json!("closed");
                row.as_object_mut().unwrap().remove("running_in");
                let edits = json!([{"change": "update", "id": p["install"]}, {"change": "update", "id": p["install"]}]);
                Self::ok(
                    json!({"status": "ok", "result": {"outcome": "applied", "edits": edits}, "schema": schema}),
                )
            }
            _ => panic!("unexpected ont call {a:?}"),
        }
    }
}
impl Drop for W {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
/// Whether this process holds `path` open on a descriptor without close-on-exec.
fn inherited(path: &str) -> bool {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .flatten()
        .any(|e| {
            let fd = e.file_name().to_string_lossy().into_owned();
            let to = std::fs::read_link(e.path()).ok();
            let flags =
                std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).unwrap_or_default();
            let flags = flags
                .lines()
                .find_map(|l| l.strip_prefix("flags:"))
                .and_then(|f| u32::from_str_radix(f.trim(), 8).ok());
            to.is_some_and(|t| t.to_str() == Some(path))
                && flags.is_some_and(|f| f & 0o2_000_000 == 0)
        })
}
impl System for W {
    fn call(&mut self, program: &str, a: &[String], body: &Value, _ms: u64) -> Answer {
        self.calls.push((program.into(), a.to_vec(), body.clone()));
        if program == self.p("bin/ap") {
            assert_eq!(a, ["--version"]);
            return match &self.file {
                Some(f) => Self::text(true, &format!("ap 0.1.0 {f}\n")),
                None => Self::text(false, ""),
            };
        }
        match program {
            "git" => self.git(a),
            "ont" => self.store(a, body),
            "df" => {
                assert_eq!(a, args(&["-BM", "--output=avail", &self.p("state")]));
                if let Some(mut second) = self.nested.take() {
                    let r = second.request();
                    self.nested_result = Some(execute(&mut *second, &r));
                }
                Self::text(true, &format!("Avail\n{}M\n", self.free))
            }
            "flock" => {
                assert_eq!(a, args(&["-n", &self.p("build.lock"), "true"]));
                Self::text(!self.build_busy, "")
            }
            _ => panic!("unexpected program {program} {a:?}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if path == self.p("bin/ap") {
            return match self.file {
                Some(_) => Ok(self.bytes.clone()),
                None => Err(std::io::ErrorKind::NotFound.into()),
            };
        }
        if let Some(rel) = path.strip_prefix(&format!("{}/", self.p("agents"))) {
            return match &self.tree {
                Some(t) => Ok(content(&self.label(t), rel)),
                None => Ok(b"half swapped".to_vec()),
            };
        }
        if !std::fs::metadata(path)?.is_file() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        std::fs::read(path)
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        self.lock_inherited.push(inherited(&self.lock));
        let env_map: serde_json::Map<String, Value> =
            env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        self.calls.push((
            argv[0].clone(),
            argv[1..].to_vec(),
            json!({"cwd": cwd, "env": env_map}),
        ));
        assert_eq!(argv[0], "bash");
        let script = if argv[1].ends_with("/scripts/install-ap.sh") {
            "ap"
        } else {
            assert!(argv[1].ends_with("/scripts/install-content.sh"));
            "content"
        };
        let name = if argv[2] == "rollback" {
            format!("{script}-rollback")
        } else {
            script.to_owned()
        };
        let inst = self.inst[name.as_str()].clone();
        if name == "ap" && inst.file.is_some() && self.file.is_some() {
            // install-ap.sh keeps the previous file as ap.before-<first 7 hex of its sha256>.
            let kept = self.p(&format!("backup/ap.before-{}", &sha(&self.bytes)[..7]));
            std::fs::write(kept, &self.bytes).unwrap();
        }
        if name == "content" && inst.exit != 2 {
            // The tarball it prints, or the fixture's: the tree it replaces.
            let printed = [(&inst.out, "backup="), (&inst.err, "kept copy ")]
                .into_iter()
                .find_map(|(text, marker)| Some(text.split_once(marker)?.1.trim().to_owned()));
            let before = self.tree.as_deref().map(|t| self.label(t));
            let path = printed.unwrap_or_else(|| self.tarball());
            std::fs::write(path, tar(before.as_deref())).unwrap();
        }
        if let Some(f) = inst.file {
            self.bytes = binary(f.as_deref().unwrap_or("none"));
            self.file = f;
        }
        if let Some(t) = inst.tree {
            self.tree = t;
        }
        let [stdin, stdout, stderr] = files;
        Command::new("sh")
            .args([
                "-c",
                "printf '%s' \"$1\"; printf '%s' \"$2\" >&2; exit \"$3\"",
                "sh",
            ])
            .args([&inst.out, &inst.err, &inst.exit.to_string()])
            .env_clear()
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("AP_INSTALL_DIR".into(), self.p("bin")),
            ("AP_BACKUP_DIR".into(), self.p("backup")),
            ("AP_CONTENT_DST".into(), self.p("agents")),
            ("AP_CONTENT_BACKUP_DIR".into(), self.p("content-backup")),
            ("AP_DELIVER_LOCK".into(), self.lock.clone()),
        ]
        .into_iter()
        .chain(self.invocation.clone().map(|v| ("INVOCATION_ID".into(), v)))
        .collect()
    }
}

fn end(v: &Value) -> (&str, &str) {
    (
        v["end"].as_str().unwrap_or("?"),
        v["code"].as_str().unwrap_or(""),
    )
}
/// The call refused with `code`, exit 2, called no installer and applied no `start_install`.
fn refused(w: &W, v: &Value, exit: u8, code: &str) {
    assert_eq!((end(v), exit), (("refused", code), 2), "{v}");
    assert!(w.launched().is_empty(), "an installer ran: {v}");
    assert!(
        w.installs.iter().all(|i| i["by"] != "session-1"),
        "an attempt was applied: {v}"
    );
    assert_eq!(v["installers"], json!([]));
    assert_eq!(w.last().as_ref(), Some(v));
}
fn rows(v: &Value) -> Vec<(String, Value, Value)> {
    v["installers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_owned(),
                r["exit"].clone(),
                r["code"].clone(),
            )
        })
        .collect()
}
fn row(name: &str, exit: i64, code: Option<&str>) -> (String, Value, Value) {
    (name.into(), json!(exit), json!(code))
}
fn kept(w: &W) -> String {
    w.p(&format!("backup/ap.before-{}", &sha(&binary(&c(0)))[..7]))
}

// ---- D1 no effect without a newly applied attempt ------------------------------------------------

#[test]
fn d1_no_installer_when_start_is_refused() {
    let mut w = W::new();
    w.start = Some(Start::Json(
        json!({"status": "refused", "code": "submission_criteria", "failed": ["review_not_accepting"]}),
    ));
    let (v, exit) = w.run();
    refused(&w, &v, exit, "store_refused");
    assert_eq!(v["detail"], json!({"store": "review_not_accepting"}));
    assert_eq!(w.applies("start_install").len(), 1);
}

#[test]
fn d1_no_installer_when_start_is_already_applied() {
    let mut w = W::new();
    let id = oid("install", "act-earlier");
    w.start = Some(Start::Json(
        json!({"status": "ok", "result": {"outcome": "already_applied", "edits": [{"change": "initial", "id": id}]}}),
    ));
    let (v, exit) = w.run();
    refused(&w, &v, exit, "store_refused");
    assert_eq!(v["detail"], json!({"store": "already_applied"}));
    assert_eq!(v["install"], Value::Null);
}

#[test]
fn d1_no_installer_when_the_store_fails() {
    let mut w = W::new();
    w.start = Some(Start::Fail);
    let (v, exit) = w.run();
    refused(&w, &v, exit, "store_unreadable");
    assert_eq!(v["detail"], json!({"store": null}));
    // An answer that is not JSON is a failure too.
    let mut w = W::new();
    w.start = Some(Start::Json(json!("not an object")));
    let (v, exit) = w.run();
    refused(&w, &v, exit, "store_unreadable");
}

// ---- D3 one commit -----------------------------------------------------------------------------

#[test]
fn d3_installed_needs_both_reads() {
    let mut w = W::new();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        (v["outcome"].clone(), v["to"].clone()),
        (json!("installed"), json!(c(2)))
    );
    assert_eq!(rows(&v), [row("ap", 0, None), row("content", 0, None)]);
    // Both installers exit 0 and the tree is still at the installed commit: not installed. The
    // file is not at I, so it is rolled back; the tree is at I, so it is not.
    let mut w = W::new();
    w.inst.get_mut("content").unwrap().tree = None;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("failed_restored", ""), 3), "{v}");
    assert_eq!(w.launched(), ["ap", "content", "ap-rollback"]);
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        (close["outcome"].clone(), close["to"].clone()),
        (json!("failed_restored"), json!(c(0)))
    );
    // The file stays where it was and the tree moves: not installed either.
    let mut w = W::new();
    w.inst.get_mut("ap").unwrap().file = None;
    let (v, _) = w.run();
    assert_ne!(v["outcome"], "installed", "{v}");
    assert_eq!(w.launched(), ["ap", "content", "content-rollback"]);
}

#[test]
fn d3_content_failure_rolls_the_binary_back() {
    let mut w = W::new();
    let tarball = w.tarball();
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (None, 1, String::new());
    content.err =
        format!("install_failed_restored@/x: the previous files are back; kept copy {tarball}\n");
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "content_failed_restored"), 3),
        "{v}"
    );
    assert_eq!(
        rows(&v),
        [
            row("ap", 0, None),
            row("content", 1, Some("content_failed_restored")),
            row("ap-rollback", 0, None)
        ]
    );
    let rollback = w
        .calls
        .iter()
        .find(|(p, a, _)| p == "bash" && a[1] == "rollback")
        .unwrap();
    assert_eq!(rollback.1[2], kept(&w));
    assert_eq!(w.file.as_deref(), Some(c(0).as_str()));
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(close["code"], "content_failed_restored");
    assert_eq!(
        (
            close["rollback_binary"].clone(),
            close["rollback_content"].clone()
        ),
        (json!(kept(&w)), json!(tarball))
    );
}

#[test]
fn d3_rollback_that_fails_ends_failed() {
    let mut w = W::new();
    let tarball = w.tarball();
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (Some(None), 1, String::new());
    content.err =
        format!("restore_failed@/x/AGENTS.md\ninstall_failed_restored@/x: kept copy {tarball}\n");
    let back = w.inst.get_mut("content-rollback").unwrap();
    (back.tree, back.exit, back.err) = (None, 2, "refuse: tarball missing\n".into());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed", "content_failed_restored"), 4),
        "{v}"
    );
    assert_eq!(
        rows(&v),
        [
            row("ap", 0, None),
            row("content", 1, Some("content_failed_restored")),
            row("ap-rollback", 0, None),
            row("content-rollback", 2, Some("content_refused")),
        ]
    );
    let rollback = w.calls.iter().rfind(|(p, _, _)| p == "bash").unwrap();
    assert_eq!(rollback.1[1..], [String::from("rollback"), tarball]);
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        (close["outcome"].clone(), close["to"].clone()),
        (json!("failed"), json!("neither"))
    );
}

#[test]
fn d3_install_busy_is_not_started() {
    let mut w = W::new();
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.file, ap.exit, ap.err) = (None, 1, "install_busy@install-ap.sh:40\n".into());
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("not_started", "install_busy"), 3), "{v}");
    assert_eq!(rows(&v), [row("ap", 1, Some("install_busy"))]);
    // Another code of the first installer with the pair where it was: failed_restored.
    let mut w = W::new();
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.file, ap.exit, ap.err) = (
        None,
        1,
        "compiling\nbuild_failed@install-ap.sh:151\n".into(),
    );
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "build_failed"), 3),
        "{v}"
    );
}

#[test]
fn d3_no_printed_tarball_no_content_rollback() {
    let mut w = W::new();
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (Some(None), 3, String::new());
    content.err = "tar: write error\n".into();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("failed", "content_exit_3"), 4), "{v}");
    assert_eq!(w.launched(), ["ap", "content", "ap-rollback"]);
    assert_eq!(v["rollback_content"], Value::Null);
    assert_eq!(v["to"], "neither");
}

/// Tip and `from` have the same `content/SHA256SUMS`, so the tree passes both manifests; the
/// installers' lock stops the first installer and the file stays at `from`: the pair is at `from`.
#[test]
fn d3_same_content_install_busy_is_not_started() {
    let mut w = W::new();
    w.same_content = true;
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.file, ap.exit, ap.err) = (None, 1, "install_busy@install-ap.sh:40\n".into());
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("not_started", "install_busy"), 3), "{v}");
    assert_eq!(
        (v["outcome"].clone(), v["to"].clone()),
        (json!("not_started"), json!(c(0)))
    );
    assert_eq!(w.launched(), ["ap"]);
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        (close["outcome"].clone(), close["to"].clone()),
        (json!("not_started"), json!(c(0)))
    );
}

#[test]
fn d3_same_content_failed_build_is_failed_restored() {
    let mut w = W::new();
    w.same_content = true;
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.file, ap.exit, ap.err) = (None, 1, "build_failed@install-ap.sh:151\n".into());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "build_failed"), 3),
        "{v}"
    );
    assert_eq!(v["to"], c(0));
    assert_eq!(w.launched(), ["ap"]);
}

/// The file is at the tip and the content installer refuses or fails and restores: the tree it
/// leaves passes the tip's manifest too, so the pair is at the tip.
#[test]
fn d3_same_content_content_installer_fails_is_installed() {
    for (exit_code, code) in [(1, "content_failed_restored"), (2, "content_refused")] {
        let mut w = W::new();
        w.same_content = true;
        let tarball = w.tarball();
        let content = w.inst.get_mut("content").unwrap();
        (content.tree, content.exit, content.out) = (None, exit_code, String::new());
        content.err = format!("install_failed_restored@/x: kept copy {tarball}\n");
        let (v, exit) = w.run();
        assert_eq!((end(&v), exit), (("installed", code), 0), "{v}");
        assert_eq!(
            (v["outcome"].clone(), v["to"].clone()),
            (json!("installed"), json!(c(2)))
        );
        assert_eq!(w.launched(), ["ap", "content"]);
        assert_eq!(w.applies("close_install")[0]["parameters"]["code"], code);
    }
}

// ---- D4 accepted -------------------------------------------------------------------------------

fn not_accepted(w: &mut W, commit: &str, why: &str) {
    let (v, exit) = w.run();
    refused(w, &v, exit, "not_accepted");
    assert_eq!(v["detail"], json!({"commit": commit, "why": why}));
}

#[test]
fn d4_not_on_chain() {
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(other()), Some(other()), binary(&other()));
    not_accepted(&mut w, &other(), "not_on_chain");
}

#[test]
fn d4_not_a_merge() {
    let mut w = W::new();
    w.chain[1].parents.truncate(1);
    not_accepted(&mut w, &c(1), "not_a_merge");
}

#[test]
fn d4_tree_differs() {
    let mut w = W::new();
    w.chain[1].tree = tree(77);
    not_accepted(&mut w, &c(1), "tree_differs");
}

#[test]
fn d4_no_accepting_review() {
    let mut w = W::new();
    // Left: the running review, the repair and an accepting review of another repository.
    w.reviews
        .retain(|r| !(r["head"] == h(1) && r["verdict"] == "accept" && r["repository"] == "o/r"));
    w.review(1, "x/other", "accept", "closed");
    not_accepted(&mut w, &c(1), "no_accepting_review");
}

#[test]
fn d4_revoked_review() {
    let mut w = W::new();
    let id = w.accepting(2);
    w.reviews.iter_mut().find(|r| r["id"] == id).unwrap()["status"] = json!("revoked");
    not_accepted(&mut w, &c(2), "no_accepting_review");
}

#[test]
fn d4_report_missing_at_tip() {
    // A report that is gone below the tip does not matter.
    let mut w = W::new();
    let first = w
        .reviews
        .iter()
        .find(|r| r["id"] == w.accepting(1))
        .unwrap()["report"]
        .clone();
    std::fs::remove_file(first.as_str().unwrap()).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let mut w = W::new();
    let tip = w
        .reviews
        .iter()
        .find(|r| r["id"] == w.accepting(2))
        .unwrap()["report"]
        .clone();
    std::fs::remove_file(tip.as_str().unwrap()).unwrap();
    not_accepted(&mut w, &c(2), "report_missing");
}

#[test]
fn d4_report_differs_at_tip() {
    let mut w = W::new();
    let tip = w
        .reviews
        .iter()
        .find(|r| r["id"] == w.accepting(2))
        .unwrap()["report"]
        .clone();
    std::fs::write(tip.as_str().unwrap(), "Verdict: accept\nchanged\n").unwrap();
    not_accepted(&mut w, &c(2), "report_differs");
}

#[test]
fn d4_past_exempts() {
    let mut w = W::new();
    w.chain[1].parents.truncate(1);
    w.reviews.retain(|r| r["head"] != h(1));
    let mut r = w.request();
    r["past"] = json!([c(1)]);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let start = &w.applies("start_install")[0]["parameters"];
    assert_eq!(start["past"], c(1));
    assert_eq!(start["reviews"], json!([w.accepting(2)]));
}

#[test]
fn d4_tip_in_past_is_checked() {
    let mut w = W::new();
    w.reviews.retain(|r| r["head"] != h(2));
    let mut r = w.request();
    r["past"] = json!([c(2)]);
    let (v, exit) = w.run_with(&r);
    refused(&w, &v, exit, "not_accepted");
    assert_eq!(
        v["detail"],
        json!({"commit": c(2), "why": "no_accepting_review"})
    );
}

#[test]
fn d4_past_not_on_chain() {
    let mut w = W::new();
    let mut r = w.request();
    r["past"] = json!([c(1), other()]);
    let (v, exit) = w.run_with(&r);
    refused(&w, &v, exit, "past_not_on_chain");
    assert_eq!(v["detail"], json!({"commit": other()}));
    // The installed commit itself is not one of the commits after it.
    let mut w = W::new();
    let mut r = w.request();
    r["past"] = json!([c(0)]);
    let (v, exit) = w.run_with(&r);
    refused(&w, &v, exit, "past_not_on_chain");
}

#[test]
fn d4_search_names_required_properties_only() {
    let mut w = W::new();
    w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let searches = w.searches();
    assert!(searches.len() >= 4, "{searches:?}");
    for s in &searches {
        let allowed: &[&str] = match s["object_type"].as_str().unwrap() {
            "review" => &["kind", "head", "status"],
            "install" => &["target", "status", "commit", "kind"],
            other => panic!("search of {other}"),
        };
        for f in s["where"]["all"].as_array().unwrap() {
            assert!(allowed.contains(&f["eq"][0].as_str().unwrap()), "{s}");
        }
    }
    // The accepting reviews are the ones of this repository, compared on the answer.
    assert_eq!(
        w.applies("start_install")[0]["parameters"]["reviews"],
        json!([w.accepting(1), w.accepting(2)])
    );
}

// ---- D5 a refusal and `current` apply nothing ---------------------------------------------------

#[test]
fn d5_refusals_apply_nothing() {
    type Setup = fn(&mut W) -> Value;
    let cases: Vec<(&str, Setup)> = vec![
        ("attempt_running", |w| {
            // A dead rollback at neither of its commits is not repaired.
            w.dead_attempt(&c(2), &c(0), "0");
            w.installs[0]["kind"] = json!("rollback");
            (w.file, w.tree) = (Some(c(1)), Some(c(1)));
            w.request()
        }),
        ("fetch_failed", |w| {
            w.fetch_fails = true;
            w.request()
        }),
        ("pair_differs", |w| {
            w.tree = Some(c(2));
            w.request()
        }),
        ("engine_differs", |w| {
            w.pin = "a".repeat(40);
            w.request()
        }),
        ("schema_differs", |w| {
            w.tip_schema = 9;
            w.request()
        }),
        ("not_accepted", |w| {
            w.chain[2].tree = tree(5);
            w.request()
        }),
        ("past_not_on_chain", |w| {
            let mut r = w.request();
            r["past"] = json!([other()]);
            r
        }),
        ("disk_low", |w| {
            w.free = 100;
            w.request()
        }),
        ("build_busy", |w| {
            w.build_busy = true;
            w.request()
        }),
        ("store_refused", |w| {
            w.start = Some(Start::Json(
                json!({"status": "refused", "code": "submission_criteria", "failed": ["install_running"]}),
            ));
            w.request()
        }),
        ("store_unreadable", |w| {
            w.start = Some(Start::Fail);
            w.request()
        }),
    ];
    for (code, setup) in cases {
        let mut w = W::new();
        let r = setup(&mut w);
        let (v, exit) = w.run_with(&r);
        refused(&w, &v, exit, code);
        assert!(w.applies("close_install").is_empty(), "{code}: {v}");
    }
    // At the tip already: `current`, exit 0, no index, no start, no installer.
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert!(w.launched().is_empty());
    assert!(
        w.calls
            .iter()
            .all(|(p, a, _)| !(p == "ont" && (a[0] == "action" || a[0] == "index")))
    );
    assert_eq!(v["outcome"], Value::Null);
}

#[test]
fn d5_disk_low() {
    let mut w = W::new();
    w.free = 10_239;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "disk_low");
    let mut w = W::new();
    w.free = 10_240;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
}

#[test]
fn d5_build_busy() {
    let mut w = W::new();
    w.build_busy = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "build_busy");
    assert!(
        w.calls
            .iter()
            .all(|(p, a, _)| !(p == "ont" && a[0] == "index"))
    );
}

#[test]
fn d5_engine_differs() {
    let mut w = W::new();
    w.ont = "b".repeat(40);
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_differs");
    assert_eq!(
        v["detail"],
        json!({"tip": "e".repeat(40), "installed": "b".repeat(40)})
    );
}

#[test]
fn d5_schema_differs() {
    let mut w = W::new();
    w.tip_schema = 9;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "schema_differs");
    assert_eq!(v["detail"], json!({"tip": 9, "installed": 8}));
}

#[test]
fn d5_pair_differs() {
    // The file at the tip and the tree at the installed commit; the file at I and the tree at
    // none; no file at all.
    for (file, tree) in [
        (Some(c(2)), Some(c(0))),
        (Some(c(0)), None),
        (None, Some(c(0))),
    ] {
        let mut w = W::new();
        (w.file, w.tree) = (file, tree);
        let (v, exit) = w.run();
        refused(&w, &v, exit, "pair_differs");
        assert_eq!(v["tip"], c(2));
    }
}

// ---- D6 a dead call's attempt is closed only by a read ------------------------------------------

#[test]
fn d6_dead_call_at_from_is_closed_not_started() {
    let mut w = W::new();
    let id = w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(
        closes[0]["parameters"],
        json!({"install": id, "outcome": "not_started", "to": c(0), "code": "dead_call"})
    );
    assert_eq!(closes[0]["request_id"], format!("ap.{}.1.close", c(2)));
    assert_eq!(
        v["closed"],
        json!({"install": id, "outcome": "not_started"})
    );
    // The call went on: the next attempt of this commit is 2.
    assert_eq!(
        (
            v["attempt"].clone(),
            w.applies("start_install")[0]["request_id"].clone()
        ),
        (json!(2), json!(format!("ap.{}.2", c(2))))
    );
}

#[test]
fn d6_dead_call_at_commit_is_closed_installed() {
    let mut w = W::new();
    let from_sha = sha(&binary(&c(0)));
    let id = w.dead_attempt(&c(2), &c(0), &from_sha);
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    // The file handle counts only when the kept copy's whole digest is `from_sha256`.
    std::fs::write(kept(&w), binary(&c(0))).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        *close,
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
    assert_eq!(v["closed"], json!({"install": id, "outcome": "installed"}));
    assert!(w.launched().is_empty());
}

#[test]
fn d6_dead_call_at_commit_records_the_oldest_tarball() {
    let mut w = W::new();
    let id = w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    // Older than `started_at` (10:00:00); of the start's second but of another tree; then the oldest
    // that holds the tree of `from`, then a later one; and files that are not tarballs.
    tarball_at(&w, "095959Z-1", Some(0));
    tarball_at(&w, "100000Z-2", Some(1));
    let oldest = tarball_at(&w, "100500Z-3", Some(0));
    tarball_at(&w, "100600Z-4", Some(0));
    for name in [
        "agents-before-20261114T100000Z-2.tar.sha256",
        "agents-before-20261114T093000Z-4.tar.paths",
    ] {
        std::fs::write(w.p(&format!("content-backup/{name}")), b"x").unwrap();
    }
    let (v, _) = w.run();
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        *close,
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_content": oldest}),
        "{v}"
    );
}

// ---- D6 the repair of a dead install call ------------------------------------------------------

/// A dead call's attempt of c(2) from c(0); the kept copy of c(0)'s file is on disk.
fn dead_forward(w: &mut W, file: Option<u32>, tree: Option<u32>) -> Value {
    let id = w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
    std::fs::write(kept(w), binary(&c(0))).unwrap();
    (w.file, w.tree) = (file.map(c), tree.map(c));
    w.bytes = binary(w.file.as_deref().unwrap_or("none"));
    id
}
/// A tarball stamped `stamp` that holds the tree of `c(n)` (None: a half-swapped tree).
fn tarball_at(w: &W, stamp: &str, n: Option<u32>) -> String {
    let path = w.p(&format!(
        "content-backup/agents-before-20261114T{stamp}.tar"
    ));
    std::fs::write(&path, tar(n.map(c).as_deref())).unwrap();
    path
}

#[test]
fn d6_repair_forward_installs_the_missing_part() {
    // The file at the commit and the tree at `from`: only the content installer.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(0));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(w.launched(), ["content"]);
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w),
            "rollback_content": w.tarball()})
    );
    assert_eq!(v["closed"], json!({"install": id, "outcome": "installed"}));
    // The file at `from` and the tree at the commit: only the file installer.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(0), Some(2));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(w.launched(), ["ap"]);
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
    // Both at neither: both installers, for the attempt's commit.
    let mut w = W::new();
    dead_forward(&mut w, None, None);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(w.launched(), ["ap", "content"]);
    let words: Vec<_> = w
        .calls
        .iter()
        .filter(|(p, _, _)| p == "bash")
        .map(|(_, a, _)| a[1..].to_vec())
        .collect();
    assert_eq!(
        words,
        [args(&["install", &c(2)]), args(&["install", &c(2)])]
    );
}

#[test]
fn d6_repair_falls_back_to_the_rollback() {
    // The clone's origin/main is not the attempt's commit: no installer, the command's rollback.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    let tarball = tarball_at(&w, "100003Z-9", Some(0));
    w.origin_once = Some(c(1));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        w.launched(),
        ["ap-rollback", "content-rollback", "ap", "content"]
    );
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "failed_restored", "to": c(0), "rollback_binary": kept(&w),
            "rollback_content": tarball})
    );
    assert_eq!(
        v["closed"],
        json!({"install": id, "outcome": "failed_restored"})
    );
    // The installer called again leaves the tree off the commit: the same rollback.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    let tarball = tarball_at(&w, "100003Z-9", Some(0));
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (None, 2, String::new());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "content_refused"), 3),
        "{v}"
    );
    assert_eq!(
        w.launched()[..3],
        ["content", "ap-rollback", "content-rollback"]
    );
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        *close,
        json!({"install": id, "outcome": "failed_restored", "to": c(0), "rollback_binary": kept(&w),
            "rollback_content": tarball, "code": "content_refused"})
    );
}

#[test]
fn d6_repair_uses_the_oldest_tarball() {
    // The dead call's tarball (10:00:05) is the handle: not a tarball of another tree in the start's
    // second, not the one the second install printed (a tarball of a half-swapped tree).
    for fallback in [false, true] {
        let mut w = W::new();
        let id = dead_forward(&mut w, Some(2), None);
        // A tarball of the start's second that holds another tree is passed over.
        tarball_at(&w, "100000Z-1", Some(1));
        let first = tarball_at(&w, "100005Z-2", Some(0));
        let second = w.p("content-backup/agents-before-20261114T100009Z-3.tar");
        w.inst.get_mut("content").unwrap().out = format!("installed backup={second}\n");
        if fallback {
            w.origin_once = Some(c(1));
        }
        let (v, _) = w.run();
        let close = &w.applies("close_install")[0]["parameters"];
        assert_eq!(close["install"], id);
        assert_eq!(close["rollback_content"], json!(first), "{v}");
        if fallback {
            let rollback = w
                .calls
                .iter()
                .find(|(p, a, _)| p == "bash" && a[0].ends_with("install-content.sh"))
                .unwrap();
            assert_eq!(rollback.1[1..], [String::from("rollback"), first.clone()]);
        }
    }
}

#[test]
fn d6_handle_is_the_tarball_that_holds_the_tree_of_from() {
    // Two tarballs of the second of `started_at`: the one of another tree is passed over, the one
    // that holds the tree of `from` is the handle.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    tarball_at(&w, "100000Z-1", Some(1));
    let held = tarball_at(&w, "100000Z-2", Some(0));
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w),
            "rollback_content": held})
    );
    // None that passes (another tree, a half-swapped tree, no archive at all): no handle.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    tarball_at(&w, "100000Z-1", Some(1));
    tarball_at(&w, "100001Z-2", None);
    std::fs::write(
        w.p("content-backup/agents-before-20261114T100002Z-3.tar"),
        b"tar",
    )
    .unwrap();
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
}

#[test]
fn d6_a_close_after_an_unrecorded_repair_records_no_partial_tree() {
    // The repair keeps a tarball of the half-swapped tree, installs the tree, and its close is not
    // applied: `unrecorded`. The next call closes the attempt by the read at its commit; the
    // tarball of the partial tree is never its handle.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    w.close_fails = true;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("unrecorded", ""), 5), "{v}");
    assert!(std::fs::exists(w.tarball()).unwrap());
    assert_eq!(v["rollback_content"], Value::Null);
    w.close_fails = false;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(
        closes.last().unwrap()["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
}

#[test]
fn d6_repair_never_records_its_own_tarball_of_a_partial_tree() {
    // No tarball of the dead call, and the tree at no manifest: the repair's content install keeps a
    // tarball of the half-swapped tree, which is never the handle.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(w.launched(), ["content"]);
    assert!(std::fs::exists(w.tarball()).unwrap());
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
    // The same with the fallback: no content rollback with that tarball.
    let mut w = W::new();
    dead_forward(&mut w, Some(2), None);
    let content = w.inst.get_mut("content").unwrap();
    content.tree = None;
    let (v, _) = w.run();
    assert_eq!(w.launched()[..2], ["content", "ap-rollback"], "{v}");
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        (close["outcome"].clone(), close.get("rollback_content")),
        (json!("failed"), None),
        "{v}"
    );
    // The tree at `from` when the repair began: its own printed tarball holds that tree.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(0));
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w),
            "rollback_content": w.tarball()})
    );
}

#[test]
fn d3_first_installer_fails_after_it_replaced_the_file() {
    // Different manifests: the file is rolled back; the pair is at `from`.
    let mut w = W::new();
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.exit, ap.err) = (1, "restore_failed@install-ap.sh:250\n".into());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "restore_failed"), 3),
        "{v}"
    );
    assert_eq!(
        rows(&v),
        [
            row("ap", 1, Some("restore_failed")),
            row("ap-rollback", 0, None)
        ]
    );
    assert_eq!(w.file.as_deref(), Some(c(0).as_str()));
    // Identical manifests: the pair is at the tip, the read decides; the record keeps the code.
    let mut w = W::new();
    w.same_content = true;
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.exit, ap.err) = (1, "restore_failed@install-ap.sh:250\n".into());
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", "restore_failed"), 0), "{v}");
    assert_eq!(rows(&v), [row("ap", 1, Some("restore_failed"))]);
    assert_eq!(
        w.applies("close_install")[0]["parameters"]["code"],
        "restore_failed"
    );
}

#[test]
fn d6_repair_that_fails_ends_the_call_failed() {
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    let tarball = tarball_at(&w, "100003Z-9", Some(0));
    w.origin_once = Some(c(1));
    let back = w.inst.get_mut("ap-rollback").unwrap();
    (back.file, back.exit, back.err) = (None, 1, "rollback_move_failed@install-ap.sh:9\n".into());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed", "rollback_move_failed"), 4),
        "{v}"
    );
    assert_eq!(v["closed"], json!({"install": id, "outcome": "failed"}));
    assert_eq!(
        (v["outcome"].clone(), v["to"].clone(), v["install"].clone()),
        (json!("failed"), json!("neither"), id.clone())
    );
    assert_eq!(
        (v["from"].clone(), v["tip"].clone()),
        (json!(c(0)), Value::Null)
    );
    assert_eq!(
        (v["rollback_binary"].clone(), v["rollback_content"].clone()),
        (json!(kept(&w)), json!(tarball))
    );
    assert_eq!(
        w.applies("close_install")[0]["parameters"]["outcome"],
        "failed"
    );
    // The call ended there: no fetch, no attempt.
    assert!(
        !w.calls
            .iter()
            .any(|(p, a, _)| p == "git" && a[2] == "fetch")
    );
    assert!(w.applies("start_install").is_empty());
    assert_eq!(w.last(), Some(v));
}

#[test]
fn d6_dead_rollback_at_neither_refuses() {
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), None);
    (
        w.installs[0]["kind"],
        w.installs[0]["commit"],
        w.installs[0]["from"],
    ) = (json!("rollback"), json!(c(0)), json!(c(2)));
    let (v, exit) = w.run();
    refused(&w, &v, exit, "attempt_running");
    assert_eq!(v["detail"], json!({"install": id}));
    assert!(w.applies("close_install").is_empty());
    // A dead rollback at its `from` is closed by the read, under its own request id.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    (
        w.installs[0]["kind"],
        w.installs[0]["commit"],
        w.installs[0]["from"],
    ) = (json!("rollback"), json!(c(0)), json!(c(2)));
    let (v, _) = w.run();
    let close = &w.applies("close_install")[0];
    assert_eq!(close["request_id"], format!("ap.{}.rb1.close", c(0)));
    assert_eq!(
        close["parameters"],
        json!({"install": id, "outcome": "not_started", "to": c(2), "code": "dead_call"}),
        "{v}"
    );
}

#[test]
fn d6_clone_that_cannot_answer_refuses() {
    for (file, tree) in [(Some(1), Some(1)), (Some(2), Some(0)), (None, None)] {
        let mut w = W::new();
        let id = dead_forward(&mut w, file, tree);
        w.lost = true;
        let (v, exit) = w.run();
        refused(&w, &v, exit, "attempt_running");
        assert_eq!(v["detail"], json!({"install": id}));
        assert!(w.applies("close_install").is_empty());
        assert_eq!(w.installs[0]["status"], "running");
    }
}

// ---- D12 a Lead's rollback ---------------------------------------------------------------------

/// The pair at c(2), installed by attempt A (forward, from c(0)) that kept both handles.
fn installed_by_a(w: &mut W) -> Value {
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let a = w.closed("forward", &c(2), &c(0), "installed", "session-0");
    let (copy, tarball) = (kept(w), w.tarball());
    std::fs::write(&copy, binary(&c(0))).unwrap();
    std::fs::write(&tarball, tar(Some(&c(0)))).unwrap();
    let row = w.installs.last_mut().unwrap();
    (row["rollback_binary"], row["rollback_content"]) = (json!(copy), json!(tarball));
    a
}
fn rolls(w: &mut W) -> (Value, u8) {
    let r = w.rollback();
    w.run_with(&r)
}

#[test]
fn d12_rollback_exact_calls() {
    let mut w = W::new();
    let a = installed_by_a(&mut w);
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let (back, here) = (c(0), c(2));
    let clone = w.p("state/r");
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let search = |all: Vec<Value>| {
        (
            "ont".to_owned(),
            args(&["search"]),
            json!({"object_type": "install", "where": {"all": all}, "limit": 200}),
        )
    };
    let show = |commit: &str| {
        (
            "git".to_owned(),
            args(&[
                "-C",
                &clone,
                "show",
                &format!("{commit}:content/SHA256SUMS"),
            ]),
            Value::Null,
        )
    };
    let version = (w.p("bin/ap"), args(&["--version"]), Value::Null);
    let apply = |body: Value| {
        (
            "ont".to_owned(),
            args(&["action", "apply", "--client", "ap-deliver"]),
            body,
        )
    };
    let index = (
        "ont".to_owned(),
        args(&["index", "--source", "installed_binaries"]),
        Value::Null,
    );
    let mut env: serde_json::Map<String, Value> = w
        .environment()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    env.insert("CARGO_TARGET_DIR".into(), json!(w.p("state/target")));
    let launch = |script: &str, handle: &str| {
        (
            "bash".to_owned(),
            args(&[&format!("{clone}/scripts/{script}"), "rollback", handle]),
            json!({"cwd": clone, "env": env}),
        )
    };
    let id = oid("install", &format!("act-ap.{back}.rb1"));
    let want = vec![
        search(vec![eq("target", target()), eq("status", json!("running"))]),
        version.clone(),
        show(&here),
        search(vec![
            eq("target", target()),
            eq("commit", json!(here)),
            eq("kind", json!("forward")),
        ]),
        // The recorded tarball is checked against the manifest of the commit it rolls back to; the
        // read after the installers uses the same manifest (one read per commit and call).
        show(&back),
        index.clone(),
        search(vec![
            eq("target", target()),
            eq("commit", json!(back)),
            eq("kind", json!("rollback")),
        ]),
        apply(
            json!({"action_type": "start_install", "request_id": format!("ap.{back}.rb1"), "agent": "ap-deliver",
            "parameters": {"target": target(), "kind": "rollback", "repository": "o/r", "commit": back, "attempt": 1,
                "from": here, "from_sha256": sha(&binary(&here)), "rolls_back": a, "by": "session-1",
                "reason": "after a merge"}}),
        ),
        launch("install-content.sh", &w.tarball()),
        launch("install-ap.sh", &kept(&w)),
        version,
        apply(
            json!({"action_type": "close_install", "request_id": format!("ap.{back}.rb1.close"), "agent": "ap-deliver",
            "parameters": {"install": id, "outcome": "installed", "to": back}}),
        ),
        index,
    ];
    assert_eq!(w.calls.len(), want.len(), "{:#?}", w.calls);
    for (n, (got, want)) in w.calls.iter().zip(&want).enumerate() {
        assert_eq!(got, want, "call {n}");
    }
    let rec = json!({"by": "session-1", "tip": null, "from": here, "end": "installed", "outcome": "installed",
        "code": null, "install": id, "attempt": 1, "to": back, "rollback_binary": null, "rollback_content": null,
        "index": {"before": "ok", "after": "ok"}, "closed": null});
    for (k, value) in rec.as_object().unwrap() {
        assert_eq!(&v[k], value, "{k}");
    }
    assert_eq!(
        rows(&v),
        [
            row("content-rollback", 0, None),
            row("ap-rollback", 0, None)
        ]
    );
    assert_eq!(members(&v), sorted());
}

#[test]
fn d12_nothing_to_roll_back() {
    let setups: [fn(&mut W); 4] = [
        // No attempt at all.
        |_| (),
        // A forward attempt of the commit that did not end installed.
        |w| {
            w.closed("forward", &c(2), &c(0), "failed_restored", "s");
        },
        // An installed forward attempt of another commit.
        |w| {
            w.closed("forward", &c(1), &c(0), "installed", "s");
        },
        // An installed attempt of the commit of kind rollback.
        |w| {
            w.closed("rollback", &c(2), &c(1), "installed", "s");
        },
    ];
    for setup in setups {
        let mut w = W::new();
        (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
        std::fs::write(kept(&w), binary(&c(0))).unwrap();
        setup(&mut w);
        let (v, exit) = rolls(&mut w);
        refused(&w, &v, exit, "nothing_to_roll_back");
        assert!(w.applies("start_install").is_empty());
        assert_eq!(v["detail"], Value::Null);
    }
    // The pair at no single commit.
    let mut w = W::new();
    installed_by_a(&mut w);
    w.tree = Some(c(1));
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "pair_differs");
}

#[test]
fn d12_kept_copy_digest_differs() {
    let mut w = W::new();
    installed_by_a(&mut w);
    std::fs::write(kept(&w), b"other bytes").unwrap();
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "nothing_to_roll_back");
    let mut w = W::new();
    installed_by_a(&mut w);
    std::fs::remove_file(kept(&w)).unwrap();
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "nothing_to_roll_back");
}

#[test]
fn d12_no_content_handle_and_content_differs() {
    let mut w = W::new();
    installed_by_a(&mut w);
    w.installs[0]
        .as_object_mut()
        .unwrap()
        .remove("rollback_content");
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "nothing_to_roll_back");
    // The same manifest at both commits: the file alone is rolled back.
    let mut w = W::new();
    w.same_content = true;
    installed_by_a(&mut w);
    w.installs[0]
        .as_object_mut()
        .unwrap()
        .remove("rollback_content");
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(w.launched(), ["ap-rollback"]);
}

#[test]
fn d12_recorded_tarball_that_does_not_hold_the_tree_counts_as_absent() {
    // The recorded tarball holds another tree: absent; the manifests differ, so nothing to roll back.
    for other in [Some(c(1)), None] {
        let mut w = W::new();
        installed_by_a(&mut w);
        std::fs::write(w.tarball(), tar(other.as_deref())).unwrap();
        let (v, exit) = rolls(&mut w);
        refused(&w, &v, exit, "nothing_to_roll_back");
    }
    // The same manifest at both commits: the file alone, never a content rollback with that tarball.
    let mut w = W::new();
    w.same_content = true;
    installed_by_a(&mut w);
    std::fs::write(w.tarball(), tar(None)).unwrap();
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(w.launched(), ["ap-rollback"]);
}

#[test]
fn d12_content_refusal_is_not_started() {
    let mut w = W::new();
    installed_by_a(&mut w);
    let back = w.inst.get_mut("content-rollback").unwrap();
    (back.tree, back.exit, back.err) = (None, 2, "refuse: tarball missing\n".into());
    let (v, exit) = rolls(&mut w);
    assert_eq!(
        (end(&v), exit),
        (("not_started", "content_refused"), 3),
        "{v}"
    );
    assert_eq!(w.launched(), ["content-rollback"]);
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(
        (close["outcome"].clone(), close["to"].clone()),
        (json!("not_started"), json!(c(2)))
    );
}

#[test]
fn d12_content_exit_1_skips_the_binary() {
    let mut w = W::new();
    installed_by_a(&mut w);
    let back = w.inst.get_mut("content-rollback").unwrap();
    (back.tree, back.exit) = (None, 1);
    let (v, exit) = rolls(&mut w);
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "content_failed_restored"), 3),
        "{v}"
    );
    assert_eq!(w.launched(), ["content-rollback"]);
    // Exit 1 with the tree at no commit: failed, and still no file rollback.
    let mut w = W::new();
    installed_by_a(&mut w);
    let back = w.inst.get_mut("content-rollback").unwrap();
    (back.tree, back.exit) = (Some(None), 1);
    let (v, exit) = rolls(&mut w);
    assert_eq!(
        (end(&v), exit, v["to"].clone()),
        (("failed", "content_failed_restored"), 4, json!("neither")),
        "{v}"
    );
    assert_eq!(w.launched(), ["content-rollback"]);
}

#[test]
fn d12_binary_failure_ends_failed() {
    let mut w = W::new();
    installed_by_a(&mut w);
    let back = w.inst.get_mut("ap-rollback").unwrap();
    (back.file, back.exit, back.err) = (None, 1, "rollback_move_failed@install-ap.sh:9\n".into());
    let (v, exit) = rolls(&mut w);
    assert_eq!(
        (end(&v), exit, v["to"].clone()),
        (("failed", "rollback_move_failed"), 4, json!("neither")),
        "{v}"
    );
    assert_eq!(w.launched(), ["content-rollback", "ap-rollback"]);
}

#[test]
fn d12_request_forms() {
    let cases: [(&[&str], Value, &str); 9] = [
        (&["by", "reason"], json!({"rollback": true}), "by"),
        (&["reason"], json!({"rollback": true}), "reason"),
        (&[], json!({"rollback": true, "past": [c(1)]}), "past"),
        (&[], json!({"rollback": false}), "rollback"),
        (&[], json!({"rollback": "yes"}), "rollback"),
        (&["reason"], json!({}), "reason"),
        (&["by"], json!({}), "by"),
        (&["by", "reason"], json!({"past": [c(1)]}), "by"),
        (&["by", "reason"], json!({"rollback": null}), "rollback"),
    ];
    for (remove, add, field) in cases {
        let mut w = W::new();
        let mut r = w.request();
        for k in remove {
            r.as_object_mut().unwrap().remove(*k);
        }
        for (k, v) in add.as_object().unwrap() {
            r[k] = v.clone();
        }
        let (v, exit) = w.run_with(&r);
        assert_eq!(
            (v["code"].as_str(), exit, v["detail"].clone()),
            (Some("invalid_input"), 2, json!({ "field": field })),
            "{r}"
        );
        assert!(w.calls.is_empty());
    }
    // The three forms that are admitted.
    let forms: [fn(&W) -> Value; 3] = [W::request, W::timer, W::rollback];
    for form in forms {
        let mut w = W::new();
        let r = form(&w);
        let (v, _) = w.run_with(&r);
        assert_ne!(v["code"], "invalid_input", "{r}");
    }
}

// ---- D2 the timer's limits ---------------------------------------------------------------------

/// The timer's call ended on a limit: no installer, no attempt, nothing after step 6.
fn limited(w: &W, v: &Value, exit: u8, code: &str, want: u8, install: &Value) {
    assert_eq!((end(v), exit), (("refused", code), want), "{v}");
    assert_eq!(v["detail"], json!({ "install": install }));
    assert!(w.launched().is_empty());
    assert!(w.applies("start_install").is_empty());
    assert!(!w.calls.iter().any(|(p, _, _)| p == "df"), "{v}");
    assert_eq!(w.last().as_ref(), Some(v));
}
fn tick(w: &mut W) -> (Value, u8) {
    let r = w.timer();
    w.run_with(&r)
}
/// The pair at c(1), which a Lead's install call installed last.
fn at_one(w: &mut W) {
    (w.file, w.tree, w.bytes) = (Some(c(1)), Some(c(1)), binary(&c(1)));
}

#[test]
fn d2_timer_is_suppressed_after_a_failure() {
    for outcome in ["failed_restored", "failed"] {
        let mut w = W::new();
        let id = w.closed("forward", &c(2), &c(0), outcome, "timer:1");
        let (v, exit) = tick(&mut w);
        limited(&w, &v, exit, "suppressed", 3, &id);
    }
    // Lifted by a Lead's install call that ended installed.
    let mut w = W::with(3);
    at_one(&mut w);
    w.closed("forward", &c(1), &c(0), "failed_restored", "timer:1");
    w.closed("forward", &c(1), &c(0), "installed", "session-0");
    let (v, exit) = tick(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
}

#[test]
fn d2_not_started_does_not_lift_the_suppression() {
    let mut w = W::new();
    let failed = w.closed("forward", &c(2), &c(0), "failed", "timer:1");
    w.closed("forward", &c(2), &c(0), "not_started", "session-0");
    w.closed("rollback", &c(0), &c(2), "not_started", "session-0");
    let (v, exit) = tick(&mut w);
    limited(&w, &v, exit, "suppressed", 3, &failed);
}

#[test]
fn d2_timer_is_held_by_a_rollback() {
    let mut w = W::new();
    w.closed("forward", &c(1), &c(0), "installed", "session-0");
    let rb = w.closed("rollback", &c(0), &c(1), "installed", "session-0");
    w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
    let (v, exit) = tick(&mut w);
    limited(&w, &v, exit, "held_by_rollback", 2, &rb);
    // Lifted by a Lead's install call that ended installed.
    let mut w = W::with(3);
    at_one(&mut w);
    w.closed("rollback", &c(0), &c(1), "installed", "session-0");
    w.closed("forward", &c(1), &c(0), "installed", "session-0");
    let (v, exit) = tick(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
}

#[test]
fn d2_timer_attempt_exists() {
    for outcome in ["installed", "failed_restored", "failed"] {
        let mut w = W::new();
        at_one(&mut w);
        let id = w.closed("forward", &c(2), &c(0), outcome, "timer:1");
        w.closed("forward", &c(1), &c(0), "installed", "session-0");
        let (v, exit) = tick(&mut w);
        limited(&w, &v, exit, "attempt_exists", 2, &id);
    }
}

#[test]
fn d2_timer_attempts_exhausted() {
    let mut w = W::new();
    for _ in 0..2 {
        w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
    }
    let third = w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
    let (v, exit) = tick(&mut w);
    limited(&w, &v, exit, "attempts_exhausted", 3, &third);
    // Two are not three: the third attempt runs.
    let mut w = W::new();
    for _ in 0..2 {
        w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
    }
    let (v, exit) = tick(&mut w);
    assert_eq!(
        (end(&v), exit, v["attempt"].clone()),
        (("installed", ""), 0, json!(3)),
        "{v}"
    );
}

#[test]
fn d2_limits_count_forward_attempts_only() {
    let mut w = W::new();
    w.closed("rollback", &c(2), &c(1), "installed", "session-0");
    w.closed("forward", &c(0), &c(2), "installed", "session-0");
    for _ in 0..3 {
        w.closed("rollback", &c(2), &c(0), "not_started", "session-0");
    }
    let (v, exit) = tick(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
}

#[test]
fn d2_lead_request_has_no_limits() {
    let setups: [fn(&mut W); 4] = [
        |w| {
            w.closed("forward", &c(2), &c(0), "failed", "timer:1");
        },
        |w| {
            w.closed("rollback", &c(0), &c(1), "installed", "session-0");
        },
        |w| {
            w.closed("forward", &c(2), &c(0), "installed", "timer:1");
        },
        |w| {
            for _ in 0..3 {
                w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
            }
        },
    ];
    for setup in setups {
        let mut w = W::new();
        setup(&mut w);
        let (v, exit) = w.run();
        assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
        assert_eq!(v["by"], "session-1");
    }
}

#[test]
fn d2_timer_by_is_the_invocation() {
    for (invocation, by) in [
        (Some("6a1f0c"), "timer:6a1f0c"),
        (None, "timer"),
        (Some(""), "timer"),
    ] {
        let mut w = W::new();
        w.invocation = invocation.map(str::to_owned);
        let (v, exit) = tick(&mut w);
        assert_eq!(
            (end(&v), exit, v["by"].clone()),
            (("installed", ""), 0, json!(by))
        );
        let start = &w.applies("start_install")[0]["parameters"];
        assert_eq!(start["by"], by);
        assert!(start.get("reason").is_none(), "{start}");
        assert_eq!(w.installs[0]["by"], by);
    }
}

#[test]
fn d2_pair_differs_of_the_timer_exits_3_and_names_the_attempt() {
    // After `failed` the pair is at no commit: the timer's call ends at step 4, exit 3, and names the
    // newest attempt that did not end `not_started`.
    let mut w = W::new();
    let failed = w.closed("forward", &c(2), &c(0), "failed", "timer:1");
    w.closed("forward", &c(2), &c(0), "not_started", "session-0");
    w.tree = None;
    let (v, exit) = tick(&mut w);
    assert_eq!((end(&v), exit), (("refused", "pair_differs"), 3), "{v}");
    assert_eq!(v["detail"], json!({ "install": failed }));
    assert!(w.launched().is_empty() && w.applies("start_install").is_empty());
    assert_eq!(w.last().as_ref(), Some(&v));
    // No such attempt: null.
    let mut w = W::new();
    w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
    w.tree = None;
    let (v, exit) = tick(&mut w);
    assert_eq!(
        (end(&v), exit, v["detail"].clone()),
        (("refused", "pair_differs"), 3, Value::Null)
    );
    // A Lead's request keeps exit 2 and no detail.
    let mut w = W::new();
    w.closed("forward", &c(2), &c(0), "failed", "timer:1");
    w.tree = None;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "pair_differs");
    assert_eq!(v["detail"], Value::Null);
}

#[test]
fn d2_exit_codes_of_the_limits() {
    type Setup = fn(&mut W);
    let setups: [(Setup, &str, u8); 4] = [
        (
            |w| {
                w.closed("forward", &c(2), &c(0), "failed_restored", "timer:1");
            },
            "suppressed",
            3,
        ),
        (
            |w| {
                w.closed("rollback", &c(0), &c(1), "installed", "session-0");
            },
            "held_by_rollback",
            2,
        ),
        (
            |w| {
                w.closed("forward", &c(2), &c(0), "installed", "timer:1");
            },
            "attempt_exists",
            2,
        ),
        (
            |w| {
                for _ in 0..3 {
                    w.closed("forward", &c(2), &c(0), "not_started", "timer:1");
                }
            },
            "attempts_exhausted",
            3,
        ),
    ];
    for (setup, code, want) in setups {
        let mut w = W::new();
        setup(&mut w);
        let (v, exit) = tick(&mut w);
        assert_eq!((end(&v), exit), (("refused", code), want), "{v}");
    }
}

// ---- D7 the record -----------------------------------------------------------------------------

const MEMBERS: [&str; 18] = [
    "at",
    "by",
    "repository",
    "target",
    "tip",
    "from",
    "end",
    "outcome",
    "code",
    "detail",
    "install",
    "attempt",
    "installers",
    "to",
    "rollback_binary",
    "rollback_content",
    "index",
    "closed",
];
fn members(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}
fn sorted() -> Vec<String> {
    let mut m: Vec<String> = MEMBERS.iter().map(|s| (*s).to_owned()).collect();
    m.sort();
    m
}

#[test]
fn d7_record_members_and_last_json() {
    let mut w = W::new();
    let (v, _) = w.run();
    assert_eq!(members(&v), sorted());
    assert_eq!(w.last(), Some(v.clone()));
    let at = v["at"].as_str().unwrap();
    assert!(at.ends_with('Z') && at.len() == 20, "{at}");
    let start = &w.applies("start_install")[0];
    let want = json!({"by": "session-1", "repository": "o/r", "target": TARGET, "tip": c(2), "from": c(0),
        "end": "installed", "outcome": "installed", "code": null, "detail": null, "install": oid("install", &format!("act-ap.{}.1", c(2))),
        "attempt": 1, "to": c(2), "rollback_binary": kept(&w), "rollback_content": w.tarball(),
        "index": {"before": "ok", "after": "ok"}, "closed": null});
    for (k, value) in want.as_object().unwrap() {
        assert_eq!(&v[k], value, "{k}");
    }
    assert_eq!(start["parameters"]["from_sha256"], sha(&binary(&c(0))));
    // A refusal is recorded the same way; the record holds no text of a report or an installer.
    let mut w = W::new();
    w.build_busy = true;
    let (v, _) = w.run();
    assert_eq!(members(&v), sorted());
    assert_eq!(w.last(), Some(v));
    // invalid_input: printed only, nothing made.
    let mut w = W::new();
    let mut r = w.request();
    r["extra"] = json!(1);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("refused", "invalid_input"), 2));
    assert_eq!(members(&v), sorted());
    assert!(!w.t.join("state").exists());
    assert!(w.calls.is_empty());
}

#[test]
fn d7_exit_codes() {
    let mut w = W::new();
    assert_eq!(w.run().1, 0);
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    assert_eq!(w.run().1, 0);
    let mut w = W::new();
    w.free = 1;
    assert_eq!(w.run().1, 2);
    let mut w = W::new();
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.file, ap.exit, ap.err) = (None, 1, "install_busy@install-ap.sh:40\n".into());
    assert_eq!(w.run().1, 3);
    let mut w = W::new();
    w.inst.get_mut("content").unwrap().tree = None;
    assert_eq!(w.run().1, 3);
    let mut w = W::new();
    let ap_back = w.inst.get_mut("ap-rollback").unwrap();
    (ap_back.file, ap_back.exit) = (None, 1);
    w.inst.get_mut("content").unwrap().tree = None;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("failed", ""), 4), "{v}");
    let mut w = W::new();
    w.close_fails = true;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("unrecorded", ""), 5), "{v}");
}

#[test]
fn d7_unrecorded_keeps_the_outcome() {
    let mut w = W::new();
    w.close_fails = true;
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["outcome"].clone(), v["to"].clone()),
        (("unrecorded", ""), 5, json!("installed"), json!(c(2)))
    );
    assert_eq!(w.installs[0]["status"], "running");
    assert_eq!(v["install"], w.installs[0]["id"]);
    assert_eq!(w.last(), Some(v));
}

#[test]
fn d7_unrecorded_failed_exits_4() {
    let mut w = W::new();
    w.close_fails = true;
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (Some(None), 3, String::new());
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["outcome"].clone()),
        (("unrecorded", "content_exit_3"), 4, json!("failed")),
        "{v}"
    );
}

#[test]
fn d7_detail_per_code() {
    let mut w = W::new();
    let mut r = w.request();
    r["by"] = json!("");
    assert_eq!(w.run_with(&r).0["detail"], json!({"field": "by"}));
    for (member, value) in [
        ("repository", json!("no-slash")),
        ("url", json!(7)),
        ("state", json!("relative")),
        ("build_lock", json!("/a/../b")),
        ("reason", json!("x".repeat(401))),
        ("past", json!(["short"])),
    ] {
        let mut r = w.request();
        r[member] = value;
        let (v, exit) = w.run_with(&r);
        assert_eq!(
            (v["code"].as_str(), exit, v["detail"].clone()),
            (Some("invalid_input"), 2, json!({"field": member}))
        );
    }
    let mut r = w.request();
    r.as_object_mut().unwrap().remove("reason");
    assert_eq!(w.run_with(&r).0["detail"], json!({"field": "reason"}));
    let mut r = w.request();
    r["past"] = json!(vec![c(1); 51]);
    assert_eq!(w.run_with(&r).0["detail"], json!({"field": "past"}));
    // Codes without a detail.
    let setups: [fn(&mut W); 4] = [
        |w| w.fetch_fails = true,
        |w| w.free = 0,
        |w| w.build_busy = true,
        |w| w.tree = None,
    ];
    for setup in setups {
        let mut w = W::new();
        setup(&mut w);
        let (v, _) = w.run();
        assert_eq!(v["detail"], Value::Null, "{v}");
    }
    let mut w = W::new();
    let (v, _) = w.run();
    assert_eq!(v["detail"], Value::Null);
}

// ---- D9 one call at a time ---------------------------------------------------------------------

#[test]
fn d9_second_call_is_busy() {
    // While a call runs (at its `df`), a second call with another `state` and the same host lock.
    let mut w = W::new();
    let mut second = Box::new(W::new());
    second.lock = w.lock.clone();
    w.nested = Some(second);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let (second, code) = w.nested_result.clone().unwrap();
    assert_eq!((end(&second), code), (("refused", "busy"), 2), "{second}");
    assert_eq!(
        (second["detail"].clone(), second["installers"].clone()),
        (Value::Null, json!([]))
    );
    // After the first call ended the lock is free: the same second call goes on. (A child that
    // another test of this process started while the lock was inheritable may hold it a moment.)
    let free = (0..100).any(|_| {
        let mut again = W::new();
        again.lock = w.lock.clone();
        let (v, exit) = again.run();
        if v["code"] == "busy" {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        exit == 0
    });
    assert!(free);
}

#[test]
fn d9_installers_inherit_the_lock() {
    let mut w = W::new();
    let (v, _) = w.run();
    assert_eq!(w.launched(), ["ap", "content"], "{v}");
    assert_eq!(w.lock_inherited, [true, true]);
}

// ---- D10 the exact calls of one install ---------------------------------------------------------

#[test]
fn d10_exact_calls_of_an_install() {
    let mut w = W::new();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let (tip, from) = (c(2), c(0));
    let clone = w.p("state/r");
    let git = |words: &[&str]| {
        let mut a = args(&["-C", &clone]);
        a.extend(args(words));
        ("git".to_owned(), a, Value::Null)
    };
    let eq = |k: &str, v: Value| json!({"eq": [k, v]});
    let search = |kind: &str, all: Vec<Value>| {
        (
            "ont".to_owned(),
            args(&["search"]),
            json!({"object_type": kind, "where": {"all": all}, "limit": 200}),
        )
    };
    let version = (w.p("bin/ap"), args(&["--version"]), Value::Null);
    let apply = |body: Value| {
        (
            "ont".to_owned(),
            args(&["action", "apply", "--client", "ap-deliver"]),
            body,
        )
    };
    let index = (
        "ont".to_owned(),
        args(&["index", "--source", "installed_binaries"]),
        Value::Null,
    );
    let mut env: serde_json::Map<String, Value> = w
        .environment()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    env.insert("CARGO_TARGET_DIR".into(), json!(w.p("state/target")));
    let launch = |script: &str| {
        (
            "bash".to_owned(),
            args(&[&format!("{clone}/scripts/{script}"), "install", &tip]),
            json!({"cwd": clone, "env": env}),
        )
    };
    let review = |n: u32| {
        search(
            "review",
            vec![
                eq("kind", json!("packet")),
                eq("head", json!(h(n))),
                eq("status", json!("closed")),
            ],
        )
    };
    let kept = kept(&w);
    let want = vec![
        search(
            "install",
            vec![eq("target", target()), eq("status", json!("running"))],
        ),
        (
            "git".to_owned(),
            args(&["init", "--quiet", &clone]),
            Value::Null,
        ),
        git(&[
            "fetch",
            "--quiet",
            "--no-tags",
            "https://example.invalid/o/r.git",
            "+refs/heads/main:refs/remotes/origin/main",
        ]),
        git(&["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"]),
        git(&["checkout", "--quiet", "--force", "--detach", &tip]),
        version.clone(),
        git(&["show", &format!("{from}:content/SHA256SUMS")]),
        git(&["show", &format!("{tip}:ontology/ENGINE-PIN")]),
        ("ont".to_owned(), args(&["--version"]), Value::Null),
        git(&["show", &format!("{tip}:ontology/work.schema.json")]),
        git(&[
            "rev-list",
            "--first-parent",
            "--parents",
            &format!("{from}..{tip}"),
        ]),
        git(&[
            "rev-parse",
            &format!("{}^{{tree}}", c(1)),
            &format!("{}^2^{{tree}}", c(1)),
        ]),
        review(1),
        git(&[
            "rev-parse",
            &format!("{tip}^{{tree}}"),
            &format!("{tip}^2^{{tree}}"),
        ]),
        review(2),
        (
            "df".to_owned(),
            args(&["-BM", "--output=avail", &w.p("state")]),
            Value::Null,
        ),
        (
            "flock".to_owned(),
            args(&["-n", &w.p("build.lock"), "true"]),
            Value::Null,
        ),
        index.clone(),
        search(
            "install",
            vec![eq("target", target()), eq("commit", json!(tip))],
        ),
        apply(
            json!({"action_type": "start_install", "request_id": format!("ap.{tip}.1"), "agent": "ap-deliver",
            "parameters": {"target": target(), "kind": "forward", "repository": "o/r", "commit": tip, "attempt": 1,
                "from": from, "from_sha256": sha(&binary(&from)), "by": "session-1", "reason": "after a merge",
                "head": h(2), "review": w.accepting(2), "reviews": [w.accepting(1), w.accepting(2)]}}),
        ),
        launch("install-ap.sh"),
        launch("install-content.sh"),
        version,
        git(&["show", &format!("{tip}:content/SHA256SUMS")]),
        apply(
            json!({"action_type": "close_install", "request_id": format!("ap.{tip}.1.close"), "agent": "ap-deliver",
            "parameters": {"install": oid("install", &format!("act-ap.{tip}.1")), "outcome": "installed", "to": tip,
                "rollback_binary": kept, "rollback_content": w.tarball()}}),
        ),
        index,
    ];
    assert_eq!(w.calls.len(), want.len(), "{:#?}", w.calls);
    for (n, (got, want)) in w.calls.iter().zip(&want).enumerate() {
        assert_eq!(got, want, "call {n}");
    }
}
