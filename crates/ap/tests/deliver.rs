//! `ap deliver` against a fake host (README "Delivery"). The fake `System` is a small model of the
//! world the command reads: the clone (a first-parent chain of merges, each with a second parent
//! and a tree), the pair (the commit `ap --version` of the installed file names and the commit
//! whose content the tree holds), the store (reviews and install attempts, searched by `all` of
//! `eq`), `df`, `flock`, and the installers, which change the pair as the case says and print
//! what the case says from a real child. Every call is recorded in order. No test builds,
//! installs, fetches or touches the host's store; the lock, the backup directories, the reports
//! and the state directory are files of a temporary directory.
use ap::deliver::{check_request, execute};
use ap::host::{Answer, Failure, Reply, System};
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

/// An entry of `deploy/`: its mode, its object type and its bytes.
type Unit = (&'static str, &'static str, Vec<u8>);
/// A setup of the fake and the `systemctl` calls it leads to.
type Case = (fn(&mut W), &'static [&'static str]);

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
    build_released: bool,
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
    /// The tarballs `install-content.sh rollback-check` refuses; and a script that does not know the
    /// verb (it answers exit 2 for every tarball).
    refused: Vec<String>,
    old_script: bool,
    /// Every `rollback-check` asked, in order: (script, tarball). It is no installer and no row of
    /// `calls`.
    checks: Vec<(String, String)>,
    /// The engine (README "The engine and the schema"): whether the request carries `engine`, the
    /// engine repository's chain and heads, whether the pin is under its `origin/main`, and its
    /// fetch.
    engine: bool,
    echain: Vec<Commit>,
    eheads: BTreeMap<String, String>,
    e_ancestor: bool,
    efetch_fails: bool,
    /// What `install-ont.sh install` and `rollback` leave the engine at (None: unchanged), and
    /// their exits.
    ont_install: (Option<String>, i32),
    ont_rollback: (Option<String>, i32),
    /// The mode of `ontology/migrate` at the tip (None: absent), its exit, and each run: its
    /// arguments and the marker as it was then.
    migrate_mode: Option<&'static str>,
    migrate_exit: i32,
    migrates: Vec<(Vec<String>, Option<String>)>,
    backup_fails: bool,
    schema_install_fails: bool,
    /// The install answers ok and the store's version stays.
    schema_noop: bool,
    /// The marker as it was at each `ont schema install`.
    marker_at_install: Vec<Option<String>>,
    /// `git ls-tree` of the tip fails.
    ls_tree_fails: bool,
    /// The checkouts a request's `checkouts` names, as the fake models them; with `real_git` the
    /// commands of a checkout run the real `git` instead.
    cos: Vec<Co>,
    real_git: bool,
    /// `deploy/` of each commit (none named: an empty directory): name, mode, object type, bytes.
    deploy: BTreeMap<String, BTreeMap<String, Unit>>,
    /// `git ls-tree <commit> deploy/` fails; a checkout fails.
    units_fail: bool,
    checkout_fails: bool,
    /// The unit files whose `git show` fails.
    lost_units: Vec<String>,
    /// The `systemctl` calls (their words joined by a space) that exit 1, and the timers
    /// `is-enabled` does not answer `enabled` for.
    systemctl_fails: Vec<String>,
    not_enabled: Vec<String>,
    /// The length of `calls` at each `rollback-check`.
    checks_at: Vec<usize>,
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
        for d in [
            "bin",
            "backup",
            "agents",
            "content-backup",
            "reports",
            "units",
        ] {
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
            build_released: false,
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
            refused: vec![],
            old_script: false,
            checks: vec![],
            engine: false,
            echain: vec![],
            eheads: BTreeMap::new(),
            e_ancestor: true,
            efetch_fails: false,
            ont_install: (None, 0),
            ont_rollback: (None, 0),
            migrate_mode: None,
            migrate_exit: 0,
            migrates: vec![],
            backup_fails: false,
            schema_install_fails: false,
            schema_noop: false,
            marker_at_install: vec![],
            ls_tree_fails: false,
            cos: vec![],
            real_git: false,
            deploy: BTreeMap::new(),
            units_fail: false,
            checkout_fails: false,
            lost_units: vec![],
            systemctl_fails: vec![],
            not_enabled: vec![],
            checks_at: vec![],
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
    /// The entry `deploy/<name>` of `commit`: its mode, object type and bytes.
    fn unit_at(&self, commit: &str, name: &str) -> Option<Unit> {
        self.deploy.get(commit)?.get(name).cloned()
    }
    /// `deploy/<name>` with `bytes` at each commit c(n) of `ns`.
    fn unit(&mut self, ns: impl IntoIterator<Item = u32>, name: &str, bytes: &str) {
        for n in ns {
            let entry = ("100644", "blob", bytes.as_bytes().to_vec());
            self.deploy
                .entry(c(n))
                .or_default()
                .insert(name.into(), entry);
        }
    }
    /// The host's copy of a unit file, under `AP_UNIT_DIR`.
    fn host(&self, name: &str) -> String {
        self.p(&format!("units/{name}"))
    }
    /// The `systemctl` calls, in order, their words joined by a space.
    fn systemctl(&self) -> Vec<String> {
        self.calls
            .iter()
            .filter(|(p, _, _)| p == "systemctl")
            .map(|(_, a, _)| a.join(" "))
            .collect()
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
        let mut r = json!({"repository":"o/r","url":"https://example.invalid/o/r.git","state":self.p("state"),
            "build_lock":self.p("build.lock"),"by":"session-1","reason":"after a merge"});
        if self.engine {
            r["engine"] = json!({"repository": "o/engine", "url": EURL});
        }
        r
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
            .filter(|(p, _, _)| p == "bash" || p.ends_with("/ontology/migrate"))
            .map(|(p, a, _)| {
                if p != "bash" {
                    return "migrate".to_owned();
                }
                let script = if a[0].ends_with("install-ap.sh") {
                    "ap"
                } else if a[0].ends_with("install-ont.sh") {
                    "ont"
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
    fn git(&mut self, a: &[String], ms: u64) -> Answer {
        if a[0] == "--no-optional-locks" {
            return self.checkout(a, ms);
        }
        let clone = self.p("state/r");
        let eclone = self.p("state/engine");
        if a[0] == "init" && a[2] == eclone {
            assert_eq!(a, args(&["init", "--quiet", &eclone]));
            return Self::text(real_git(a), "");
        }
        if a[0] == "init" {
            assert_eq!(a, args(&["init", "--quiet", &clone]));
            return Self::text(true, "");
        }
        if a[..2] == ["-C", eclone.as_str()] {
            return self.egit(a);
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
            ["checkout", "--quiet", "--force", "--detach", _] => {
                Self::text(!self.checkout_fails, "")
            }
            ["ls-tree", k, "deploy/"] => {
                let lines: String = self
                    .deploy
                    .get(*k)
                    .into_iter()
                    .flatten()
                    .map(|(n, (mode, kind, b))| {
                        format!("{mode} {kind} {}\tdeploy/{n}\n", &sha(b)[..40])
                    })
                    .collect();
                Self::text(!self.units_fail, &lines)
            }
            ["ls-tree", k, path] if path.starts_with("deploy/") => {
                let line = self
                    .unit_at(k, &path["deploy/".len()..])
                    .map_or(String::new(), |(mode, kind, b)| {
                        format!("{mode} {kind} {}\t{path}\n", &sha(&b)[..40])
                    });
                Self::text(true, &line)
            }
            ["rev-list", "--first-parent", base, "--", path] => {
                let name = path.strip_prefix("deploy/").unwrap();
                let mut at = self.chain.iter().find(|x| x.id == *base);
                let mut lines = String::new();
                while let Some(x) = at {
                    let parent = self.chain.iter().find(|p| x.parents.first() == Some(&p.id));
                    let before = parent.and_then(|p| self.unit_at(&p.id, name));
                    if self.unit_at(&x.id, name) != before {
                        lines.push_str(&format!("{}\n", x.id));
                    }
                    at = parent;
                }
                Self::text(true, &lines)
            }
            ["ls-tree", tip, "--", "ontology/migrate"] => {
                assert_eq!(*tip, self.tip());
                let entry = self.migrate_mode.map_or(String::new(), |m| {
                    format!("{m} blob {}\tontology/migrate\n", "a".repeat(40))
                });
                Self::text(!self.ls_tree_fails, &entry)
            }
            ["show", spec] => {
                let (commit, path) = spec.split_once(':').unwrap();
                match path {
                    "content/SHA256SUMS" => Self::text(true, &self.sums(commit)),
                    "ontology/ENGINE-PIN" => Self::text(true, &format!("{}\n", self.pin)),
                    "ontology/work.schema.json" => Self::text(
                        true,
                        &json!({"format": 1, "version": self.tip_schema}).to_string(),
                    ),
                    p if p.starts_with("deploy/") => match self.unit_at(commit, &p[7..]) {
                        _ if self.lost_units.iter().any(|n| p[7..] == **n) => Self::text(false, ""),
                        Some((_, "blob", b)) => Self::text(true, &String::from_utf8(b).unwrap()),
                        _ => Self::text(false, ""),
                    },
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
            "backup" => {
                assert_eq!(a, args(&["backup"]));
                let path = self.p("ont-state/ont/backup/ontology-20261117T000000Z.sqlite");
                let status = if self.backup_fails { "refused" } else { "ok" };
                Self::ok(json!({"status": status, "result": {"path": path}, "schema": schema}))
            }
            "schema" => {
                let (clone, tip) = (self.p("state/r"), self.tip());
                let want = ["schema", "install", "--from", &clone, "--commit", &tip];
                assert_eq!(
                    a,
                    args(&[&want[..], &["--path", "ontology/work.schema.json"]].concat())
                );
                let marker = std::fs::read_to_string(self.p("state/migrate.pending")).ok();
                self.marker_at_install.push(marker);
                if self.schema_install_fails {
                    return Self::ok(json!({"status": "refused", "code": "schema_invalid"}));
                }
                if !self.schema_noop {
                    self.schema = self.tip_schema;
                }
                Self::ok(json!({"status": "ok", "result": {"version": self.schema}}))
            }
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
    fn call(&mut self, program: &str, a: &[String], body: &Value, ms: u64) -> Answer {
        if program == "bash" && a.len() == 3 && a[1] == "rollback-check" {
            self.checks.push((a[0].clone(), a[2].clone()));
            self.checks_at.push(self.calls.len());
            return Self::text(!self.old_script && !self.refused.contains(&a[2]), "");
        }
        self.calls.push((program.into(), a.to_vec(), body.clone()));
        if program == self.p("ont-bin/ont") {
            assert_eq!(a, ["--version"]);
            return Self::text(true, &format!("ont 0.1.0 {}\n", self.ont));
        }
        if program == self.p("bin/ap") {
            assert_eq!(a, ["--version"]);
            return match &self.file {
                Some(f) => Self::text(true, &format!("ap 0.1.0 {f}\n")),
                None => Self::text(false, ""),
            };
        }
        match program {
            "git" => self.git(a, ms),
            "ont" => self.store(a, body),
            "df" => {
                assert_eq!(a, args(&["-BM", "--output=avail", &self.p("state")]));
                if let Some(mut second) = self.nested.take() {
                    let r = second.request();
                    self.nested_result = Some(execute(&mut *second, &r));
                }
                Self::text(true, &format!("Avail\n{}M\n", self.free))
            }
            "systemctl" => {
                assert_eq!(ms, 120_000);
                let words = a.join(" ");
                if a[0] == "is-enabled" {
                    let on = !self.not_enabled.contains(&a[1]);
                    return Self::text(on, if on { "enabled\n" } else { "disabled\n" });
                }
                Self::text(!self.systemctl_fails.contains(&words), "")
            }
            "flock" => {
                if a[0] == "-n" {
                    assert_eq!(a, args(&["-n", &self.p("build.lock"), "true"]));
                    Self::text(!self.build_busy, "")
                } else {
                    assert!(self.applies("start_install").is_empty() && self.launched().is_empty());
                    assert_eq!(a, args(&["-w", "3", &self.p("build.lock"), "true"]));
                    assert_eq!(ms, 13_000);
                    Self::text(self.build_released, "")
                }
            }
            _ => panic!("unexpected program {program} {a:?}"),
        }
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if path == self.p("ont-bin/ont") {
            return Ok(engine_binary(&self.ont));
        }
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
        if argv[0].ends_with("/ontology/migrate") || argv[1].ends_with("/scripts/install-ont.sh") {
            return self.launch_engine(argv, files);
        }
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
            ("AP_UNIT_DIR".into(), self.p("units")),
            ("ONT_INSTALL_BIN_DIR".into(), self.p("ont-bin")),
            ("ONT_INSTALL_STATE_DIR".into(), self.p("ont-state")),
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

// spec: specs/ap/deliver.md#R27
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

// spec: specs/ap/deliver.md#R27
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

// spec: specs/ap/deliver.md#R27
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

// spec: specs/ap/deliver.md#R15
// spec: specs/ap/deliver.md#R29
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

// spec: specs/ap/deliver.md#R29
// spec: specs/ap/deliver.md#R34
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

// spec: specs/ap/deliver.md#R29
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

// spec: specs/ap/deliver.md#R29
// spec: specs/ap/deliver.md#R34
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

// spec: specs/ap/deliver.md#R29
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
// spec: specs/ap/deliver.md#R15
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

// spec: specs/ap/deliver.md#R15
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
// spec: specs/ap/deliver.md#R15
// spec: specs/ap/deliver.md#R30
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

// spec: specs/ap/deliver.md#R25
#[test]
fn d4_not_on_chain() {
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(other()), Some(other()), binary(&other()));
    not_accepted(&mut w, &other(), "not_on_chain");
}

// spec: specs/ap/deliver.md#R25
#[test]
fn d4_not_a_merge() {
    let mut w = W::new();
    w.chain[1].parents.truncate(1);
    not_accepted(&mut w, &c(1), "not_a_merge");
}

// spec: specs/ap/deliver.md#R25
#[test]
fn d4_tree_differs() {
    let mut w = W::new();
    w.chain[1].tree = tree(77);
    not_accepted(&mut w, &c(1), "tree_differs");
}

// spec: specs/ap/deliver.md#R25
#[test]
fn d4_no_accepting_review() {
    let mut w = W::new();
    // Left: the running review, the repair and an accepting review of another repository.
    w.reviews
        .retain(|r| !(r["head"] == h(1) && r["verdict"] == "accept" && r["repository"] == "o/r"));
    w.review(1, "x/other", "accept", "closed");
    not_accepted(&mut w, &c(1), "no_accepting_review");
}

// spec: specs/ap/deliver.md#R25
#[test]
fn d4_revoked_review() {
    let mut w = W::new();
    let id = w.accepting(2);
    w.reviews.iter_mut().find(|r| r["id"] == id).unwrap()["status"] = json!("revoked");
    not_accepted(&mut w, &c(2), "no_accepting_review");
}

// spec: specs/ap/deliver.md#R25
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

// spec: specs/ap/deliver.md#R25
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

// spec: specs/ap/deliver.md#R25
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

// spec: specs/ap/deliver.md#R25
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

// spec: specs/ap/deliver.md#R25
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

// spec: specs/ap/deliver.md#R19
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

// spec: specs/ap/deliver.md#R20
// spec: specs/ap/deliver.md#R22
// spec: specs/ap/deliver.md#R23
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

// spec: specs/ap/deliver.md#R26
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

// spec: specs/ap/deliver.md#R26
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

// spec: specs/ap/deliver.md#R24
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

// spec: specs/ap/deliver.md#R24
#[test]
fn d5_schema_differs() {
    let mut w = W::new();
    w.tip_schema = 9;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "schema_differs");
    assert_eq!(v["detail"], json!({"tip": 9, "installed": 8}));
}

// spec: specs/ap/deliver.md#R23
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R18
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R18
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

// spec: specs/ap/deliver.md#R18
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

// spec: specs/ap/deliver.md#R21
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
    // The first call launched the content installer with the lock inheritable: wait for any child
    // another test started in that window to let it go.
    w.settle();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(
        closes.last().unwrap()["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
}

// spec: specs/ap/deliver.md#R18
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

// spec: specs/ap/deliver.md#R29
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R21
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

// spec: specs/ap/deliver.md#R19
// spec: specs/ap/deliver.md#R33
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
    let git = |words: &[&str]| {
        let mut a = args(&["-C", &clone]);
        a.extend(args(words));
        ("git".to_owned(), a, Value::Null)
    };
    let want = vec![
        search(vec![eq("target", target()), eq("status", json!("running"))]),
        version.clone(),
        show(&here),
        // The clone at C, the commit the pair was read at, before any of its scripts (R47).
        git(&["checkout", "--quiet", "--force", "--detach", &here]),
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
        // The unit files of A's `from` and of C (R48).
        git(&["ls-tree", &back, "deploy/"]),
        git(&["ls-tree", &here, "deploy/"]),
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

// spec: specs/ap/deliver.md#R33
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

// spec: specs/ap/deliver.md#R17
// spec: specs/ap/deliver.md#R33
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

// spec: specs/ap/deliver.md#R33
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

// spec: specs/ap/deliver.md#R18
// spec: specs/ap/deliver.md#R33
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

// spec: specs/ap/deliver.md#R33
// spec: specs/ap/deliver.md#R34
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

// spec: specs/ap/deliver.md#R33
// spec: specs/ap/deliver.md#R34
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

// spec: specs/ap/deliver.md#R33
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

// spec: specs/ap/deliver.md#R12
// spec: specs/ap/deliver.md#R13
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

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R32
#[test]
fn d2_not_started_does_not_lift_the_suppression() {
    let mut w = W::new();
    let failed = w.closed("forward", &c(2), &c(0), "failed", "timer:1");
    w.closed("forward", &c(2), &c(0), "not_started", "session-0");
    w.closed("rollback", &c(0), &c(2), "not_started", "session-0");
    let (v, exit) = tick(&mut w);
    limited(&w, &v, exit, "suppressed", 3, &failed);
}

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R32
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

// spec: specs/ap/deliver.md#R12
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

// spec: specs/ap/deliver.md#R32
// spec: specs/ap/deliver.md#R36
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

/// A running attempt that a call could not settle ends the timer's call with exit 3, as
/// `pair_differs` does, so that `ap-deliver.service` stays failed until a call settled it; a Lead's
/// request keeps exit 2. The three ways: a dead rollback at neither commit, a clone that cannot
/// answer, a close the store did not apply.
// spec: specs/ap/deliver.md#R36
#[test]
fn d2_attempt_running_of_the_timer_exits_3() {
    type Setup = fn(&mut W) -> Value;
    let setups: [(&str, Setup); 3] = [
        ("a dead rollback at neither commit", |w| {
            let id = w.dead_attempt(&c(2), &c(0), "0");
            w.installs[0]["kind"] = json!("rollback");
            (w.file, w.tree) = (Some(c(1)), Some(c(1)));
            id
        }),
        ("a clone that cannot answer", |w| {
            let id = dead_forward(w, Some(1), Some(1));
            w.lost = true;
            id
        }),
        ("a close the store did not apply", |w| {
            let id = dead_forward(w, Some(0), Some(0));
            w.close_fails = true;
            id
        }),
    ];
    for (what, setup) in setups {
        let mut w = W::new();
        let id = setup(&mut w);
        let (v, exit) = tick(&mut w);
        assert_eq!(
            (end(&v), exit),
            (("refused", "attempt_running"), 3),
            "{what}: {v}"
        );
        assert_eq!(v["detail"], json!({"install": id}), "{what}");
        assert!(
            w.launched().is_empty() && w.installs[0]["status"] == "running",
            "{what}"
        );
        assert_eq!(w.last().as_ref(), Some(&v), "{what}");
        // The same state, a Lead's request: exit 2, as before.
        let mut w = W::new();
        setup(&mut w);
        let (v, exit) = w.run();
        refused(&w, &v, exit, "attempt_running");
    }
}

// spec: specs/ap/deliver.md#R36
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

const MEMBERS: [&str; 19] = [
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
    "units",
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

// spec: specs/ap/deliver.md#R11
// spec: specs/ap/deliver.md#R31
// spec: specs/ap/deliver.md#R35
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

// spec: specs/ap/deliver.md#R36
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

// spec: specs/ap/deliver.md#R30
// spec: specs/ap/deliver.md#R35
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

// spec: specs/ap/deliver.md#R36
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

// spec: specs/ap/deliver.md#R35
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

// spec: specs/ap/deliver.md#R16
// spec: specs/ap/deliver.md#R20
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

// spec: specs/ap/deliver.md#R16
#[test]
fn d9_installers_inherit_the_lock() {
    let mut w = W::new();
    let (v, _) = w.run();
    assert_eq!(w.launched(), ["ap", "content"], "{v}");
    assert_eq!(w.lock_inherited, [true, true]);
}

// ---- D10 the exact calls of one install ---------------------------------------------------------

// spec: specs/ap/deliver.md#R11
// spec: specs/ap/deliver.md#R19
// spec: specs/ap/deliver.md#R22
// spec: specs/ap/deliver.md#R27
// spec: specs/ap/deliver.md#R28
// spec: specs/ap/deliver.md#R30
// spec: specs/ap/deliver.md#R31
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
        // T is checked out once step 6 accepted it and step 7 passed (R47).
        git(&["checkout", "--quiet", "--force", "--detach", &tip]),
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
        // The unit files of T (R48): `deploy/` of the fake holds none.
        git(&["ls-tree", &tip, "deploy/"]),
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

// ---- the content handle is a tarball the content installer takes back ---------------------------

/// A tarball the installer would refuse (its `rollback-check` exits 2) is no handle, though its
/// members hold the tree of `from`: the oldest one the installer accepts is, and none is when it
/// refuses every one, or when the script does not know the verb. The check is asked of the script of
/// the clone, only for a tarball whose members pass.
// spec: specs/ap/deliver.md#R18
#[test]
fn d6_a_tarball_the_installer_would_refuse_is_no_handle() {
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    tarball_at(&w, "100000Z-1", Some(1));
    let refused = tarball_at(&w, "100001Z-2", Some(0));
    let taken = tarball_at(&w, "100002Z-3", Some(0));
    w.refused = vec![refused.clone()];
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w),
            "rollback_content": taken})
    );
    let script = w.p("state/r/scripts/install-content.sh");
    assert_eq!(
        w.checks,
        [(script.clone(), refused.clone()), (script, taken.clone())]
    );
    // The installer refuses every one: no content handle in the record, the pair untouched.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    let only = tarball_at(&w, "100001Z-1", Some(0));
    w.refused = vec![only];
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
    // A script that does not know the verb answers exit 2: the tarball is no handle.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(2));
    tarball_at(&w, "100001Z-1", Some(0));
    w.old_script = true;
    w.run();
    assert_eq!(
        w.applies("close_install")[0]["parameters"],
        json!({"install": id, "outcome": "installed", "to": c(2), "rollback_binary": kept(&w)})
    );
    // The tarball the content installer printed in this call is judged the same way.
    let mut w = W::new();
    w.refused = vec![w.tarball()];
    let (v, _) = w.run();
    let close = &w.applies("close_install")[0]["parameters"];
    assert!(close.get("rollback_content").is_none(), "{v} {close}");
    assert!(w.checks.iter().any(|(_, t)| *t == w.tarball()));
}

/// A recorded tarball the installer would refuse counts as absent when a rollback request reads it:
/// where the manifests differ nothing is rolled back, where they are the same the file alone is, and
/// never is the refused tarball given to the installer.
// spec: specs/ap/deliver.md#R18
#[test]
fn d12_recorded_tarball_the_installer_would_refuse_counts_as_absent() {
    let mut w = W::new();
    installed_by_a(&mut w);
    w.refused = vec![w.tarball()];
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "nothing_to_roll_back");
    assert!(w.launched().is_empty());
    assert_eq!(w.checks.len(), 1);
    let mut w = W::new();
    w.same_content = true;
    installed_by_a(&mut w);
    w.refused = vec![w.tarball()];
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(w.launched(), ["ap-rollback"]);
    // A script that does not know the verb: the recorded tarball is no handle.
    let mut w = W::new();
    installed_by_a(&mut w);
    w.old_script = true;
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "nothing_to_roll_back");
}

// A positive bound probes once without waiting, then waits only after contention.
fn locks(w: &W) -> Vec<Vec<String>> {
    w.calls
        .iter()
        .filter(|(p, _, _)| p == "flock")
        .map(|(_, a, _)| a.clone())
        .collect()
}

// spec: specs/ap/deliver.md#R12
#[test]
fn w1_build_wait_outside_its_range_is_refused() {
    for value in [
        json!(-1),
        json!(901),
        json!(1.0),
        json!("3"),
        json!(null),
        json!(true),
        json!([]),
        json!({}),
    ] {
        let mut w = W::new();
        let mut r = w.request();
        r["build_wait_seconds"] = value;
        let (v, exit) = w.run_with(&r);
        assert_eq!((end(&v), exit), (("refused", "invalid_input"), 2));
        assert_eq!(v["detail"], json!({"field":"build_wait_seconds"}));
        assert!(w.calls.is_empty() && !w.t.join("state").exists());
    }
}

// spec: specs/ap/deliver.md#R26
#[test]
fn w1_free_lock_is_taken_without_waiting() {
    for bound in [0, 3, 900] {
        let mut w = W::new();
        let mut r = w.request();
        r["build_wait_seconds"] = json!(bound);
        let (v, exit) = w.run_with(&r);
        assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
        assert_eq!(locks(&w), [args(&["-n", &w.p("build.lock"), "true"])]);
    }
}

// spec: specs/ap/deliver.md#R26
#[test]
fn w1_held_lock_is_waited_for_and_the_call_goes_on() {
    let mut w = W::new();
    (w.build_busy, w.build_released) = (true, true);
    let mut r = w.request();
    r["build_wait_seconds"] = json!(3);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        locks(&w),
        [
            args(&["-n", &w.p("build.lock"), "true"]),
            args(&["-w", "3", &w.p("build.lock"), "true"])
        ]
    );
}

// spec: specs/ap/deliver.md#R26
#[test]
fn w1_lock_held_past_the_bound_is_build_busy() {
    for bound in [0, 3] {
        let mut w = W::new();
        w.build_busy = true;
        let mut r = w.request();
        r["build_wait_seconds"] = json!(bound);
        let (v, exit) = w.run_with(&r);
        refused(&w, &v, exit, "build_busy");
        assert_eq!(locks(&w).len(), if bound == 0 { 1 } else { 2 });
        assert!(w.applies("close_install").is_empty());
    }
}

// spec: specs/ap/deliver.md#R26
#[test]
fn w2_waiting_line_only_when_the_call_waits() {
    const CASE: &str = "AP_TEST_WAIT_CASE";
    if let Ok(case) = std::env::var(CASE) {
        let mut w = W::new();
        w.build_busy = case != "free";
        let mut r = w.request();
        r["build_wait_seconds"] = json!(if case == "zero" { 0 } else { 3 });
        w.run_with(&r);
        return;
    }
    for case in ["free", "zero", "wait"] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "w2_waiting_line_only_when_the_call_waits",
                "--nocapture",
            ])
            .env(CASE, case)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        let err = String::from_utf8(out.stderr).unwrap();
        if case == "wait" {
            assert_eq!(err.lines().count(), 1, "{err}");
            let v: Value = serde_json::from_str(err.trim()).unwrap();
            assert_eq!(members(&v), ["at", "seconds", "waiting"]);
            assert_eq!(v["waiting"], "build_lock");
            assert_eq!(v["seconds"], 3);
            let at = v["at"].as_str().unwrap();
            assert_eq!(at.len(), 20);
            assert!(at.ends_with('Z') && at.as_bytes()[10] == b'T');
        } else {
            assert!(err.is_empty(), "{err}");
        }
    }
}

// ---- E the engine and the schema (README "The engine and the schema") ---------------------------

const ENGINE: &str = "/usr/local/bin/ont";
const EURL: &str = "https://example.invalid/o/engine.git";
/// A commit of the engine's chain: `ec(0)` is installed, `ec(2)` the pin.
fn ec(n: u32) -> String {
    format!("{:040}", 500 + n)
}
fn eh(n: u32) -> String {
    format!("{:040}", 600 + n)
}
fn engine_binary(commit: &str) -> Vec<u8> {
    format!("ont binary of {commit}").into_bytes()
}
fn etarget() -> Value {
    oid("installed_artifact", ENGINE)
}
/// Real `git` for the engine clone's `init` and `config`, so that its configuration is a file a
/// test reads back.
fn real_git(a: &[String]) -> bool {
    Command::new("git")
        .args(a)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .is_ok_and(|o| o.status.success())
}
fn git_config(clone: &str, key: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", clone, "config", "--get", key])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
const MAIN: &str = "+refs/heads/main:refs/remotes/origin/main";

impl W {
    /// A request with `engine`; the engine at ec(0), the pin ec(2), two accepted merges of the
    /// engine's repository between them.
    fn engine() -> Self {
        let mut w = W::new();
        w.engine = true;
        for d in ["ont-bin", "ont-state/install"] {
            std::fs::create_dir_all(w.t.join(d)).unwrap();
        }
        (w.ont, w.pin) = (ec(0), ec(2));
        w.echain = vec![Commit {
            id: ec(0),
            parents: vec![],
            tree: tree(50),
        }];
        for n in 1..=2 {
            w.echain.push(Commit {
                id: ec(n),
                parents: vec![ec(n - 1), eh(n)],
                tree: tree(50 + n),
            });
            w.eheads.insert(eh(n), tree(50 + n));
            w.ereview(n);
        }
        w.ont_install = (Some(ec(2)), 0);
        w.ont_rollback = (Some(ec(0)), 0);
        w
    }
    /// The engine already at the pin: only the schema and the pair may move.
    fn at_pin() -> Self {
        let mut w = W::engine();
        w.ont = ec(2);
        w
    }
    fn ereview(&mut self, n: u32) -> Value {
        let key = format!("rv-{}", self.reviews.len() + 1);
        let report = self.p(&format!("reports/{key}.md"));
        let bytes = format!("Verdict: accept\n{}\n", eh(n));
        std::fs::write(&report, &bytes).unwrap();
        let row = json!({"id": oid("review", &key), "kind": "packet", "head": eh(n), "status": "closed",
            "verdict": "accept", "repository": "o/engine", "report": report,
            "report_sha256": sha(bytes.as_bytes()), "closed_at": "2026-11-13T00:01:00Z"});
        self.reviews.push(row.clone());
        row["id"].clone()
    }
    fn eclosed(&mut self, kind: &str, commit: &str, from: &str, outcome: &str) -> Value {
        let id = self.closed(kind, commit, from, outcome, "timer:1");
        let row = self.installs.last_mut().unwrap();
        (row["target"], row["repository"]) = (etarget(), json!("o/engine"));
        id
    }
    /// Waits until the host lock is free before a further call: a child that another test of this
    /// process started while this call's lock was inheritable may hold it a moment (as in
    /// `d9_second_call_is_busy`). No new holder can appear once the call ended.
    fn settle(&self) {
        let free = Command::new("flock")
            .args(["-w", "30", &self.lock, "true"])
            .status()
            .is_ok_and(|s| s.success());
        assert!(free, "the host lock stayed held");
    }
    fn marker_present(&self) -> bool {
        self.t.join("state/migrate.pending").exists()
    }
    fn marker(&self) -> Option<Value> {
        let bytes = std::fs::read(self.p("state/migrate.pending")).ok()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }
    fn set_marker(&self, from: u64, to: u64) {
        std::fs::create_dir_all(self.p("state")).unwrap();
        let m = json!({"from": from, "to": to}).to_string();
        std::fs::write(self.p("state/migrate.pending"), m).unwrap();
    }
    /// The index in `calls` of the first call whose program or first argument ends with `what`.
    fn at(&self, what: &str) -> Option<usize> {
        self.calls.iter().position(|(p, a, b)| {
            p.ends_with(what)
                || a.first().is_some_and(|x| x.ends_with(what))
                || b["action_type"] == what
        })
    }
    fn egit(&mut self, a: &[String]) -> Answer {
        let rest: Vec<&str> = a[2..].iter().map(String::as_str).collect();
        match rest.as_slice() {
            ["config", _, _] => Self::text(real_git(a), ""),
            ["fetch", "--quiet", "--no-tags", "origin", refspec] => {
                assert_eq!(*refspec, MAIN);
                // A fetch by the remote's name finds nothing without the remote.
                let url = git_config(&a[1], "remote.origin.url");
                Self::text(!self.efetch_fails && url.as_deref() == Some(EURL), "")
            }
            [
                "merge-base",
                "--is-ancestor",
                pin,
                "refs/remotes/origin/main",
            ] => {
                assert_eq!(*pin, self.pin);
                Self::text(self.e_ancestor, "")
            }
            ["checkout", "--quiet", "--force", "--detach", pin] => {
                assert_eq!(*pin, self.pin);
                Self::text(true, "")
            }
            ["rev-list", "--first-parent", "--parents", range] => {
                let (from, to) = range.split_once("..").unwrap();
                assert_eq!(to, self.pin);
                let mut lines = vec![];
                for commit in self.echain.iter().rev() {
                    if commit.id == from {
                        break;
                    }
                    let mut words = vec![commit.id.clone()];
                    words.extend(commit.parents.iter().cloned());
                    lines.push(words.join(" "));
                }
                Self::text(true, &format!("{}\n", lines.join("\n")))
            }
            ["rev-parse", own, _] => {
                let commit = own.strip_suffix("^{tree}").unwrap();
                let c = self.echain.iter().find(|c| c.id == commit).unwrap();
                match c.parents.get(1).and_then(|p| self.eheads.get(p)) {
                    Some(t) => Self::text(true, &format!("{}\n{t}\n", c.tree)),
                    None => Self::text(false, ""),
                }
            }
            _ => panic!("unexpected engine git call {a:?}"),
        }
    }
    /// `install-ont.sh` (it keeps the file it replaces as `ont.before-<12 hex>`) and
    /// `ontology/migrate`.
    fn launch_engine(
        &mut self,
        argv: &[String],
        files: [std::fs::File; 3],
    ) -> std::io::Result<Child> {
        let exit = if argv[0].ends_with("/ontology/migrate") {
            let marker = std::fs::read_to_string(self.p("state/migrate.pending")).ok();
            self.migrates.push((argv[1..].to_vec(), marker));
            self.migrate_exit
        } else {
            let (after, exit) = if argv[2] == "install" {
                let old = engine_binary(&self.ont);
                let kept = format!("ont-state/install/ont.before-{}", &sha(&old)[..12]);
                std::fs::write(self.p(&kept), old).unwrap();
                self.ont_install.clone()
            } else {
                self.ont_rollback.clone()
            };
            if let Some(v) = after {
                self.ont = v;
            }
            exit
        };
        let [stdin, stdout, stderr] = files;
        Command::new("sh")
            .args(["-c", "exit \"$1\"", "sh", &exit.to_string()])
            .env_clear()
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
    }
}
fn ekept(w: &W) -> String {
    let old = engine_binary(&ec(0));
    w.p(&format!(
        "ont-state/install/ont.before-{}",
        &sha(&old)[..12]
    ))
}

// spec: specs/ap/deliver.md#R37
// spec: specs/ap/deliver.md#R42
// spec: specs/ap/deliver.md#R43
#[test]
fn e_engine_at_the_pin_after_the_installer_is_installed_and_the_call_ends() {
    let mut w = W::engine();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let (pin, from) = (ec(2), ec(0));
    let want = json!({"target": ENGINE, "tip": c(2), "from": from, "to": pin, "outcome": "installed",
        "install": oid("install", &format!("act-ont.{pin}.1")), "attempt": 1, "rollback_binary": ekept(&w),
        "rollback_content": null, "index": {"before": "ok", "after": "ok"}, "schema": null});
    for (k, value) in want.as_object().unwrap() {
        assert_eq!(&v[k], value, "{k}");
    }
    assert_eq!(rows(&v), [row("ont", 0, None)]);
    assert_eq!(w.launched(), ["ont"]);
    // The attempt: target, kind, the engine's repository, the pin, from, the digest, the walk.
    let start = &w.applies("start_install")[0];
    assert_eq!(start["request_id"], format!("ont.{pin}.1"));
    let review = |n: u32| {
        let r = w.reviews.iter().find(|r| r["head"] == eh(n)).unwrap();
        r["id"].clone()
    };
    assert_eq!(
        start["parameters"],
        json!({"target": etarget(), "kind": "forward", "repository": "o/engine", "commit": pin,
            "attempt": 1, "from": from, "from_sha256": sha(&engine_binary(&from)), "by": "session-1",
            "reason": "after a merge", "head": eh(2), "review": review(2), "reviews": [review(1), review(2)]})
    );
    let close = &w.applies("close_install")[0];
    assert_eq!(close["request_id"], format!("ont.{pin}.1.close"));
    // The installer: the clone's own script, `install <pin>`, in the clone, with the command's
    // environment and nothing added.
    let eclone = w.p("state/engine");
    let launch = w.calls.iter().find(|(p, _, _)| p == "bash").unwrap();
    assert_eq!(
        launch.1,
        args(&[&format!("{eclone}/scripts/install-ont.sh"), "install", &pin])
    );
    let env: serde_json::Map<String, Value> = w
        .environment()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    assert_eq!(launch.2, json!({"cwd": eclone, "env": env}));
    assert_eq!(w.lock_inherited, [true]);
    // The next call finds the engine at the pin and installs the pair.
    w.settle();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(TARGET)),
        "{v}"
    );
    assert_eq!(rows(&v), [row("ap", 0, None), row("content", 0, None)]);
}

// spec: specs/ap/deliver.md#R43
#[test]
fn e_engine_clone_has_the_remote_origin() {
    let mut w = W::engine();
    let (v, _) = w.run();
    assert_eq!(v["outcome"], "installed", "{v}");
    let eclone = w.p("state/engine");
    assert_eq!(
        git_config(&eclone, "remote.origin.url").as_deref(),
        Some(EURL)
    );
    assert_eq!(
        git_config(&eclone, "remote.origin.fetch").as_deref(),
        Some(MAIN)
    );
    // The repository's own clone is fetched by URL and has no remote.
    let fetch = w
        .calls
        .iter()
        .find(|(p, a, _)| p == "git" && a[1] == eclone && a[2] == "fetch")
        .unwrap();
    assert_eq!(
        fetch.1[2..],
        args(&["fetch", "--quiet", "--no-tags", "origin", MAIN])
    );
    assert!(w.calls.iter().any(|(p, a, _)| p == "git"
        && a[1] == w.p("state/r")
        && a[2..]
            == args(&[
                "fetch",
                "--quiet",
                "--no-tags",
                "https://example.invalid/o/r.git",
                MAIN
            ])));
}

// spec: specs/ap/deliver.md#R43
#[test]
fn e_engine_at_from_after_the_installer_is_failed_restored() {
    let mut w = W::engine();
    w.ont_install = (None, 2);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "engine_refused"), 3),
        "{v}"
    );
    assert_eq!(
        (v["to"].clone(), v["target"].clone()),
        (json!(ec(0)), json!(ENGINE))
    );
    assert_eq!(rows(&v), [row("ont", 2, Some("engine_refused"))]);
    assert_eq!(w.launched(), ["ont"]);
}

// spec: specs/ap/deliver.md#R43
#[test]
fn e_engine_at_neither_is_rolled_back_once() {
    let mut w = W::engine();
    w.ont_install = (Some("f".repeat(40)), 1);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "engine_exit_1"), 3),
        "{v}"
    );
    assert_eq!(
        rows(&v),
        [
            row("ont", 1, Some("engine_exit_1")),
            row("ont-rollback", 0, None)
        ]
    );
    assert_eq!(v["to"], ec(0));
    // A rollback that leaves it at neither: failed, exit 4.
    let mut w = W::engine();
    w.ont_install = (Some("f".repeat(40)), 0);
    w.ont_rollback = (None, 2);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["to"].clone()),
        (("failed", "engine_refused"), 4, json!("neither")),
        "{v}"
    );
    assert_eq!(w.launched(), ["ont", "ont-rollback"]);
}

// spec: specs/ap/deliver.md#R43
#[test]
fn e_engine_chain_not_accepted_installs_nothing() {
    // The pin not under the engine's origin/main.
    let mut w = W::engine();
    w.e_ancestor = false;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_not_accepted");
    assert_eq!(v["detail"], json!({"commit": ec(2), "why": "not_on_chain"}));
    assert_eq!(v["target"], ENGINE);
    // A merge with no accepting review in the engine's repository; the request's `past` is of T's
    // chain only.
    for past in [false, true] {
        let mut w = W::engine();
        let row = w.reviews.iter_mut().find(|r| r["head"] == eh(1)).unwrap();
        row["repository"] = json!("o/r");
        let mut r = w.request();
        if past {
            r["past"] = json!([c(1)]);
        }
        let (v, exit) = w.run_with(&r);
        refused(&w, &v, exit, "engine_not_accepted");
        assert_eq!(
            v["detail"],
            json!({"commit": ec(1), "why": "no_accepting_review"})
        );
    }
    // A pin behind the installed engine is not on the chain either.
    let mut w = W::engine();
    (w.ont, w.pin) = (ec(2), ec(1));
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_not_accepted");
    assert_eq!(v["detail"], json!({"commit": ec(2), "why": "not_on_chain"}));
    // The tip's chain is still walked first.
    let mut w = W::engine();
    w.chain[2].tree = tree(5);
    let (v, exit) = w.run();
    refused(&w, &v, exit, "not_accepted");
    assert!(
        !w.calls
            .iter()
            .any(|(_, a, _)| a.contains(&w.p("state/engine")))
    );
}

// spec: specs/ap/deliver.md#R40
#[test]
fn e_engine_refusals_before_the_first_effect() {
    // The engine unreadable, or a pin that is not 40 hex: engine_differs, as without `engine`.
    let mut w = W::engine();
    w.pin = "not a pin".into();
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_differs");
    let mut w = W::engine();
    w.efetch_fails = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "fetch_failed");
    assert_eq!(v["detail"], json!({"repository": "o/engine"}));
    for setup in [|w: &mut W| w.free = 1, |w: &mut W| w.build_busy = true] {
        let mut w = W::engine();
        setup(&mut w);
        let (v, exit) = w.run();
        assert_eq!((end(&v).0, exit), ("refused", 2));
        assert!(w.launched().is_empty() && w.applies("start_install").is_empty());
        assert!(
            !w.calls
                .iter()
                .any(|(_, a, _)| a.contains(&w.p("state/engine")))
        );
    }
}

// spec: specs/ap/deliver.md#R41
#[test]
fn e_engine_limits_of_the_timer() {
    let pin = ec(2);
    // Three not_started attempts of one pin stop further ones; two do not.
    let mut w = W::engine();
    for _ in 0..2 {
        w.eclosed("forward", &pin, &ec(0), "not_started");
    }
    let third = w.eclosed("forward", &pin, &ec(0), "not_started");
    let (v, exit) = tick(&mut w);
    limited(&w, &v, exit, "attempts_exhausted", 3, &third);
    assert_eq!(v["target"], ENGINE);
    let mut w = W::engine();
    for _ in 0..2 {
        w.eclosed("forward", &pin, &ec(0), "not_started");
    }
    let (v, exit) = tick(&mut w);
    assert_eq!(
        (end(&v), exit, v["attempt"].clone()),
        (("installed", ""), 0, json!(3)),
        "{v}"
    );
    // A failed attempt suppresses; a rollback holds; a pin installed once is not installed again.
    let cases: [(&str, &str, &str, u8); 4] = [
        ("forward", "failed_restored", "suppressed", 3),
        ("forward", "failed", "suppressed", 3),
        ("rollback", "installed", "held_by_rollback", 2),
        ("forward", "installed", "attempt_exists", 2),
    ];
    for (kind, outcome, code, want) in cases {
        let mut w = W::engine();
        let commit = if kind == "rollback" {
            ec(0)
        } else {
            pin.clone()
        };
        let id = w.eclosed(kind, &commit, &ec(0), outcome);
        let (v, exit) = tick(&mut w);
        limited(&w, &v, exit, code, want, &id);
        // A Lead's request has none of them.
        w.settle();
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit, v["target"].clone()),
            (("installed", ""), 0, json!(ENGINE)),
            "{v}"
        );
    }
}

// spec: specs/ap/deliver.md#R38
#[test]
fn e_dead_engine_attempt_is_closed_by_a_read() {
    let dead = |w: &mut W| {
        let id = oid("install", "act-dead-ont");
        w.installs.push(json!({"id": id, "target": etarget(), "kind": "forward", "repository": "o/engine",
            "commit": ec(2), "attempt": 1, "from": ec(0), "from_sha256": sha(&engine_binary(&ec(0))),
            "by": "dead", "status": "running", "started_at": STARTED, "running_in": etarget()}));
        id
    };
    // At its commit: installed, and the call goes on to the pair.
    let mut w = W::at_pin();
    let id = dead(&mut w);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["closed"], json!({"install": id, "outcome": "installed"}));
    // At its from: not_started with dead_call; the call goes on and installs the engine.
    let mut w = W::engine();
    let id = dead(&mut w);
    let (v, _) = w.run();
    assert_eq!(
        v["closed"],
        json!({"install": id, "outcome": "not_started"})
    );
    let closed = w.installs.iter().find(|i| i["id"] == id).unwrap();
    assert_eq!(closed["code"], "dead_call");
    assert_eq!(
        (v["target"].clone(), v["outcome"].clone()),
        (json!(ENGINE), json!("installed"))
    );
    // At neither: the engine's rollback once; back at from the call goes on, else it ends failed.
    for (back, want) in [(Some(ec(0)), "failed_restored"), (None, "failed")] {
        let mut w = W::engine();
        let id = dead(&mut w);
        w.ont = "f".repeat(40);
        w.ont_rollback = (back, 0);
        let script = w.p("state/engine/scripts/install-ont.sh");
        std::fs::create_dir_all(w.p("state/engine/scripts")).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let (v, exit) = w.run();
        let row = w.installs.iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(row["outcome"], want, "{v}");
        if want == "failed" {
            assert_eq!(
                (end(&v), exit, v["target"].clone()),
                (("failed", ""), 4, json!(ENGINE))
            );
            assert_eq!(w.launched(), ["ont-rollback"]);
        } else {
            assert_eq!(w.launched()[0], "ont-rollback");
            assert_eq!(v["closed"], json!({"install": id, "outcome": want}));
        }
    }
    // A clone without its installer cannot repair it: attempt_running.
    let mut w = W::engine();
    let id = dead(&mut w);
    w.ont = "f".repeat(40);
    let (v, exit) = w.run();
    refused(&w, &v, exit, "attempt_running");
    assert_eq!(v["detail"], json!({ "install": id }));
}

// spec: specs/ap/deliver.md#R42
// spec: specs/ap/deliver.md#R44
// spec: specs/ap/deliver.md#R46
#[test]
fn e_schema_higher_is_backed_up_then_installed_and_the_pair_follows() {
    let mut w = W::at_pin();
    w.tip_schema = 9;
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(TARGET)),
        "{v}"
    );
    let backup = w.p("ont-state/ont/backup/ontology-20261117T000000Z.sqlite");
    assert_eq!(
        v["schema"],
        json!({"from": 8, "to": 9, "backup": backup, "migrate": "none"})
    );
    assert_eq!(w.schema, 9);
    let (b, i) = (w.at("backup").unwrap(), w.at("schema").unwrap());
    assert!(
        b < i && i < w.at("start_install").unwrap(),
        "{:#?}",
        w.calls
    );
    assert_eq!(w.marker_at_install, [None]);
    assert_eq!(rows(&v), [row("ap", 0, None), row("content", 0, None)]);
    // No `install` object stands for the schema.
    assert_eq!(w.applies("start_install").len(), 1);
}

// spec: specs/ap/deliver.md#R40
#[test]
fn e_schema_equal_goes_on_and_lower_is_refused() {
    let mut w = W::at_pin();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["schema"].clone()),
        (("installed", ""), 0, Value::Null)
    );
    assert!(w.at("backup").is_none() && w.at("schema").is_none());
    let mut w = W::at_pin();
    w.tip_schema = 7;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "schema_differs");
    assert_eq!(v["detail"], json!({"tip": 7, "installed": 8}));
    assert!(w.at("backup").is_none());
    // A higher version waits for the engine: the call that installs the engine ends there.
    let mut w = W::engine();
    w.tip_schema = 9;
    let (v, _) = w.run();
    assert_eq!((v["target"].clone(), w.schema), (json!(ENGINE), 8));
    assert!(w.at("backup").is_none());
}

// spec: specs/ap/deliver.md#R44
// spec: specs/ap/deliver.md#R46
#[test]
fn e_schema_install_failed_installs_nothing_else() {
    let setups: [fn(&mut W); 3] = [
        |w| w.schema_install_fails = true,
        |w| w.schema_noop = true,
        |w| w.backup_fails = true,
    ];
    for (n, setup) in setups.into_iter().enumerate() {
        let mut w = W::at_pin();
        w.tip_schema = 9;
        setup(&mut w);
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit),
            (("failed", "schema_install_failed"), 4),
            "{v}"
        );
        assert_eq!(v["outcome"], Value::Null);
        assert!(w.launched().is_empty() && w.applies("start_install").is_empty());
        assert_eq!(v["detail"]["from"], 8);
        assert_eq!(w.at("schema").is_some(), n < 2, "{v}");
    }
}

// spec: specs/ap/deliver.md#R44
#[test]
fn e_migrate_present_runs_after_the_install_and_clears_the_marker() {
    let mut w = W::at_pin();
    (w.tip_schema, w.migrate_mode) = (9, Some("100755"));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["schema"]["migrate"], "ok");
    // The marker is there before the install and while the migration runs; it is gone after.
    let marker = Some(json!({"from": 8, "to": 9}).to_string());
    assert_eq!(w.marker_at_install, std::slice::from_ref(&marker));
    let store = w.p("ont-state/ont/ontology.sqlite");
    assert_eq!(w.migrates, [(args(&[&store, "8", "9"]), marker)]);
    assert_eq!(w.marker(), None);
    assert_eq!(w.launched(), ["migrate", "ap", "content"]);
    assert!(w.at("/ontology/migrate").unwrap() > w.at("schema").unwrap());
    // A file that is not executable is no migration.
    let mut w = W::at_pin();
    (w.tip_schema, w.migrate_mode) = (9, Some("100644"));
    let (v, _) = w.run();
    assert_eq!(
        (v["schema"]["migrate"].clone(), w.marker_at_install.clone()),
        (json!("none"), vec![None])
    );
    assert!(w.migrates.is_empty());
}

// spec: specs/ap/deliver.md#R44
// spec: specs/ap/deliver.md#R46
#[test]
fn e_migrate_failing_leaves_the_marker_and_the_next_call_retries_first() {
    let mut w = W::at_pin();
    (w.tip_schema, w.migrate_mode, w.migrate_exit) = (9, Some("100755"), 1);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("failed", "migrate_failed"), 4), "{v}");
    assert_eq!(v["detail"], json!({"from": 8, "to": 9, "store": 9}));
    assert_eq!(v["schema"]["migrate"], "failed");
    assert_eq!(rows(&v), [row("migrate", 1, Some("migrate_exit_1"))]);
    assert_eq!(w.schema, 9, "the schema stays installed");
    assert_eq!(w.marker(), Some(json!({"from": 8, "to": 9})));
    assert!(w.applies("start_install").is_empty());
    // The next call, with the engine moved on as well: the migration first, then the engine.
    (w.calls, w.migrate_exit, w.pin) = (vec![], 0, ec(2));
    w.ont = ec(0);
    w.settle();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(ENGINE)),
        "{v}"
    );
    assert_eq!(w.launched(), ["migrate", "ont"]);
    assert_eq!(w.marker(), None);
    assert!(w.at("/ontology/migrate").unwrap() < w.at("start_install").unwrap());
    // And the one after it installs the pair, with nothing owed.
    w.calls.clear();
    w.settle();
    let (v, _) = w.run();
    assert_eq!(
        (v["target"].clone(), w.launched()),
        (json!(TARGET), vec!["ap".to_owned(), "content".to_owned()])
    );
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_marker_with_the_store_at_to_is_migrated_first() {
    // A call killed between the schema install and the migration: the marker, the store at `to`.
    let mut w = W::at_pin();
    (w.schema, w.tip_schema, w.migrate_mode) = (9, 9, Some("100755"));
    w.set_marker(8, 9);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let store = w.p("ont-state/ont/ontology.sqlite");
    assert_eq!(w.migrates[0].0, args(&[&store, "8", "9"]));
    assert_eq!(w.launched(), ["migrate", "ap", "content"]);
    assert_eq!(
        v["schema"],
        json!({"from": 8, "to": 9, "backup": null, "migrate": "ok"})
    );
    assert_eq!(w.marker(), None);
    assert!(w.at("backup").is_none());
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_marker_with_the_store_at_from_is_removed_and_the_schema_step_runs() {
    let mut w = W::at_pin();
    (w.tip_schema, w.migrate_mode) = (9, Some("100755"));
    w.set_marker(8, 9);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    // Written again before the install, by the schema step, then cleared by its migration.
    assert_eq!(
        w.marker_at_install,
        [Some(json!({"from": 8, "to": 9}).to_string())]
    );
    assert_eq!(w.migrates.len(), 1);
    assert_eq!(w.marker(), None);
    // Without a migration at the tip the marker goes and none is written.
    let mut w = W::at_pin();
    w.tip_schema = 9;
    w.set_marker(8, 9);
    let (v, _) = w.run();
    assert_eq!(
        (v["outcome"].clone(), w.marker_at_install.clone()),
        (json!("installed"), vec![None])
    );
    assert_eq!(w.marker(), None);
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_marker_that_cannot_be_acted_on_is_migrate_failed() {
    // The tip holds no `ontology/migrate`; the store at a third version; a marker unreadable.
    let cases: [(u64, Option<&'static str>, Value); 3] = [
        (9, None, json!({"from": 8, "to": 9, "store": 9})),
        (10, Some("100755"), json!({"from": 8, "to": 9, "store": 10})),
        (
            9,
            Some("100755"),
            json!({"from": null, "to": null, "store": 9}),
        ),
    ];
    for (n, (store, mode, detail)) in cases.into_iter().enumerate() {
        let mut w = W::at_pin();
        (w.schema, w.tip_schema, w.migrate_mode) = (store, store, mode);
        w.set_marker(8, 9);
        if n == 2 {
            std::fs::write(w.p("state/migrate.pending"), "{").unwrap();
        }
        let (v, exit) = w.run();
        assert_eq!((end(&v), exit), (("failed", "migrate_failed"), 4), "{v}");
        assert_eq!(v["detail"], detail);
        assert!(w.launched().is_empty() && w.applies("start_install").is_empty());
        assert!(w.marker_present());
    }
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_marker_waits_for_the_refusals_before_an_effect() {
    type Setup = fn(&mut W) -> Value;
    let cases: [(&str, Setup); 4] = [
        ("not_accepted", |w| {
            w.chain[2].tree = tree(5);
            w.request()
        }),
        ("suppressed", |w| {
            w.closed("forward", &c(2), &c(0), "failed", "timer:1");
            w.timer()
        }),
        ("disk_low", |w| {
            w.free = 1;
            w.request()
        }),
        ("build_busy", |w| {
            w.build_busy = true;
            w.request()
        }),
    ];
    for (code, setup) in cases {
        let mut w = W::at_pin();
        (w.schema, w.tip_schema, w.migrate_mode) = (9, 9, Some("100755"));
        w.set_marker(8, 9);
        let r = setup(&mut w);
        let (v, _) = w.run_with(&r);
        assert_eq!(end(&v), ("refused", code), "{v}");
        assert!(w.migrates.is_empty() && w.marker_present(), "{code}");
    }
}

// spec: specs/ap/deliver.md#R39
#[test]
fn e_marker_with_the_pair_at_the_tip_is_acted_on_before_current() {
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    (w.schema, w.tip_schema, w.migrate_mode) = (9, 9, Some("100755"));
    w.set_marker(8, 9);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(
        (w.launched(), w.marker()),
        (vec!["migrate".to_owned()], None)
    );
    // A failing migration does not end current.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    (w.schema, w.tip_schema, w.migrate_mode, w.migrate_exit) = (9, 9, Some("100755"), 3);
    w.set_marker(8, 9);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("failed", "migrate_failed"), 4), "{v}");
    // The timer's limits that are about T do not stop it; those that are not, and the room, do.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    (w.schema, w.tip_schema, w.migrate_mode) = (9, 9, Some("100755"));
    w.closed("forward", &c(2), &c(0), "installed", "timer:1");
    w.set_marker(8, 9);
    let (v, _) = tick(&mut w);
    assert_eq!(end(&v), ("current", ""), "{v}");
    assert_eq!(w.migrates.len(), 1);
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    (w.schema, w.tip_schema, w.migrate_mode, w.build_busy) = (9, 9, Some("100755"), true);
    w.set_marker(8, 9);
    let (v, _) = w.run();
    assert_eq!(end(&v), ("refused", "build_busy"), "{v}");
    assert!(w.migrates.is_empty());
    // Without a marker the pair at T ends current at once, as without `engine`.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let (v, _) = w.run();
    assert_eq!(end(&v), ("current", ""));
    assert!(!w.calls.iter().any(|(p, _, _)| p == "df"));
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_rollback_request_is_not_subject_to_the_marker() {
    let mut w = W::at_pin();
    let a = installed_by_a(&mut w);
    let _ = a;
    (w.schema, w.tip_schema, w.migrate_mode) = (9, 9, Some("100755"));
    w.set_marker(8, 9);
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert!(w.migrates.is_empty() && w.marker_present());
}

// spec: specs/ap/deliver.md#R45
#[test]
fn e_without_engine_the_marker_is_not_read_and_the_record_is_unchanged() {
    let mut w = W::new();
    (w.schema, w.tip_schema, w.migrate_mode) = (8, 8, Some("100755"));
    w.set_marker(7, 8);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert!(w.migrates.is_empty() && w.marker_present());
    assert_eq!(members(&v), sorted());
    // With `engine` the record has one more member, `schema`.
    let mut w = W::at_pin();
    let (v, _) = w.run();
    let mut want = sorted();
    want.push("schema".into());
    want.sort();
    assert_eq!(members(&v), want);
    assert_eq!(w.last(), Some(v));
}

// spec: specs/ap/deliver.md#R14
#[test]
fn e_engine_member_is_checked() {
    let w = W::engine();
    let bad = [
        json!({"repository": "o/engine"}),
        json!({"repository": "o/engine", "url": EURL, "x": 1}),
        json!({"repository": "o/r", "url": EURL}),
        json!({"repository": "o/migrate.pending", "url": EURL}),
        json!({"repository": "o/target", "url": EURL}),
        json!({"repository": "o/engine", "url": "-x"}),
        json!("o/engine"),
    ];
    for e in bad {
        let mut r = w.request();
        r["engine"] = e.clone();
        let (v, exit) = execute(&mut W::new(), &r);
        assert_eq!((end(&v), exit), (("refused", "invalid_input"), 2), "{e}");
        assert_eq!(v["detail"], json!({"field": "engine"}), "{e}");
    }
    let mut r = w.request();
    r["repository"] = json!("o/migrate.pending");
    let (v, _) = execute(&mut W::new(), &r);
    assert_eq!(v["detail"], json!({"field": "repository"}));
}

// spec: specs/ap/deliver.md#R44
#[test]
fn e_schema_with_an_unreadable_migrate_entry_installs_nothing() {
    // Whether the tip owes a migration cannot be read: no backup, no install, no marker; the next
    // call, which can read it, installs the schema under the marker and migrates.
    let mut w = W::at_pin();
    (w.tip_schema, w.migrate_mode, w.ls_tree_fails) = (9, Some("100755"), true);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed", "schema_install_failed"), 4),
        "{v}"
    );
    assert_eq!(v["detail"], json!({"from": 8, "to": 9, "store": 8}));
    assert_eq!(w.schema, 8);
    assert!(w.at("backup").is_none() && w.at("schema").is_none());
    assert!(w.launched().is_empty() && w.applies("start_install").is_empty());
    assert_eq!(w.marker(), None);
    w.ls_tree_fails = false;
    w.settle();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!((w.schema, v["schema"]["migrate"].clone()), (9, json!("ok")));
    assert_eq!(w.migrates.len(), 1);
    assert_eq!(w.marker(), None);
}

// ---- D2 `checkouts`: the fast-forward of the host's main checkouts ----------------------------------

/// One checkout as the fake models it. `fails` names the commands that exit non-zero (`toplevel`,
/// `branch`, `status`, `fetch`, `head`, `tip`, `merge-base`, `merge`, `after`: the read after the
/// merge); `after` is what that read prints once a merge was called.
#[derive(Clone)]
struct Co {
    path: String,
    top: String,
    branch: String,
    dirty: bool,
    head: String,
    tip: String,
    after: String,
    fails: Vec<&'static str>,
    merged: bool,
}
impl Co {
    /// A checkout that fast-forwards from c(10) to c(11).
    fn new(path: String) -> Self {
        Self {
            top: path.clone(),
            path,
            branch: "main".into(),
            dirty: false,
            head: c(10),
            tip: c(11),
            after: c(11),
            fails: vec![],
            merged: false,
        }
    }
    fn failing(mut self, what: &'static str) -> Self {
        self.fails.push(what);
        self
    }
}
const TOP: &str = "rev-parse --show-toplevel";
const BRANCH: &str = "symbolic-ref --short -q HEAD";
const STATUS: &str = "status --porcelain --untracked-files=no";
const HEAD: &str = "rev-parse --verify HEAD^{commit}";
const TIP: &str = "rev-parse --verify refs/remotes/origin/main^{commit}";
fn fetch_words() -> String {
    format!("fetch --quiet --no-tags origin {MAIN}")
}
fn ancestor_words(co: &Co) -> String {
    format!("merge-base --is-ancestor {} {}", co.head, co.tip)
}
fn merge_words(co: &Co) -> String {
    format!("merge --ff-only --quiet {}", co.tip)
}
fn real_checkout(a: &[String]) -> Answer {
    let out = Command::new("git")
        .args(a)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    Ok(Reply {
        success: out.status.success(),
        bytes: out.stdout.clone(),
        stdout: out.stdout,
    })
}
impl W {
    /// A checkout command: the fixed prefix, the bound of its step, and the model's answer.
    fn checkout(&mut self, a: &[String], ms: u64) -> Answer {
        assert_eq!(a[..2], args(&["--no-optional-locks", "-C"]));
        let prefix = [
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ];
        assert_eq!(a[3..9], args(&prefix));
        let words: Vec<&str> = a[9..].iter().map(String::as_str).collect();
        let bound = match words[0] {
            "fetch" => 60_000,
            "merge" => 600_000,
            _ => 30_000,
        };
        assert_eq!(ms, bound, "{words:?}");
        if self.real_git {
            return real_checkout(a);
        }
        let i = self.cos.iter().position(|co| co.path == a[2]);
        let co = &mut self.cos[i.expect("a checkout the test does not model")];
        let joined = words.join(" ");
        let (name, out) = match joined.as_str() {
            TOP => ("toplevel", format!("{}\n", co.top)),
            BRANCH => ("branch", format!("{}\n", co.branch)),
            STATUS => ("status", if co.dirty { " M f\n" } else { "" }.to_owned()),
            HEAD if co.merged => ("after", format!("{}\n", co.after)),
            HEAD => ("head", format!("{}\n", co.head)),
            TIP => ("tip", format!("{}\n", co.tip)),
            _ if joined == fetch_words() => ("fetch", String::new()),
            _ if joined == ancestor_words(co) => ("merge-base", String::new()),
            _ if joined == merge_words(co) => {
                co.merged = true;
                ("merge", String::new())
            }
            _ => panic!("unexpected checkout command {joined}"),
        };
        Self::text(!co.fails.contains(&name), &out)
    }
    /// The commands run on checkouts, in order: (path, words).
    fn at_checkouts(&self) -> Vec<(String, String)> {
        let git = self
            .calls
            .iter()
            .filter(|(p, a, _)| p == "git" && a[0] == "--no-optional-locks");
        git.map(|(_, a, _)| (a[2].clone(), a[9..].join(" ")))
            .collect()
    }
    fn co_path(&self, name: &str) -> String {
        self.p(&format!("co/{name}"))
    }
    /// A request with `checkouts` naming the paths of `cos`, which the fake then models.
    fn with_checkouts(&mut self, cos: Vec<Co>) -> Value {
        let mut r = self.request();
        r["checkouts"] = json!(cos.iter().map(|co| co.path.clone()).collect::<Vec<_>>());
        self.cos = cos;
        r
    }
}
/// The commands of one checkout up to and including the step `upto` of the plain run.
fn steps_of(co: &Co, upto: usize) -> Vec<(String, String)> {
    let all = [
        TOP.to_owned(),
        BRANCH.to_owned(),
        STATUS.to_owned(),
        fetch_words(),
        HEAD.to_owned(),
        TIP.to_owned(),
        ancestor_words(co),
        merge_words(co),
        HEAD.to_owned(),
    ];
    all[..upto]
        .iter()
        .map(|w| (co.path.clone(), w.clone()))
        .collect()
}
/// Runs one checkout of `co` in a call that installs and returns the world and the one row.
fn one(co: impl FnOnce(String) -> Co) -> (W, Co, Value) {
    let mut w = W::new();
    let co = co(w.co_path("a"));
    let r = w.with_checkouts(vec![co.clone()]);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(w.last().as_ref(), Some(&v));
    let row = v["checkouts"][0].clone();
    (w, co, row)
}
fn row_is(row: &Value, co: &Co, from: Option<&str>, to: Option<&str>, result: &str) {
    assert_eq!(
        *row,
        json!({"path": co.path, "from": from, "to": to, "result": result})
    );
}

#[test]
fn co_absent_member_runs_no_checkout_command() {
    // spec: specs/ap/deliver.md#R7
    let mut w = W::new();
    w.cos = vec![Co::new(w.co_path("a"))];
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(members(&v), sorted());
    assert!(w.at_checkouts().is_empty());
    let path = w.co_path("a");
    assert!(w.calls.iter().all(|(_, a, _)| !a.contains(&path)));
    // The same for a call that ends `current`.
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let (v, _) = w.run();
    assert_eq!(end(&v), ("current", ""));
    assert_eq!(members(&v), sorted());
    assert!(w.at_checkouts().is_empty());
}

#[test]
fn co_the_record_has_the_member_only_with_the_request_member() {
    // spec: specs/ap/deliver.md#R6
    let mut w = W::new();
    let r = w.with_checkouts(vec![Co::new(w.co_path("a"))]);
    let (v, _) = w.run_with(&r);
    let mut want = sorted();
    want.push("checkouts".into());
    want.sort();
    assert_eq!(members(&v), want);
    assert_eq!(w.last(), Some(v));
}

#[test]
fn co_a_current_call_runs_the_steps() {
    // spec: specs/ap/deliver.md#R2
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let co = Co::new(w.co_path("a"));
    let r = w.with_checkouts(vec![co.clone()]);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["checkouts"][0]["result"], "forwarded");
    assert_eq!(w.at_checkouts(), steps_of(&co, 9));
}

#[test]
fn co_a_refused_call_moves_no_checkout() {
    // spec: specs/ap/deliver.md#R2
    type Setup = fn(&mut W);
    let setups: [(&str, Setup); 4] = [
        ("fetch_failed", |w| w.fetch_fails = true),
        ("build_busy", |w| w.build_busy = true),
        ("engine_differs", |w| w.pin = "f".repeat(40)),
        ("not_accepted", |w| w.reviews.clear()),
    ];
    for (code, setup) in setups {
        let mut w = W::new();
        setup(&mut w);
        let r = w.with_checkouts(vec![Co::new(w.co_path("a"))]);
        let (v, exit) = w.run_with(&r);
        assert_eq!((end(&v), exit), (("refused", code), 2), "{v}");
        assert_eq!(v["checkouts"], Value::Null, "{code}");
        assert!(w.at_checkouts().is_empty(), "{code}");
        assert_eq!(w.last().as_ref(), Some(&v));
    }
}

#[test]
fn co_a_failed_call_moves_no_checkout() {
    // spec: specs/ap/deliver.md#R2
    let mut w = W::new();
    let tarball = w.tarball();
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (None, 1, String::new());
    content.err = format!("install_failed_restored@/x: back; kept copy {tarball}\n");
    let r = w.with_checkouts(vec![Co::new(w.co_path("a"))]);
    let (v, exit) = w.run_with(&r);
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "content_failed_restored"), 3)
    );
    assert_eq!(v["checkouts"], Value::Null);
    assert!(w.at_checkouts().is_empty());
}

#[test]
fn co_a_rollback_call_carrying_checkouts_moves_none() {
    // spec: specs/ap/deliver.md#R2
    let mut w = W::new();
    installed_by_a(&mut w);
    let mut r = w.rollback();
    r["checkouts"] = json!([w.co_path("a")]);
    w.cos = vec![Co::new(w.co_path("a"))];
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["checkouts"], Value::Null);
    assert!(w.at_checkouts().is_empty());
    assert_eq!(w.last().as_ref(), Some(&v));
}

#[test]
fn co_forwarded() {
    // spec: specs/ap/deliver.md#R3
    let (w, co, row) = one(Co::new);
    row_is(&row, &co, Some(&c(10)), Some(&c(11)), "forwarded");
    assert_eq!(w.at_checkouts(), steps_of(&co, 9));
}

#[test]
fn co_the_merge_is_given_the_tip_digits() {
    // spec: specs/ap/deliver.md#R3
    let (w, co, _) = one(Co::new);
    let merge = &w.at_checkouts()[7].1;
    assert_eq!(*merge, format!("merge --ff-only --quiet {}", co.tip));
    assert!(merge.rsplit(' ').next().unwrap().len() == 40);
}

#[test]
fn co_the_read_after_the_merge_decides() {
    // spec: specs/ap/deliver.md#R4
    // A merge that exits non-zero but leaves HEAD at the tip is forwarded.
    let (_, co, row) = one(|p| Co::new(p).failing("merge"));
    row_is(&row, &co, Some(&c(10)), Some(&c(11)), "forwarded");
    // A refused merge: git_failed with HEAD where it was.
    let (_, co, row) = one(|p| {
        let mut co = Co::new(p).failing("merge");
        co.after = c(10);
        co
    });
    row_is(&row, &co, Some(&c(10)), Some(&c(10)), "git_failed");
    // A merge that exits 0 and leaves another head.
    let (_, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.after = c(12);
        co
    });
    row_is(&row, &co, Some(&c(10)), Some(&c(12)), "git_failed");
    // A failed read after the merge.
    let (w, co, row) = one(|p| Co::new(p).failing("after"));
    row_is(&row, &co, Some(&c(10)), None, "git_failed");
    assert_eq!(w.at_checkouts(), steps_of(&co, 9));
}

#[test]
fn co_current() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.tip = co.head.clone();
        co
    });
    row_is(&row, &co, Some(&c(10)), Some(&c(10)), "current");
    assert_eq!(w.at_checkouts(), steps_of(&co, 6));
}

#[test]
fn co_not_a_checkout() {
    // spec: specs/ap/deliver.md#R4
    // Another top level, and a failed command: nothing after step 1.
    let (w, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.top = "/elsewhere".into();
        co
    });
    row_is(&row, &co, None, None, "not_a_checkout");
    assert_eq!(w.at_checkouts(), steps_of(&co, 1));
    let (w, co, row) = one(|p| Co::new(p).failing("toplevel"));
    row_is(&row, &co, None, None, "not_a_checkout");
    assert_eq!(w.at_checkouts(), steps_of(&co, 1));
}

#[test]
fn co_not_on_main() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.branch = "topic".into();
        co
    });
    row_is(&row, &co, None, None, "not_on_main");
    assert_eq!(w.at_checkouts(), steps_of(&co, 2));
    // A detached HEAD: `symbolic-ref -q` exits 1.
    let (w, co, row) = one(|p| Co::new(p).failing("branch"));
    row_is(&row, &co, None, None, "not_on_main");
    assert_eq!(w.at_checkouts(), steps_of(&co, 2));
}

#[test]
fn co_dirty() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.dirty = true;
        co
    });
    row_is(&row, &co, None, None, "dirty");
    assert_eq!(w.at_checkouts(), steps_of(&co, 3));
}

#[test]
fn co_a_failed_status_is_git_failed_with_no_fetch_and_no_merge() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| Co::new(p).failing("status"));
    row_is(&row, &co, None, None, "git_failed");
    assert_eq!(w.at_checkouts(), steps_of(&co, 3));
}

#[test]
fn co_fetch_failed() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| Co::new(p).failing("fetch"));
    row_is(&row, &co, None, None, "fetch_failed");
    assert_eq!(w.at_checkouts(), steps_of(&co, 4));
}

#[test]
fn co_diverged() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| Co::new(p).failing("merge-base"));
    row_is(&row, &co, Some(&c(10)), None, "diverged");
    assert_eq!(w.at_checkouts(), steps_of(&co, 7));
}

#[test]
fn co_a_failed_read_of_step_five_is_git_failed() {
    // spec: specs/ap/deliver.md#R4
    let (w, co, row) = one(|p| Co::new(p).failing("head"));
    row_is(&row, &co, None, None, "git_failed");
    assert_eq!(w.at_checkouts(), steps_of(&co, 5));
    let (w, co, row) = one(|p| Co::new(p).failing("tip"));
    row_is(&row, &co, Some(&c(10)), None, "git_failed");
    assert_eq!(w.at_checkouts(), steps_of(&co, 6));
    // A read that prints no commit.
    let (_, co, row) = one(|p| {
        let mut co = Co::new(p);
        co.tip = "main".into();
        co
    });
    row_is(&row, &co, Some(&c(10)), None, "git_failed");
}

#[test]
fn co_the_next_checkout_is_taken_after_a_result_that_stops_one() {
    // spec: specs/ap/deliver.md#R4
    let mut w = W::new();
    let mut first = Co::new(w.co_path("a"));
    first.dirty = true;
    let second = Co::new(w.co_path("b"));
    let r = w.with_checkouts(vec![first.clone(), second.clone()]);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let rows = v["checkouts"].as_array().unwrap();
    row_is(&rows[0], &first, None, None, "dirty");
    row_is(&rows[1], &second, Some(&c(10)), Some(&c(11)), "forwarded");
    let want: Vec<_> = [steps_of(&first, 3), steps_of(&second, 9)].concat();
    assert_eq!(w.at_checkouts(), want);
}

#[test]
fn co_checkouts_are_valid_or_refused() {
    // spec: specs/ap/deliver.md#R1
    let long = format!("/{}", "a".repeat(4096));
    let many: Vec<String> = (0..21).map(|n| format!("/co/{n}")).collect();
    let bad = [
        json!([]),
        json!("/a"),
        json!(null),
        json!({"a": 1}),
        json!([1]),
        json!([null]),
        json!(many),
        json!(["a/b"]),
        json!([""]),
        json!(["/a/../b"]),
        json!(["/a/.."]),
        json!(["/a/./b"]),
        json!(["/."]),
        json!(["/a/"]),
        json!(["/"]),
        json!(["/a\nb"]),
        json!(["/a\u{0}b"]),
        json!(["/a\u{7f}b"]),
        json!(["/a", "/b", "/a"]),
        json!([long]),
    ];
    for b in bad {
        let mut w = W::new();
        let mut r = w.request();
        r["checkouts"] = b.clone();
        let (v, exit) = w.run_with(&r);
        assert_eq!((end(&v), exit), (("refused", "invalid_input"), 2), "{b}");
        assert_eq!(v["detail"], json!({"field": "checkouts"}), "{b}");
        assert!(v.get("checkouts").is_none(), "{b}");
        assert!(w.last().is_none() && w.calls.is_empty(), "{b}");
    }
    // 20 distinct paths, one of exactly 4,096 bytes, are admitted, in the request's order.
    let mut w = W::new();
    let mut cos: Vec<Co> = (0..19)
        .map(|n| Co::new(w.co_path(&n.to_string())))
        .collect();
    cos.push(Co::new(format!("/{}", "a".repeat(4095))));
    let paths: Vec<String> = cos.iter().map(|co| co.path.clone()).collect();
    let r = w.with_checkouts(cos);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let got: Vec<&str> = v["checkouts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(got, paths);
    // Every other request member is still checked, and the timer's form admits the member.
    let mut w = W::new();
    let mut r = w.timer();
    r["checkouts"] = json!([w.co_path("a")]);
    w.cos = vec![Co::new(w.co_path("a"))];
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["checkouts"][0]["result"], "forwarded");
}

// ---- the real `git` on a scratch bare origin ------------------------------------------------------

/// `git <words>` in `dir` with an identity of its own and no configuration of the host; its output.
fn plain(dir: &str, words: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-C",
            dir,
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(words)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {words:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
/// A bare origin, a seed that pushes to it and a clone of it, all under `W.t`; the origin then
/// holds one commit more than the clone.
struct Scratch {
    origin: String,
    seed: String,
    clone: String,
}
fn scratch(w: &W) -> Scratch {
    let root = std::fs::canonicalize(&w.t).unwrap();
    let at = |n: &str| root.join(n).to_str().unwrap().to_owned();
    let s = Scratch {
        origin: at("origin.git"),
        seed: at("seed"),
        clone: at("co/clone"),
    };
    std::fs::create_dir_all(&s.seed).unwrap();
    std::fs::create_dir_all(&s.origin).unwrap();
    plain(&s.origin, &["init", "--quiet", "--bare", "-b", "main"]);
    plain(&s.seed, &["init", "--quiet", "-b", "main"]);
    std::fs::write(format!("{}/f.txt", s.seed), "one\n").unwrap();
    plain(&s.seed, &["add", "f.txt"]);
    plain(&s.seed, &["commit", "--quiet", "-m", "one"]);
    plain(&s.seed, &["remote", "add", "origin", &s.origin]);
    plain(&s.seed, &["push", "--quiet", "origin", "main"]);
    plain(&at("."), &["clone", "--quiet", &s.origin, &s.clone]);
    advance(&s);
    s
}
/// One more commit on the origin's `main`, in the seed.
fn advance(s: &Scratch) {
    std::fs::write(format!("{}/g.txt", s.seed), "two\n").unwrap();
    plain(&s.seed, &["add", "g.txt"]);
    plain(&s.seed, &["commit", "--quiet", "-m", "two"]);
    plain(&s.seed, &["push", "--quiet", "origin", "main"]);
}
fn real_call(w: &mut W, s: &Scratch) -> Value {
    w.real_git = true;
    let mut r = w.request();
    r["checkouts"] = json!([s.clone]);
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    v["checkouts"][0].clone()
}
fn rev(dir: &str, name: &str) -> String {
    plain(dir, &["rev-parse", "--verify", name])
}

#[test]
fn co_real_git_forwards_to_the_origin() {
    // spec: specs/ap/deliver.md#R3
    let mut w = W::new();
    let s = scratch(&w);
    let before = rev(&s.clone, "HEAD");
    let row = real_call(&mut w, &s);
    let tip = rev(&s.origin, "refs/heads/main");
    assert_eq!(
        row,
        json!({"path": s.clone, "from": before, "to": tip, "result": "forwarded"})
    );
    assert_eq!(rev(&s.clone, "HEAD"), tip);
    assert_eq!(
        std::fs::read(format!("{}/g.txt", s.clone)).unwrap(),
        b"two\n"
    );
    assert_eq!(
        plain(&s.clone, &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
    // The next run reads it current.
    let row = real_call(&mut W::new(), &s);
    assert_eq!(row["result"], "current");
}

#[test]
fn co_real_git_leaves_a_dirty_checkout_byte_equal() {
    // spec: specs/ap/deliver.md#R5
    let mut w = W::new();
    let s = scratch(&w);
    let file = format!("{}/f.txt", s.clone);
    std::fs::write(&file, "edited\n").unwrap();
    let (head, remote) = (
        rev(&s.clone, "HEAD"),
        rev(&s.clone, "refs/remotes/origin/main"),
    );
    let row = real_call(&mut w, &s);
    assert_eq!(row["result"], "dirty");
    assert_eq!((&row["from"], &row["to"]), (&Value::Null, &Value::Null));
    assert_eq!(std::fs::read(&file).unwrap(), b"edited\n");
    assert_eq!(rev(&s.clone, "HEAD"), head);
    // No fetch ran: step 4 comes after step 3.
    assert_eq!(rev(&s.clone, "refs/remotes/origin/main"), remote);
    assert_eq!(plain(&s.clone, &["status", "--porcelain"]), "M f.txt");
}

#[test]
fn co_real_git_keeps_a_local_commit_when_diverged() {
    // spec: specs/ap/deliver.md#R5
    let mut w = W::new();
    let s = scratch(&w);
    std::fs::write(format!("{}/h.txt", s.clone), "local\n").unwrap();
    plain(&s.clone, &["add", "h.txt"]);
    plain(&s.clone, &["commit", "--quiet", "-m", "local"]);
    let local = rev(&s.clone, "HEAD");
    let row = real_call(&mut w, &s);
    assert_eq!(
        row,
        json!({"path": s.clone, "from": local, "to": null, "result": "diverged"})
    );
    assert_eq!(rev(&s.clone, "HEAD"), local);
    assert_eq!(rev(&s.clone, "refs/heads/main"), local);
    assert!(std::fs::metadata(format!("{}/g.txt", s.clone)).is_err());
    // The fetch of step 4 ran: the remote-tracking ref is the origin's.
    assert_eq!(
        rev(&s.clone, "refs/remotes/origin/main"),
        rev(&s.origin, "refs/heads/main")
    );
}

#[test]
fn co_real_git_leaves_a_detached_head_and_a_blocked_merge_alone() {
    // spec: specs/ap/deliver.md#R5
    let mut w = W::new();
    let s = scratch(&w);
    let head = rev(&s.clone, "HEAD");
    plain(&s.clone, &["checkout", "--quiet", "--detach"]);
    let row = real_call(&mut w, &s);
    assert_eq!(row["result"], "not_on_main");
    assert_eq!(rev(&s.clone, "HEAD"), head);
    assert_eq!(
        plain(&s.clone, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD"
    );
    plain(&s.clone, &["checkout", "--quiet", "main"]);
    // An untracked file the fast-forward would overwrite: the merge refuses, nothing is written.
    let blocking = format!("{}/g.txt", s.clone);
    std::fs::write(&blocking, "mine\n").unwrap();
    let row = real_call(&mut W::new(), &s);
    assert_eq!(row["result"], "git_failed");
    assert_eq!((&row["from"], &row["to"]), (&json!(head), &json!(head)));
    assert_eq!(std::fs::read(&blocking).unwrap(), b"mine\n");
    assert_eq!(rev(&s.clone, "HEAD"), head);
    // A lock file of the index: the merge cannot start, and the lock is not removed.
    std::fs::remove_file(&blocking).unwrap();
    let lock = format!("{}/.git/index.lock", s.clone);
    std::fs::write(&lock, "").unwrap();
    let row = real_call(&mut W::new(), &s);
    assert_eq!(row["result"], "git_failed");
    assert_eq!(rev(&s.clone, "HEAD"), head);
    assert!(std::fs::metadata(&lock).is_ok());
    assert!(std::fs::metadata(format!("{}/g.txt", s.clone)).is_err());
}

fn without_at(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("at");
    v
}

// spec: specs/ap/deliver.md#R8
// spec: specs/ap/deliver.md#R9
#[test]
fn r8_check_passes_today_s_request_and_is_refused_as_deliver_refuses() {
    for timer in [false, true] {
        let w = W::new();
        let r = if timer { w.timer() } else { w.request() };
        assert_eq!(check_request(&r), (json!({"status": "ok"}), 0));
        assert!(!w.t.join("state").exists());
        assert!(w.calls.is_empty());
    }
    // Each refusal is the record `execute` prints for the request, but for `at`.
    let w = W::new();
    let mut unknown = w.request();
    unknown["extra"] = json!(1);
    let mut missing = w.request();
    missing.as_object_mut().unwrap().remove("url");
    for r in [unknown, missing, json!([1]), Value::Null] {
        let (checked, exit) = check_request(&r);
        let mut fresh = W::new();
        let (ran, ran_exit) = fresh.run_with(&r);
        assert_eq!((exit, ran_exit), (2, 2), "{r}");
        assert_eq!(without_at(checked.clone()), without_at(ran), "{r}");
        assert_eq!(
            (end(&checked), checked["detail"].is_object()),
            (("refused", "invalid_input"), true)
        );
        assert!(fresh.calls.is_empty());
    }
    assert!(!w.t.join("state").exists());
}

fn ap_deliver(args: &[&str], stdin: &[u8], lock: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_ap"))
        .arg("deliver")
        .args(args)
        .env("AP_DELIVER_LOCK", lock)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

// spec: specs/ap/deliver.md#R8
// spec: specs/ap/deliver.md#R9
// spec: specs/ap/deliver.md#R10
#[test]
fn r8_the_command_checks_makes_nothing_and_leaves_the_other_forms_alone() {
    let w = W::new();
    let lock = w.p("check.lock");
    let r = w.request();
    let body = r.to_string();
    let out = ap_deliver(&["check"], body.as_bytes(), &lock);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "{\"status\":\"ok\"}\n"
    );
    // Nothing is made: no state directory (so no `last.json`), no lock file.
    assert!(!w.t.join("state").exists());
    assert!(!std::path::Path::new(&lock).exists());
    // A refused request: exit 2 and the record of `ap deliver` for it.
    let mut bad = r.clone();
    bad["extra"] = json!(1);
    let body = bad.to_string();
    let checked = ap_deliver(&["check"], body.as_bytes(), &lock);
    let plain = ap_deliver(&[], body.as_bytes(), &lock);
    assert_eq!(
        (checked.status.code(), plain.status.code()),
        (Some(2), Some(2))
    );
    let (a, b): (Value, Value) = (
        serde_json::from_slice(&checked.stdout).unwrap(),
        serde_json::from_slice(&plain.stdout).unwrap(),
    );
    assert_eq!(without_at(a.clone()), without_at(b));
    assert_eq!(
        (a["code"].as_str(), a["detail"].clone()),
        (Some("invalid_input"), json!({"field": "extra"}))
    );
    // Standard input that is not JSON is refused as `stdin`; another argument as `args`.
    let out = ap_deliver(&["check"], b"{", &lock);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        (out.status.code(), v["detail"].clone()),
        (Some(2), json!({"field": "stdin"}))
    );
    for args in [&["check", "x"][..], &["x"], &["check", "check"]] {
        let out = ap_deliver(args, body.as_bytes(), &lock);
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(v["detail"], json!({"field": "args"}), "{args:?}");
    }
    assert!(!w.t.join("state").exists());
    assert!(!std::path::Path::new(&lock).exists());
}

// ---- The request and the service (specs/ap/deliver.md) ------------------------------------------

// spec: specs/ap/deliver.md#R13
#[test]
fn deliver_r13_the_clone_may_not_take_a_name_of_the_state_directory() {
    for name in [".", "..", "target", "last.json"] {
        let mut w = W::new();
        let mut r = w.request();
        r["repository"] = json!(format!("o/{name}"));
        let (v, exit) = w.run_with(&r);
        assert_eq!(
            (v["code"].as_str(), exit, v["detail"].clone()),
            (Some("invalid_input"), 2, json!({"field": "repository"})),
            "{name}"
        );
        assert!(w.calls.is_empty(), "{name}");
    }
}

// spec: specs/ap/deliver.md#R36
#[test]
fn deliver_r36_the_service_counts_exit_2_as_no_failure_of_the_unit() {
    let unit = include_str!("../../../deploy/ap-deliver.service");
    let success: Vec<_> = unit
        .lines()
        .filter(|l| l.starts_with("SuccessExitStatus="))
        .collect();
    assert_eq!(success, ["SuccessExitStatus=2"]);
}

// ---- U the clone's tree and the unit files (specs/ap/deliver.md R47 to R52) ---------------------

/// The positions in `calls` of the clone's `git checkout`s.
fn checkouts(w: &W, clone: &str) -> Vec<(usize, String)> {
    w.calls
        .iter()
        .enumerate()
        .filter(|(_, (p, a, _))| p == "git" && a[0] == "-C" && a[1] == clone && a[2] == "checkout")
        .map(|(n, (_, a, _))| (n, a.last().unwrap().clone()))
        .collect()
}
/// The record's rows of `units`: (name, result, enabled).
fn unit_rows(v: &Value) -> Vec<(String, String, Value)> {
    v["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let keys: Vec<&String> = r.as_object().unwrap().keys().collect();
            assert_eq!(
                keys,
                ["after", "before", "enabled", "name", "result"],
                "{r}"
            );
            (
                r["name"].as_str().unwrap().to_owned(),
                r["result"].as_str().unwrap().to_owned(),
                r["enabled"].clone(),
            )
        })
        .collect()
}
fn urow(name: &str, result: &str, enabled: Value) -> (String, String, Value) {
    (name.into(), result.into(), enabled)
}
/// The names under `AP_UNIT_DIR`, sorted.
fn host_names(w: &W) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(w.p("units"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

// spec: specs/ap/deliver.md#R47
#[test]
fn u1_a_refusal_of_steps_3_to_7_makes_no_checkout() {
    let setups: [fn(&mut W); 6] = [
        |w| w.reviews.retain(|r| r["head"] != h(2)),
        |w| w.pin = "f".repeat(40),
        |w| w.tip_schema = 9,
        |w| w.free = 10,
        |w| w.build_busy = true,
        |w| {
            w.closed("forward", &c(2), &c(0), "installed", "timer:1");
        },
    ];
    for (n, setup) in setups.iter().enumerate() {
        let mut w = W::new();
        setup(&mut w);
        let r = if n == 5 { w.timer() } else { w.request() };
        let (v, _) = w.run_with(&r);
        assert_eq!(v["end"], "refused", "{n} {v}");
        assert_eq!(checkouts(&w, &w.p("state/r")), [], "{n} {v}");
        assert_eq!(v["units"], Value::Null, "{n}");
    }
}

// spec: specs/ap/deliver.md#R47
// spec: specs/ap/deliver.md#R28
#[test]
fn u1_the_tip_is_checked_out_after_its_chain_and_before_the_first_script() {
    let mut w = W::new();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let co = checkouts(&w, &w.p("state/r"));
    assert_eq!(co.len(), 1);
    assert_eq!(co[0].1, c(2));
    let last_review = w
        .calls
        .iter()
        .rposition(|(p, a, b)| p == "ont" && a[0] == "search" && b["object_type"] == "review")
        .unwrap();
    let first_launch = w.calls.iter().position(|(p, _, _)| p == "bash").unwrap();
    let flock = w.calls.iter().position(|(p, _, _)| p == "flock").unwrap();
    assert!(last_review < co[0].0 && flock < co[0].0 && co[0].0 < first_launch);
    assert!(co[0].0 < w.at("start_install").unwrap());
    // The handle's `rollback-check` runs from that tree too.
    assert!(!w.checks_at.is_empty() && w.checks_at.iter().all(|n| *n > co[0].0));
    // A checkout that fails is `fetch_failed`, before any attempt.
    let mut w = W::new();
    w.checkout_fails = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "fetch_failed");
    assert!(w.applies("start_install").is_empty());
}

// spec: specs/ap/deliver.md#R47
// spec: specs/ap/deliver.md#R33
#[test]
fn u1_a_rollback_checks_c_out_before_rollback_check() {
    let mut w = W::new();
    installed_by_a(&mut w);
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let co = checkouts(&w, &w.p("state/r"));
    assert_eq!(co.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), [c(2)]);
    assert!(!w.checks_at.is_empty() && w.checks_at.iter().all(|n| *n > co[0].0));
    let first_launch = w.calls.iter().position(|(p, _, _)| p == "bash").unwrap();
    assert!(co[0].0 < first_launch);
    // A checkout that fails: `fetch_failed`, no attempt.
    let mut w = W::new();
    installed_by_a(&mut w);
    w.checkout_fails = true;
    let (v, exit) = rolls(&mut w);
    refused(&w, &v, exit, "fetch_failed");
    assert!(w.checks.is_empty());
}

// spec: specs/ap/deliver.md#R47
// spec: specs/ap/deliver.md#R21
#[test]
fn u1_a_dead_call_is_read_and_repaired_from_its_commit() {
    // At its commit: the close's `rollback-check` runs from the tree of that commit.
    let mut w = W::new();
    w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    tarball_at(&w, "100500Z-3", Some(0));
    let (v, _) = w.run();
    assert_eq!(v["closed"]["outcome"], "installed", "{v}");
    let co = checkouts(&w, &w.p("state/r"));
    assert_eq!(co[0].1, c(2));
    assert!(!w.checks_at.is_empty() && co[0].0 < w.checks_at[0]);
    // At neither: the repair checks the attempt's commit out before its first script.
    let mut w = W::new();
    dead_forward(&mut w, Some(2), Some(0));
    let (v, _) = w.run();
    let co = checkouts(&w, &w.p("state/r"));
    let first_launch = w.calls.iter().position(|(p, _, _)| p == "bash").unwrap();
    assert_eq!(co[0].1, c(2), "{v}");
    assert!(co[0].0 < first_launch);
    // A checkout that fails there: `attempt_running` with the attempt.
    let mut w = W::new();
    let id = dead_forward(&mut w, Some(2), Some(0));
    w.checkout_fails = true;
    let (v, _) = w.run();
    assert_eq!(end(&v), ("refused", "attempt_running"), "{v}");
    assert_eq!(v["detail"], json!({"install": id}));
    assert!(w.launched().is_empty());
}

// spec: specs/ap/deliver.md#R47
// spec: specs/ap/deliver.md#R43
#[test]
fn u1_the_pin_is_checked_out_after_its_chain() {
    let mut w = W::engine();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let co = checkouts(&w, &w.p("state/engine"));
    assert_eq!(co.len(), 1);
    assert_eq!(co[0].1, ec(2));
    let last_review = w
        .calls
        .iter()
        .rposition(|(p, a, b)| p == "ont" && a[0] == "search" && b["object_type"] == "review")
        .unwrap();
    let launch = w.calls.iter().position(|(p, _, _)| p == "bash").unwrap();
    assert!(last_review < co[0].0 && co[0].0 < launch);
    // The clone of the repository is at T, accepted, before the marker and the engine (R42).
    let own = checkouts(&w, &w.p("state/r"));
    assert_eq!(own.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), [c(2)]);
    // `engine_not_accepted` leaves the engine clone's HEAD where it was.
    let mut w = W::engine();
    w.reviews.retain(|r| r["head"] != eh(2));
    let (v, _) = w.run();
    assert_eq!(end(&v), ("refused", "engine_not_accepted"), "{v}");
    assert_eq!(checkouts(&w, &w.p("state/engine")), []);
    assert_eq!(v["units"], Value::Null);
}

/// Three merges, the pair at c(1): the unit step goes from c(1) to c(3), and c(0) is older than its
/// base on the first-parent chain.
fn from_one() -> W {
    let mut w = W::with(3);
    (w.file, w.tree, w.bytes) = (Some(c(1)), Some(c(1)), binary(&c(1)));
    w
}

// spec: specs/ap/deliver.md#R48
// spec: specs/ap/deliver.md#R49
// spec: specs/ap/deliver.md#R51
#[test]
fn u2_each_result_of_one_file() {
    let mut w = from_one();
    w.unit(2..=3, "a.timer", "A new\n");
    w.unit(0..=3, "b.service", "B\n");
    w.unit(0..=1, "c.service", "C base\n");
    w.unit(2..=3, "c.service", "C new\n");
    w.unit([0], "d.service", "D old\n");
    w.unit([1], "d.service", "D base\n");
    w.unit(2..=3, "d.service", "D new\n");
    w.unit(0..=1, "e.service", "E\n");
    w.unit(2..=3, "e.service", "E new\n");
    w.unit(0..=3, "f.timer", "F\n");
    w.unit(0..=3, "g.timer", "G\n");
    // Entries that are no unit files: a link, a directory, other files, a name out of the set.
    let tip = w.deploy.get_mut(&c(3)).unwrap();
    tip.insert("h.timer".into(), ("120000", "blob", b"a.timer".to_vec()));
    tip.insert("i.timer".into(), ("040000", "tree", vec![]));
    tip.insert(
        "README.md".into(),
        ("100644", "blob", b"# deploy\n".to_vec()),
    );
    tip.insert("deliver.json".into(), ("100644", "blob", b"{}\n".to_vec()));
    tip.insert("j k.service".into(), ("100644", "blob", b"J\n".to_vec()));
    tip.insert("l.timer.d".into(), ("100644", "blob", b"L\n".to_vec()));
    std::fs::write(w.host("b.service"), "B\n").unwrap();
    std::fs::write(w.host("c.service"), "C base\n").unwrap();
    std::fs::write(w.host("d.service"), "D old\n").unwrap();
    std::fs::write(w.host("e.service"), "E by hand\n").unwrap();
    std::os::unix::fs::symlink(w.host("b.service"), w.host("f.timer")).unwrap();
    std::fs::create_dir(w.host("g.timer")).unwrap();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("installed", "units_host_differs"), 0),
        "{v}"
    );
    assert_eq!(
        (v["outcome"].clone(), v["to"].clone()),
        (json!("installed"), json!(c(3)))
    );
    let close = &w.applies("close_install")[0]["parameters"];
    assert_eq!(close["code"], "units_host_differs");
    assert_eq!(close["outcome"], "installed");
    let t = json!(true);
    assert_eq!(
        unit_rows(&v),
        [
            urow("a.timer", "written", t),
            urow("b.service", "same", Value::Null),
            urow("c.service", "written", Value::Null),
            urow("d.service", "written", Value::Null),
            urow("e.service", "host_differs", Value::Null),
            urow("f.timer", "host_differs", Value::Null),
            urow("g.timer", "host_differs", Value::Null),
        ]
    );
    let read = |n: &str| std::fs::read_to_string(w.host(n)).unwrap();
    assert_eq!(
        [
            read("a.timer"),
            read("c.service"),
            read("d.service"),
            read("e.service")
        ],
        ["A new\n", "C new\n", "D new\n", "E by hand\n"]
    );
    let mode = std::fs::metadata(w.host("a.timer")).unwrap();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&mode.permissions()) & 0o777,
        0o644
    );
    assert!(
        std::fs::symlink_metadata(w.host("f.timer"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(std::fs::metadata(w.host("g.timer")).unwrap().is_dir());
    // Nothing else is made: no temporary name stays, no entry that is no unit file.
    assert_eq!(
        host_names(&w),
        [
            "a.timer",
            "b.service",
            "c.service",
            "d.service",
            "e.service",
            "f.timer",
            "g.timer"
        ]
    );
    let units = v["units"].as_array().unwrap();
    let digest = |s: &str| json!(sha(s.as_bytes()));
    assert_eq!(
        (&units[0]["before"], &units[0]["after"]),
        (&Value::Null, &digest("A new\n"))
    );
    assert_eq!(
        (&units[2]["before"], &units[2]["after"]),
        (&digest("C base\n"), &digest("C new\n"))
    );
    assert_eq!(
        (&units[4]["before"], &units[4]["after"]),
        (&digest("E by hand\n"), &digest("E by hand\n"))
    );
    assert_eq!(
        (&units[5]["before"], &units[6]["after"]),
        (&Value::Null, &Value::Null)
    );
    assert_eq!(
        w.systemctl(),
        [
            "daemon-reload",
            "enable --now a.timer",
            "is-enabled a.timer"
        ]
    );
    assert_eq!(w.last().as_ref(), Some(&v));
}

// spec: specs/ap/deliver.md#R49
#[test]
fn u2_a_file_whose_history_the_clone_cannot_answer_is_failed() {
    let mut w = W::new();
    w.unit(0..=2, "a.service", "A\n");
    std::fs::write(w.host("a.service"), "A by hand\n").unwrap();
    // The bytes of the tip cannot be read: failed, the host's copy untouched.
    w.deploy
        .get_mut(&c(2))
        .unwrap()
        .get_mut("a.service")
        .unwrap()
        .1 = "commit";
    w.deploy
        .get_mut(&c(2))
        .unwrap()
        .get_mut("a.service")
        .unwrap()
        .0 = "160000";
    let (v, exit) = w.run();
    // A gitlink is no unit file: no row at all.
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(unit_rows(&v), []);
    // `git show` of the tip's file fails.
    let mut w = W::new();
    w.unit(0..=2, "a.service", "A\n");
    w.unit([2], "a.service", "A new\n");
    w.lost_units = vec!["a.service".into()];
    std::fs::write(w.host("a.service"), "A\n").unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", "units_failed"), 0), "{v}");
    assert_eq!(unit_rows(&v), [urow("a.service", "failed", Value::Null)]);
    assert_eq!(std::fs::read_to_string(w.host("a.service")).unwrap(), "A\n");
    assert_eq!(w.systemctl(), Vec::<String>::new());
}

// spec: specs/ap/deliver.md#R50
#[test]
fn u3_the_calls_of_the_service_manager_and_their_order() {
    // A timer new to the host; a changed timer; a changed service alone; nothing to write.
    let cases: [Case; 4] = [
        (
            |w| w.unit(1..=2, "n.timer", "N\n"),
            &[
                "daemon-reload",
                "enable --now n.timer",
                "is-enabled n.timer",
            ],
        ),
        (
            |w| {
                w.unit([0], "t.timer", "T0\n");
                w.unit(1..=2, "t.timer", "T2\n");
                std::fs::write(w.host("t.timer"), "T0\n").unwrap();
            },
            &["daemon-reload", "try-restart t.timer"],
        ),
        (
            |w| {
                w.unit([0], "s.service", "S0\n");
                w.unit(1..=2, "s.service", "S2\n");
                std::fs::write(w.host("s.service"), "S0\n").unwrap();
            },
            &["daemon-reload"],
        ),
        (
            |w| {
                w.unit(0..=2, "t.timer", "T\n");
                w.unit(0..=2, "s.service", "S\n");
                std::fs::write(w.host("t.timer"), "T\n").unwrap();
                std::fs::write(w.host("s.service"), "S\n").unwrap();
            },
            &[],
        ),
    ];
    for (n, (setup, want)) in cases.iter().enumerate() {
        let mut w = W::new();
        setup(&mut w);
        let before = host_names(&w);
        let (v, exit) = w.run();
        assert_eq!((end(&v), exit), (("installed", ""), 0), "{n} {v}");
        assert_eq!(w.systemctl(), *want, "{n}");
        // The service manager is called after the files are written.
        if let Some(reload) = w.calls.iter().position(|(p, _, _)| p == "systemctl") {
            assert!(
                w.calls
                    .iter()
                    .skip(reload)
                    .all(|(p, a, _)| p != "git" || a[2] != "show")
            );
        }
        if want.is_empty() {
            assert_eq!(host_names(&w), before);
            let same = unit_rows(&v).iter().all(|r| r.1 == "same");
            assert!(same, "{v}");
        }
    }
}

// spec: specs/ap/deliver.md#R51
// spec: specs/ap/deliver.md#R30
// spec: specs/ap/deliver.md#R34
// spec: specs/ap/deliver.md#R36
#[test]
fn u4_the_codes_change_neither_the_end_nor_the_exit() {
    let failing: [fn(&mut W); 4] = [
        |w| w.systemctl_fails = vec!["daemon-reload".into()],
        |w| w.not_enabled = vec!["n.timer".into()],
        |w| w.units_fail = true,
        |w| std::fs::remove_dir(w.p("units")).unwrap(),
    ];
    for (n, setup) in failing.iter().enumerate() {
        let mut w = W::new();
        w.unit(1..=2, "n.timer", "N\n");
        setup(&mut w);
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit),
            (("installed", "units_failed"), 0),
            "{n} {v}"
        );
        assert_eq!(v["outcome"], "installed");
        assert_eq!(
            w.applies("close_install")[0]["parameters"]["code"],
            "units_failed"
        );
        let rows = unit_rows(&v);
        match n {
            0 => {
                assert_eq!(rows, [urow("n.timer", "written", Value::Null)]);
                assert_eq!(w.systemctl(), ["daemon-reload"]);
            }
            1 => assert_eq!(rows, [urow("n.timer", "written", json!(false))]),
            2 => {
                assert_eq!(rows, []);
                assert_eq!(w.systemctl(), Vec::<String>::new());
            }
            _ => {
                assert_eq!(rows, [urow("n.timer", "failed", Value::Null)]);
                assert_eq!(v["units"][0]["after"], Value::Null);
                assert_eq!(w.systemctl(), Vec::<String>::new());
                assert!(!w.t.join("units").exists());
            }
        }
    }
    // An installer's code comes first.
    let mut w = W::new();
    w.same_content = true;
    let content = w.inst.get_mut("content").unwrap();
    (content.tree, content.exit, content.out) = (None, 2, String::new());
    w.unit(1..=2, "n.timer", "N\n");
    w.not_enabled = vec!["n.timer".into()];
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("installed", "content_refused"), 0),
        "{v}"
    );
    assert_eq!(unit_rows(&v), [urow("n.timer", "written", json!(false))]);
    // No step at another end: `failed_restored`, `current`, a refusal, a dead call's close.
    let others: [fn(&mut W); 3] = [
        |w| w.inst.get_mut("content").unwrap().tree = None,
        |w| (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2))),
        |w| {
            w.dead_attempt(&c(2), &c(0), &sha(&binary(&c(0))));
            (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
        },
    ];
    for (n, setup) in others.iter().enumerate() {
        let mut w = W::new();
        w.unit(1..=2, "n.timer", "N\n");
        setup(&mut w);
        let (v, _) = w.run();
        assert_ne!(v["end"], "installed", "{n} {v}");
        assert_eq!(v["units"], Value::Null, "{n}");
        assert_eq!(w.systemctl(), Vec::<String>::new());
        assert_eq!(host_names(&w), Vec::<String>::new());
    }
}

// spec: specs/ap/deliver.md#R48
// spec: specs/ap/deliver.md#R49
// spec: specs/ap/deliver.md#R50
#[test]
fn u5_a_rollback_restores_and_removes() {
    let setup = |w: &mut W| {
        installed_by_a(w);
        w.unit([0], "x.timer", "X0\n");
        w.unit(1..=2, "x.timer", "X2\n");
        w.unit(0..=2, "y.service", "Y\n");
        w.unit(1..=2, "z.timer", "Z\n");
        w.unit([2], "v.service", "V\n");
        w.unit([2], "w.timer", "W\n");
        for (n, b) in [
            ("x.timer", "X2\n"),
            ("y.service", "Y\n"),
            ("z.timer", "Z\n"),
            ("w.timer", "W by hand\n"),
        ] {
            std::fs::write(w.host(n), b).unwrap();
        }
    };
    let mut w = W::new();
    setup(&mut w);
    let (v, exit) = rolls(&mut w);
    assert_eq!(
        (end(&v), exit),
        (("installed", "units_host_differs"), 0),
        "{v}"
    );
    assert_eq!(v["to"], c(0));
    assert_eq!(
        unit_rows(&v),
        [
            urow("v.service", "same", Value::Null),
            urow("w.timer", "host_differs", Value::Null),
            urow("x.timer", "written", Value::Null),
            urow("y.service", "same", Value::Null),
            urow("z.timer", "removed", Value::Null),
        ]
    );
    assert_eq!(
        w.systemctl(),
        [
            "disable --now z.timer",
            "daemon-reload",
            "try-restart x.timer"
        ]
    );
    assert_eq!(std::fs::read_to_string(w.host("x.timer")).unwrap(), "X0\n");
    assert_eq!(host_names(&w), ["w.timer", "x.timer", "y.service"]);
    assert_eq!(v["units"][4]["after"], Value::Null);
    // A timer whose `disable --now` fails keeps its file.
    let mut w = W::new();
    setup(&mut w);
    w.systemctl_fails = vec!["disable --now z.timer".into()];
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", "units_failed"), 0), "{v}");
    assert_eq!(unit_rows(&v)[4], urow("z.timer", "failed", Value::Null));
    assert_eq!(
        host_names(&w),
        ["w.timer", "x.timer", "y.service", "z.timer"]
    );
    assert_eq!(
        w.systemctl(),
        [
            "disable --now z.timer",
            "daemon-reload",
            "try-restart x.timer"
        ]
    );
}

/// The lines every service of `deploy/` carries in `[Service]` (R52).
const CONFINEMENT: [&str; 10] = [
    "NoNewPrivileges=yes",
    "ProtectKernelModules=yes",
    "ProtectKernelLogs=yes",
    "ProtectKernelTunables=yes",
    "ProtectControlGroups=yes",
    "ProtectHostname=yes",
    "LockPersonality=yes",
    "RestrictRealtime=yes",
    "RestrictSUIDSGID=yes",
    "SystemCallArchitectures=native",
];

// spec: specs/ap/deliver.md#R52
#[test]
fn u6_every_service_of_deploy_is_confined() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if !name.ends_with(".service") {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let mut section = "";
        let mut service = vec![];
        for l in text.lines() {
            if l.starts_with('[') {
                section = l;
            } else if section == "[Service]" {
                service.push(l);
            }
        }
        let count = |line: &str| text.lines().filter(|l| *l == line).count();
        for line in CONFINEMENT {
            assert_eq!(count(line), 1, "{name}: {line}");
            assert!(service.contains(&line), "{name}: {line} outside [Service]");
        }
        let full = usize::from(name != "ap-deliver.service");
        assert_eq!(count("ProtectSystem=full"), full, "{name}");
        assert_eq!(full == 1, service.contains(&"ProtectSystem=full"), "{name}");
        assert!(
            !text
                .lines()
                .any(|l| l.starts_with("ProtectSystem=") && l != "ProtectSystem=full")
        );
    }
    assert!(seen >= 7);
}
