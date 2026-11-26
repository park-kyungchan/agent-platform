//! `ap deliver` against a fake host (README "Delivery"). The fake `System` is a small model of the
//! world the command reads: the clone (a first-parent chain of merges, each with a second parent
//! and a tree), the pair (the commit `ap --version` of the installed file names and the commit
//! whose content the tree holds), the store (reviews and install attempts, searched by `all` of
//! `eq`), `df`, `flock`, and the installers, which change the pair as the case says and print
//! what the case says from a real child. Every call is recorded in order. No test builds,
//! installs, fetches or touches the host's store; the lock, the backup directories, the reports
//! and the state directory are files of a temporary directory.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
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
    /// The same for each call of a program that is no launch: none of them inherits the lock.
    call_inherited: Vec<bool>,
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
    /// Whether the request still carries the member `engine`, which is accepted and ignored (R14).
    engine: bool,
    /// What `install-ont.sh install` and `rollback` leave the engine at (None: unchanged), and
    /// their exits.
    ont_install: (Option<String>, i32),
    ont_rollback: (Option<String>, i32),
    /// The mode of `ontology/migrate` at the tip (None: absent), its exit, and each run: its
    /// arguments and the marker as it was then.
    migrate_mode: Option<&'static str>,
    /// `deploy/roles.json` at the tip, and whether the clone answers for it.
    roles_blob: Option<&'static str>,
    roles_git_fails: bool,
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
    /// The configuration files (`specs/ap/deliver.md` R53 to R57): whether the request carries `etc`,
    /// the operations repository's chain and heads, the clone's `HEAD`, its fetch, its checkout, a
    /// search of its reviews that fails, and what each run of `install-etc.sh` exits and prints on
    /// stderr, in order.
    etc: bool,
    xchain: Vec<Commit>,
    xheads: BTreeMap<String, String>,
    xhead: Option<String>,
    xfetch_fails: bool,
    xcheckout_fails: bool,
    xsearch_fails: bool,
    xruns: Vec<(i32, String)>,
    /// `etc` names the delivered repository itself (R71); the `HEAD` of the repository's clone, which a
    /// checkout of it moves (None: `rev-parse` fails).
    own_etc: bool,
    rhead: Option<String>,
    /// A checkout of the repository's clone fails, and nothing else does.
    rcheckout_fails: bool,
    /// PC push (`specs/ap/deliver.md` R60 to R63): optional `pc` member of request, how
    /// `tailscale ssh` behaves, local `ont` fault injection, and virtual clock for timeouts.
    pc: Option<Value>,
    pc_ssh: PcSsh,
    ont_unreadable: bool,
    ont_version_bad: bool,
    extra_env: Vec<(String, String)>,
    clock_offset: std::time::Duration,
    advance_on_pause: std::time::Duration,
    /// The engine of the tip (`specs/ap/deliver.md` R66): `pin` is the Git tree id of its `engine/`;
    /// the tip holds no `engine/`; `git ls-tree` of its `engine` fails.
    neither: bool,
    engine_ls_fails: bool,
}

#[derive(Clone)]
enum PcSsh {
    Local,
    Script(std::collections::VecDeque<(i32, String)>),
    Hang,
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
            "ont-bin",
            "ont-state/install",
            "ont-state/ont",
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
            call_inherited: vec![],
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
            ont_install: (None, 0),
            ont_rollback: (None, 0),
            migrate_mode: None,
            roles_blob: None,
            roles_git_fails: false,
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
            etc: false,
            xchain: vec![],
            xheads: BTreeMap::new(),
            xhead: None,
            xfetch_fails: false,
            xcheckout_fails: false,
            xsearch_fails: false,
            xruns: vec![],
            own_etc: false,
            rhead: None,
            rcheckout_fails: false,
            pc: None,
            pc_ssh: PcSsh::Local,
            ont_unreadable: false,
            ont_version_bad: false,
            extra_env: vec![],
            clock_offset: std::time::Duration::ZERO,
            advance_on_pause: std::time::Duration::ZERO,
            neither: false,
            engine_ls_fails: false,
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
        if self.etc {
            r["etc"] = json!({"repository": "o/ops", "url": XURL});
        }
        if self.own_etc {
            r["etc"] = json!({"repository": "o/r", "url": "https://example.invalid/other/r.git"});
        }
        if let Some(pc) = &self.pc {
            r["pc"] = pc.clone();
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
                } else if a[0].ends_with("install-etc.sh") {
                    "etc"
                } else {
                    "content"
                };
                if script == "etc" && a[1] == "check" {
                    "etc-check".into()
                } else if a[1] == "rollback" {
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
        let xclone = self.p("state/ops");
        if a[0] == "init" && a[2] == xclone {
            assert_eq!(a.len(), 3);
            assert_eq!(a[1], "--quiet");
            return Self::text(real_git(a), "");
        }
        if a[..2] == ["-C", xclone.as_str()] {
            return self.xgit(a);
        }
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
            ["checkout", "--quiet", "--force", "--detach", k] => {
                let fails = self.checkout_fails || self.rcheckout_fails;
                if !fails {
                    self.rhead = Some((*k).to_owned());
                }
                Self::text(!fails, "")
            }
            ["rev-parse", "--verify", "HEAD^{commit}"] => match &self.rhead {
                Some(h) => Self::text(true, &format!("{h}\n")),
                None => Self::text(false, ""),
            },
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
            ["ls-tree", tip, "--", "deploy/roles.json"] => {
                assert_eq!(*tip, self.tip());
                let entry = self.roles_blob.map_or(String::new(), |_| {
                    format!("100644 blob {}\tdeploy/roles.json\n", "b".repeat(40))
                });
                Self::text(!self.roles_git_fails, &entry)
            }
            ["ls-tree", tip, "--", "engine"] => {
                assert_eq!(*tip, self.tip());
                let entry = match self.neither {
                    true => String::new(),
                    false => format!("040000 tree {}\tengine\n", self.pin),
                };
                Self::text(!self.engine_ls_fails, &entry)
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
                    "deploy/roles.json" => {
                        Self::text(self.roles_blob.is_some(), self.roles_blob.unwrap_or(""))
                    }
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
                let head = &body["where"]["all"][1]["eq"][1];
                let xhead = head.as_str().is_some_and(|h| self.xheads.contains_key(h));
                if self.xsearch_fails && kind == "review" && xhead {
                    return Err(Failure {
                        code: "process_failed",
                        field: "ont".into(),
                        detail: Value::Null,
                    });
                }
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
                assert_eq!(body["role"], "deliver");
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
                    if self
                        .installs
                        .iter()
                        .any(|i| i["target"] == p["target"] && i["status"] == "running")
                    {
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
        self.call_inherited.push(inherited(&self.lock));
        if program == "bash" && a.len() == 3 && a[1] == "rollback-check" {
            self.checks.push((a[0].clone(), a[2].clone()));
            self.checks_at.push(self.calls.len());
            return Self::text(!self.old_script && !self.refused.contains(&a[2]), "");
        }
        self.calls.push((program.into(), a.to_vec(), body.clone()));
        if program == self.p("ont-bin/ont") {
            assert_eq!(a, ["--version"]);
            if self.ont_version_bad {
                return Self::text(true, "ont 0.1.0 unknown\n");
            }
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
            if self.ont_unreadable {
                return Err(std::io::ErrorKind::NotFound.into());
            }
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(bytes);
            }
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
        if argv[0] == "tailscale" {
            assert_eq!(argv.len(), 4, "tailscale argv: {argv:?}");
            assert_eq!(argv[1], "ssh");
            let [stdin, stdout, stderr] = files;
            return match &mut self.pc_ssh {
                PcSsh::Local => Command::new("sh")
                    .args(["-c", &argv[3]])
                    .stdin(stdin)
                    .stdout(stdout)
                    .stderr(stderr)
                    .spawn(),
                PcSsh::Script(q) => {
                    let (exit, out) = q.pop_front().unwrap_or((1, String::new()));
                    Command::new("sh")
                        .args([
                            "-c",
                            "printf '%s' \"$1\"; exit \"$2\"",
                            "sh",
                            &out,
                            &exit.to_string(),
                        ])
                        .env_clear()
                        .stdin(stdin)
                        .stdout(stdout)
                        .stderr(stderr)
                        .spawn()
                }
                PcSsh::Hang => Command::new("sh")
                    .args(["-c", "sleep 60"])
                    .stdin(stdin)
                    .stdout(stdout)
                    .stderr(stderr)
                    .spawn(),
            };
        }
        if argv[0].ends_with("/ontology/migrate") || argv[1].ends_with("/scripts/install-ont.sh") {
            return self.launch_engine(argv, files);
        }
        if argv[1].ends_with("/scripts/install-etc.sh") {
            return self.launch_etc(argv, files);
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
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now() + self.clock_offset
    }
    fn pause(&mut self, duration: std::time::Duration) {
        if self.advance_on_pause.is_zero() {
            std::thread::sleep(duration);
        } else {
            self.clock_offset += self.advance_on_pause;
        }
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
        .chain(self.extra_env.clone())
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

/// A failed install keeps the tail of what the failed installer wrote: `<state>/output/<key>.log`,
/// mode 0600 in a directory 0700, at most 65,536 bytes, the files beyond the 20 newest removed first;
/// the record names the path, and no text of it is in the record or the store.
// spec: specs/ap/deliver.md#R65
#[test]
fn failed_install_keeps_output() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut w = W::new();
    let dir = w.t.join("state/output");
    std::fs::create_dir_all(&dir).unwrap();
    let base = std::time::SystemTime::now() - std::time::Duration::from_secs(10_000);
    for n in 0..22u64 {
        let f = std::fs::File::create(dir.join(format!("old-{n:02}.log"))).unwrap();
        f.set_modified(base + std::time::Duration::from_secs(n * 10))
            .unwrap();
    }
    let ap = w.inst.get_mut("ap").unwrap();
    let noise = "x".repeat(70_000);
    (ap.file, ap.exit) = (None, 1);
    ap.out = "stdout line\n".into();
    ap.err = format!("{noise}error[E0432]: unresolved import\nbuild_failed@install-ap.sh:151\n");
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed_restored", "build_failed"), 3),
        "{v}"
    );
    let key = format!("act-ap.{}.1", c(2));
    let path = w.p(&format!("state/output/{key}.log"));
    assert_eq!(v["output"], json!(path), "{v}");
    assert_eq!(w.last().as_ref(), Some(&v));
    let kept = std::fs::read_to_string(&path).unwrap();
    assert_eq!(kept.len(), 65_536);
    assert!(kept.ends_with("error[E0432]: unresolved import\nbuild_failed@install-ap.sh:151\n"));
    assert!(!kept.contains("stdout line"));
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // The 20 newest of the earlier files stay, the two oldest go, and the new file is the 21st.
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 21, "{names:?}");
    assert!(!names.contains(&"old-00.log".to_owned()) && !names.contains(&"old-01.log".to_owned()));
    assert!(names.contains(&"old-02.log".to_owned()) && names.contains(&format!("{key}.log")));
    // Nothing of the text is in the record or in any call to the store.
    assert!(!v.to_string().contains("unresolved import"));
    assert!(
        w.calls
            .iter()
            .all(|(_, _, b)| !b.to_string().contains("unresolved import"))
    );
}

/// An attempt that ends installed keeps no file and has `output` null; so does a call refused before
/// its installers ran.
// spec: specs/ap/deliver.md#R65
#[test]
fn installed_keeps_no_output() {
    let mut w = W::new();
    let ap = w.inst.get_mut("ap").unwrap();
    (ap.out, ap.err) = ("compiling\n".into(), "warning: noise\n".into());
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["output"], Value::Null);
    assert!(!w.t.join("state/output").exists());
    let mut w = W::new();
    w.free = 1;
    let (v, _) = w.run();
    assert_eq!(end(&v), ("refused", "disk_low"), "{v}");
    assert_eq!(v["output"], Value::Null);
    assert!(!w.t.join("state/output").exists());
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
            w.ont_version_bad = true;
            w.request()
        }),
        ("schema_differs", |w| {
            w.tip_schema = 7;
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
    // Step 1 (R20): the host lock is held by another process, so the call is `busy` before any
    // step: no installer, no store read of the pair, no attempt applied.
    let mut w = W::new();
    let mut holder = Command::new("flock")
        .args(["-n", &w.lock, "-c", "echo held; exec sleep 60"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut held = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(holder.stdout.take().unwrap()),
        &mut held,
    )
    .unwrap();
    assert_eq!(held, "held\n", "the holder took the lock");
    let (v, exit) = w.run();
    holder.kill().unwrap();
    holder.wait().unwrap();
    assert_eq!((end(&v), exit), (("refused", "busy"), 2), "{v}");
    assert!(w.launched().is_empty(), "an installer ran: {v}");
    assert!(w.applies("start_install").is_empty(), "{v}");
    assert!(w.applies("close_install").is_empty(), "{v}");
    assert_eq!(v["installers"], json!([]));
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
// spec: specs/ap/deliver.md#R40
#[test]
fn d5_engine_differs() {
    // An engine whose revision cannot be read: refused, whatever the tip names.
    let mut w = W::new();
    w.ont_version_bad = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_differs");
    assert_eq!(
        v["detail"],
        json!({"tip": "e".repeat(40), "installed": null})
    );
    // Another revision is no refusal: the call installs the engine of the tip (R67).
    let mut w = W::new();
    w.ont = "b".repeat(40);
    w.ont_install = (Some("e".repeat(40)), 0);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(ENGINE)),
        "{v}"
    );
}

// spec: specs/ap/deliver.md#R24
// spec: specs/ap/deliver.md#R40
#[test]
fn d5_schema_differs() {
    // A lower version is refused; a higher one is installed (R44).
    let mut w = W::new();
    w.tip_schema = 7;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "schema_differs");
    assert_eq!(v["detail"], json!({"tip": 7, "installed": 8}));
    let mut w = W::new();
    w.tip_schema = 9;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["schema"]["to"], 9);
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
#[test]
fn r19_every_start_install_names_the_host_local() {
    let host = oid("host", "local");
    // The pair's forward attempt.
    let mut w = W::new();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let starts = w.applies("start_install");
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["parameters"]["host"], host);
    assert_eq!(w.installs[0]["host"], host);
    let close = &w.applies("close_install")[0]["parameters"];
    assert!(close.get("host").is_none(), "{close}");
    // A Lead's rollback.
    let mut w = W::new();
    installed_by_a(&mut w);
    let (v, exit) = rolls(&mut w);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let starts = w.applies("start_install");
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["parameters"]["kind"], "rollback");
    assert_eq!(starts[0]["parameters"]["host"], host);
    assert_eq!(w.installs.last().unwrap()["host"], host);
    assert!(
        w.applies("close_install")[0]["parameters"]
            .get("host")
            .is_none()
    );
    // The engine's attempt.
    let mut w = W::engine();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let starts = w.applies("start_install");
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["parameters"]["target"], etarget());
    assert_eq!(starts[0]["parameters"]["host"], host);
    assert_eq!(w.installs.last().unwrap()["host"], host);
    assert!(
        w.applies("close_install")[0]["parameters"]
            .get("host")
            .is_none()
    );
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
        // Step 2 on every call: the PC's running attempts of the engine, then the engine's (R61).
        search(vec![
            eq("target", etarget()),
            eq("status", json!("running")),
        ]),
        search(vec![
            eq("target", etarget()),
            eq("status", json!("running")),
        ]),
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
            json!({"action_type": "start_install", "request_id": format!("ap.{back}.rb1"), "agent": "ap-deliver", "role": "deliver",
            "parameters": {"target": target(), "kind": "rollback", "repository": "o/r", "commit": back, "attempt": 1,
                "from": here, "from_sha256": sha(&binary(&here)), "rolls_back": a, "by": "session-1",
                "reason": "after a merge", "host": oid("host", "local")}}),
        ),
        launch("install-content.sh", &w.tarball()),
        launch("install-ap.sh", &kept(&w)),
        version,
        // The unit files of A's `from` and of C (R48).
        git(&["ls-tree", &back, "deploy/"]),
        git(&["ls-tree", &here, "deploy/"]),
        apply(
            json!({"action_type": "close_install", "request_id": format!("ap.{back}.rb1.close"), "agent": "ap-deliver", "role": "deliver",
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

const MEMBERS: [&str; 22] = [
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
    "output",
    "schema",
    "roles",
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

// spec: specs/ap/deliver.md#R16
#[test]
fn d9_no_other_program_of_a_call_inherits_the_lock() {
    // Every installer launch saw the lock inheritable; no call of a program (the reads of the
    // pair, `git`, the build lock, `rollback-check`) did: the lock is close-on-exec again after
    // each launch, whatever the launch answered. A rollback installer is a launch and inherits it.
    let mut w = W::new();
    let (v, _) = w.run();
    assert_eq!(w.launched(), ["ap", "content"], "{v}");
    assert!(!w.lock_inherited.is_empty() && w.lock_inherited.iter().all(|&i| i));
    assert!(!w.call_inherited.is_empty() && w.call_inherited.iter().all(|&i| !i));
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
        // Step 2 on every call: the PC's running attempts of the engine, then the engine's (R61).
        search(
            "install",
            vec![eq("target", etarget()), eq("status", json!("running"))],
        ),
        search(
            "install",
            vec![eq("target", etarget()), eq("status", json!("running"))],
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
        // Step 5: the tree id of engine/ at T and the engine's revision (R66).
        git(&["ls-tree", &tip, "--", "engine"]),
        (w.p("ont-bin/ont"), args(&["--version"]), Value::Null),
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
        // The roles file (R58).
        git(&["ls-tree", &tip, "--", "deploy/roles.json"]),
        index.clone(),
        search(
            "install",
            vec![eq("target", target()), eq("commit", json!(tip))],
        ),
        apply(
            json!({"action_type": "start_install", "request_id": format!("ap.{tip}.1"), "agent": "ap-deliver", "role": "deliver",
            "parameters": {"target": target(), "kind": "forward", "repository": "o/r", "commit": tip, "attempt": 1,
                "from": from, "from_sha256": sha(&binary(&from)), "by": "session-1", "reason": "after a merge",
                "head": h(2), "review": w.accepting(2), "reviews": [w.accepting(1), w.accepting(2)],
                "host": oid("host", "local")}}),
        ),
        launch("install-ap.sh"),
        launch("install-content.sh"),
        version,
        git(&["show", &format!("{tip}:content/SHA256SUMS")]),
        // The unit files of T (R48): `deploy/` of the fake holds none.
        git(&["ls-tree", &tip, "deploy/"]),
        apply(
            json!({"action_type": "close_install", "request_id": format!("ap.{tip}.1.close"), "agent": "ap-deliver", "role": "deliver",
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
/// The URL of the member `engine` a request may still carry, which is ignored.
const EURL: &str = "https://example.invalid/o/engine.git";
/// A tree id of `engine/`: `ec(0)` is installed, `ec(2)` that of the tip.
fn ec(n: u32) -> String {
    format!("{:040}", 500 + n)
}
fn engine_binary(commit: &str) -> Vec<u8> {
    format!("ont binary of {commit}").into_bytes()
}
fn etarget() -> Value {
    oid("installed_artifact", ENGINE)
}
/// Real `git` for the operations clone's `init` and `config`, so that its configuration is a file a
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
    /// A request that still carries `engine` (ignored); the engine at ec(0), the tree id of `engine/` at
    /// the tip ec(2).
    fn engine() -> Self {
        let mut w = W::new();
        w.engine = true;
        for d in ["ont-bin", "ont-state/install", "ont-state/ont"] {
            std::fs::create_dir_all(w.t.join(d)).unwrap();
        }
        (w.ont, w.pin) = (ec(0), ec(2));
        w.ont_install = (Some(ec(2)), 0);
        w.ont_rollback = (Some(ec(0)), 0);
        w
    }
    /// The engine already at the tree id of the tip: only the schema and the pair may move.
    fn at_pin() -> Self {
        let mut w = W::engine();
        w.ont = ec(2);
        w
    }
    fn eclosed(&mut self, kind: &str, commit: &str, from: &str, outcome: &str) -> Value {
        let id = self.closed(kind, commit, from, outcome, "timer:1");
        let row = self.installs.last_mut().unwrap();
        (row["target"], row["repository"]) = (etarget(), json!("o/r"));
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

// spec: specs/ap/deliver.md#R67
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

// spec: specs/ap/deliver.md#R67
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

// spec: specs/ap/deliver.md#R40
#[test]
fn e_engine_refusals_before_the_first_effect() {
    // An engine whose revision cannot be read, or an entry engine of the tip that cannot be read:
    // engine_differs.
    let mut w = W::engine();
    w.ont_version_bad = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_differs");
    assert_eq!(v["detail"], json!({"tip": ec(2), "installed": null}));
    let mut w = W::engine();
    w.engine_ls_fails = true;
    let (v, exit) = w.run();
    refused(&w, &v, exit, "engine_differs");
    assert_eq!(v["detail"], json!({"tip": null, "installed": ec(0)}));
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
    // An attempt of the engine's former repository (a call that died before this rule) is read and,
    // at neither, rolled back by this repository's installer (R68).
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
        let script = w.p("state/r/engine/scripts/install-ont.sh");
        std::fs::create_dir_all(w.p("state/r/engine/scripts")).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let (v, exit) = w.run();
        let row = w.installs.iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(row["outcome"], want, "{v}");
        let launch = w.calls.iter().find(|(p, _, _)| p == "bash").unwrap();
        assert_eq!(launch.1, args(&[&script, "rollback"]));
        assert!(!w.engine_clone_touched());
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

// spec: specs/ap/deliver.md#R14
// spec: specs/ap/deliver.md#R45
// spec: specs/ap/deliver.md#R46
// spec: specs/ap/deliver.md#R59
#[test]
fn e_with_or_without_engine_the_marker_is_read_and_the_record_is_the_same() {
    // The member `engine` changes nothing: the marker is acted on and the record has `schema` and `roles`
    // in both forms.
    for with_engine in [false, true] {
        let mut w = W::new();
        w.engine = with_engine;
        (w.schema, w.tip_schema, w.migrate_mode) = (8, 8, Some("100755"));
        w.set_marker(7, 8);
        let (v, exit) = w.run();
        assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
        assert_eq!(w.migrates.len(), 1, "{v}");
        assert!(!w.marker_present());
        assert_eq!(members(&v), sorted());
        assert_eq!(w.last(), Some(v));
    }
}

// spec: specs/ap/deliver.md#R8
// spec: specs/ap/deliver.md#R14
#[test]
fn e_engine_member_is_accepted_and_ignored() {
    let w = W::new();
    // Not an object: refused, in `ap deliver` and in `ap deliver check` alike.
    for e in [json!("o/engine"), json!(null), json!([]), json!(1)] {
        let mut r = w.request();
        r["engine"] = e.clone();
        let (v, exit) = execute(&mut W::new(), &r);
        assert_eq!((end(&v), exit), (("refused", "invalid_input"), 2), "{e}");
        assert_eq!(v["detail"], json!({"field": "engine"}), "{e}");
        assert_eq!(check_request(&r), (v, 2), "{e}");
    }
    // Any object, naming any repository with or without a URL, is accepted by the check.
    for e in [
        json!({"repository": "o/engine", "url": EURL}),
        json!({"repository": "park-kyungchan/ontology-platform", "url": "https://github.com/park-kyungchan/ontology-platform.git"}),
        json!({"repository": "o/engine"}),
        json!({"repository": "o/r", "url": "-x"}),
        json!({"repository": "o/migrate.pending"}),
        json!({}),
    ] {
        let mut r = w.request();
        r["engine"] = e.clone();
        assert_eq!(check_request(&r), (json!({"status": "ok"}), 0), "{e}");
    }
    // The call with it is the call without it: the same programs and actions, the same record but `at`.
    let mut plain = W::engine();
    plain.engine = false;
    let (a, _) = plain.run();
    let mut with = W::engine();
    let (b, _) = with.run();
    let strip = |v: &Value, w: &W| {
        let mut v = v.clone();
        v["at"] = Value::Null;
        v.to_string().replace(&w.p(""), "<root>/")
    };
    assert_eq!(strip(&a, &plain), strip(&b, &with));
    assert_eq!(plain.launched(), with.launched());
    assert_eq!(plain.applies("start_install").len(), 1);
    assert_eq!(
        plain.applies("start_install")[0]["parameters"],
        with.applies("start_install")[0]["parameters"]
    );
    assert!(!with.engine_clone_touched());
    // The repository's name `migrate.pending` is refused on every call.
    for with_engine in [false, true] {
        let mut w = W::new();
        w.engine = with_engine;
        let mut r = w.request();
        r["repository"] = json!("o/migrate.pending");
        let (v, _) = execute(&mut W::new(), &r);
        assert_eq!(v["detail"], json!({"field": "repository"}));
    }
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
        ("engine_differs", |w| w.ont_version_bad = true),
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
    for name in [".", "..", "target", "last.json", "etc.tip"] {
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
        |w| w.ont_version_bad = true,
        |w| w.tip_schema = 7,
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
// spec: specs/ap/deliver.md#R67
#[test]
fn u1_the_engine_is_installed_from_the_clone_checked_out_at_t() {
    let mut w = W::engine();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["target"], ENGINE);
    // No clone of another repository is made or checked out.
    assert_eq!(checkouts(&w, &w.p("state/engine")), []);
    assert!(!w.engine_clone_touched());
    // The clone of the repository is at T, accepted, before the engine's installer (R42).
    let own = checkouts(&w, &w.p("state/r"));
    assert_eq!(own.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), [c(2)]);
    let last_review = w
        .calls
        .iter()
        .rposition(|(p, a, b)| p == "ont" && a[0] == "search" && b["object_type"] == "review")
        .unwrap();
    let launch = w.calls.iter().position(|(p, _, _)| p == "bash").unwrap();
    assert!(last_review < own[0].0 && own[0].0 < launch);
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

/// What `specs/ap/clause.md` R11 says of `ap deliver`: it installs the clause service and its timer
/// and enables the timer. The merged files of `deploy/` are the unit files of the tip, the host has
/// neither, and the call writes both byte for byte, reloads the manager, and enables the timer.
// spec: specs/ap/clause.md#R11
#[test]
fn r11_clause_timer_is_installed_and_enabled_by_deliver() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let service = std::fs::read_to_string(root.join("deploy/ap-clause.service")).unwrap();
    let timer = std::fs::read_to_string(root.join("deploy/ap-clause.timer")).unwrap();
    let mut w = W::new();
    w.unit(1..=2, "ap-clause.service", &service);
    w.unit(1..=2, "ap-clause.timer", &timer);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        unit_rows(&v),
        [
            urow("ap-clause.service", "written", Value::Null),
            urow("ap-clause.timer", "written", json!(true)),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(w.host("ap-clause.service")).unwrap(),
        service
    );
    assert_eq!(
        std::fs::read_to_string(w.host("ap-clause.timer")).unwrap(),
        timer
    );
    assert_eq!(
        w.systemctl(),
        [
            "daemon-reload",
            "enable --now ap-clause.timer",
            "is-enabled ap-clause.timer"
        ]
    );
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

// ---- X the configuration files (specs/ap/deliver.md R53 to R57) ----------------------------------

const XURL: &str = "https://example.invalid/o/ops.git";
/// A commit of the operations repository's chain: `xc(0)` is the clone's `HEAD`, `xc(2)` its tip.
fn xc(n: u32) -> String {
    format!("{:040}", 700 + n)
}
fn xh(n: u32) -> String {
    format!("{:040}", 800 + n)
}

impl W {
    /// A request with `etc`: the clone at xc(0), two accepted merges up to the tip xc(2); the first
    /// `check` answers that the host differs, `install` and the second `check` succeed.
    fn ops() -> Self {
        let mut w = W::new();
        w.etc = true;
        w.xchain = vec![Commit {
            id: xc(0),
            parents: vec![],
            tree: tree(70),
        }];
        for n in 1..=2 {
            w.xchain.push(Commit {
                id: xc(n),
                parents: vec![xc(n - 1), xh(n)],
                tree: tree(70 + n),
            });
            w.xheads.insert(xh(n), tree(70 + n));
            w.xreview(n);
        }
        w.xhead = Some(xc(0));
        w.xruns = vec![(1, String::new()), (0, String::new()), (0, String::new())];
        w
    }
    fn xtip(&self) -> String {
        self.xchain.last().unwrap().id.clone()
    }
    fn xclone(&self) -> String {
        self.p("state/ops")
    }
    fn xreview(&mut self, n: u32) -> Value {
        let key = format!("rv-{}", self.reviews.len() + 1);
        let report = self.p(&format!("reports/{key}.md"));
        let bytes = format!("Verdict: accept\n{}\n", xh(n));
        std::fs::write(&report, &bytes).unwrap();
        let row = json!({"id": oid("review", &key), "kind": "packet", "head": xh(n), "status": "closed",
            "verdict": "accept", "repository": "o/ops", "report": report,
            "report_sha256": sha(bytes.as_bytes()), "closed_at": "2026-11-13T00:02:00Z"});
        self.reviews.push(row.clone());
        row["id"].clone()
    }
    /// The calls that reach the operations clone, each as its words after `-C <clone>` (an `init` as
    /// its own words), and the programs of the clone as `etc-check <E>` or `etc <E>`.
    fn xcalls(&self) -> Vec<String> {
        let xclone = self.xclone();
        self.calls
            .iter()
            .filter_map(|(p, a, _)| {
                if p == "git" && a.first().is_some_and(|x| x == "init") && a[2] == xclone {
                    return Some("init".to_owned());
                }
                if p == "git" && a.len() > 2 && a[1] == xclone {
                    return Some(a[2..].join(" "));
                }
                if p == "bash" && a[0] == format!("{xclone}/scripts/install-etc.sh") {
                    let name = if a[1] == "check" { "etc-check" } else { "etc" };
                    return Some(format!("{name} {}", a[2..].join(" ")));
                }
                None
            })
            .collect()
    }
    fn xgit(&mut self, a: &[String]) -> Answer {
        let rest: Vec<&str> = a[2..].iter().map(String::as_str).collect();
        match rest.as_slice() {
            ["rev-parse", "--verify", "HEAD^{commit}"] => match &self.xhead {
                Some(h) => Self::text(true, &format!("{h}\n")),
                None => Self::text(false, ""),
            },
            ["config", _, _] => Self::text(real_git(a), ""),
            ["fetch", "--quiet", "--no-tags", "origin", refspec] => {
                assert_eq!(*refspec, MAIN);
                let url = git_config(&a[1], "remote.origin.url");
                Self::text(!self.xfetch_fails && url.as_deref() == Some(XURL), "")
            }
            ["rev-parse", "--verify", "refs/remotes/origin/main^{commit}"] => {
                Self::text(true, &format!("{}\n", self.xtip()))
            }
            ["rev-list", "--parents", "-n", "1", tip] => {
                assert_eq!(*tip, self.xtip());
                let c = self.xchain.iter().find(|c| c.id == *tip).unwrap();
                let mut words = vec![c.id.clone()];
                words.extend(c.parents.iter().cloned());
                Self::text(true, &format!("{}\n", words.join(" ")))
            }
            ["rev-list", "--first-parent", "--parents", range] => {
                let (from, to) = range.split_once("..").unwrap();
                assert_eq!(to, self.xtip());
                let mut lines = vec![];
                for commit in self.xchain.iter().rev() {
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
                let c = self.xchain.iter().find(|c| c.id == commit).unwrap();
                match c.parents.get(1).and_then(|p| self.xheads.get(p)) {
                    Some(t) => Self::text(true, &format!("{}\n{t}\n", c.tree)),
                    None => Self::text(false, ""),
                }
            }
            ["checkout", "--quiet", "--force", "--detach", k] => {
                if self.xcheckout_fails {
                    return Self::text(false, "");
                }
                self.xhead = Some((*k).to_owned());
                Self::text(true, "")
            }
            _ => panic!("unexpected operations git call {a:?}"),
        }
    }
    /// `install-etc.sh`: the next of `xruns`, run from the clone's tree, which must be at the commit
    /// it is given.
    fn launch_etc(&mut self, argv: &[String], files: [std::fs::File; 3]) -> std::io::Result<Child> {
        assert_eq!(argv[0], "bash");
        assert_eq!(argv.len(), 4, "{argv:?}");
        assert!(["check", "install"].contains(&argv[2].as_str()), "{argv:?}");
        let own = argv[1] == self.p("state/r/host/scripts/install-etc.sh");
        let head = if own { &self.rhead } else { &self.xhead };
        assert_eq!(
            head.as_deref(),
            Some(argv[3].as_str()),
            "a program ran off E"
        );
        let (exit, err) = self.xruns.remove(0);
        let [stdin, stdout, stderr] = files;
        Command::new("sh")
            .args(["-c", "printf '%s' \"$1\" >&2; exit \"$2\"", "sh"])
            .args([&err, &exit.to_string()])
            .env_clear()
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
    }
}
fn etc_rows(v: &Value) -> Vec<(String, Value, Value)> {
    rows(v)
        .into_iter()
        .filter(|(n, _, _)| n.starts_with("etc"))
        .collect()
}
fn xrecord(
    from: Option<&str>,
    tip: Option<&str>,
    result: &str,
    code: Value,
    detail: Value,
) -> Value {
    json!({"repository": "o/ops", "from": from, "tip": tip, "result": result, "code": code, "detail": detail})
}

// spec: specs/ap/deliver.md#R53
// spec: specs/ap/deliver.md#R57
#[test]
fn x_without_etc_the_record_and_the_calls_are_as_before() {
    for setup in [
        |_: &mut W| {},
        |w: &mut W| {
            (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
        },
    ] {
        let mut w = W::new();
        setup(&mut w);
        let (v, exit) = w.run();
        assert_eq!(exit, 0, "{v}");
        assert_eq!(members(&v), sorted());
        assert!(w.xcalls().is_empty());
        assert!(w.calls.iter().all(|(p, a, _)| {
            !p.contains("install-etc.sh")
                && !a
                    .iter()
                    .any(|x| x.contains("install-etc.sh") || x.contains("/state/ops"))
        }));
        assert!(etc_rows(&v).is_empty());
    }
    // With `etc` the record has the one member more, on stdout and in `last.json`.
    let mut w = W::ops();
    let (v, _) = w.run();
    let mut want = sorted();
    want.push("etc".into());
    want.sort();
    assert_eq!(members(&v), want);
    assert_eq!(w.last(), Some(v));
}

// spec: specs/ap/deliver.md#R53
#[test]
fn x_etc_is_checked() {
    let bad = [
        json!("o/ops"),
        json!({"repository": "o/ops"}),
        json!({"url": XURL}),
        json!({"repository": "o/ops", "url": XURL, "x": 1}),
        json!({"repository": "ops", "url": XURL}),
        json!({"repository": "o/o ps", "url": XURL}),
        json!({"repository": "o/ops", "url": "-uhttps://x"}),
        json!({"repository": "o/ops", "url": ""}),
        json!({"repository": "o/ops", "url": 1}),
        // The repository's own name under another owner (R53, R71).
        json!({"repository": "x/r", "url": XURL}),
        json!({"repository": "o/migrate.pending", "url": XURL}),
        json!({"repository": "o/etc.tip", "url": XURL}),
        json!({"repository": "o/.", "url": XURL}),
        json!({"repository": "o/..", "url": XURL}),
        json!({"repository": "o/target", "url": XURL}),
        json!({"repository": "o/last.json", "url": XURL}),
        json!(null),
    ];
    for etc in bad {
        let mut w = W::new();
        w.engine = true;
        let mut r = w.request();
        r["etc"] = etc.clone();
        let (v, exit) = w.run_with(&r);
        assert_eq!(
            (v["code"].as_str(), exit, v["detail"].clone()),
            (Some("invalid_input"), 2, json!({"field": "etc"})),
            "{etc}"
        );
        assert!(v.get("etc").is_none(), "{etc}");
        assert!(w.calls.is_empty() && !w.t.join("state").exists(), "{etc}");
        assert_eq!(check_request(&r), (v, 2), "{etc}");
    }
    // The engine's name is free for `etc` without `engine`; each form takes the member.
    let w = W::ops();
    let mut r = w.request();
    r["etc"] = json!({"repository": "x/engine", "url": XURL});
    assert_eq!(check_request(&r), (json!({"status": "ok"}), 0));
    for r in [w.request(), w.timer(), w.rollback()] {
        assert_eq!(check_request(&r), (json!({"status": "ok"}), 0), "{r}");
    }
}

// spec: specs/ap/deliver.md#R54
#[test]
fn x_the_step_runs_only_after_current_or_installed_and_never_in_a_rollback() {
    type Setup = fn(&mut W);
    let ends: [(&str, Setup); 7] = [
        ("refused", |w| w.build_busy = true),
        ("refused", |w| w.reviews.retain(|r| r["head"] != h(2))),
        ("refused", |w| w.fetch_fails = true),
        ("failed_restored", |w| {
            w.inst.get_mut("content").unwrap().exit = 1;
            w.inst.get_mut("content").unwrap().tree = None;
        }),
        ("not_started", |w| {
            w.inst.insert(
                "ap",
                installer(None, None, 2, "", "install_busy@install-ap.sh:40\n"),
            );
        }),
        ("failed", |w| {
            let content = w.inst.get_mut("content").unwrap();
            (content.tree, content.exit, content.out) = (Some(None), 1, String::new());
        }),
        ("unrecorded", |w| w.close_fails = true),
    ];
    for (want, setup) in ends {
        let mut w = W::ops();
        setup(&mut w);
        let (v, _) = w.run();
        assert_eq!(v["end"], want, "{v}");
        assert_eq!(v["etc"], Value::Null, "{want}");
        assert!(w.xcalls().is_empty(), "{want}: {:?}", w.xcalls());
        assert_eq!(w.xhead, Some(xc(0)));
    }
    // A Lead's rollback that ends `installed` runs no step either.
    let mut w = W::ops();
    installed_by_a(&mut w);
    let r = w.rollback();
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["etc"], Value::Null);
    assert!(w.xcalls().is_empty());
}

// spec: specs/ap/deliver.md#R54
// spec: specs/ap/deliver.md#R55
// spec: specs/ap/deliver.md#R56
// spec: specs/ap/deliver.md#R57
#[test]
fn x_a_moved_tip_that_is_accepted_is_installed() {
    let mut w = W::ops();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        v["etc"],
        xrecord(
            Some(&xc(0)),
            Some(&xc(2)),
            "installed",
            Value::Null,
            Value::Null
        )
    );
    let tip = xc(2);
    assert_eq!(
        w.xcalls(),
        [
            "init".to_owned(),
            "rev-parse --verify HEAD^{commit}".into(),
            format!("config remote.origin.url {XURL}"),
            format!("config remote.origin.fetch {MAIN}"),
            format!("fetch --quiet --no-tags origin {MAIN}"),
            "rev-parse --verify refs/remotes/origin/main^{commit}".into(),
            format!("rev-list --first-parent --parents {}..{tip}", xc(0)),
            format!("rev-parse {}^{{tree}} {}^2^{{tree}}", xc(1), xc(1)),
            format!("rev-parse {tip}^{{tree}} {tip}^2^{{tree}}"),
            format!("checkout --quiet --force --detach {tip}"),
            format!("etc-check {tip}"),
            format!("etc {tip}"),
            format!("etc-check {tip}"),
        ]
    );
    // The remote is the request's, the clone's `HEAD` the tip; the step comes after every step of the
    // pair, and its rows follow the pair's; the record of the pair is unchanged.
    assert_eq!(
        git_config(&w.xclone(), "remote.origin.url").as_deref(),
        Some(XURL)
    );
    assert_eq!(
        git_config(&w.xclone(), "remote.origin.fetch").as_deref(),
        Some(MAIN)
    );
    assert_eq!(w.xhead, Some(tip.clone()));
    let names: Vec<String> = rows(&v).into_iter().map(|r| r.0).collect();
    assert_eq!(names, ["ap", "content", "etc-check", "etc", "etc-check"]);
    let close = w.at("close_install").unwrap();
    let first = w
        .calls
        .iter()
        .position(|(_, a, _)| a.contains(&w.xclone()))
        .unwrap();
    assert!(close < first);
    assert_eq!(
        (v["outcome"].clone(), v["code"].clone()),
        (json!("installed"), Value::Null)
    );
    assert_eq!(w.applies("start_install").len(), 1);
    assert_eq!(w.applies("close_install").len(), 1);
    assert_eq!(w.last(), Some(v));
}

// spec: specs/ap/deliver.md#R55
// spec: specs/ap/deliver.md#R57
#[test]
fn x_a_new_clone_judges_the_tip_alone() {
    let mut w = W::ops();
    w.xhead = None;
    // The merge before the tip has no accepting review: only the tip is judged.
    w.reviews.retain(|r| r["head"] != xh(1));
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(
        v["etc"],
        xrecord(None, Some(&xc(2)), "installed", Value::Null, Value::Null)
    );
    let tip = xc(2);
    let judged: Vec<String> = w
        .xcalls()
        .into_iter()
        .filter(|c| c.starts_with("rev-"))
        .collect();
    assert_eq!(
        judged,
        [
            "rev-parse --verify HEAD^{commit}".to_owned(),
            "rev-parse --verify refs/remotes/origin/main^{commit}".into(),
            format!("rev-list --parents -n 1 {tip}"),
            format!("rev-list --first-parent --parents {}..{tip}", xc(1)),
            format!("rev-parse {tip}^{{tree}} {tip}^2^{{tree}}"),
        ]
    );
    // A tip that is no merge is refused without a program.
    let mut w = W::ops();
    w.xhead = None;
    w.xchain.truncate(1);
    let (v, _) = w.run();
    let no = json!({"commit": xc(0), "why": "not_a_merge"});
    assert_eq!(
        v["etc"],
        xrecord(None, Some(&xc(0)), "not_accepted", Value::Null, no)
    );
    assert!(etc_rows(&v).is_empty());
}

// spec: specs/ap/deliver.md#R55
// spec: specs/ap/deliver.md#R56
// spec: specs/ap/deliver.md#R57
#[test]
fn x_a_tip_equal_to_head_runs_one_check() {
    let mut w = W::ops();
    w.xhead = Some(xc(2));
    w.xruns = vec![(0, String::new())];
    let (v, _) = w.run();
    assert_eq!(
        v["etc"],
        xrecord(Some(&xc(2)), Some(&xc(2)), "same", Value::Null, Value::Null)
    );
    assert_eq!(etc_rows(&v), [row("etc-check", 0, None)]);
    let calls = w.xcalls();
    assert!(
        calls.iter().all(|c| !c.starts_with("rev-list")),
        "{calls:?}"
    );
    assert!(calls.contains(&format!("checkout --quiet --force --detach {}", xc(2))));
    // A new tip whose bytes the host already has is installed anyway: the installer's record follows.
    let mut w = W::ops();
    w.xruns = vec![(0, String::new()), (0, String::new()), (0, String::new())];
    let (v, _) = w.run();
    assert_eq!(v["etc"]["result"], "installed");
    assert_eq!(
        etc_rows(&v),
        [
            row("etc-check", 0, None),
            row("etc", 0, None),
            row("etc-check", 0, None)
        ]
    );
    // The first call of a clone new to delivery when the installer's record names the tip already.
    let mut w = W::ops();
    w.xhead = None;
    w.xruns = vec![
        (0, String::new()),
        (2, "not_forward a3f 88d\n".into()),
        (0, String::new()),
    ];
    let (v, _) = w.run();
    assert_eq!(
        v["etc"],
        xrecord(
            None,
            Some(&xc(2)),
            "same",
            json!("not_forward"),
            Value::Null
        )
    );
}

// spec: specs/ap/deliver.md#R55
// spec: specs/ap/deliver.md#R57
#[test]
fn x_a_tip_that_is_not_accepted_runs_no_program_and_keeps_head() {
    let mut w = W::ops();
    w.reviews.retain(|r| r["head"] != xh(2));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let no = json!({"commit": xc(2), "why": "no_accepting_review"});
    assert_eq!(
        v["etc"],
        xrecord(Some(&xc(0)), Some(&xc(2)), "not_accepted", Value::Null, no)
    );
    assert!(etc_rows(&v).is_empty());
    assert!(
        w.xcalls()
            .iter()
            .all(|c| !c.starts_with("checkout") && !c.starts_with("etc"))
    );
    assert_eq!(w.xhead, Some(xc(0)));
    // A `HEAD` that is not on the tip's first-parent chain.
    let mut w = W::ops();
    w.xhead = Some(other());
    let (v, _) = w.run();
    let no = json!({"commit": other(), "why": "not_on_chain"});
    assert_eq!(
        v["etc"],
        xrecord(
            Some(&other()),
            Some(&xc(2)),
            "not_accepted",
            Value::Null,
            no
        )
    );
    assert_eq!(w.xhead, Some(other()));
    // A store that cannot be read.
    let mut w = W::ops();
    w.xsearch_fails = true;
    let (v, _) = w.run();
    let want = xrecord(
        Some(&xc(0)),
        Some(&xc(2)),
        "failed",
        json!("store_unreadable"),
        Value::Null,
    );
    assert_eq!(v["etc"], want);
    assert!(etc_rows(&v).is_empty());
    assert_eq!(w.xhead, Some(xc(0)));
}

// spec: specs/ap/deliver.md#R55
#[test]
fn x_a_fetch_or_checkout_that_fails_is_fetch_failed() {
    let mut w = W::ops();
    w.xfetch_fails = true;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        v["etc"],
        xrecord(Some(&xc(0)), None, "fetch_failed", Value::Null, Value::Null)
    );
    assert!(etc_rows(&v).is_empty());
    assert_eq!(w.xhead, Some(xc(0)));
    let mut w = W::ops();
    w.xcheckout_fails = true;
    let (v, _) = w.run();
    assert_eq!(
        v["etc"],
        xrecord(
            Some(&xc(0)),
            Some(&xc(2)),
            "fetch_failed",
            Value::Null,
            Value::Null
        )
    );
    assert!(etc_rows(&v).is_empty());
}

// spec: specs/ap/deliver.md#R56
// spec: specs/ap/deliver.md#R57
#[test]
fn x_a_hand_edit_differs() {
    // The tip has not moved: the installer refuses to overwrite the edit.
    let mut w = W::ops();
    w.xhead = Some(xc(2));
    w.xruns = vec![
        (1, String::new()),
        (
            2,
            "reading the record\nnot_forward 0000700 is the record\n".into(),
        ),
        (1, String::new()),
    ];
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(
        v["etc"],
        xrecord(
            Some(&xc(2)),
            Some(&xc(2)),
            "differs",
            json!("not_forward"),
            Value::Null
        )
    );
    assert_eq!(
        etc_rows(&v),
        [
            row("etc-check", 1, None),
            row("etc", 2, Some("not_forward")),
            row("etc-check", 1, None)
        ]
    );
    // The tip moved: `target_changed`; the record holds the code and nothing else of the output.
    let mut w = W::ops();
    w.xruns = vec![
        (1, String::new()),
        (
            2,
            "target_changed /etc/agent-platform/policy.json SECRET\n".into(),
        ),
        (1, "differs SECRET\n".into()),
    ];
    let (v, _) = w.run();
    assert_eq!(v["etc"]["result"], "differs");
    assert_eq!(v["etc"]["code"], "target_changed");
    assert!(!v.to_string().contains("SECRET"));
}

// spec: specs/ap/deliver.md#R56
// spec: specs/ap/deliver.md#R57
#[test]
fn x_the_last_check_decides_never_the_install() {
    let mut w = W::ops();
    w.xruns = vec![
        (1, String::new()),
        (1, "verify_failed x\n".into()),
        (0, String::new()),
    ];
    let (v, _) = w.run();
    assert_eq!(v["etc"]["result"], "same");
    assert_eq!(v["etc"]["code"], "verify_failed");
    // A first check that refuses ends the programs; an unreadable last word is `etc_exit_<n>`.
    let mut w = W::ops();
    w.xruns = vec![(2, "busy /var/lib/agent-platform/etc/lock\n".into())];
    let (v, _) = w.run();
    assert_eq!(v["etc"]["result"], "failed");
    assert_eq!(etc_rows(&v), [row("etc-check", 2, Some("busy"))]);
    let mut w = W::ops();
    w.xruns = vec![
        (1, String::new()),
        (0, String::new()),
        (3, "usage: check\n".into()),
    ];
    let (v, _) = w.run();
    assert_eq!(v["etc"]["result"], "failed");
    assert_eq!(v["etc"]["code"], "etc_exit_3");
    // An install that exited 0 while the last check differs is `differs`.
    let mut w = W::ops();
    w.xruns = vec![(1, String::new()), (0, String::new()), (1, String::new())];
    let (v, _) = w.run();
    assert_eq!(
        (v["etc"]["result"].clone(), v["etc"]["code"].clone()),
        (json!("differs"), Value::Null)
    );
}

// spec: specs/ap/deliver.md#R56
#[test]
fn x_each_program_inherits_the_lock_with_the_environment_unchanged() {
    let mut w = W::ops();
    let (v, _) = w.run();
    assert_eq!(
        w.launched(),
        ["ap", "content", "etc-check", "etc", "etc-check"],
        "{v}"
    );
    assert_eq!(w.lock_inherited, [true; 5]);
    let env: serde_json::Map<String, Value> = w
        .environment()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    let script = format!("{}/scripts/install-etc.sh", w.xclone());
    let launches: Vec<&Value> = w
        .calls
        .iter()
        .filter(|(p, a, _)| p == "bash" && a[0] == script)
        .map(|(_, _, b)| b)
        .collect();
    assert_eq!(launches.len(), 3);
    for b in launches {
        assert_eq!(b["cwd"], json!(w.xclone()));
        assert_eq!(b["env"], json!(env));
    }
}

// spec: specs/ap/deliver.md#R54
#[test]
fn x_the_step_runs_after_current_and_before_the_checkouts() {
    let mut w = W::ops();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let co = Co::new(w.co_path("a"));
    let mut r = w.with_checkouts(vec![co]);
    r["etc"] = json!({"repository": "o/ops", "url": XURL});
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["etc"]["result"], "installed");
    let last_etc = w
        .calls
        .iter()
        .rposition(|(p, a, _)| p == "bash" && a[0].ends_with("install-etc.sh"))
        .unwrap();
    let first_co = w
        .calls
        .iter()
        .position(|(_, a, _)| a.first().is_some_and(|x| x == "--no-optional-locks"));
    assert!(first_co.is_some_and(|n| n > last_etc));
    assert!(w.applies("start_install").is_empty() && w.applies("close_install").is_empty());
}

/// The roles file the engine reads, in the directory of the scratch store's path.
fn roles_path(w: &W) -> PathBuf {
    w.t.join("ont-state/ont/roles.json")
}
const ROLES_NEW: &str = "{\"principals\":{\"uid:0\":[\"lead\"]}}\n";
const ROLES_TS: &str =
    "{\"principals\":{\"ts:park-kyungchan@github\":[\"lead\"],\"uid:0\":[\"lead\"]}}\n";
const ROLES_OLD: &str = "{\"principals\":{\"uid:0\":[]}}\n";
fn no_store_write(w: &W) -> bool {
    !w.calls.iter().any(|c| {
        c.0 == "ont"
            && matches!(
                c.1.first().map(String::as_str),
                Some("backup" | "schema" | "action")
            )
    })
}

// spec: specs/ap/deliver.md#R58
// spec: specs/ap/deliver.md#R59
#[test]
fn r58_roles_file_equal_is_same_and_the_record_names_both_digests() {
    let mut w = W::at_pin();
    w.roles_blob = Some(ROLES_NEW);
    std::fs::write(roles_path(&w), ROLES_NEW).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let d = sha(ROLES_NEW.as_bytes());
    assert_eq!(
        v["roles"],
        json!({"before": d, "after": d, "result": "same"})
    );
    assert_eq!(std::fs::read_to_string(roles_path(&w)).unwrap(), ROLES_NEW);
    assert_eq!(w.last(), Some(v));
}

// spec: specs/ap/deliver.md#R58
// spec: specs/ap/deliver.md#R59
#[test]
fn r58_roles_file_that_differs_is_kept_and_written_through_a_temporary_name() {
    use std::os::unix::fs::PermissionsExt;
    let mut w = W::at_pin();
    w.roles_blob = Some(ROLES_NEW);
    std::fs::write(roles_path(&w), ROLES_OLD).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let old = sha(ROLES_OLD.as_bytes());
    assert_eq!(
        v["roles"],
        json!({"before": old, "after": sha(ROLES_NEW.as_bytes()), "result": "written"})
    );
    assert_eq!(std::fs::read_to_string(roles_path(&w)).unwrap(), ROLES_NEW);
    let kept =
        w.t.join(format!("ont-state/ont/roles.json.before-{}", &old[..12]));
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), ROLES_OLD);
    for p in [kept, roles_path(&w)] {
        assert_eq!(
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
    let names: Vec<String> = std::fs::read_dir(w.t.join("ont-state/ont"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    // No host file: written, nothing kept.
    let mut w = W::at_pin();
    w.roles_blob = Some(ROLES_NEW);
    let (v, _) = w.run();
    assert_eq!(
        v["roles"],
        json!({"before": null, "after": sha(ROLES_NEW.as_bytes()), "result": "written"})
    );
    assert_eq!(
        std::fs::read_dir(w.t.join("ont-state/ont"))
            .unwrap()
            .count(),
        1
    );
}

// spec: specs/ap/deliver.md#R58
// spec: specs/ap/deliver.md#R59
#[test]
fn r58_no_blob_at_the_tip_is_absent_and_writes_nothing() {
    let mut w = W::at_pin();
    std::fs::write(roles_path(&w), ROLES_OLD).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let d = sha(ROLES_OLD.as_bytes());
    assert_eq!(
        v["roles"],
        json!({"before": d, "after": d, "result": "absent"})
    );
    assert_eq!(std::fs::read_to_string(roles_path(&w)).unwrap(), ROLES_OLD);
    assert_eq!(
        std::fs::read_dir(w.t.join("ont-state/ont"))
            .unwrap()
            .count(),
        1
    );
}

// spec: specs/ap/deliver.md#R42
// spec: specs/ap/deliver.md#R58
// spec: specs/ap/deliver.md#R59
#[test]
fn r42_engine_moves_before_roles_step_so_forward_only_roles_wait_for_the_new_engine() {
    // Call 1 installs the engine only (`roles` is null and host `roles.json` stays untouched, even
    // when it differs or is absent), so an older engine never reads forward-only `roles.json` bytes
    // before the new engine is installed; Call 2 installs `roles.json` and the pair.
    let mut w = W::engine();
    w.roles_blob = Some(ROLES_TS);
    std::fs::write(roles_path(&w), ROLES_OLD).unwrap();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone(), v["roles"].clone()),
        (("installed", ""), 0, json!(ENGINE), Value::Null),
        "{v}"
    );
    assert_eq!(std::fs::read_to_string(roles_path(&w)).unwrap(), ROLES_OLD);
    assert_eq!(
        std::fs::read_dir(w.t.join("ont-state/ont"))
            .unwrap()
            .count(),
        1
    );
    w.settle();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(TARGET)),
        "{v}"
    );
    let old = sha(ROLES_OLD.as_bytes());
    assert_eq!(
        v["roles"],
        json!({"before": old, "after": sha(ROLES_TS.as_bytes()), "result": "written"})
    );
    assert_eq!(std::fs::read_to_string(roles_path(&w)).unwrap(), ROLES_TS);

    // Absent host `roles.json` stays absent on Call 1 when the engine moves.
    let mut w = W::engine();
    w.roles_blob = Some(ROLES_TS);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone(), v["roles"].clone()),
        (("installed", ""), 0, json!(ENGINE), Value::Null),
        "{v}"
    );
    assert!(!roles_path(&w).exists());
}

// spec: specs/ap/deliver.md#R58
// spec: specs/ap/deliver.md#R59
#[test]
fn r58_a_host_entry_that_is_no_regular_file_fails_before_the_schema_and_the_pair() {
    for kind in ["directory", "link"] {
        // The engine is at the pin and the schema is higher: neither schema nor pair may happen.
        let mut w = W::at_pin();
        (w.schema, w.tip_schema) = (8, 9);
        w.roles_blob = Some(ROLES_NEW);
        let path = roles_path(&w);
        match kind {
            "directory" => std::fs::create_dir(&path).unwrap(),
            _ => {
                let target = w.t.join("elsewhere");
                std::fs::write(&target, ROLES_OLD).unwrap();
                std::os::unix::fs::symlink(&target, &path).unwrap();
            }
        }
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit, &v["outcome"]),
            (("failed", "roles_install_failed"), 4, &Value::Null),
            "{kind}: {v}"
        );
        assert_eq!(v["detail"], json!({"path": path.to_string_lossy()}));
        assert_eq!(v["roles"]["result"], "failed");
        assert!(w.launched().is_empty() && no_store_write(&w), "{kind}: {v}");
        // The entry is as it was.
        match kind {
            "directory" => assert!(std::fs::symlink_metadata(&path).unwrap().is_dir()),
            _ => {
                assert!(
                    std::fs::symlink_metadata(&path)
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(
                    std::fs::read_to_string(w.t.join("elsewhere")).unwrap(),
                    ROLES_OLD
                );
            }
        }
        assert_eq!(w.last(), Some(v));
    }
    // A directory at the path is `failed` also when T has no blob: the host entry is judged first.
    let mut w = W::at_pin();
    std::fs::create_dir(roles_path(&w)).unwrap();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed", "roles_install_failed"), 4),
        "{v}"
    );
    assert!(w.launched().is_empty() && no_store_write(&w));
    // A clone that cannot answer is no "absent" either.
    let mut w = W::at_pin();
    (w.roles_blob, w.roles_git_fails) = (Some(ROLES_NEW), true);
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit),
        (("failed", "roles_install_failed"), 4),
        "{v}"
    );
    assert!(w.launched().is_empty() && no_store_write(&w));
}

// spec: specs/ap/deliver.md#R14
// spec: specs/ap/deliver.md#R59
#[test]
fn r59_every_call_reads_the_roles_file_and_records_it() {
    // With or without the member `engine`: a directory at the path fails the call.
    for with_engine in [false, true] {
        let mut w = W::new();
        w.engine = with_engine;
        w.roles_blob = Some(ROLES_NEW);
        std::fs::create_dir_all(w.t.join("ont-state/ont/roles.json")).unwrap();
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit),
            (("failed", "roles_install_failed"), 4),
            "{v}"
        );
        assert_eq!(v["roles"]["result"], "failed");
        assert!(w.launched().is_empty());
    }
    // The step did not run on a refusal before it, nor on an end `current`: the member is null.
    let mut w = W::at_pin();
    w.build_busy = true;
    let (v, _) = w.run();
    assert_eq!(v["end"], "refused");
    assert_eq!(v["roles"], Value::Null);
}

// ---- PC push (specs/ap/deliver.md R60 to R64) --------------------------------------------------

fn fake_ont_script(commit: &str) -> Vec<u8> {
    format!("#!/bin/sh\nif [ \"${{1:-}}\" = --version ]; then printf 'ont 0.1.0 {commit}\\n'; fi\n")
        .into_bytes()
}

fn pc_target(w: &W) -> Value {
    json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": w.p("pc-bin/ont")})
}

fn plant_local_ont_installed(w: &mut W, commit: &str, rev_idx: u32) -> Value {
    let rev = w.accepting(rev_idx);
    let id = oid("install", &format!("act-local-ont-{}", w.installs.len()));
    w.installs.push(json!({
        "id": id,
        "target": etarget(),
        "host": oid("host", "local"),
        "kind": "forward",
        "repository": "o/r",
        "commit": commit,
        "attempt": 1,
        "from": ec(0),
        "from_sha256": sha(&engine_binary(&ec(0))),
        "by": "timer:1",
        "head": h(rev_idx),
        "review": rev,
        "reviews": [rev],
        "status": "closed",
        "outcome": "installed",
        "to": commit,
        "started_at": "2026-11-14T08:00:00Z",
        "closed_at": "2026-11-14T08:01:00Z"
    }));
    id
}

fn plant_pc_install(
    w: &mut W,
    host: &str,
    commit: &str,
    from: &str,
    from_sha: Option<&str>,
    outcome: Option<&str>,
    attempt: usize,
) -> Value {
    let n = w.installs.len();
    let id = oid("install", &format!("act-pc-{host}-{n}"));
    let status = if outcome.is_some() {
        "closed"
    } else {
        "running"
    };
    let mut row = json!({
        "id": id,
        "target": etarget(),
        "host": oid("host", host),
        "kind": "forward",
        "repository": "o/engine",
        "commit": commit,
        "attempt": attempt,
        "from": from,
        "by": "timer:1",
        "status": status,
        "started_at": format!("2026-11-14T09:{n:02}:00Z"),
    });
    if let Some(sha) = from_sha {
        row["from_sha256"] = json!(sha);
    }
    if let Some(out) = outcome {
        row["outcome"] = json!(out);
        row["closed_at"] = json!(format!("2026-11-14T09:{n:02}:30Z"));
    } else {
        row["running_in"] = etarget();
    }
    w.installs.push(row);
    id
}

// spec: specs/ap/deliver.md#R60
#[test]
fn r60_pc_request_validation_and_omission_on_invalid_input() {
    let w = W::new();
    let mut ok_req = w.request();
    ok_req["pc"] =
        json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/home/apcheck/.cargo/bin/ont"});
    let (v, exit) = check_request(&ok_req);
    assert_eq!((v, exit), (json!({"status": "ok"}), 0));

    let bad_cases: Vec<(Value, &str)> = vec![
        (json!(null), "pc"),
        (json!("pc"), "pc"),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/a/b", "extra": 1}),
            "pc.extra",
        ),
        (
            json!({"ssh": "apcheck@ap-checks", "bin": "/a/b"}),
            "pc.host",
        ),
        (
            json!({"host": "local", "ssh": "apcheck@ap-checks", "bin": "/a/b"}),
            "pc.host",
        ),
        (
            json!({"host": "-bad", "ssh": "apcheck@ap-checks", "bin": "/a/b"}),
            "pc.host",
        ),
        (
            json!({"host": "PC", "ssh": "apcheck@ap-checks", "bin": "/a/b"}),
            "pc.host",
        ),
        (
            json!({"host": "a".repeat(60), "ssh": "apcheck@ap-checks", "bin": "/a/b"}),
            "pc.host",
        ),
        (
            json!({"host": "pc", "ssh": "root@ap-checks", "bin": "/a/b"}),
            "pc.ssh",
        ),
        (
            json!({"host": "pc", "ssh": "user@ap-checks", "bin": "/a/b"}),
            "pc.ssh",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@", "bin": "/a/b"}),
            "pc.ssh",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@-bad", "bin": "/a/b"}),
            "pc.ssh",
        ),
        (
            json!({"host": "pc", "ssh": format!("apcheck@{}", "a".repeat(129)), "bin": "/a/b"}),
            "pc.ssh",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "rel/ont"}),
            "pc.bin",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/"}),
            "pc.bin",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/home/apcheck/bad path"}),
            "pc.bin",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/home/apcheck/../ont"}),
            "pc.bin",
        ),
        (
            json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": format!("/{}", "a".repeat(400))}),
            "pc.bin",
        ),
    ];
    for (pc_val, want_field) in bad_cases {
        let mut w = W::new();
        let mut r = w.request();
        r["pc"] = pc_val.clone();
        let (v, exit) = w.run_with(&r);
        assert_eq!(
            (end(&v), exit),
            (("refused", "invalid_input"), 2),
            "{pc_val}: {v}"
        );
        assert_eq!(v["detail"], json!({"field": want_field}), "{pc_val}: {v}");
        assert!(
            v.get("pc").is_none(),
            "pc must be omitted from invalid_input: {v}"
        );
        assert!(w.calls.is_empty() && !w.t.join("state").exists());
    }
}

// spec: specs/ap/deliver.md#R61
#[test]
fn r61_pc_attempts_are_isolated_from_local_and_pc_dead_clears_running_attempts() {
    // 1. A failed PC attempt on /usr/local/bin/ont never suppresses local VPS engine delivery
    //    and never increments local attempt numbering.
    let mut w = W::engine();
    plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, Some("failed"), 1);
    let r = w.timer();
    let (v, exit) = w.run_with(&r);
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["target"], ENGINE);
    assert_eq!(v["attempt"], 1);
    let started = &w.applies("start_install")[0];
    assert_eq!(started["parameters"]["host"], oid("host", "local"));

    // 2. A running PC attempt on /usr/local/bin/ont without `pc` in request: `pc_dead` closes
    //    it `failed` (dead_call, to: neither) BEFORE `engine_dead` and local `engine_install`.
    let mut w = W::engine();
    let dead_id = plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, None, 1);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(closes.len(), 2, "{closes:?}");
    assert_eq!(closes[0]["request_id"], format!("pc.pc.{}.1.close", ec(2)));
    assert_eq!(
        closes[0]["parameters"],
        json!({"install": dead_id, "outcome": "failed", "code": "dead_call", "to": "neither"})
    );
    assert!(w.launched().iter().all(|l| l != "ont-rollback"));

    // 3. `pc_dead` with `pc` in request: remote matches commit -> closed `installed`.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    let bin_bytes = fake_ont_script(&ec(2));
    std::fs::write(w.p("ont-bin/ont"), &bin_bytes).unwrap();
    let pc_bin = w.p("pc-bin/ont");
    std::fs::create_dir_all(w.t.join("pc-bin")).unwrap();
    std::fs::write(&pc_bin, &bin_bytes).unwrap();
    std::fs::set_permissions(&pc_bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let dead_id = plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, None, 1);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(closes.len(), 1);
    assert_eq!(
        closes[0]["parameters"],
        json!({"install": dead_id, "outcome": "installed", "to": ec(2)})
    );
    assert_eq!(v["pc"]["result"], "same");

    // 4. `pc_dead` with `pc` in request: remote matches `from` -> closed `not_started` (dead_call).
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    let old_bytes = fake_ont_script(&ec(0));
    let new_bytes = fake_ont_script(&ec(2));
    std::fs::write(w.p("ont-bin/ont"), &new_bytes).unwrap();
    let pc_bin = w.p("pc-bin/ont");
    std::fs::create_dir_all(w.t.join("pc-bin")).unwrap();
    std::fs::write(&pc_bin, &old_bytes).unwrap();
    std::fs::set_permissions(&pc_bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let dead_id = plant_pc_install(
        &mut w,
        "pc",
        &ec(2),
        &ec(0),
        Some(&sha(&old_bytes)),
        None,
        1,
    );
    plant_local_ont_installed(&mut w, &ec(2), 2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    let closes = w.applies("close_install");
    assert_eq!(closes.len(), 2, "{closes:?}");
    assert_eq!(
        closes[0]["parameters"],
        json!({"install": dead_id, "outcome": "not_started", "code": "dead_call", "to": ec(0)})
    );
    assert_eq!(v["pc"]["result"], "installed");
}

// spec: specs/ap/deliver.md#R62
#[test]
fn r62_pc_push_guards_unreachable_timeout_same_suppression_and_tailscale_env() {
    // 1. Without `pc`, `pc` is omitted completely from record.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0));
    assert!(v.get("pc").is_none());

    // 2. Refusal or rollback leaves `pc: null` and never launches `tailscale`.
    let mut w = W::at_pin();
    w.pc = Some(pc_target(&w));
    w.build_busy = true;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("refused", "build_busy"), 2));
    assert_eq!(v["pc"], Value::Null);
    assert!(!w.calls.iter().any(|(p, _, _)| p == "tailscale"));

    // 3. Unreachable PC (and timeout) records `unreachable` / `pc_unreachable` and keeps VPS exit 0;
    //    `tailscale` receives ONLY `TAILSCALE_ENV` and no bare `--`.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    w.extra_env = vec![
        ("HOME".into(), "/root".into()),
        ("USER".into(), "root".into()),
        ("LOGNAME".into(), "root".into()),
        ("LANG".into(), "C.UTF-8".into()),
        ("LC_ALL".into(), "en_US.UTF-8".into()),
        ("SSH_AUTH_SOCK".into(), "/tmp/agent.sock".into()),
    ];
    w.pc_ssh = PcSsh::Script(std::collections::VecDeque::from([(255, String::new())]));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(
        v["pc"],
        json!({
            "host": "pc",
            "target": w.p("pc-bin/ont"),
            "from": null,
            "to": null,
            "sha256": null,
            "install": null,
            "result": "unreachable",
            "code": "pc_unreachable"
        })
    );
    let ts_call = w.calls.iter().find(|(p, _, _)| p == "tailscale").unwrap();
    assert_eq!(ts_call.1[0], "ssh");
    assert_eq!(ts_call.1[1], "apcheck@ap-checks");
    assert_eq!(ts_call.1.len(), 3);
    let env_obj = ts_call.2["env"].as_object().unwrap();
    let mut keys: Vec<&str> = env_obj.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["HOME", "LOGNAME", "PATH", "USER"]);

    // 4. Probe timeout (> 15 s) also ends `unreachable` without failing VPS delivery.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    w.pc_ssh = PcSsh::Hang;
    w.advance_on_pause = std::time::Duration::from_secs(20);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "unreachable");
    assert_eq!(v["pc"]["code"], "pc_unreachable");

    // 5. Local `ont` unreadable -> `failed` / `engine_unreadable`, VPS exit unchanged.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    w.ont_unreadable = true;
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "failed");
    assert_eq!(v["pc"]["code"], "engine_unreadable");

    // 6. Timer suppression (`suppressed`, `attempt_exists`, `attempts_exhausted`) vs Lead bypass.
    for (setup, want_code) in [
        ("failed", "suppressed"),
        ("installed", "attempt_exists"),
        ("three_not_started", "attempts_exhausted"),
    ] {
        let mut w = W::at_pin();
        (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
        w.pc = Some(pc_target(&w));
        std::fs::write(w.p("ont-bin/ont"), fake_ont_script(&ec(2))).unwrap();
        match setup {
            "failed" => {
                plant_pc_install(&mut w, "pc", &ec(1), &ec(0), None, Some("failed"), 1);
            }
            "installed" => {
                plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, Some("installed"), 1);
            }
            _ => {
                for n in 1..=3 {
                    plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, Some("not_started"), n);
                }
            }
        }
        let r = w.timer();
        let (v, exit) = w.run_with(&r);
        assert_eq!((end(&v), exit), (("current", ""), 0), "{setup}: {v}");
        assert_eq!(v["pc"]["result"], "suppressed", "{setup}: {v}");
        assert_eq!(v["pc"]["code"], want_code, "{setup}: {v}");
        assert!(w.applies("start_install").is_empty(), "{setup}");
    }

    // Lead call (`by` present) bypasses timer suppression.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    std::fs::write(w.p("ont-bin/ont"), fake_ont_script(&ec(2))).unwrap();
    plant_pc_install(&mut w, "pc", &ec(2), &ec(0), None, Some("failed"), 1);
    plant_local_ont_installed(&mut w, &ec(2), 2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "installed");
    assert_eq!(
        w.applies("start_install")[0]["request_id"],
        format!("pc.pc.{}.2", ec(2))
    );
}

// spec: specs/ap/deliver.md#R63
#[test]
fn r63_pc_push_installs_backs_up_rolls_back_and_resolves_review() {
    use std::os::unix::fs::PermissionsExt;

    // 1. Fresh install: the review is that of the local installed attempt at the id (R69).
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    let v2 = fake_ont_script(&ec(2));
    std::fs::write(w.p("ont-bin/ont"), &v2).unwrap();
    plant_local_ont_installed(&mut w, &ec(2), 2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "installed", "{v}");
    assert_eq!(v["pc"]["to"], ec(2));
    assert_eq!(v["pc"]["sha256"], sha(&v2));
    assert_eq!(v["pc"]["code"], Value::Null);
    let pc_bin = w.p("pc-bin/ont");
    assert_eq!(std::fs::read(&pc_bin).unwrap(), v2);
    assert_eq!(
        std::fs::metadata(&pc_bin).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let start = &w.applies("start_install")[0];
    assert_eq!(start["request_id"], format!("pc.pc.{}.1", ec(2)));
    assert_eq!(start["parameters"]["host"], oid("host", "pc"));
    assert_eq!(start["parameters"]["from"], "0".repeat(40));
    assert_eq!(start["parameters"]["head"], h(2));
    assert_eq!(start["parameters"]["repository"], "o/r");

    // 2. Upgrade existing binary: backs up to `.before-<sha12>` and passes `rollback_binary`.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    let old = fake_ont_script(&ec(0));
    let new = fake_ont_script(&ec(2));
    std::fs::write(w.p("ont-bin/ont"), &new).unwrap();
    std::fs::create_dir_all(w.t.join("pc-bin")).unwrap();
    std::fs::write(w.p("pc-bin/ont"), &old).unwrap();
    std::fs::set_permissions(w.p("pc-bin/ont"), std::fs::Permissions::from_mode(0o755)).unwrap();
    plant_local_ont_installed(&mut w, &ec(2), 2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "installed");
    assert_eq!(v["pc"]["from"], ec(0));
    let backup = format!("{}.before-{}", w.p("pc-bin/ont"), &sha(&old)[..12]);
    assert_eq!(std::fs::read(&backup).unwrap(), old);
    assert_eq!(
        w.applies("close_install")[0]["parameters"]["rollback_binary"],
        backup
    );

    // 3. Failed readback after rename: remote script restores `.before-<sha12>` atomically and
    //    records `failed_restored` while leaving VPS exit 0.
    let mut w = W::at_pin();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    let old = fake_ont_script(&ec(0));
    let broken_new = fake_ont_script(&ec(1)); // prints ec(1) instead of expected ec(2)
    std::fs::write(w.p("ont-bin/ont"), &broken_new).unwrap();
    std::fs::create_dir_all(w.t.join("pc-bin")).unwrap();
    std::fs::write(w.p("pc-bin/ont"), &old).unwrap();
    std::fs::set_permissions(w.p("pc-bin/ont"), std::fs::Permissions::from_mode(0o755)).unwrap();
    plant_local_ont_installed(&mut w, &ec(2), 2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "failed_restored", "{v}");
    assert_eq!(v["pc"]["code"], "readback_failed");
    assert_eq!(std::fs::read(w.p("pc-bin/ont")).unwrap(), old);
    assert_eq!(
        w.applies("close_install")[0]["parameters"]["outcome"],
        "failed_restored"
    );

    // 4. No accepting review -> `not_accepted`, nothing pushed.
    let mut w = W::new();
    (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
    w.pc = Some(pc_target(&w));
    std::fs::create_dir_all(w.t.join("ont-bin")).unwrap();
    std::fs::write(w.p("ont-bin/ont"), fake_ont_script(&"e".repeat(40))).unwrap();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "not_accepted", "{v}");
    assert!(w.applies("start_install").is_empty());
}

// spec: specs/ap/deliver.md#R14
// spec: specs/ap/deliver.md#R60
// spec: specs/ap/deliver.md#R64
#[test]
fn r64_deliver_json_and_pc_pull_script_present() {
    use std::os::unix::fs::PermissionsExt;
    let request: Value =
        serde_json::from_str(include_str!("../../../deploy/deliver.json")).unwrap();
    let (v, exit) = check_request(&request);
    assert_eq!((v, exit), (json!({"status": "ok"}), 0));
    // The head's request has no member `engine`; the host's older form, which still names the engine's
    // former repository, is accepted as well (R14).
    assert!(request.get("engine").is_none());
    let mut host = request.clone();
    host["engine"] = json!({"repository": "park-kyungchan/ontology-platform",
        "url": "https://github.com/park-kyungchan/ontology-platform.git"});
    assert_eq!(check_request(&host), (json!({"status": "ok"}), 0));
    assert_eq!(
        request["pc"],
        json!({"host": "pc", "ssh": "apcheck@ap-checks", "bin": "/home/apcheck/.cargo/bin/ont"})
    );
    let pull = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/pc-pull.sh");
    let meta = std::fs::metadata(&pull).unwrap();
    assert!(meta.is_file() && meta.permissions().mode() & 0o111 != 0);
}

// ---- The engine of `engine/` at the tip (`specs/ap/deliver.md` R66 to R69) ----------------------

/// The Git tree id of `engine/` at the tip, under the tree rule.
fn etree() -> String {
    format!("{:040}", 777)
}
impl W {
    /// A tip whose `engine/` has the id `etree()`; the engine at ec(0), an engine of an older tree.
    fn engine_tree() -> Self {
        let mut w = W::engine();
        w.pin = etree();
        w.ont_install = (Some(etree()), 0);
        w
    }
    /// Whether any program touched a clone of the engine's former repository.
    fn engine_clone_touched(&self) -> bool {
        let eclone = self.p("state/engine");
        self.calls.iter().any(|(_, a, b)| {
            a.iter().any(|x| x.starts_with(&eclone)) || b["cwd"].as_str() == Some(eclone.as_str())
        })
    }
    fn git_words(&self, word: &str) -> usize {
        self.calls
            .iter()
            .filter(|(p, a, _)| p == "git" && a.iter().any(|x| x == word))
            .count()
    }
}

// spec: specs/ap/deliver.md#R37
// spec: specs/ap/deliver.md#R42
// spec: specs/ap/deliver.md#R66
// spec: specs/ap/deliver.md#R67
#[test]
fn r67_engine_tree_the_engine_of_t_is_built_from_its_engine_and_the_call_ends() {
    let mut w = W::engine_tree();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    let (id, from) = (etree(), ec(0));
    let want = json!({"target": ENGINE, "tip": c(2), "from": from, "to": id, "outcome": "installed",
        "install": oid("install", &format!("act-ont.{id}.1")), "attempt": 1, "rollback_binary": ekept(&w),
        "rollback_content": null, "index": {"before": "ok", "after": "ok"}, "schema": null});
    for (k, value) in want.as_object().unwrap() {
        assert_eq!(&v[k], value, "{k}");
    }
    assert_eq!(rows(&v), [row("ont", 0, None)]);
    // The attempt names this repository and the tree id; its review is that of T's chain.
    let start = &w.applies("start_install")[0];
    assert_eq!(start["request_id"], format!("ont.{id}.1"));
    assert_eq!(
        start["parameters"],
        json!({"target": etarget(), "kind": "forward", "repository": "o/r", "commit": id,
            "attempt": 1, "from": from, "from_sha256": sha(&engine_binary(&from)), "by": "session-1",
            "reason": "after a merge", "head": h(2), "review": w.accepting(2),
            "reviews": [w.accepting(1), w.accepting(2)], "host": oid("host", "local")})
    );
    // The installer: engine/scripts/install-ont.sh of this repository's clone, checked out at T
    // before, `install <T>`, with the command's environment.
    let clone = w.p("state/r");
    let launch = w.calls.iter().find(|(p, _, _)| p == "bash").unwrap();
    assert_eq!(
        launch.1,
        args(&[
            &format!("{clone}/engine/scripts/install-ont.sh"),
            "install",
            &c(2)
        ])
    );
    let env: serde_json::Map<String, Value> = w
        .environment()
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    assert_eq!(launch.2, json!({"cwd": clone, "env": env}));
    let checkout = w.calls.iter().position(|(p, a, _)| {
        p == "git" && a.iter().any(|x| x == "checkout") && a.last() == Some(&c(2))
    });
    assert!(checkout.unwrap() < w.at("start_install").unwrap());
    // No engine clone, no fetch of another repository, no ancestor test of a pin.
    assert!(!w.engine_clone_touched(), "{:?}", w.calls);
    assert_eq!(w.git_words("merge-base"), 0);
    assert_eq!(w.git_words("fetch"), 1);
    // The next call finds the engine at the tree id and installs the pair from the same commit.
    w.settle();
    let (v, exit) = w.run();
    assert_eq!(
        (end(&v), exit, v["target"].clone()),
        (("installed", ""), 0, json!(TARGET)),
        "{v}"
    );
    assert_eq!(rows(&v), [row("ap", 0, None), row("content", 0, None)]);
    assert!(!w.engine_clone_touched());
    // A failed build leaves the engine where it was: failed_restored, the rollback not needed.
    let mut w = W::engine_tree();
    w.ont_install = (None, 2);
    let (v, exit) = w.run();
    assert_eq!(
        (v["outcome"].clone(), exit),
        (json!("failed_restored"), 3),
        "{v}"
    );
    assert_eq!(rows(&v), [row("ont", 2, Some("engine_refused"))]);
}

// spec: specs/ap/deliver.md#R66
#[test]
fn r66_engine_tree_the_id_is_compared_and_a_tree_without_engine_is_refused() {
    // With or without the member `engine`: the installed engine against the tree id; at it, the pair
    // with no engine attempt; another id, the engine of the tip.
    for with_engine in [false, true] {
        let mut w = W::engine_tree();
        w.engine = with_engine;
        w.ont = etree();
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit, v["target"].clone()),
            (("installed", ""), 0, json!(TARGET)),
            "{v}"
        );
        assert!(!w.engine_clone_touched());
        let mut w = W::engine_tree();
        w.engine = with_engine;
        let (v, exit) = w.run();
        assert_eq!(
            (end(&v), exit, v["target"].clone(), v["to"].clone()),
            (("installed", ""), 0, json!(ENGINE), json!(etree())),
            "{v}"
        );
    }
    // No engine/: engine_missing before any attempt, with or without `engine`.
    for with_engine in [false, true] {
        let mut w = if with_engine { W::engine() } else { W::new() };
        w.neither = true;
        let (v, exit) = w.run();
        refused(&w, &v, exit, "engine_missing");
        assert_eq!(v["detail"], json!({"tip": c(2), "why": "no engine/"}));
        assert!(w.applies("start_install").is_empty());
        assert!(!w.engine_clone_touched());
    }
    // A tip that still holds the file of the retired pin (an older branch, a rollback target) is read by
    // the tree rule: only `git ls-tree <T> -- engine` is asked, and no file of `ontology/` is shown.
    let old_pin = concat!("ontology/ENGINE", "-PIN");
    let mut w = W::engine_tree();
    w.ont = etree();
    let (v, _) = w.run();
    assert_eq!(end(&v), ("installed", ""), "{v}");
    let reads: Vec<&Vec<String>> = w
        .calls
        .iter()
        .filter(|(p, a, _)| p == "git" && a.iter().any(|x| x == "engine" || x.ends_with(old_pin)))
        .map(|(_, a, _)| a)
        .collect();
    assert_eq!(
        reads,
        [&args(&[
            "-C",
            &w.p("state/r"),
            "ls-tree",
            &c(2),
            "--",
            "engine"
        ])]
    );
}

// spec: specs/ap/deliver.md#R38
// spec: specs/ap/deliver.md#R68
#[test]
fn r68_engine_tree_a_dead_engine_attempt_reaches_no_checkout_and_no_rev_list() {
    let dead = |w: &mut W| {
        let id = oid("install", "act-dead-ont");
        w.installs.push(json!({"id": id, "target": etarget(), "kind": "forward", "repository": "o/r",
            "commit": etree(), "attempt": 1, "from": ec(0), "from_sha256": sha(&engine_binary(&ec(0))),
            "by": "dead", "status": "running", "started_at": STARTED, "running_in": etarget()}));
        id
    };
    // Before the dead attempt is closed, no program checks out or walks a chain.
    let quiet_until_close = |w: &W| {
        let close = w.at("close_install").unwrap();
        w.calls[..close]
            .iter()
            .all(|(p, a, _)| p != "git" || !a.iter().any(|x| x == "checkout" || x == "rev-list"))
    };
    // At its commit: installed, and the call goes on to the pair.
    let mut w = W::engine_tree();
    w.ont = etree();
    let id = dead(&mut w);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["closed"], json!({"install": id, "outcome": "installed"}));
    assert!(quiet_until_close(&w));
    // At its from: not_started; the call goes on and installs the engine of the tree.
    let mut w = W::engine_tree();
    let id = dead(&mut w);
    let (v, _) = w.run();
    assert_eq!(
        v["closed"],
        json!({"install": id, "outcome": "not_started"})
    );
    assert!(quiet_until_close(&w));
    assert_eq!(
        (v["target"].clone(), v["to"].clone()),
        (json!(ENGINE), json!(etree()))
    );
    // At neither: this repository's installer rolls back once, from the clone as it stands.
    for (back, want) in [(Some(ec(0)), "failed_restored"), (None, "failed")] {
        let mut w = W::engine_tree();
        let id = dead(&mut w);
        w.ont = "f".repeat(40);
        w.ont_rollback = (back, 0);
        let script = w.p("state/r/engine/scripts/install-ont.sh");
        std::fs::create_dir_all(w.p("state/r/engine/scripts")).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let (v, _) = w.run();
        let row = w.installs.iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(row["outcome"], want, "{v}");
        let launch = w.calls.iter().find(|(p, _, _)| p == "bash").unwrap();
        assert_eq!(launch.1, args(&[&script, "rollback"]));
        assert_eq!(launch.2["cwd"], json!(w.p("state/r")));
        assert!(quiet_until_close(&w));
        assert!(!w.engine_clone_touched());
    }
    // Without this repository's installer in the clone: attempt_running.
    let mut w = W::engine_tree();
    let id = dead(&mut w);
    w.ont = "f".repeat(40);
    let (v, exit) = w.run();
    refused(&w, &v, exit, "attempt_running");
    assert_eq!(v["detail"], json!({ "install": id }));
}

// spec: specs/ap/deliver.md#R63
// spec: specs/ap/deliver.md#R69
#[test]
fn r69_engine_tree_the_pc_push_depends_only_on_the_local_installed_attempt() {
    let setup = || {
        let mut w = W::engine_tree();
        w.ont = etree();
        (w.file, w.tree, w.bytes) = (Some(c(2)), Some(c(2)), binary(&c(2)));
        w.pc = Some(pc_target(&w));
        std::fs::write(w.p("ont-bin/ont"), fake_ont_script(&etree())).unwrap();
        w
    };
    // No local installed attempt at the tree id: not_accepted, and no rev-list of an engine clone.
    let mut w = setup();
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "not_accepted", "{v}");
    assert!(w.applies("start_install").is_empty());
    assert!(!w.engine_clone_touched());
    assert_eq!(w.git_words("rev-list"), 0);
    // The local attempt at the tree id carries the review: the PC attempt names this repository.
    let mut w = setup();
    let review = w.accepting(2);
    w.installs.push(json!({"id": oid("install", "act-local-tree"), "target": etarget(),
        "host": oid("host", "local"), "kind": "forward", "repository": "o/r", "commit": etree(),
        "attempt": 1, "from": ec(0), "from_sha256": sha(&engine_binary(&ec(0))), "by": "timer:1",
        "head": h(2), "review": review, "reviews": [review], "status": "closed", "outcome": "installed",
        "to": etree(), "started_at": "2026-11-14T08:00:00Z", "closed_at": "2026-11-14T08:01:00Z"}));
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["pc"]["result"], "installed", "{v}");
    assert_eq!(v["pc"]["to"], etree());
    let start = &w.applies("start_install")[0];
    assert_eq!(start["request_id"], format!("pc.pc.{}.1", etree()));
    assert_eq!(start["parameters"]["repository"], "o/r");
    assert_eq!(start["parameters"]["commit"], etree());
    assert_eq!(start["parameters"]["head"], h(2));
    assert!(!w.engine_clone_touched());
}

impl W {
    /// A request whose `etc` names the delivered repository itself (R71): the pair goes from c(0) to
    /// c(2); each run of `host/scripts/install-etc.sh` exits 0.
    fn own() -> Self {
        let mut w = W::new();
        w.own_etc = true;
        w.xruns = vec![(0, String::new()); 3];
        w
    }
    /// The runs of `host/scripts/install-etc.sh`, as `etc-check <E>` or `etc <E>`, and the calls of
    /// `git` on the repository's clone that read its `HEAD` or check it out, from the call `from` on.
    fn own_calls(&self, from: usize) -> Vec<String> {
        let clone = self.p("state/r");
        self.calls[from..]
            .iter()
            .filter_map(|(p, a, body)| {
                if p == "bash" && a[0] == format!("{clone}/host/scripts/install-etc.sh") {
                    assert_eq!(body["cwd"], json!(clone));
                    let name = if a[1] == "check" { "etc-check" } else { "etc" };
                    return Some(format!("{name} {}", a[2..].join(" ")));
                }
                (p == "git" && a.len() > 2 && a[1] == clone && a[2] != "fetch")
                    .then(|| a[2..].join(" "))
            })
            .filter(|w| w.starts_with("etc") || w.starts_with("checkout") || w.contains("HEAD"))
            .collect()
    }
    fn own_tip(&self) -> Option<String> {
        std::fs::read_to_string(self.p("state/etc.tip")).ok()
    }
}
fn own_record(from: Option<&str>, tip: Option<&str>, result: &str) -> Value {
    json!({"repository": "o/r", "from": from, "tip": tip, "result": result, "code": null, "detail": null})
}

// spec: specs/ap/deliver.md#R53
// spec: specs/ap/deliver.md#R71
// spec: specs/ap/deliver.md#R72
// spec: specs/ap/deliver.md#R73
// spec: specs/ap/deliver.md#R74
#[test]
fn etc_same_repository() {
    // The request: the request's own `owner/name` is accepted whatever its URL, in each form; the same
    // name under another owner stays refused.
    let w = W::own();
    for r in [w.request(), w.timer(), w.rollback()] {
        assert_eq!(check_request(&r), (json!({"status": "ok"}), 0), "{r}");
    }
    let mut r = w.request();
    r["etc"] = json!({"repository": "x/r", "url": XURL});
    let (v, exit) = check_request(&r);
    assert_eq!((v["detail"].clone(), exit), (json!({"field": "etc"}), 2));

    // A first call: the pair moves to T = c(2), then the configuration step at T with F null (no call
    // recorded one): `check`, `install` and `check` of `host/scripts/install-etc.sh` of the clone.
    let mut w = W::own();
    let tip = c(2);
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("installed", ""), 0), "{v}");
    assert_eq!(v["etc"], own_record(None, Some(&tip), "installed"));
    let names: Vec<String> = rows(&v).into_iter().map(|r| r.0).collect();
    assert_eq!(names, ["ap", "content", "etc-check", "etc", "etc-check"]);
    assert_eq!(
        w.own_calls(w.at("close_install").unwrap()),
        [
            "rev-parse --verify HEAD^{commit}".to_owned(),
            format!("etc-check {tip}"),
            format!("etc {tip}"),
            format!("etc-check {tip}"),
        ]
    );
    // No second clone and no second fetch: nothing reaches another directory of `<state>`, the URL of
    // `etc` is never read, and the one fetch is the pair's.
    assert!(w.calls.iter().all(|(_, a, _)| {
        !a.iter()
            .any(|x| x.contains("/state/ops") || x.contains("other/r.git"))
    }));
    let fetches = w
        .calls
        .iter()
        .filter(|(p, a, _)| p == "git" && a.get(2).is_some_and(|x| x == "fetch"))
        .count();
    assert_eq!(fetches, 1);
    // The program's environment is the command's, unchanged; its record is F's file.
    let program = w
        .calls
        .iter()
        .find(|(_, a, _)| {
            a.first()
                .is_some_and(|x| x.ends_with("host/scripts/install-etc.sh"))
        })
        .unwrap();
    assert!(program.2["env"].get("CARGO_TARGET_DIR").is_none());
    assert_eq!(w.own_tip(), Some(format!("{tip}\n")));
    assert_eq!(w.last(), Some(v));

    // The next call ends `current`: F is the recorded tip, E the same T, one `check` that answers the
    // host at T, `same`.
    w.xruns = vec![(0, String::new())];
    let (v, exit) = w.run();
    assert_eq!((end(&v), exit), (("current", ""), 0), "{v}");
    assert_eq!(v["etc"], own_record(Some(&tip), Some(&tip), "same"));
    assert_eq!(rows(&v).len(), 1, "{v}");

    // A call whose record has `etc` null (a refusal) keeps F.
    w.fetch_fails = true;
    let (v, _) = w.run();
    assert_eq!(
        (v["code"].clone(), v["etc"].clone()),
        (json!("fetch_failed"), Value::Null)
    );
    assert_eq!(w.own_tip(), Some(format!("{tip}\n")));
    w.fetch_fails = false;

    // The clone's `HEAD` is not T: it is checked out at T (a kind of R47) before the first program; a
    // host that differs at the same T is installed again and checked.
    w.rhead = Some(c(1));
    w.xruns = vec![(1, String::new()), (0, String::new()), (0, String::new())];
    let start = w.calls.len();
    let (v, _) = w.run();
    assert_eq!(v["etc"], own_record(Some(&tip), Some(&tip), "installed"));
    assert_eq!(
        w.own_calls(start),
        [
            "rev-parse --verify HEAD^{commit}".to_owned(),
            format!("checkout --quiet --force --detach {tip}"),
            format!("etc-check {tip}"),
            format!("etc {tip}"),
            format!("etc-check {tip}"),
        ]
    );
    // A checkout that fails runs no program: `fetch_failed` with the tip.
    w.rhead = Some(c(1));
    w.rcheckout_fails = true;
    w.xruns = vec![];
    let (v, _) = w.run();
    assert_eq!(end(&v), ("current", ""), "{v}");
    assert_eq!(v["etc"], own_record(Some(&tip), Some(&tip), "fetch_failed"));
    assert!(etc_rows(&v).is_empty());
    w.rcheckout_fails = false;

    // F's file lost (or never written): a check that answers the host at T still installs, so that
    // the installer's own record follows T.
    std::fs::remove_file(w.p("state/etc.tip")).unwrap();
    w.xruns = vec![(0, String::new()); 3];
    let (v, _) = w.run();
    assert_eq!(v["etc"], own_record(None, Some(&tip), "installed"));
    // A rollback runs no step and leaves F.
    let mut w = W::own();
    w.run();
    installed_by_a(&mut w);
    let (v, _) = w.run_with(&w.rollback());
    assert_eq!(v["etc"], Value::Null, "{v}");
    assert_eq!(w.own_tip(), Some(format!("{tip}\n")));
}
