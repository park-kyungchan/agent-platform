//! `ap watch` (`specs/ap/watch.md`) against a fake host: the fake answers the store (`ont search`,
//! `ont log` and `ont action apply` of the four finding actions, with the schema's criteria), `stat`,
//! `find`, `sh` (for `sshd -T`), `systemctl`, `df`, `ps` and the files a pass reads. The watch directory
//! is a temporary directory: no test reaches the host's store, units or files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Failure, Reply, System};
use ap::watch::RUNTIMES;
use ap::watch::checks::{GITHUB_APPLY, GITHUB_REPOSITORIES};
use ap::watch::detect::{APT, BACKUP, CHECKS_LOG, DELIVERY, DOOR_LOG, MEMORY_EVENTS};
use ap::watch::drift::{
    DELIVER_CLONE, ETC_CLONE, INSTALLED_AP, INVENTORY_PATH, UNIT_DIR, VPS_OPS_REPO,
};
use ap::watch::runtimes::flag_on_line;
use ap::watch::{epoch, execute, family, key, utc};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::ErrorKind,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::{Child, Command},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// 2026-11-19T00:00:00Z.
const BASE: u64 = 1_795_046_400;
const TOTP: &str = "/invented/root/.totp-secret";
const SOCKET_D: &str = "/etc/systemd/system/ssh.socket.d";
const OVERRIDE: &str = "/etc/systemd/system/ssh.socket.d/override.conf";
const REQUIRES: &str = "/etc/systemd/system/ssh.service.requires";

fn ok(stdout: &str) -> Answer {
    Ok(Reply {
        success: true,
        stdout: stdout.as_bytes().to_vec(),
        bytes: stdout.as_bytes().to_vec(),
    })
}
fn failed(stderr: &str) -> Answer {
    Ok(Reply {
        success: false,
        stdout: vec![],
        bytes: stderr.as_bytes().to_vec(),
    })
}
fn json_reply(success: bool, v: &Value) -> Answer {
    let bytes = serde_json::to_vec(v).unwrap();
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn broken(program: &str) -> Answer {
    Err(Failure {
        code: "process_failed",
        field: program.into(),
        detail: Value::Null,
    })
}
fn dup(a: &Answer) -> Answer {
    match a {
        Ok(r) => Ok(Reply {
            success: r.success,
            bytes: r.bytes.clone(),
            stdout: r.stdout.clone(),
        }),
        Err(f) => Err(Failure {
            code: f.code,
            field: f.field.clone(),
            detail: f.detail.clone(),
        }),
    }
}
fn wrap(fields: &Value) -> Value {
    let o = fields.as_object().unwrap().iter();
    o.map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect()
}
fn fid(key: &str) -> Value {
    json!({"object_type": "finding", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": key})
}
/// A timestamp as `systemctl show` prints it.
fn shown(secs: u64) -> String {
    let t = utc(secs);
    format!("Thu {} {} UTC", &t[..10], &t[11..19])
}

/// The commit the installed `ap` names (R28).
fn installed() -> String {
    "b".repeat(40)
}
/// The tip of delivery's last fetch of the operations repository (R29).
fn ops_tip() -> String {
    "c".repeat(40)
}

/// The fake host.
struct F {
    dir: PathBuf,
    wall: Cell<u64>,
    uuid: Cell<u32>,
    /// Files `System::read` answers; an absent one is `NotFound`.
    files: BTreeMap<String, Vec<u8>>,
    /// Paths whose read is refused (`PermissionDenied`).
    unreadable: BTreeSet<String>,
    /// The type `stat -c %F` prints per path; an absent path is not found.
    types: BTreeMap<String, String>,
    /// Paths whose `stat` fails other than by not finding them.
    stat_fails: BTreeSet<String>,
    /// What `stat -c '%a %F'` prints per path (the key file of R31); an absent path is not found.
    modes: BTreeMap<String, String>,
    /// `find` output per directory; a directory in `find_fails` cannot be listed.
    finds: BTreeMap<String, String>,
    find_fails: BTreeSet<String>,
    /// The output of `sh -c 'sshd -T ...'`.
    sshd: String,
    /// `delivery`'s record and its age in seconds by the modification time; None: absent.
    deliver: Option<Value>,
    deliver_age: u64,
    /// The backup's record; its `at` is the pass's time less `backup_age` when `at` is null.
    backup: Option<Value>,
    backup_age: u64,
    /// `systemctl show` properties per unit; `trigger_age` sets a timer's `LastTriggerUSec`.
    units: BTreeMap<String, BTreeMap<String, String>>,
    trigger_age: BTreeMap<String, u64>,
    /// The lines of `systemctl --failed`.
    failed_units: Vec<String>,
    df: String,
    ps: String,
    /// Programs whose calls cannot run.
    dead: BTreeSet<String>,
    /// The store: findings by key; applied action rows of the log.
    findings: BTreeMap<String, Value>,
    log: Vec<Value>,
    /// Actions the store refuses with an error code.
    refuse: BTreeMap<String, String>,
    /// Policy file path (`policy_file()`); None unless set.
    policy: Option<PathBuf>,
    /// `run` objects in the store, keyed by native key.
    runs: BTreeMap<String, Value>,
    /// `gh run list -R <repo>` answers per repository.
    gh_runs: BTreeMap<String, Answer>,
    /// `gh api repos/<repo>/dependabot/alerts...` answers per repository.
    gh_alerts: BTreeMap<String, Answer>,
    /// `python3 -I -B GITHUB_APPLY --check ...` answer.
    apply_check: Answer,
    /// `<cli> --version` answers by runtime name.
    cli_version: BTreeMap<String, Answer>,
    /// `<cli> [<subcommand>] --help` answers by `(name, subcommand)`.
    cli_help: BTreeMap<(String, String), Answer>,
    /// `git -C VPS_OPS_REPO rev-parse` answer.
    git_commit: Answer,
    /// `git -C VPS_OPS_REPO show <commit>:deployments/host-artifacts.json` answer.
    git_show: Answer,
    /// `git -C VPS_OPS_REPO cat-file -t <commit>:<dest>` answers by `<commit>:<dest>`.
    git_cat_file: BTreeMap<String, Answer>,
    /// `/usr/local/bin/ap --version` (R28).
    ap_version: Answer,
    /// `deploy/` at the commit the installed `ap` names, in delivery's clone: name, mode, type, bytes.
    deploy: BTreeMap<String, (String, String, Vec<u8>)>,
    /// `etc/` at the fetched tip of delivery's clone of the operations repository, by path; and the
    /// answer of its `for-each-ref` (R29). The clone exists when `<clone>/.git/HEAD` is a file.
    etc: BTreeMap<String, (String, String, Vec<u8>)>,
    etc_ref: Answer,
    /// The `git` calls that fail, as `<clone> <word>`.
    git_fails: BTreeSet<String>,
    calls: RefCell<Vec<(String, Vec<String>, Value)>>,
    reads: RefCell<Vec<String>>,
}
impl F {
    /// A healthy host: every detector reads and yields nothing once `login.json` is recorded.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ap-watch-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = F {
            dir: dir.join("watch"),
            wall: Cell::new(BASE),
            uuid: Cell::new(0),
            files: BTreeMap::new(),
            unreadable: BTreeSet::new(),
            types: BTreeMap::new(),
            stat_fails: BTreeSet::new(),
            modes: BTreeMap::new(),
            finds: BTreeMap::new(),
            find_fails: BTreeSet::new(),
            sshd: "port 22\npasswordauthentication no\nexit 0\n".into(),
            deliver: Some(
                json!({"at": "2026-11-18T23:59:00Z", "end": "current", "code": null,
                "outcome": null, "tip": "a".repeat(40), "checkouts": []}),
            ),
            deliver_age: 60,
            backup: Some(
                json!({"at": null, "backup": "/invented/backups/b.sqlite", "remote": "copied", "code": null}),
            ),
            backup_age: 600,
            units: BTreeMap::new(),
            trigger_age: BTreeMap::new(),
            failed_units: vec![],
            df: " Avail\n35816M\n".into(),
            ps: "      1 1018769 Sat Nov  7 07:03:38 2026 systemd\n".into(),
            dead: BTreeSet::new(),
            findings: BTreeMap::new(),
            log: vec![],
            refuse: BTreeMap::new(),
            policy: None,
            runs: BTreeMap::new(),
            gh_runs: BTreeMap::new(),
            gh_alerts: BTreeMap::new(),
            apply_check: json_reply(true, &json!({"mode": "check", "repositories": []})),
            cli_version: BTreeMap::new(),
            cli_help: BTreeMap::new(),
            git_commit: ok(&format!("{}\n", "a".repeat(40))),
            git_show: ok("[]"),
            git_cat_file: BTreeMap::new(),
            ap_version: ok(&format!("ap 0.1.0 {}\n", installed())),
            deploy: BTreeMap::new(),
            etc: BTreeMap::new(),
            etc_ref: ok(&format!("{}\n", ops_tip())),
            git_fails: BTreeSet::new(),
            calls: RefCell::default(),
            reads: RefCell::default(),
        };
        for spec in &RUNTIMES {
            f.cli_version
                .insert(spec.name.into(), ok(&format!("{} 1.0.0\n", spec.name)));
            if spec.subcommands.is_empty() {
                let help = spec
                    .flags
                    .iter()
                    .map(|fl| format!("  {fl}  flag\n"))
                    .collect::<String>();
                f.cli_help
                    .insert((spec.name.into(), String::new()), ok(&help));
            } else {
                f.cli_help.insert(
                    (spec.name.into(), String::new()),
                    ok("Commands:\n  exec  run a stage\n"),
                );
                let help = spec
                    .flags
                    .iter()
                    .map(|fl| format!("  {fl}  flag\n"))
                    .collect::<String>();
                for sub in spec.subcommands {
                    f.cli_help
                        .insert((spec.name.into(), (*sub).into()), ok(&help));
                }
            }
        }
        f.files.insert(
            GITHUB_REPOSITORIES.into(),
            json!({"rulesets": [], "repositories": []})
                .to_string()
                .into_bytes(),
        );
        f.file("/etc/ssh/sshd_config.d", "directory", None);
        f.finds.insert(
            "/etc/ssh/sshd_config.d".into(),
            "50-cloud-init.conf\tf\t\n".into(),
        );
        f.files.insert(
            "/etc/ssh/sshd_config.d/50-cloud-init.conf".into(),
            b"PasswordAuthentication no\n".to_vec(),
        );
        f.file(
            "/etc/pam.d/sshd",
            "regular file",
            Some("@include common-auth\n"),
        );
        f.file(SOCKET_D, "directory", None);
        f.finds
            .insert(SOCKET_D.into(), "override.conf\tf\t\n".into());
        f.file(OVERRIDE, "regular file", Some("[Socket]\n"));
        f.file("/etc/sudoers", "regular file", Some("root ALL=(ALL) ALL\n"));
        f.file("/etc/sudoers.d", "directory", None);
        f.finds
            .insert("/etc/sudoers.d".into(), "README\tf\t\n".into());
        f.files
            .insert("/etc/sudoers.d/README".into(), b"# readme\n".to_vec());
        f.file(
            "/usr/lib/systemd/system/ssh.service",
            "regular file",
            Some("[Service]\nExecStart=/usr/sbin/sshd -D\n"),
        );
        f.file(
            "/usr/lib/systemd/system/ssh.socket",
            "regular file",
            Some("[Socket]\nListenStream=22\n"),
        );
        f.show(
            "ssh.service",
            &[
                ("FragmentPath", "/usr/lib/systemd/system/ssh.service"),
                ("DropInPaths", ""),
            ],
        );
        f.show(
            "ssh.socket",
            &[
                ("FragmentPath", "/usr/lib/systemd/system/ssh.socket"),
                ("DropInPaths", OVERRIDE),
            ],
        );
        f.types.insert(TOTP.into(), "regular file".into());
        for name in ["ap-deliver", "ap-quota", "ont-backup", "ap-watch"] {
            f.show(
                &format!("{name}.timer"),
                &[("LoadState", "loaded"), ("ActiveState", "active")],
            );
            f.show(
                &format!("{name}.service"),
                &[("LoadState", "loaded"), ("ActiveState", "inactive")],
            );
            f.trigger_age.insert(format!("{name}.timer"), 5);
        }
        // The unit files of the installed commit, each on the host with its bytes (R28).
        for name in ["ap-watch.service", "ap-watch.timer"] {
            let bytes = format!("[Unit]\nDescription={name}\n").into_bytes();
            f.deploy
                .insert(name.into(), ("100644".into(), "blob".into(), bytes.clone()));
            f.files.insert(format!("{UNIT_DIR}/{name}"), bytes);
        }
        f.files
            .insert("/proc/uptime".into(), b"100000.00 190000.00\n".to_vec());
        f.files
            .insert(MEMORY_EVENTS.into(), b"low 0\nhigh 5\nmax 0\n".to_vec());
        f.files.insert(
            APT.into(),
            b"\nStart-Date: 2026-11-01  08:05:21\nCommandline: apt-get upgrade\nEnd-Date: 2026-11-01  08:06:00\n"
                .to_vec(),
        );
        f
    }
    fn file(&mut self, path: &str, t: &str, bytes: Option<&str>) {
        self.types.insert(path.into(), t.into());
        if let Some(b) = bytes {
            self.files.insert(path.into(), b.as_bytes().to_vec());
        }
    }
    fn show(&mut self, unit: &str, props: &[(&str, &str)]) {
        let u = self.units.entry(unit.into()).or_default();
        for (k, v) in props {
            u.insert((*k).into(), (*v).into());
        }
    }
    fn now(&self) -> u64 {
        self.wall.get()
    }
    /// The clock at `m` minutes after the base.
    fn at(&self, m: u64) {
        self.wall.set(BASE + m * 60);
    }
    fn run(&mut self, words: &[&str]) -> (Value, u8) {
        let argv: Vec<OsString> = words.iter().map(OsString::from).collect();
        let dir = self.dir.clone();
        execute(self, &dir, &argv)
    }
    fn pass(&mut self) -> Value {
        let (v, exit) = self.run(&[]);
        assert_eq!(exit, 0, "{v}");
        v
    }
    fn record(&mut self) {
        let (v, exit) = self.run(&["--record-login", "--totp-file", TOTP]);
        assert_eq!(exit, 0, "{v}");
    }
    /// A host with `login.json` recorded and one quiet pass made (the baseline of `state.json`).
    fn quiet() -> Self {
        let mut f = F::new();
        f.record();
        let v = f.pass();
        assert_eq!(v["opened"], json!([]), "{v}");
        f.reset();
        f
    }
    fn reset(&mut self) {
        self.calls.borrow_mut().clear();
        self.reads.borrow_mut().clear();
    }
    /// The applied or refused actions, as (action, key).
    fn actions(&self) -> Vec<(String, String)> {
        let calls = self.calls.borrow();
        calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1.starts_with(&["action".to_owned(), "apply".to_owned()]))
            .map(|c| {
                let p = &c.2["parameters"]["finding"]["native_key"];
                (
                    c.2["action_type"].as_str().unwrap().to_owned(),
                    p.as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
    fn bodies(&self) -> Vec<Value> {
        let calls = self.calls.borrow();
        calls
            .iter()
            .filter(|c| c.0 == "ont" && c.1.starts_with(&["action".to_owned(), "apply".to_owned()]))
            .map(|c| c.2.clone())
            .collect()
    }
    fn finding(&self, k: &str) -> &Value {
        self.findings
            .get(k)
            .unwrap_or_else(|| panic!("no finding {k}"))
    }
    fn plant(&mut self, k: &str, fields: Value) {
        self.findings.insert(k.into(), fields);
    }
    fn state(&self) -> Value {
        let b = std::fs::read(self.dir.join("state.json")).unwrap();
        serde_json::from_slice(&b).unwrap()
    }
    fn programs(&self) -> Vec<(String, Vec<String>)> {
        let calls = self.calls.borrow();
        calls.iter().map(|c| (c.0.clone(), c.1.clone())).collect()
    }

    /// `git -C <delivery's clone>`: `ls-tree` and `show` of the installed commit's `deploy/`, and of
    /// the operations clone `for-each-ref`, `ls-tree -r -z` and `show` of `etc/` at its tip.
    fn clone_git(&self, args: &[String]) -> Answer {
        let (clone, word) = (args[1].as_str(), args[2].as_str());
        if self.git_fails.contains(&format!("{clone} {word}")) {
            return failed("fatal: bad object\n");
        }
        let a: Vec<&str> = args[2..].iter().map(String::as_str).collect();
        let entry = |mode: &str, kind: &str, b: &[u8], path: &str| {
            format!(
                "{mode} {kind} {}\t{path}",
                &sha(&String::from_utf8_lossy(b))[..40]
            )
        };
        let bytes = |b: &[u8]| {
            Ok(Reply {
                success: true,
                stdout: b.to_vec(),
                bytes: b.to_vec(),
            })
        };
        match (clone, a.as_slice()) {
            (DELIVER_CLONE, ["ls-tree", k, "deploy/"]) => {
                assert_eq!(*k, installed());
                let lines: String = self
                    .deploy
                    .iter()
                    .map(|(n, (m, t, b))| entry(m, t, b, &format!("deploy/{n}")) + "\n")
                    .collect();
                ok(&lines)
            }
            (DELIVER_CLONE, ["show", spec]) => {
                let name = spec
                    .strip_prefix(&format!("{}:deploy/", installed()))
                    .unwrap_or_else(|| panic!("unexpected show {spec}"));
                bytes(&self.deploy[name].2)
            }
            (
                ETC_CLONE,
                [
                    "for-each-ref",
                    "--format=%(objectname)",
                    "refs/remotes/origin/main",
                ],
            ) => dup(&self.etc_ref),
            (ETC_CLONE, ["ls-tree", "-r", "-z", tip, "etc/"]) => {
                assert_eq!(*tip, ops_tip());
                let all: String = self
                    .etc
                    .iter()
                    .map(|(p, (m, t, b))| entry(m, t, b, p) + "\0")
                    .collect();
                ok(&all)
            }
            (ETC_CLONE, ["show", spec]) => {
                let path = spec
                    .strip_prefix(&format!("{}:", ops_tip()))
                    .unwrap_or_else(|| panic!("unexpected show {spec}"));
                bytes(&self.etc[path].2)
            }
            _ => panic!("unexpected git {args:?}"),
        }
    }
    fn systemctl(&self, args: &[String]) -> Answer {
        if args[0] == "--failed" {
            let lines: String = self.failed_units.iter().map(|l| format!("{l}\n")).collect();
            return ok(&lines);
        }
        assert_eq!(args[0], "show", "{args:?}");
        let (mut props, mut names) = (vec![], vec![]);
        let mut words = args[1..].iter();
        while let Some(w) = words.next() {
            match w.as_str() {
                "--" => {
                    names.extend(words.cloned());
                    break;
                }
                "-p" => props.push(words.next().unwrap().clone()),
                u => names.push(u.to_owned()),
            }
        }
        let mut blocks = vec![];
        for name in names {
            let u = self.units.get(&name);
            let mut lines = vec![];
            for p in &props {
                let v = match (p.as_str(), u) {
                    ("Id", _) => name.clone(),
                    ("LastTriggerUSec", _) => match self.trigger_age.get(&name) {
                        Some(a) => shown(self.now() - a),
                        None => u.and_then(|u| u.get(p).cloned()).unwrap_or_default(),
                    },
                    ("LoadState", None) => "not-found".into(),
                    ("ActiveState", None) => "inactive".into(),
                    (_, Some(u)) => u.get(p).cloned().unwrap_or_default(),
                    (_, None) => String::new(),
                };
                lines.push(format!("{p}={v}"));
            }
            blocks.push(lines.join("\n"));
        }
        ok(&format!("{}\n", blocks.join("\n\n")))
    }
    fn ont(&mut self, args: &[String], input: &Value) -> Answer {
        assert!(!args.contains(&"--store".to_owned()), "the host's store");
        match args[0].as_str() {
            "search" => match input["object_type"].as_str() {
                Some("finding") => {
                    let eq = &input["where"]["all"][0]["eq"];
                    assert_eq!(eq[0], "detector");
                    let objects: Vec<Value> = self
                        .findings
                        .iter()
                        .filter(|(_, f)| f["detector"] == eq[1])
                        .map(|(k, f)| json!({"id": fid(k), "head": {"fields": wrap(f)}}))
                        .collect();
                    json_reply(
                        true,
                        &json!({"status": "ok", "result": {"basis": {"qualified": true},
                        "objects": objects, "continuation": null}}),
                    )
                }
                Some("run") if input["where"]["all"][0]["eq"] == json!(["role", "batch"]) => {
                    // `batch_runs` of `crates/ap/src/api.rs`: role `batch`, started after a time.
                    let gt = &input["where"]["all"][1]["gt"];
                    assert_eq!(gt[0], "started_at");
                    let after = gt[1].as_str().and_then(epoch).unwrap();
                    let objects: Vec<Value> = self
                        .runs
                        .iter()
                        .filter(|(_, f)| f["role"] == "batch")
                        .filter(|(_, f)| {
                            f["started_at"]
                                .as_str()
                                .and_then(epoch)
                                .is_some_and(|t| t > after)
                        })
                        .map(|(k, f)| json!({
                            "id": {"object_type": "run", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": k},
                            "head": {"fields": wrap(f)}
                        }))
                        .collect();
                    json_reply(
                        true,
                        &json!({"status": "ok", "result": {"basis": {"qualified": true},
                        "objects": objects, "continuation": null}}),
                    )
                }
                Some("run") => {
                    let ge = &input["where"]["all"][0]["ge"];
                    assert_eq!(ge[0], "started_at");
                    let since = ge[1].as_str().and_then(epoch).unwrap();
                    let objects: Vec<Value> = self
                        .runs
                        .iter()
                        .filter(|(_, f)| {
                            f["started_at"]
                                .as_str()
                                .and_then(epoch)
                                .is_some_and(|t| t >= since)
                        })
                        .map(|(k, f)| json!({
                            "id": {"object_type": "run", "namespace": {"instance": "vps://palantirkc", "account": "root"}, "native_key": k},
                            "head": {"fields": wrap(f)}
                        }))
                        .collect();
                    json_reply(
                        true,
                        &json!({"status": "ok", "result": {"basis": {"qualified": true},
                        "objects": objects, "continuation": null}}),
                    )
                }
                other => panic!("search {other:?}"),
            },
            "log" => {
                let since = input["since"].as_str().and_then(epoch).unwrap();
                assert_eq!(input["outcome"], "applied");
                let rows: Vec<&Value> = (self.log.iter())
                    .filter(|r| r["submitted_at"].as_str().and_then(epoch).unwrap() >= since)
                    .take(1)
                    .collect();
                json_reply(
                    true,
                    &json!({"status": "ok", "result": {"rows": rows, "withheld": 0, "skipped": 0,
                    "scan_limited": false, "continuation": null}}),
                )
            }
            "action" => self.apply(input),
            other => panic!("ont {other}"),
        }
    }
    fn apply(&mut self, b: &Value) -> Answer {
        let action = b["action_type"].as_str().unwrap().to_owned();
        // Every apply of `ap watch` claims `watch` (specs/ap/roles.md R3).
        assert_eq!(b["role"], "watch", "{b}");
        if let Some(code) = self.refuse.get(&action) {
            return json_reply(false, &json!({"status": "refused", "code": code}));
        }
        let p = &b["parameters"];
        let k = p["finding"]["native_key"].as_str().unwrap().to_owned();
        let now = utc(self.now());
        let not = |codes: &str| {
            json_reply(
                false,
                &json!({"status": "refused", "code": "submission_criteria", "failed": [codes]}),
            )
        };
        let state = self.findings.get(&k).map(|f| f["state"].clone());
        let live = matches!(
            state.as_ref().and_then(Value::as_str),
            Some("open" | "planned")
        );
        match action.as_str() {
            "open_finding" => {
                if state.is_some() {
                    return json_reply(
                        false,
                        &json!({"status": "refused", "code": "revision_conflict"}),
                    );
                }
                for (field, max) in [("summary", 600), ("evidence", 2_000), ("detector", 60)] {
                    assert!(p[field].as_str().unwrap().len() <= max, "{field}");
                }
                self.findings.insert(
                    k.clone(),
                    json!({"kind": p["kind"], "detector": p["detector"], "summary": p["summary"],
                    "evidence": p["evidence"], "severity": p["severity"], "state": "open",
                    "first_seen_at": now, "last_seen_at": now}),
                );
                // Schema 24: the rule sets `subject` when it is given.
                if p.get("subject").is_some() {
                    self.findings.get_mut(&k).unwrap()["subject"] = p["subject"].clone();
                }
            }
            "see_finding_again" => {
                if !live {
                    return not("finding_not_live");
                }
                let f = self.findings.get_mut(&k).unwrap();
                f["evidence"] = p["evidence"].clone();
                if p.get("severity").is_some() {
                    f["severity"] = p["severity"].clone();
                }
                f["last_seen_at"] = json!(now);
                if p.get("subject").is_some() {
                    f["subject"] = p["subject"].clone();
                }
            }
            "reopen_finding" => {
                if live || state.is_none() {
                    return not("finding_live");
                }
                assert!(!p["evidence"].as_str().unwrap().is_empty());
                let f = self.findings.get_mut(&k).unwrap();
                (f["evidence"], f["state"], f["last_seen_at"]) =
                    (p["evidence"].clone(), json!("open"), json!(now));
                if p.get("subject").is_some() {
                    f["subject"] = p["subject"].clone();
                }
            }
            "close_finding" => {
                if !live {
                    return not("finding_not_live");
                }
                let f = self.findings.get_mut(&k).unwrap();
                (f["state"], f["reason"], f["ended_at"]) =
                    (json!("closed"), p["reason"].clone(), json!(now));
            }
            other => panic!("action {other}"),
        }
        json_reply(
            true,
            &json!({"status": "ok", "result": {"outcome": "applied", "edits": []}}),
        )
    }
}
impl System for F {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        self.calls
            .borrow_mut()
            .push((program.into(), args.to_vec(), input.clone()));
        assert!(ms <= 30_000);
        if self.dead.contains(program) {
            return broken(program);
        }
        match program {
            "ont" => self.ont(args, input),
            "systemctl" => self.systemctl(args),
            "stat" => {
                let path = args[2].as_str();
                if self.stat_fails.contains(path) {
                    return failed(&format!("stat: cannot statx '{path}': Permission denied\n"));
                }
                if args[1] == "%a %F" {
                    return match self.modes.get(path) {
                        Some(m) => ok(&format!("{m}\n")),
                        None => failed(&format!(
                            "stat: cannot statx '{path}': No such file or directory\n"
                        )),
                    };
                }
                if args[1] == "%Y" {
                    assert_eq!(path, DELIVERY);
                    return match self.deliver {
                        Some(_) => ok(&format!("{}\n", self.now() - self.deliver_age)),
                        None => failed("stat: cannot statx: No such file or directory\n"),
                    };
                }
                assert_eq!(args[..2], ["-c", "%F"]);
                match self.types.get(path) {
                    Some(t) => ok(&format!("{t}\n")),
                    None => failed(&format!(
                        "stat: cannot statx '{path}': No such file or directory\n"
                    )),
                }
            }
            "find" => {
                let dir = args[0].as_str();
                assert_eq!(
                    args[1..],
                    [
                        "-mindepth",
                        "1",
                        "-maxdepth",
                        "1",
                        "-printf",
                        "%f\\t%y\\t%l\\n"
                    ]
                );
                assert_eq!(self.types.get(dir).map(String::as_str), Some("directory"));
                match self.find_fails.contains(dir) {
                    true => failed("find: Permission denied\n"),
                    false => ok(self.finds.get(dir).map_or("", String::as_str)),
                }
            }
            "sh" => {
                assert_eq!(args[0], "-c");
                assert!(args[1].starts_with("sshd -T"));
                ok(&self.sshd.clone())
            }
            "df" => {
                assert_eq!(args, ["-BM", "--output=avail", "/var/lib/agent-platform"]);
                ok(&self.df.clone())
            }
            "ps" => {
                assert_eq!(args, ["-eo", "pid=,etimes=,lstart=,comm="]);
                ok(&self.ps.clone())
            }
            "gh" => {
                if args.first().map(String::as_str) == Some("run") {
                    assert_eq!(args[1], "list");
                    assert_eq!(args[2], "-R");
                    self.gh_runs.get(&args[3]).map_or_else(|| ok("[]"), dup)
                } else if args.first().map(String::as_str) == Some("api") {
                    let rest = args[1]
                        .strip_prefix("repos/")
                        .and_then(|r| r.strip_suffix("/dependabot/alerts?state=open&per_page=100"))
                        .unwrap_or_else(|| panic!("unexpected gh api {args:?}"));
                    self.gh_alerts.get(rest).map_or_else(|| ok("[]"), dup)
                } else {
                    panic!("unexpected gh {args:?}")
                }
            }
            "python3" => {
                assert_eq!(
                    args,
                    [
                        "-I",
                        "-B",
                        GITHUB_APPLY,
                        "--check",
                        "--file",
                        GITHUB_REPOSITORIES
                    ]
                );
                dup(&self.apply_check)
            }
            "git"
                if args
                    .get(1)
                    .is_some_and(|c| c == DELIVER_CLONE || c == ETC_CLONE) =>
            {
                self.clone_git(args)
            }
            INSTALLED_AP => {
                assert_eq!(args, ["--version"]);
                dup(&self.ap_version)
            }
            "git" => {
                assert_eq!(args[0], "-C");
                assert_eq!(args[1], VPS_OPS_REPO);
                match args[2].as_str() {
                    "rev-parse" => dup(&self.git_commit),
                    "show" => {
                        assert!(args[3].ends_with(&format!(":{INVENTORY_PATH}")), "{args:?}");
                        dup(&self.git_show)
                    }
                    "cat-file" => {
                        assert_eq!(args[3], "-t");
                        self.git_cat_file
                            .get(&args[4])
                            .map_or_else(|| ok("blob\n"), dup)
                    }
                    other => panic!("unexpected git {other} {args:?}"),
                }
            }
            "claude" | "muse" | "agy" => {
                if args == ["--version"] {
                    self.cli_version
                        .get(program)
                        .map_or_else(|| broken(program), dup)
                } else if args == ["--help"] {
                    self.cli_help
                        .get(&(program.into(), String::new()))
                        .map_or_else(|| broken(program), dup)
                } else if args.len() == 2 && args[1] == "--help" {
                    self.cli_help
                        .get(&(program.into(), args[0].clone()))
                        .map_or_else(|| broken(program), dup)
                } else {
                    panic!("unexpected cli {program} {args:?}")
                }
            }
            other => panic!("unexpected program {other} {args:?}"),
        }
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.into());
        assert_ne!(path, TOTP, "the TOTP file is read");
        if path == "/proc/sys/kernel/random/uuid" {
            let n = self.uuid.get() + 1;
            self.uuid.set(n);
            return Ok(format!("{n:08x}-1111-2222-3333-444455556666\n").into_bytes());
        }
        if self.unreadable.contains(path) {
            return Err(ErrorKind::PermissionDenied.into());
        }
        let record = |v: &Option<Value>, age: u64| -> std::io::Result<Vec<u8>> {
            let mut v = v.clone().ok_or(ErrorKind::NotFound)?;
            if v["at"].is_null() {
                v["at"] = json!(utc(self.now() - age));
            }
            Ok(v.to_string().into_bytes())
        };
        match path {
            DELIVERY => record(&self.deliver, self.deliver_age),
            BACKUP => record(&self.backup, self.backup_age),
            _ => self
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| ErrorKind::NotFound.into()),
        }
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.now())
    }
}
impl Drop for F {
    fn drop(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
fn k(detector: &str, signature: &str) -> String {
    key(detector, signature)
}
/// A finding as the pass would have opened it, in `state`, last seen at `seen` (seconds).
fn stored(detector: &str, summary: &str, severity: &str, state: &str, seen: u64) -> Value {
    json!({"kind": "incident", "detector": format!("watch:{detector}"), "summary": summary,
        "evidence": "e", "severity": severity, "state": state, "first_seen_at": utc(seen),
        "last_seen_at": utc(seen)})
}
fn probe(f: &mut F, unit: &str) {
    f.failed_units
        .push(format!("{unit} loaded failed failed {unit}"));
    f.show(
        unit,
        &[
            ("Result", "exit-code"),
            ("ExecMainStatus", "1"),
            ("StateChangeTimestamp", "Thu 2026-11-19 00:00:00 UTC"),
            ("InactiveEnterTimestamp", "Thu 2026-11-19 00:00:00 UTC"),
            (
                "ExecStart",
                "{ path=/bin/false ; argv[]=/bin/false secret-argument }",
            ),
        ],
    );
}

// ---- The form, the lock, the record of the login reading.

// spec: specs/ap/watch.md#R1
#[test]
fn r1_another_argument_is_invalid_usage_and_nothing_is_done() {
    let bad: [&[&str]; 8] = [
        &["--store", "/x"],
        &["pass"],
        &["--record-login"],
        &["--totp-file", TOTP],
        &["--replace"],
        &["--record-login", "--totp-file", "relative/path"],
        &["--record-login", "--record-login", "--totp-file", TOTP],
        &[
            "--record-login",
            "--totp-file",
            TOTP,
            "--replace",
            "--replace",
        ],
    ];
    for words in bad {
        let mut f = F::new();
        let (v, exit) = f.run(words);
        assert_eq!(exit, 2, "{words:?}");
        assert_eq!(v["error"]["code"], "invalid_usage", "{words:?}");
        assert!(f.calls.borrow().is_empty() && f.reads.borrow().is_empty());
        assert!(!f.dir.exists(), "{words:?}: the directory was made");
    }
}

// spec: specs/ap/watch.md#R2
#[test]
fn r2_a_held_lock_is_watch_busy_with_nothing_done() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = F::new();
    std::fs::create_dir_all(&f.dir).unwrap();
    let held = std::fs::File::create(f.dir.join("pass.lock")).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    for words in [&[][..], &["--record-login", "--totp-file", TOTP][..]] {
        let (v, exit) = f.run(words);
        assert_eq!((v, exit), (json!({"status": "watch_busy"}), 0));
    }
    assert!(f.calls.borrow().is_empty() && f.reads.borrow().is_empty());
    assert!(!f.dir.join("state.json").exists() && !f.dir.join("login.json").exists());
    drop(held);
    std::fs::remove_dir_all(&f.dir).unwrap();
    f.pass();
    let mode = std::fs::metadata(&f.dir).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

// spec: specs/ap/watch.md#R3
#[test]
fn r3_record_login_writes_once_refuses_and_never_reads_the_totp_file() {
    let mut f = F::new();
    let (v, exit) = f.run(&["--totp-file", TOTP, "--record-login"]);
    assert_eq!(exit, 0, "{v}");
    assert_eq!(v["totp_file"], TOTP);
    assert_eq!(v["parts"]["totp file"], json!({"type": "regular file"}));
    let parts = v["parts"].as_object().unwrap();
    assert_eq!(parts.len(), ap::watch::login::PARTS.len());
    let written: Value =
        serde_json::from_slice(&std::fs::read(f.dir.join("login.json")).unwrap()).unwrap();
    assert_eq!(written, v);
    let (v, exit) = f.run(&["--record-login", "--totp-file", TOTP]);
    assert_eq!(
        (v["error"]["code"].as_str(), exit),
        (Some("login_recorded"), 2)
    );
    f.types.insert(TOTP.into(), "directory".into());
    let (v, exit) = f.run(&["--record-login", "--totp-file", TOTP, "--replace"]);
    assert_eq!(
        (v["error"]["code"].as_str(), exit),
        (Some("totp_file_absent"), 2)
    );
    f.types.remove(TOTP);
    let (v, exit) = f.run(&["--record-login", "--replace", "--totp-file", TOTP]);
    assert_eq!(
        (v["error"]["code"].as_str(), exit),
        (Some("totp_file_absent"), 2)
    );
    f.types.insert(TOTP.into(), "regular file".into());
    f.sshd = "port 2222\nexit 0\n".into();
    let (v, exit) = f.run(&["--record-login", "--replace", "--totp-file", TOTP]);
    assert_eq!(exit, 0, "{v}");
    assert_eq!(
        v["parts"]["sshd -T"],
        json!(format!("sha256 {}", sha("port 2222")))
    );
    // The fake's `read` fails the test on a read of the TOTP path; it was stated, never read.
    assert!(!f.reads.borrow().iter().any(|p| p == TOTP));
    let stated = f
        .programs()
        .iter()
        .any(|(p, a)| p == "stat" && a[2] == TOTP);
    assert!(stated);
    assert!(f.findings.is_empty(), "--record-login applies no action");
}
fn sha(t: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(t.as_bytes()))
}

// ---- The rules of a pass.

// spec: specs/ap/watch.md#R4
// spec: specs/ap/watch.md#R5
#[test]
fn r5_a_signal_opens_one_finding_and_the_next_pass_records_nothing_for_it() {
    let mut f = F::quiet();
    probe(&mut f, "watch-probe-1.service");
    let v = f.pass();
    let kp = k("services", "failed watch-probe-1.service");
    assert_eq!(v["opened"], json!([kp]));
    let o = f.finding(&kp);
    assert_eq!(o["detector"], "watch:services");
    assert_eq!(
        (o["kind"].as_str(), o["severity"].as_str()),
        (Some("incident"), Some("act"))
    );
    assert!(
        o["summary"]
            .as_str()
            .unwrap()
            .starts_with("failed watch-probe-1.service")
    );
    let evidence = o["evidence"].as_str().unwrap();
    assert!(evidence.contains("Result=exit-code") && evidence.contains("ExecMainStatus=1"));
    assert!(!evidence.contains("secret-argument") && !evidence.contains("/bin/false"));
    f.reset();
    f.at(5);
    let v = f.pass();
    assert!(f.actions().is_empty(), "{:?}", f.actions());
    assert_eq!(
        v["detectors"]["services"],
        json!({"state": "ok", "signals": 1})
    );
    // The probe reset: the next pass closes it.
    f.failed_units.clear();
    f.at(10);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kp]));
    let reason = f.finding(&kp)["reason"].as_str().unwrap().to_owned();
    assert_eq!(
        reason,
        format!("not seen by ap watch services at {}", utc(BASE + 600))
    );
}

// spec: specs/ap/watch.md#R5
#[test]
fn r5_over_two_hours_a_signal_is_opened_once_and_seen_again_at_60_and_120_minutes() {
    let mut f = F::quiet();
    probe(&mut f, "watch-probe-2.service");
    let kp = k("services", "failed watch-probe-2.service");
    let mut log = vec![];
    for m in (0..=120).step_by(5) {
        f.at(m);
        f.reset();
        f.pass();
        for (a, key) in f.actions() {
            assert_eq!(key, kp);
            log.push((m, a));
        }
    }
    assert_eq!(
        log,
        [
            (0, "open_finding".to_owned()),
            (60, "see_finding_again".to_owned()),
            (120, "see_finding_again".to_owned())
        ]
    );
}

// spec: specs/ap/watch.md#R5
#[test]
fn r5_a_severity_change_is_seen_at_once_with_the_new_severity() {
    let mut f = F::quiet();
    probe(&mut f, "u.service");
    let kp = k("services", "failed u.service");
    f.plant(
        &kp,
        stored("services", "failed u.service: x", "info", "planned", BASE),
    );
    f.at(1);
    f.pass();
    assert_eq!(f.actions(), [("see_finding_again".to_owned(), kp.clone())]);
    assert_eq!(f.bodies()[0]["parameters"]["severity"], "act");
    assert_eq!(f.finding(&kp)["severity"], "act");
    assert_eq!(f.finding(&kp)["state"], "planned");
    // The same severity and a recent sight: no severity is sent, nothing is applied.
    f.reset();
    f.at(2);
    f.pass();
    assert!(f.actions().is_empty());
}

// spec: specs/ap/watch.md#R5
#[test]
fn r5_a_closed_finding_is_reopened_and_a_dismissed_one_gets_nothing() {
    let mut f = F::quiet();
    probe(&mut f, "a.service");
    probe(&mut f, "b.service");
    let (ka, kb) = (
        k("services", "failed a.service"),
        k("services", "failed b.service"),
    );
    f.plant(
        &ka,
        stored(
            "services",
            "failed a.service: x",
            "act",
            "closed",
            BASE - 7_200,
        ),
    );
    f.plant(
        &kb,
        stored(
            "services",
            "failed b.service: x",
            "act",
            "dismissed",
            BASE - 7_200,
        ),
    );
    let v = f.pass();
    assert_eq!(v["reopened"], json!([ka]));
    assert_eq!(f.actions(), [("reopen_finding".to_owned(), ka.clone())]);
    assert!(
        f.finding(&ka)["evidence"]
            .as_str()
            .unwrap()
            .contains("Result=exit-code")
    );
    assert_eq!(f.finding(&kb)["state"], "dismissed");
}

/// The `subject` an applied action carried: its type and native key, or none.
fn subject_of(body: &Value) -> Option<(String, String)> {
    let s = body["parameters"].get("subject")?;
    Some((
        s["object_type"].as_str().unwrap().to_owned(),
        s["native_key"].as_str().unwrap().to_owned(),
    ))
}
// spec: specs/ap/watch.md#R32
#[test]
fn r32_findings_of_host_name_the_host_local() {
    let host = |key: &str| Some(("host".to_owned(), key.to_owned()));
    let kd = k("host", "disk floor");
    // `open_finding`, then `see_finding_again` an hour later, for a signal of the detector `host`.
    let mut f = F::quiet();
    f.df = " Avail\n10239M\n".into();
    f.pass();
    let bodies = f.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(
        (bodies[0]["action_type"].as_str(), subject_of(&bodies[0])),
        (Some("open_finding"), host("local"))
    );
    assert_eq!(f.finding(&kd)["subject"]["native_key"], "local");
    f.reset();
    f.at(60);
    f.pass();
    let bodies = f.bodies();
    assert_eq!(
        (bodies[0]["action_type"].as_str(), subject_of(&bodies[0])),
        (Some("see_finding_again"), host("local"))
    );
    // `reopen_finding` of a closed finding that had no subject sets it.
    let mut f = F::quiet();
    f.df = " Avail\n10239M\n".into();
    f.plant(&kd, stored("host", "disk floor: x", "act", "closed", BASE - 7_200));
    assert!(f.finding(&kd).get("subject").is_none());
    f.pass();
    let bodies = f.bodies();
    assert_eq!(
        (bodies[0]["action_type"].as_str(), subject_of(&bodies[0])),
        (Some("reopen_finding"), host("local"))
    );
    assert_eq!(f.finding(&kd)["subject"]["native_key"], "local");
    // `pc down` (R30) names the host `pc`.
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(15);
    let now = f.now();
    put(&mut f, CHECKS_LOG, &[checks_line(now - 600, "local", json!("pc_unreachable"))]);
    f.pass();
    let bodies = f.bodies();
    assert_eq!(
        (bodies[0]["action_type"].as_str(), subject_of(&bodies[0])),
        (Some("open_finding"), host("pc"))
    );
    // The action of another detector carries none, and nor does a close.
    let mut f = F::quiet();
    probe(&mut f, "watch-probe-1.service");
    f.pass();
    assert_eq!(subject_of(&f.bodies()[0]), None);
    assert!(f.bodies()[0]["parameters"].get("subject").is_none());
    f.reset();
    f.failed_units.clear();
    f.at(10);
    f.pass();
    assert_eq!(f.bodies()[0]["action_type"], "close_finding");
    assert_eq!(subject_of(&f.bodies()[0]), None);
    let mut f = F::quiet();
    f.df = " Avail\n10239M\n".into();
    f.pass();
    f.reset();
    f.df = " Avail\n10240M\n".into();
    f.at(5);
    f.pass();
    assert_eq!(f.bodies()[0]["action_type"], "close_finding");
    assert_eq!(subject_of(&f.bodies()[0]), None, "a close of a host finding carries none");
}

// spec: specs/ap/watch.md#R6
#[test]
fn r6_an_unseen_live_finding_of_a_detector_that_read_is_closed() {
    let mut f = F::quiet();
    let (open, planned, closed) = (
        k("services", "failed gone.service"),
        k("services", "timer gone"),
        k("services", "failed old.service"),
    );
    f.plant(
        &open,
        stored("services", "failed gone.service: x", "act", "open", BASE),
    );
    f.plant(
        &planned,
        stored("services", "timer gone: x", "act", "planned", BASE),
    );
    f.plant(
        &closed,
        stored("services", "failed old.service: x", "act", "closed", BASE),
    );
    let v = f.pass();
    let mut c: Vec<String> = serde_json::from_value(v["closed"].clone()).unwrap();
    c.sort();
    let mut want = vec![open.clone(), planned.clone()];
    want.sort();
    assert_eq!(c, want);
    for key in [&open, &planned] {
        assert!(
            f.finding(key)["reason"]
                .as_str()
                .unwrap()
                .starts_with("not seen by ap watch services at ")
        );
    }
}

// spec: specs/ap/watch.md#R6
#[test]
fn r6_an_unreadable_detector_closes_nothing_and_opens_its_own_finding() {
    let mut f = F::quiet();
    let live = k("services", "failed x.service");
    f.plant(
        &live,
        stored("services", "failed x.service: x", "act", "open", BASE),
    );
    f.dead.insert("systemctl".into());
    let v = f.pass();
    assert_eq!(
        v["detectors"]["services"],
        json!({"state": "unreadable", "signals": 1})
    );
    assert_eq!(v["detectors"]["delivery"]["state"], "ok");
    // `login` reads `systemctl show` too: it is unreadable for the same cause.
    assert_eq!(v["detectors"]["login"]["state"], "unreadable");
    let ku = k("services", "unreadable");
    assert!(v["opened"].as_array().unwrap().contains(&json!(ku)));
    assert_eq!(f.finding(&ku)["severity"], "act");
    assert!(
        f.finding(&ku)["summary"]
            .as_str()
            .unwrap()
            .starts_with("unreadable")
    );
    assert_eq!(f.finding(&live)["state"], "open");
    assert_eq!(v["closed"], json!([]));
    // Readable again: its `unreadable` finding closes.
    f.dead.clear();
    f.at(5);
    let v = f.pass();
    assert!(v["closed"].as_array().unwrap().contains(&json!(ku)));
    assert!(v["closed"].as_array().unwrap().contains(&json!(live)));
}

// spec: specs/ap/watch.md#R6
#[test]
fn r6_a_finding_of_another_detector_is_never_read_for_closing_or_changed() {
    let mut f = F::quiet();
    let mine = k("services", "failed mine.service");
    f.plant(
        &mine,
        stored("services", "failed mine.service: x", "act", "open", BASE),
    );
    let mut lead = stored("services", "failed lead.service: x", "act", "open", BASE);
    lead["detector"] = json!("lead");
    f.plant("lead-note-1", lead.clone());
    let mut look_alike = stored("services", "failed y.service: x", "act", "open", BASE);
    look_alike["detector"] = json!("watching:services");
    f.plant("watching-1", look_alike.clone());
    let v = f.pass();
    assert_eq!(v["closed"], json!([mine]));
    assert_eq!(f.finding("lead-note-1"), &lead);
    assert_eq!(f.finding("watching-1"), &look_alike);
    let touched: Vec<String> = f.actions().into_iter().map(|(_, k)| k).collect();
    assert_eq!(touched, [mine]);
    for c in f.calls.borrow().iter().filter(|c| c.1[0] == "search") {
        assert!(
            c.2["where"]["all"][0]["eq"][1]
                .as_str()
                .unwrap()
                .starts_with("watch:")
        );
    }
}

// spec: specs/ap/watch.md#R7
#[test]
fn r7_the_family_of_a_finding_is_the_longest_prefix_of_its_summary() {
    assert_eq!(
        family("records", "backup copy stale: x").as_deref(),
        Some("backup copy stale")
    );
    assert_eq!(
        family("records", "backup stale: x").as_deref(),
        Some("backup stale")
    );
    assert_eq!(
        family("login", "login /etc/sudoers.d/: x").as_deref(),
        Some("login /etc/sudoers.d/")
    );
    assert_eq!(
        family("login", "login /etc/sudoers: x").as_deref(),
        Some("login /etc/sudoers")
    );
    assert_eq!(
        family("delivery", "end refused build_busy: x").as_deref(),
        Some("end")
    );
    assert_eq!(
        family("services", "unreadable: x").as_deref(),
        Some("unreadable")
    );
    assert_eq!(family("host", "nothing known"), None);
    // A `login /etc/sudoers.d/` finding is closed while `login /etc/sudoers` is not when only the
    // first family is known; here no `login.json`: every part family is unknown.
    let mut f = F::new();
    f.pass();
    let part = k("login", "login /etc/sudoers.d/");
    f.plant(
        &part,
        stored("login", "login /etc/sudoers.d/: x", "urgent", "open", BASE),
    );
    f.reset();
    f.at(5);
    f.pass();
    assert_eq!(f.finding(&part)["state"], "open");
    assert!(f.actions().iter().all(|(_, k)| k != &part));
}

// spec: specs/ap/watch.md#R8
#[test]
fn r8_no_state_json_is_the_baseline_of_the_comparing_signals() {
    let mut f = F::new();
    f.record();
    f.files.insert(MEMORY_EVENTS.into(), b"high 900\n".to_vec());
    f.deliver.as_mut().unwrap()["end"] = json!("refused");
    f.deliver.as_mut().unwrap()["code"] = json!("build_busy");
    f.backup.as_mut().unwrap()["remote"] = json!("failed");
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    let s = f.state();
    assert_eq!(s["host"]["memory_high"], 900);
    assert_eq!(s["host"]["memory_rose_at"], Value::Null);
    assert_eq!(s["host"]["apt_start"], "2026-11-01 08:05:21");
    assert_eq!(
        s["delivery"]["refusal"],
        json!({"end": "refused", "code": "build_busy", "since": BASE})
    );
    assert_eq!(s["records"]["copied_at"], utc(BASE));
    assert_eq!(s["records"]["backup_at"], utc(BASE - 600));
    // A state.json that does not parse is the baseline again.
    std::fs::write(f.dir.join("state.json"), b"{not json").unwrap();
    f.files.insert(MEMORY_EVENTS.into(), b"high 950\n".to_vec());
    f.at(5);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    assert_eq!(f.state()["host"]["memory_high"], 950);
    assert!(!f.dir.join(".state.json.tmp").exists());
}

// spec: specs/ap/watch.md#R8
#[test]
fn r8_an_unreadable_detectors_member_is_carried_over_unchanged() {
    let mut f = F::quiet();
    let before = f.state();
    f.files.insert(MEMORY_EVENTS.into(), b"high 7\n".to_vec());
    f.dead.insert("df".into());
    f.at(5);
    let v = f.pass();
    assert_eq!(v["detectors"]["host"]["state"], "unreadable");
    assert_eq!(f.state()["host"], before["host"]);
    f.dead.clear();
    f.at(10);
    f.pass();
    assert_eq!(f.state()["host"]["memory_high"], 7);
}

// spec: specs/ap/watch.md#R9
#[test]
fn r9_actions_go_as_ap_watch_with_request_ids_of_the_pass_and_cut_texts() {
    let mut f = F::quiet();
    let long = format!("{}.service", "é".repeat(400));
    probe(&mut f, &long);
    f.pass();
    let bodies = f.bodies();
    assert_eq!(bodies.len(), 1);
    let b = &bodies[0];
    assert_eq!(b["agent"], "ap-watch");
    let rid = b["request_id"].as_str().unwrap();
    let session = b["session"].as_str().unwrap();
    assert!(
        rid.starts_with(&format!("{session}-")) && rid.ends_with("-1"),
        "{rid}"
    );
    let hex = session.strip_prefix("ww-").unwrap();
    assert!(
        hex.len() == 16 && hex.bytes().all(|c| c.is_ascii_hexdigit()),
        "{session}"
    );
    let summary = b["parameters"]["summary"].as_str().unwrap();
    assert!(
        summary.len() <= 600 && summary.len() > 590,
        "{}",
        summary.len()
    );
    assert!(summary.starts_with("failed é"));
    // Each pass draws its own ids.
    f.reset();
    f.failed_units.clear();
    f.at(5);
    f.pass();
    let second = f.bodies()[0]["request_id"].as_str().unwrap().to_owned();
    assert_ne!(second, rid);
}

// spec: specs/ap/watch.md#R4
#[test]
fn r4_the_key_has_the_schemas_form_for_any_signature() {
    for d in ap::watch::DETECTORS {
        let key = key(d, "a signature with spaces /and/slashes");
        let hex = key.strip_prefix(&format!("watch-{d}-")).unwrap();
        assert_eq!(hex.len(), 16);
        assert!(
            key.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        );
        assert!((27..=31).contains(&key.len()), "{d}: {}", key.len());
    }
    for (d, sig) in [
        (
            "delivery",
            "checkout /home/some one/a path with spaces dirty",
        ),
        ("services", &format!("failed {}.service", "x".repeat(300))),
        ("host", "old cli 123 Sat Nov  7 07:03:38 2026"),
    ] {
        let key = key(d, sig);
        assert!((27..=31).contains(&key.len()), "{}", key.len());
    }
    assert_ne!(key("host", "apt a"), key("records", "apt a"));
}

// ---- delivery.

// spec: specs/ap/watch.md#R10
#[test]
fn r10_a_stale_last_json_and_an_end_not_current() {
    let mut f = F::quiet();
    f.deliver_age = 31 * 60;
    let d = f.deliver.as_mut().unwrap();
    (d["end"], d["code"]) = (json!("refused"), json!("pair_differs"));
    let v = f.pass();
    let (stale, end) = (
        k("delivery", "stale"),
        k("delivery", "end refused pair_differs"),
    );
    let mut opened: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    opened.sort();
    let mut want = vec![stale.clone(), end.clone()];
    want.sort();
    assert_eq!(opened, want);
    assert_eq!(f.finding(&end)["severity"], "act");
    assert_eq!(f.finding(&stale)["severity"], "act");
    // installed: both close.
    f.deliver_age = 60;
    f.deliver.as_mut().unwrap()["end"] = json!("installed");
    f.at(5);
    let v = f.pass();
    assert_eq!(v["closed"].as_array().unwrap().len(), 2);
}

// spec: specs/ap/watch.md#R10
#[test]
fn r10_failed_is_urgent_and_failed_then_suppressed_keeps_one_urgent_finding_open() {
    let mut f = F::quiet();
    let d = f.deliver.as_mut().unwrap();
    (d["end"], d["code"]) = (json!("failed"), json!("migrate_failed"));
    let v = f.pass();
    let kf = k("delivery", "end failed migrate_failed");
    assert_eq!(v["opened"], json!([kf]));
    assert_eq!(f.finding(&kf)["severity"], "urgent");
    for (m, code) in [
        (5, "suppressed"),
        (10, "pair_differs"),
        (15, "held_by_rollback"),
    ] {
        let d = f.deliver.as_mut().unwrap();
        (d["end"], d["code"]) = (json!("refused"), json!(code));
        f.reset();
        f.at(m);
        let v = f.pass();
        assert!(f.actions().is_empty(), "{code}: {v}");
        assert_eq!(f.finding(&kf)["state"], "open");
        assert_eq!(v["detectors"]["delivery"]["signals"], 1);
    }
    // Another refusal signals beside it; the kept failure holds.
    f.deliver.as_mut().unwrap()["code"] = json!("dirty");
    f.at(20);
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("delivery", "end refused dirty")]));
    assert_eq!(f.finding(&kf)["state"], "open");
    // current ends the kept failure.
    let d = f.deliver.as_mut().unwrap();
    (d["end"], d["code"]) = (json!("current"), Value::Null);
    f.at(25);
    let v = f.pass();
    assert!(v["closed"].as_array().unwrap().contains(&json!(kf)));
    assert_eq!(f.state()["delivery"]["failed"], Value::Null);
}

// spec: specs/ap/watch.md#R10
#[test]
fn r10_a_build_busy_refusal_cleared_within_30_minutes_gives_no_signal() {
    let mut f = F::quiet();
    let kb = k("delivery", "end refused build_busy");
    for m in [0, 10, 20, 30] {
        let d = f.deliver.as_mut().unwrap();
        (d["end"], d["code"]) = (json!("refused"), json!("build_busy"));
        f.at(m);
        f.pass();
    }
    let d = f.deliver.as_mut().unwrap();
    (d["end"], d["code"]) = (json!("current"), Value::Null);
    f.at(35);
    f.pass();
    assert!(!f.findings.contains_key(&kb));
    // Held over 30 minutes it signals; while under, an open `end` finding is neither seen nor closed.
    let other = k("delivery", "end refused dirty");
    f.plant(
        &other,
        stored("delivery", "end refused dirty: x", "act", "open", BASE),
    );
    for m in [40, 50, 60, 70] {
        let d = f.deliver.as_mut().unwrap();
        (d["end"], d["code"]) = (json!("refused"), json!("busy"));
        f.at(m);
        f.reset();
        f.pass();
        assert_eq!(f.finding(&other)["state"], "open", "minute {m}");
        assert!(f.actions().is_empty(), "minute {m}: {:?}", f.actions());
    }
    f.at(75);
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("delivery", "end refused busy")]));
    assert_eq!(v["closed"], json!([other]));
}

// spec: specs/ap/watch.md#R10
#[test]
fn r10_a_checkout_not_forwarded_signals_and_null_checkouts_is_unknown() {
    let mut f = F::quiet();
    f.deliver.as_mut().unwrap()["checkouts"] = json!([
        {"path": "/invented/a", "from": "a", "to": "a", "result": "forwarded"},
        {"path": "/invented/b", "from": "b", "to": null, "result": "dirty"},
        {"path": "/invented/c", "from": "c", "to": "c", "result": "current"}]);
    let v = f.pass();
    let kc = k("delivery", "checkout /invented/b dirty");
    assert_eq!(v["opened"], json!([kc]));
    f.deliver.as_mut().unwrap()["checkouts"] = Value::Null;
    f.at(5);
    f.reset();
    f.pass();
    assert!(f.actions().is_empty());
    f.deliver
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("checkouts");
    f.at(10);
    f.pass();
    assert_eq!(f.finding(&kc)["state"], "open");
    f.deliver.as_mut().unwrap()["checkouts"] = json!([]);
    f.at(15);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kc]));
}

// spec: specs/ap/watch.md#R10
#[test]
fn r10_an_absent_last_json_makes_delivery_unreadable() {
    let mut f = F::quiet();
    f.deliver = None;
    let v = f.pass();
    assert_eq!(
        v["detectors"]["delivery"],
        json!({"state": "unreadable", "signals": 1})
    );
    assert_eq!(v["opened"], json!([k("delivery", "unreadable")]));
}

// ---- services.

// spec: specs/ap/watch.md#R11
#[test]
fn r11_failed_units_except_those_of_delivery_and_records() {
    let mut f = F::quiet();
    probe(&mut f, "ap-deliver.service");
    probe(&mut f, "ont-backup.service");
    probe(&mut f, "nginx.service");
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("services", "failed nginx.service")]));
}

// spec: specs/ap/watch.md#R11
#[test]
fn r11_a_timer_that_did_not_fire_unless_its_service_is_active() {
    let mut f = F::quiet();
    f.trigger_age.insert("ap-deliver.timer".into(), 31 * 60);
    f.show("ap-deliver.service", &[("ActiveState", "activating")]);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "a service that is active");
    f.show("ap-deliver.service", &[("ActiveState", "inactive")]);
    f.at(1);
    let v = f.pass();
    let kt = k("services", "timer ap-deliver");
    assert_eq!(v["opened"], json!([kt]));
    assert!(
        f.finding(&kt)["evidence"]
            .as_str()
            .unwrap()
            .contains("LastTriggerUSec=Thu")
    );
    // Under three intervals: no signal; the finding closes.
    f.trigger_age.insert("ap-deliver.timer".into(), 29 * 60);
    f.at(2);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kt]));
}

// spec: specs/ap/watch.md#R11
#[test]
fn r11_a_timer_never_fired_signals_only_after_three_intervals_of_uptime() {
    let mut f = F::quiet();
    f.trigger_age.remove("ont-backup.timer");
    f.files
        .insert("/proc/uptime".into(), b"10000.5 1.0\n".to_vec());
    let v = f.pass();
    assert_eq!(v["opened"], json!([]));
    f.files
        .insert("/proc/uptime".into(), b"10801.5 1.0\n".to_vec());
    f.at(1);
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("services", "timer ont-backup")]));
}

// spec: specs/ap/watch.md#R11
#[test]
fn r11_ap_work_is_read_once_installed() {
    let mut f = F::quiet();
    // Not installed (the healthy fake has no `ap-work` units, and the quiet pass opened nothing);
    // installed, a timer that never fired signals.
    f.show(
        "ap-work.timer",
        &[("LoadState", "loaded"), ("ActiveState", "active")],
    );
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("services", "timer ap-work")]));
    f.trigger_age.insert("ap-work.timer".into(), 100);
    f.at(5);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]));
    f.trigger_age.insert("ap-work.timer".into(), 181);
    f.at(10);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "a closed finding reopens");
    assert_eq!(v["reopened"], json!([k("services", "timer ap-work")]));
}

// spec: specs/ap/watch.md#R11
#[test]
fn r11_a_timestamp_not_in_utc_makes_services_unreadable() {
    let mut f = F::quiet();
    f.trigger_age.remove("ap-quota.timer");
    f.show(
        "ap-quota.timer",
        &[("LastTriggerUSec", "Thu 2026-11-19 11:03:01 KST")],
    );
    let v = f.pass();
    assert_eq!(v["detectors"]["services"]["state"], "unreadable");
    f.show("ap-quota.timer", &[("LastTriggerUSec", "n/a")]);
    f.at(5);
    let v = f.pass();
    assert_eq!(v["detectors"]["services"]["state"], "ok");
    assert!(
        v["opened"]
            .as_array()
            .unwrap()
            .contains(&json!(k("services", "timer ap-quota")))
    );
}

// ---- host.

// spec: specs/ap/watch.md#R12
#[test]
fn r12_the_disk_floor() {
    let mut f = F::quiet();
    f.df = " Avail\n10239M\n".into();
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("host", "disk floor")]));
    f.df = " Avail\n10240M\n".into();
    f.at(5);
    let v = f.pass();
    assert_eq!(v["closed"], json!([k("host", "disk floor")]));
}

// spec: specs/ap/watch.md#R12
#[test]
fn r12_slice_memory_high_holds_for_60_minutes_after_the_counter_rose() {
    let mut f = F::quiet();
    f.files.insert(MEMORY_EVENTS.into(), b"high 6\n".to_vec());
    f.at(5);
    let v = f.pass();
    let km = k("host", "slice memory high");
    assert_eq!(v["opened"], json!([km]));
    assert_eq!(f.finding(&km)["severity"], "info");
    f.at(64);
    f.reset();
    f.pass();
    assert!(f.actions().iter().all(|(a, _)| a == "see_finding_again"));
    assert_eq!(f.finding(&km)["state"], "open");
    f.at(65);
    let v = f.pass();
    assert_eq!(v["closed"], json!([km]));
    // A lower counter (the slice made again) is no rise; an absent file is unknown.
    f.files.insert(MEMORY_EVENTS.into(), b"high 0\n".to_vec());
    f.at(70);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]));
    f.files.remove(MEMORY_EVENTS);
    f.at(75);
    f.pass();
    assert_eq!(f.state()["host"]["memory_high"], 0);
}

// spec: specs/ap/watch.md#R12
#[test]
fn r12_an_apt_start_newer_than_state_json_signals_for_one_pass() {
    let mut f = F::quiet();
    let mut log = f.files[APT].clone();
    log.extend(b"\nStart-Date: 2026-11-19  00:02:00\nCommandline: apt-get install x\n");
    f.files.insert(APT.into(), log);
    f.at(5);
    let v = f.pass();
    let ka = k("host", "apt 2026-11-19 00:02:00");
    assert_eq!(v["opened"], json!([ka]));
    assert_eq!(f.finding(&ka)["severity"], "info");
    f.at(10);
    let v = f.pass();
    assert_eq!(v["closed"], json!([ka]));
    assert_eq!(f.state()["host"]["apt_start"], "2026-11-19 00:02:00");
}

// spec: specs/ap/watch.md#R12
#[test]
fn r12_an_old_agent_cli_is_listed_with_its_pid_name_and_times_only() {
    let mut f = F::quiet();
    f.ps = [
        "      1 1018769 Sat Nov  7 07:03:38 2026 systemd",
        "   4242  604801 Wed Nov 11 23:59:59 2026 claude",
        "   4243  604799 Thu Nov 12 00:00:01 2026 codex",
        "   4244  999999 Sat Nov  7 07:03:38 2026 node",
    ]
    .join("\n");
    let v = f.pass();
    let ko = k("host", "old cli 4242 Wed Nov 11 23:59:59 2026");
    assert_eq!(v["opened"], json!([ko]));
    let o = f.finding(&ko);
    assert_eq!(o["severity"], "info");
    assert_eq!(
        o["evidence"],
        "pid 4242, command claude, started Wed Nov 11 23:59:59 2026, elapsed 604801 seconds"
    );
    // Listed only: no call of the pass signals or kills.
    assert!(f.programs().iter().all(|(p, _)| p != "kill"));
}

// ---- records.

// spec: specs/ap/watch.md#R13
#[test]
fn r13_backup_stale_only_while_actions_were_applied_after_it() {
    let mut f = F::quiet();
    f.backup_age = 3 * 3_600;
    f.backup.as_mut().unwrap()["backup"] = Value::Null;
    f.at(120);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "no action since the backup");
    f.log
        .push(json!({"action_type": "record_request", "submitted_at": utc(BASE + 119 * 60)}));
    f.at(125);
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("records", "backup stale")]));
    // A new backup: closed.
    f.backup_age = 60;
    f.backup.as_mut().unwrap()["backup"] = json!("/invented/backups/c.sqlite");
    f.at(130);
    let v = f.pass();
    assert_eq!(v["closed"], json!([k("records", "backup stale")]));
}

// spec: specs/ap/watch.md#R13
#[test]
fn r13_a_remote_that_fails_for_25_hours_raises_backup_copy_stale() {
    let mut f = F::quiet();
    let kc = k("records", "backup copy stale");
    f.backup.as_mut().unwrap()["remote"] = json!("failed");
    let mut raised = None;
    for h in 1..=25u64 {
        f.backup_age = 60;
        f.at(h * 60);
        let v = f.pass();
        if raised.is_none() && v["opened"].as_array().unwrap().contains(&json!(kc)) {
            raised = Some(h);
        }
    }
    // The baseline pass read `copied` ten minutes before the base; the copy is over a day old at hour 24.
    assert_eq!(raised, Some(24));
    assert_eq!(f.finding(&kc)["state"], "open");
    f.backup.as_mut().unwrap()["remote"] = json!("unchanged");
    f.at(26 * 60);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kc]));
    // `none` (no remote) counts as not copied too.
    let mut g = F::new();
    g.record();
    g.backup.as_mut().unwrap()["remote"] = json!("none");
    g.pass();
    g.at(24 * 60 + 1);
    let v = g.pass();
    assert_eq!(v["opened"], json!([kc]));
}

// ---- login.

/// A quiet host whose `login.json` is recorded, after `plant` changed the host before the record.
fn login_host(plant: impl FnOnce(&mut F)) -> F {
    let mut f = F::new();
    plant(&mut f);
    f.record();
    f.pass();
    f.reset();
    f
}
fn login_signals(f: &mut F) -> Vec<String> {
    f.at(5);
    let v = f.pass();
    assert_eq!(v["detectors"]["login"]["state"], "ok", "{v}");
    let opened: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    let mut parts = vec![];
    for key in opened {
        let o = f.finding(&key);
        assert_eq!(o["detector"], "watch:login");
        assert_eq!(
            (o["kind"].as_str(), o["severity"].as_str()),
            (Some("security"), Some("urgent"))
        );
        let summary = o["summary"].as_str().unwrap();
        let part = summary.split(": ").next().unwrap().to_owned();
        assert_eq!(key, k("login", &part));
        parts.push(part);
    }
    parts.sort();
    parts
}

// spec: specs/ap/watch.md#R16
#[test]
fn r16_no_login_json_is_no_baseline_and_the_parts_are_unknown() {
    let mut f = F::new();
    let part = k("login", "login /etc/pam.d/sshd");
    f.plant(
        &part,
        stored("login", "login /etc/pam.d/sshd: x", "urgent", "open", BASE),
    );
    let v = f.pass();
    let kn = k("login", "no baseline");
    assert_eq!(v["opened"], json!([kn]));
    assert_eq!(
        (
            f.finding(&kn)["severity"].as_str(),
            f.finding(&kn)["kind"].as_str()
        ),
        (Some("act"), Some("security"))
    );
    assert_eq!(f.finding(&part)["state"], "open");
    f.record();
    f.at(5);
    let v = f.pass();
    let mut closed: Vec<String> = serde_json::from_value(v["closed"].clone()).unwrap();
    closed.sort();
    let mut want = vec![kn, part];
    want.sort();
    assert_eq!(closed, want);
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_quiet_login_path_gives_no_signal() {
    let mut f = login_host(|_| {});
    assert_eq!(login_signals(&mut f), Vec::<String>::new());
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_directory_absent_at_both_gives_nothing_and_an_entry_planted_in_it_one_part() {
    let mut f = login_host(|_| {});
    assert!(!f.types.contains_key("/etc/systemd/system/ssh.service.d"));
    assert_eq!(login_signals(&mut f), Vec::<String>::new());
    f.file("/etc/systemd/system/ssh.service.d", "directory", None);
    f.finds.insert(
        "/etc/systemd/system/ssh.service.d".into(),
        "evil.conf\tf\t\n".into(),
    );
    f.files.insert(
        "/etc/systemd/system/ssh.service.d/evil.conf".into(),
        b"[Service]\n".to_vec(),
    );
    assert_eq!(
        login_signals(&mut f),
        ["login /etc/systemd/system/ssh.service.d/"]
    );
    // An empty directory that appears reads as the empty set: no difference.
    let mut g = login_host(|_| {});
    g.file("/etc/systemd/system/ssh.service.d", "directory", None);
    assert_eq!(login_signals(&mut g), Vec::<String>::new());
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_link_in_ssh_service_requires_pointed_to_a_new_target() {
    let plant = |f: &mut F| {
        f.file(REQUIRES, "directory", None);
        f.finds.insert(
            REQUIRES.into(),
            "x.service\tl\t/usr/lib/systemd/system/x.service\n".into(),
        );
    };
    let mut f = login_host(plant);
    assert_eq!(login_signals(&mut f), Vec::<String>::new());
    f.finds.insert(
        REQUIRES.into(),
        "x.service\tl\t/etc/evil/x.service\n".into(),
    );
    assert_eq!(
        login_signals(&mut f),
        ["login /etc/systemd/system/ssh.service.requires/"]
    );
    // The link's target was never read.
    assert!(!f.reads.borrow().iter().any(|p| p.contains("x.service")));
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_fragment_moved_from_usr_lib_to_etc_with_the_same_bytes() {
    let mut f = login_host(|_| {});
    let bytes = f.files["/usr/lib/systemd/system/ssh.service"].clone();
    f.file("/etc/systemd/system/ssh.service", "regular file", None);
    f.files
        .insert("/etc/systemd/system/ssh.service".into(), bytes);
    f.show(
        "ssh.service",
        &[("FragmentPath", "/etc/systemd/system/ssh.service")],
    );
    assert_eq!(login_signals(&mut f), ["login ssh.service fragment"]);
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_an_empty_fragment_path_and_a_masked_unit() {
    let mut f = login_host(|_| {});
    f.show("ssh.socket", &[("FragmentPath", "")]);
    assert_eq!(login_signals(&mut f), ["login ssh.socket fragment"]);
    let mut g = login_host(|_| {});
    g.file("/etc/systemd/system/ssh.socket", "symbolic link", None);
    g.show(
        "ssh.socket",
        &[("FragmentPath", "/etc/systemd/system/ssh.socket")],
    );
    assert_eq!(login_signals(&mut g), ["login ssh.socket fragment"]);
    let parts = &g.state();
    assert!(parts.get("login").is_none());
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_second_file_beside_override_conf_in_drop_in_paths() {
    let mut f = login_host(|_| {});
    let extra = "/run/systemd/system/ssh.socket.d/zz.conf";
    f.file(
        extra,
        "regular file",
        Some("[Socket]\nListenStream=0.0.0.0:2222\n"),
    );
    f.show(
        "ssh.socket",
        &[("DropInPaths", &format!("{OVERRIDE} {extra}"))],
    );
    assert_eq!(login_signals(&mut f), ["login ssh.socket drop-ins"]);
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_override_conf_removed_gives_two_urgent_parts_and_login_reads() {
    let mut f = login_host(|_| {});
    f.types.remove(OVERRIDE);
    f.files.remove(OVERRIDE);
    f.finds.insert(SOCKET_D.into(), String::new());
    f.show("ssh.socket", &[("DropInPaths", "")]);
    assert_eq!(
        login_signals(&mut f),
        [
            "login /etc/systemd/system/ssh.socket.d/",
            "login ssh.socket drop-ins"
        ]
    );
    // Still listed by systemctl but gone from the disk: absent, a reading, not unreadable.
    let mut g = login_host(|_| {});
    g.types.remove(OVERRIDE);
    g.files.remove(OVERRIDE);
    g.finds.insert(SOCKET_D.into(), String::new());
    assert_eq!(
        login_signals(&mut g),
        [
            "login /etc/systemd/system/ssh.socket.d/",
            "login ssh.socket drop-ins"
        ]
    );
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_sshd_exiting_non_zero_is_one_part_and_login_reads() {
    let mut f = login_host(|_| {});
    f.sshd = "exit 255\n".into();
    assert_eq!(login_signals(&mut f), ["login sshd -T"]);
    // The lines are sorted: another order of the same lines is no difference.
    let mut g = login_host(|_| {});
    g.sshd = "passwordauthentication no\nport 22\nexit 0\n".into();
    assert_eq!(login_signals(&mut g), Vec::<String>::new());
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_pam_sshd_absent_is_one_part() {
    let mut f = login_host(|_| {});
    f.types.remove("/etc/pam.d/sshd");
    f.files.remove("/etc/pam.d/sshd");
    assert_eq!(login_signals(&mut f), ["login /etc/pam.d/sshd"]);
    // Its bytes changed: the same part.
    let mut g = login_host(|_| {});
    g.files.insert(
        "/etc/pam.d/sshd".into(),
        b"auth sufficient pam_permit.so\n".to_vec(),
    );
    assert_eq!(login_signals(&mut g), ["login /etc/pam.d/sshd"]);
}

// spec: specs/ap/watch.md#R14
#[test]
fn r14_a_directory_part_that_is_a_link_to_a_directory_is_recorded_as_that_type() {
    let mut f = login_host(|_| {});
    f.file("/etc/sudoers.d", "symbolic link", None);
    assert_eq!(login_signals(&mut f), ["login /etc/sudoers.d/"]);
    let login: Value =
        serde_json::from_slice(&std::fs::read(f.dir.join("login.json")).unwrap()).unwrap();
    assert_eq!(
        login["parts"]["/etc/sudoers.d/"]["entries"][0]["name"],
        "README"
    );
    let (v, _) = f.run(&["--record-login", "--replace", "--totp-file", TOTP]);
    assert_eq!(
        v["parts"]["/etc/sudoers.d/"],
        json!({"type": "symbolic link", "entries": []})
    );
    // `find` is never called on a directory part that is not a directory.
    assert!(
        !f.programs()
            .iter()
            .any(|(p, a)| p == "find" && a[0] == "/etc/sudoers.d")
    );
}

// spec: specs/ap/watch.md#R15
#[test]
fn r15_the_totp_file_is_stated_never_read_and_its_absence_is_one_part() {
    let mut f = login_host(|_| {});
    assert_eq!(login_signals(&mut f), Vec::<String>::new());
    f.types.remove(TOTP);
    assert_eq!(login_signals(&mut f), ["login totp file"]);
    assert!(!f.reads.borrow().iter().any(|p| p == TOTP));
    let o = f.finding(&k("login", "login totp file"));
    let evidence = o["evidence"].as_str().unwrap();
    assert!(
        evidence.starts_with("part totp file: recorded "),
        "{evidence}"
    );
    let hexes = evidence
        .split(' ')
        .filter(|w| w.trim_end_matches(',').len() == 64)
        .count();
    assert_eq!(hexes, 2, "{evidence}");
    assert!(!evidence.contains(TOTP));
}

// spec: specs/ap/watch.md#R15
#[test]
fn r15_only_a_read_that_cannot_be_made_makes_login_unreadable() {
    let mut f = login_host(|_| {});
    f.find_fails.insert("/etc/sudoers.d".into());
    f.at(5);
    let v = f.pass();
    assert_eq!(v["detectors"]["login"]["state"], "unreadable");
    let mut g = login_host(|_| {});
    g.unreadable.insert("/etc/sudoers".into());
    g.at(5);
    assert_eq!(g.pass()["detectors"]["login"]["state"], "unreadable");
    let mut h = login_host(|_| {});
    h.stat_fails.insert("/etc/pam.d/sshd".into());
    h.at(5);
    assert_eq!(h.pass()["detectors"]["login"]["state"], "unreadable");
    let mut i = login_host(|_| {});
    i.dead.insert("sh".into());
    i.at(5);
    assert_eq!(i.pass()["detectors"]["login"]["state"], "unreadable");
}

// ---- The answer, the exits, the unit files, the calls.

// spec: specs/ap/watch.md#R17
#[test]
fn r17_the_answer_and_the_exits() {
    let mut f = F::quiet();
    let v = f.pass();
    let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "at",
            "detectors",
            "opened",
            "seen",
            "reopened",
            "closed",
            "failed"
        ])
    );
    assert_eq!(v["at"], utc(BASE));
    let names: Vec<&str> = v["detectors"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(names.len(), 10);
    for d in ap::watch::DETECTORS {
        assert_eq!(v["detectors"][d], json!({"state": "ok", "signals": 0}));
    }
    probe(&mut f, "z.service");
    f.refuse
        .insert("open_finding".into(), "store_busy_forever".into());
    f.at(5);
    let (v, exit) = f.run(&[]);
    assert_eq!(exit, 1);
    assert_eq!(
        v["failed"],
        json!([{"key": k("services", "failed z.service"), "code": "store_busy_forever"}])
    );
    // A store that cannot be searched: that detector acts on nothing.
    f.refuse.clear();
    f.dead.insert("ont".into());
    f.at(10);
    let (v, exit) = f.run(&[]);
    assert_eq!(exit, 1);
    assert_eq!(v["failed"][0]["key"], "watch:delivery");
    assert_eq!(v["failed"].as_array().unwrap().len(), 10);
    assert!(f.dir.join("state.json").exists());
}

// spec: specs/ap/watch.md#R18
#[test]
fn r18_the_unit_files() {
    let service = include_str!("../../../deploy/ap-watch.service");
    for line in [
        "Type=oneshot",
        "ExecStart=/usr/local/bin/ap watch",
        "Nice=10",
        "Environment=TZ=UTC",
        "Environment=HOME=/root",
    ] {
        assert!(service.lines().any(|l| l == line), "{line}");
    }
    let path = service
        .lines()
        .find_map(|l| l.strip_prefix("Environment=PATH="))
        .unwrap();
    assert!(path.split(':').any(|d| d == "/usr/sbin"));
    assert!(!service.contains("Restart="));
    let timer = include_str!("../../../deploy/ap-watch.timer");
    for line in [
        "[Timer]",
        "OnCalendar=*:0/5",
        "AccuracySec=1s",
        "Persistent=false",
        "WantedBy=timers.target",
    ] {
        assert!(timer.lines().any(|l| l == line), "{line}");
    }
    let readme = include_str!("../../../deploy/README.md");
    assert!(readme.contains("## The watch"));
    let (_, section) = readme.split_once("\n## The watch\n").unwrap();
    let section = section
        .split_once("\n## ")
        .map_or(section, |(head, _)| head);
    assert!(section.contains("ap-watch.service") && section.contains("ap-watch.timer"));
    assert!(
        !section
            .lines()
            .any(|l| l.starts_with("install -m 0644 deploy/"))
    );
    assert!(readme.contains("/etc/systemd/system/ap-watch.service.d/path.conf"));
    for d in ap::watch::DETECTORS {
        assert!(readme.contains(&format!("- `{d}`:")), "{d}");
    }
}

// spec: specs/ap/watch.md#R19
#[test]
fn r19_a_pass_only_reads_and_writes_only_its_directory() {
    let mut f = F::quiet();
    probe(&mut f, "watch-probe-3.service");
    ops(&mut f);
    f.pass();
    assert!(
        f.programs()
            .iter()
            .any(|(p, a)| p == "git" && a[2] == "for-each-ref")
    );
    for (program, a) in f.programs() {
        match program.as_str() {
            "systemctl" => assert!(a[0] == "show" || a[0] == "--failed", "{a:?}"),
            "ont" => assert!(
                ["search", "log"].contains(&a[0].as_str())
                    || a.starts_with(&["action".to_owned(), "apply".to_owned()]),
                "{a:?}"
            ),
            "gh" => assert!((a[0] == "run" && a[1] == "list") || a[0] == "api", "{a:?}"),
            "python3" => assert!(a.contains(&"--check".to_owned()), "{a:?}"),
            "git" => assert!(
                ["rev-parse", "show", "cat-file", "ls-tree", "for-each-ref"]
                    .contains(&a[2].as_str()),
                "{a:?}"
            ),
            INSTALLED_AP => assert_eq!(a, ["--version"]),
            "claude" | "muse" | "agy" => assert!(
                a == ["--version"] || a == ["--help"] || a == ["exec", "--help"],
                "{a:?}"
            ),
            "stat" | "find" | "sh" | "df" | "ps" => {}
            other => panic!("{other}"),
        }
    }
    for b in f.bodies() {
        let a = b["action_type"].as_str().unwrap();
        assert!(
            [
                "open_finding",
                "see_finding_again",
                "reopen_finding",
                "close_finding"
            ]
            .contains(&a)
        );
    }
    let mut files: Vec<String> = std::fs::read_dir(&f.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, ["login.json", "pass.lock", "state.json"]);
}

// spec: specs/ap/watch.md#R8
// spec: specs/ap/watch.md#R10
#[test]
fn r8_no_state_json_keeps_a_kept_end_failed_finding_open() {
    let mut f = F::quiet();
    f.deliver.as_mut().unwrap()["end"] = json!("failed");
    f.deliver.as_mut().unwrap()["code"] = json!("restore_failed");
    f.at(5);
    f.pass();
    let kf = k("delivery", "end failed restore_failed");
    f.deliver.as_mut().unwrap()["end"] = json!("refused");
    f.deliver.as_mut().unwrap()["code"] = json!("suppressed");
    f.at(10);
    f.pass();
    std::fs::remove_file(f.dir.join("state.json")).unwrap();
    f.at(15);
    let v = f.pass();
    assert_eq!(f.finding(&kf)["state"], "open", "{v}");
    assert_eq!(v["closed"], json!([]), "{v}");
    assert_eq!(v["opened"], json!([]), "{v}");
    // Whether a failure is kept stays unknown until an end `installed`, `current` or `failed`.
    f.at(20);
    let v = f.pass();
    assert_eq!(f.finding(&kf)["state"], "open", "{v}");
    assert_eq!(
        (v["closed"].clone(), v["opened"].clone()),
        (json!([]), json!([])),
        "{v}"
    );
    f.deliver.as_mut().unwrap()["code"] = json!("dirty");
    f.at(25);
    let v = f.pass();
    assert_eq!(
        v["opened"],
        json!([k("delivery", "end refused dirty")]),
        "{v}"
    );
    assert_eq!(f.finding(&kf)["state"], "open", "{v}");
    let d = f.deliver.as_mut().unwrap();
    (d["end"], d["code"]) = (json!("current"), Value::Null);
    f.at(30);
    let v = f.pass();
    assert!(v["closed"].as_array().unwrap().contains(&json!(kf)), "{v}");
    assert_eq!(f.state()["delivery"]["failed"], Value::Null);
}

// spec: specs/ap/watch.md#R8
// spec: specs/ap/watch.md#R12
#[test]
fn r8_no_state_json_keeps_the_host_findings_that_compare_with_it() {
    let mut f = F::quiet();
    f.files.insert(MEMORY_EVENTS.into(), b"high 6\n".to_vec());
    let mut log = f.files[APT].clone();
    log.extend(b"\nStart-Date: 2026-11-19  00:02:00\nCommandline: apt-get install x\n");
    f.files.insert(APT.into(), log);
    f.at(5);
    let v = f.pass();
    let (km, ka) = (
        k("host", "slice memory high"),
        k("host", "apt 2026-11-19 00:02:00"),
    );
    assert_eq!(v["opened"].as_array().unwrap().len(), 2, "{v}");
    std::fs::remove_file(f.dir.join("state.json")).unwrap();
    f.at(10);
    let v = f.pass();
    assert_eq!(v["closed"], json!([]), "{v}");
    assert_eq!(f.finding(&km)["state"], "open");
    assert_eq!(f.finding(&ka)["state"], "open");
    // The next pass has its baseline: both families are known again.
    f.at(15);
    let v = f.pass();
    let closed = v["closed"].as_array().unwrap();
    assert!(
        closed.contains(&json!(km)) && closed.contains(&json!(ka)),
        "{v}"
    );
}

// ---- Failed unit names beginning with `-`.

// spec: specs/ap/watch.md#R11
#[test]
fn r11_a_failed_unit_starting_with_dash_passes_double_dash() {
    let mut f = F::quiet();
    probe(&mut f, "-.mount");
    let v = f.pass();
    assert_eq!(v["detectors"]["services"]["state"], "ok");
    let km = k("services", "failed -.mount");
    assert_eq!(v["opened"], json!([km]));
    let shown = f
        .programs()
        .into_iter()
        .find(|(p, a)| p == "systemctl" && a[0] == "show" && a.contains(&"-.mount".to_owned()))
        .unwrap();
    let dash = shown.1.iter().position(|a| a == "--").unwrap();
    let unit = shown.1.iter().position(|a| a == "-.mount").unwrap();
    assert!(dash < unit, "{:?}", shown.1);
}

// ---- accounts, checks, runtimes, drift.

fn set_policy(f: &mut F, v: Value) {
    let path = "/invented/policy.json";
    f.policy = Some(PathBuf::from(path));
    f.files.insert(path.into(), v.to_string().into_bytes());
}

// spec: specs/ap/watch.md#R20
#[test]
fn r20_policy_absence_and_parse_failure_and_detector_order() {
    assert_eq!(
        ap::watch::DETECTORS,
        [
            "delivery", "login", "services", "host", "records", "accounts", "checks", "runtimes",
            "drift", "credit"
        ]
    );
    let mut f = F::quiet();
    let ks = k("accounts", "sign-in /invented/claude");
    let kl = k("accounts", "limit /invented/claude");
    let km = k("checks", "main red example-owner/app rust");
    let kd = k("checks", "dependabot example-owner/app");
    let ke = k(
        "checks",
        "settings example-owner/app settings.allow_merge_commit",
    );
    f.plant(
        &ks,
        stored(
            "accounts",
            "sign-in /invented/claude: x",
            "act",
            "open",
            BASE,
        ),
    );
    f.plant(
        &kl,
        stored(
            "accounts",
            "limit /invented/claude: x",
            "info",
            "open",
            BASE,
        ),
    );
    f.plant(
        &km,
        stored(
            "checks",
            "main red example-owner/app rust: x",
            "act",
            "open",
            BASE,
        ),
    );
    f.plant(
        &kd,
        stored(
            "checks",
            "dependabot example-owner/app: x",
            "act",
            "open",
            BASE,
        ),
    );
    f.plant(
        &ke,
        stored(
            "checks",
            "settings example-owner/app settings.allow_merge_commit: x",
            "act",
            "open",
            BASE,
        ),
    );
    let v = f.pass();
    assert_eq!(
        v["detectors"]["accounts"],
        json!({"state": "ok", "signals": 0})
    );
    assert_eq!(
        v["detectors"]["checks"],
        json!({"state": "ok", "signals": 0})
    );
    assert_eq!(v["closed"], json!([]));
    assert!(
        !f.programs()
            .iter()
            .any(|(p, _)| p == "gh" || p == "python3")
    );
    // Malformed policy makes both `accounts` and `checks` unreadable and closes nothing.
    let path = "/invented/bad-policy.json";
    f.policy = Some(PathBuf::from(path));
    f.files.insert(path.into(), b"{not json".to_vec());
    f.at(5);
    let v = f.pass();
    assert_eq!(v["detectors"]["accounts"]["state"], "unreadable");
    assert_eq!(v["detectors"]["checks"]["state"], "unreadable");
    assert_eq!(v["closed"], json!([]));
    for key in [&ks, &kl, &km, &kd, &ke] {
        assert_eq!(f.finding(key)["state"], "open");
    }
}

// spec: specs/ap/watch.md#R21
#[test]
fn r21_accounts_sign_in_expiry_absent_and_no_secret_bytes() {
    let mut f = F::quiet();
    let d1 = "/invented/acc-one";
    let d2 = "/invented/acc-two";
    set_policy(
        &mut f,
        json!({
            "models": {"allowed": ["sonnet"], "never": []},
            "accounts": {
                "claude": [d1],
                "claude/implementer": [format!("{d1}/"), d2],
                "codex": ["/invented/codex-one"]
            }
        }),
    );
    // d1 expires in 72 hours exactly -> act; d2 is absent -> absent [act].
    let exp_72h_ms = (BASE + 72 * 3_600) * 1_000;
    f.files.insert(
        format!("{d1}/.credentials.json"),
        json!({
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-MADEUPSECRET123",
                "refreshToken": "sk-ant-ort01-MADEUPREFRESH456",
                "refreshTokenExpiresAt": exp_72h_ms
            }
        })
        .to_string()
        .into_bytes(),
    );
    let v = f.pass();
    let k1 = k("accounts", &format!("sign-in {d1}"));
    let k2a = k("accounts", &format!("sign-in {d2} absent"));
    let mut opened: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    opened.sort();
    let mut want = vec![k1.clone(), k2a.clone()];
    want.sort();
    assert_eq!(opened, want);
    assert_eq!(f.finding(&k1)["severity"], "act");
    assert_eq!(f.finding(&k1)["kind"], "incident");
    let ev1 = f.finding(&k1)["evidence"].as_str().unwrap().to_owned();
    assert!(ev1.contains(&utc(BASE + 72 * 3_600)), "{ev1}");
    assert!(!ev1.contains("MADEUPSECRET123") && !ev1.contains("MADEUPREFRESH456"));
    assert_eq!(f.finding(&k2a)["severity"], "act");
    // d1 past expiry -> urgent; d2 invalid JSON with secret inside -> absent [act] without parser text.
    f.files.insert(
        format!("{d2}/.credentials.json"),
        b"\"MADEUPBADTOKEN\"".to_vec(),
    );
    f.at(73 * 60);
    f.reset();
    let v = f.pass();
    assert_eq!(f.finding(&k1)["severity"], "urgent");
    assert!(v["seen"].as_array().unwrap().contains(&json!(k1)));
    assert_eq!(f.finding(&k2a)["state"], "open");
    assert!(
        !f.finding(&k2a)["evidence"]
            .as_str()
            .unwrap()
            .contains("MADEUPBADTOKEN")
    );
    // Missing refreshTokenExpiresAt or non-integer -> absent [act]; over 72h -> closes.
    f.files.insert(
        format!("{d1}/.credentials.json"),
        json!({"claudeAiOauth": {"refreshTokenExpiresAt": (BASE + 200 * 3_600) * 1_000}})
            .to_string()
            .into_bytes(),
    );
    f.files.insert(
        format!("{d2}/.credentials.json"),
        json!({"claudeAiOauth": {"refreshTokenExpiresAt": 1.5}})
            .to_string()
            .into_bytes(),
    );
    f.at(74 * 60);
    let v = f.pass();
    assert_eq!(v["closed"], json!([k1]));
    assert_eq!(f.finding(&k2a)["state"], "open");
    // Check no secret text entered state.json or any store body.
    let state_bytes = std::fs::read_to_string(f.dir.join("state.json")).unwrap();
    assert!(!state_bytes.contains("MADEUP"));
    // Unreadable credentials file -> detector unreadable.
    f.unreadable.insert(format!("{d1}/.credentials.json"));
    f.at(75 * 60);
    let v = f.pass();
    assert_eq!(v["detectors"]["accounts"]["state"], "unreadable");
}

// spec: specs/ap/watch.md#R22
#[test]
fn r22_accounts_limit_newest_per_window_and_search_failure() {
    let mut f = F::quiet();
    let d1 = "/invented/acc-a";
    let d2 = "/invented/acc-b";
    let d3 = "/invented/acc-c";
    set_policy(
        &mut f,
        json!({
            "models": {"allowed": ["sonnet"], "never": []},
            "accounts": {"claude": [d1]}
        }),
    );
    f.files.insert(
        format!("{d1}/.credentials.json"),
        json!({"claudeAiOauth": {"refreshTokenExpiresAt": (BASE + 200 * 3_600) * 1_000}})
            .to_string()
            .into_bytes(),
    );
    // d1: older 0.95 superseded by newer 0.40 -> no signal.
    f.runs.insert(
        "run-a-old".into(),
        json!({
            "account": d1,
            "started_at": utc(BASE - 3_600),
            "limit": json!({"windows": {"five_hour": {"utilization": 0.95}}}).to_string()
        }),
    );
    f.runs.insert(
        "run-a-new".into(),
        json!({
            "account": d1,
            "started_at": utc(BASE - 600),
            "limit": json!({"windows": {"five_hour": {"utilization": 0.40}}}).to_string()
        }),
    );
    // d2: older seven_day 0.92, newer run only has five_hour 0.50 -> seven_day newest is 0.92 -> signal.
    f.runs.insert(
        "run-b-old".into(),
        json!({
            "account": d2,
            "started_at": utc(BASE - 2 * 3_600),
            "limit": json!({"windows": {"seven_day": {"utilization": 0.92}}}).to_string()
        }),
    );
    f.runs.insert(
        "run-b-new".into(),
        json!({
            "account": d2,
            "started_at": utc(BASE - 300),
            "limit": json!({"window": "five_hour", "utilization": 0.50}).to_string()
        }),
    );
    // d3: exact 0.90 in single-window shape, plus an >6h old run ignored.
    f.runs.insert(
        "run-c-now".into(),
        json!({
            "account": d3,
            "started_at": utc(BASE - 100),
            "limit": json!({"window": "five_hour", "utilization": 0.90}).to_string()
        }),
    );
    f.runs.insert(
        "run-d-stale".into(),
        json!({
            "account": "/invented/acc-old",
            "started_at": utc(BASE - 6 * 3_600 - 1),
            "limit": json!({"window": "five_hour", "utilization": 0.99}).to_string()
        }),
    );
    let v = f.pass();
    let kb = k("accounts", &format!("limit {d2}"));
    let kc = k("accounts", &format!("limit {d3}"));
    let mut opened: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    opened.sort();
    let mut want = vec![kb.clone(), kc.clone()];
    want.sort();
    assert_eq!(opened, want);
    assert_eq!(f.finding(&kb)["severity"], "info");
    let ev = f.finding(&kb)["evidence"].as_str().unwrap();
    assert!(
        ev.contains("seven_day") && ev.contains("0.92") && ev.contains("run-b-old"),
        "{ev}"
    );
    // Search failure -> unreadable.
    f.dead.insert("ont".into());
    f.at(5);
    let (v, exit) = f.run(&[]);
    assert_eq!(exit, 1);
    assert_eq!(v["detectors"]["accounts"]["state"], "unreadable");
}

// spec: specs/ap/watch.md#R23
#[test]
fn r23_checks_main_red_newest_completed_workflow() {
    let mut f = F::quiet();
    let repo = "example-owner/app";
    set_policy(
        &mut f,
        json!({"repositories": {repo: {"clone": "/invented/app"}}}),
    );
    f.gh_runs.insert(
        repo.into(),
        json_reply(
            true,
            &json!([
                {"workflowName": "rust", "status": "completed", "conclusion": "failure", "headSha": "deadbeef1234567890abcdef", "createdAt": "2026-11-18T23:50:00Z"},
                {"workflowName": "rust", "status": "in_progress", "conclusion": null, "headSha": "1111111111111111", "createdAt": "2026-11-18T23:59:00Z"},
                {"workflowName": "lint", "status": "completed", "conclusion": "success", "headSha": "2222222222222222", "createdAt": "2026-11-18T23:55:00Z"},
                {"workflowName": "lint", "status": "completed", "conclusion": "failure", "headSha": "3333333333333333", "createdAt": "2026-11-18T23:40:00Z"},
                {"workflowName": "audit", "status": "completed", "conclusion": "skipped", "headSha": "4444444444444444", "createdAt": "2026-11-18T23:55:00Z"}
            ]),
        ),
    );
    let v = f.pass();
    let kr = k("checks", &format!("main red {repo} rust"));
    assert_eq!(v["opened"], json!([kr]));
    assert_eq!(f.finding(&kr)["severity"], "act");
    let ev = f.finding(&kr)["evidence"].as_str().unwrap();
    assert!(
        ev.contains("rust") && ev.contains("failure") && ev.contains("deadbeef1234"),
        "{ev}"
    );
    assert!(!ev.contains("deadbeef12345678"), "{ev}");
    // Newer completed success closes the finding.
    f.gh_runs.insert(
        repo.into(),
        json_reply(
            true,
            &json!([
                {"workflowName": "rust", "status": "completed", "conclusion": "success", "headSha": "abcdef1234567890", "createdAt": "2026-11-19T00:02:00Z"}
            ]),
        ),
    );
    f.at(5);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kr]));
}

// spec: specs/ap/watch.md#R24
#[test]
fn r24_checks_dependabot_alerts_off_vs_open_vs_other_403() {
    let mut f = F::quiet();
    let r1 = "example-owner/alpha";
    let r2 = "example-owner/beta";
    let r3 = "example-owner/gamma";
    set_policy(&mut f, json!({"repositories": {r1: {}, r2: {}, r3: {}}}));
    f.files.insert(
        GITHUB_REPOSITORIES.into(),
        json!({
            "rulesets": [],
            "repositories": [
                {"repository": r1, "vulnerability_alerts": true},
                {"repository": r2, "vulnerability_alerts": true},
                {"repository": r3}
            ]
        })
        .to_string()
        .into_bytes(),
    );
    f.gh_alerts.insert(
        r1.into(),
        json_reply(
            true,
            &json!([
                {"security_vulnerability": {"severity": "medium"}, "security_advisory": {"summary": "SECRET_ADVISORY_TEXT"}},
                {"security_vulnerability": {"severity": "critical"}, "security_advisory": {"summary": "OTHER_ADVISORY_TEXT"}}
            ]),
        ),
    );
    // r2 returns 403 disabled -> off, no signal.
    f.gh_alerts.insert(
        r2.into(),
        json_reply(
            false,
            &json!({"message": "Dependabot alerts are disabled for this repository.", "status": "403"}),
        ),
    );
    let v = f.pass();
    assert_eq!(v["detectors"]["checks"]["state"], "ok");
    let kd = k("checks", &format!("dependabot {r1}"));
    assert_eq!(v["opened"], json!([kd]));
    let ev = f.finding(&kd)["evidence"].as_str().unwrap();
    assert!(ev.contains('2') && ev.contains("critical"), "{ev}");
    assert!(!ev.contains("SECRET_ADVISORY_TEXT") && !ev.contains("OTHER_ADVISORY_TEXT"));
    // r3 was not queried for dependabot alerts.
    assert!(
        !f.programs()
            .iter()
            .any(|(p, a)| p == "gh" && a[0] == "api" && a[1].contains(r3))
    );
    // r2 returning 404 -> still off.
    f.gh_alerts.insert(
        r2.into(),
        json_reply(false, &json!({"message": "Not Found", "status": "404"})),
    );
    f.at(5);
    assert_eq!(f.pass()["detectors"]["checks"]["state"], "ok");
    // r2 returning another 403 -> unreadable.
    f.gh_alerts.insert(
        r2.into(),
        json_reply(
            false,
            &json!({"message": "Must have admin rights to Repository.", "status": "403"}),
        ),
    );
    f.at(10);
    let v = f.pass();
    assert_eq!(v["detectors"]["checks"]["state"], "unreadable");
}

// spec: specs/ap/watch.md#R25
#[test]
fn r25_checks_settings_exit_1_refused_failed_and_gh_failures() {
    let mut f = F::quiet();
    let repo = "example-owner/alpha";
    set_policy(&mut f, json!({"repositories": {repo: {}}}));
    // Exit 1 with valid JSON reading: wanted non-null [act], wanted null [info], refused [act].
    f.apply_check = json_reply(
        false,
        &json!({
            "mode": "check",
            "repositories": [
                {
                    "repository": repo,
                    "result": "refused",
                    "code": "classic_protection_present",
                    "differences": [
                        {"setting": "settings.delete_branch_on_merge", "present": false, "wanted": true},
                        {"setting": "ruleset:extra", "present": {"name": "extra"}, "wanted": null}
                    ],
                    "undo": null
                }
            ]
        }),
    );
    let v = f.pass();
    assert_eq!(v["detectors"]["checks"]["state"], "ok");
    let kc = k("checks", &format!("settings {repo} classic protection"));
    let kd = k(
        "checks",
        &format!("settings {repo} settings.delete_branch_on_merge"),
    );
    let ku = k("checks", &format!("settings {repo} ruleset:extra"));
    assert_eq!(f.finding(&kc)["severity"], "act");
    assert_eq!(f.finding(&kd)["severity"], "act");
    assert_eq!(f.finding(&ku)["severity"], "info");
    // A repository answered `failed` -> checks unreadable.
    f.apply_check = json_reply(
        false,
        &json!({
            "mode": "check",
            "repositories": [{"repository": repo, "result": "failed", "code": "read_failed", "differences": [], "undo": null}]
        }),
    );
    f.at(5);
    assert_eq!(f.pass()["detectors"]["checks"]["state"], "unreadable");
    // Exit 2 (empty stdout) -> checks unreadable.
    f.apply_check = failed("invalid_file@github/apply: bad\n");
    f.at(10);
    assert_eq!(f.pass()["detectors"]["checks"]["state"], "unreadable");
    // Exit 1 with non-JSON -> checks unreadable.
    f.apply_check = Ok(Reply {
        success: false,
        stdout: b"traceback".to_vec(),
        bytes: b"traceback".to_vec(),
    });
    f.at(15);
    assert_eq!(f.pass()["detectors"]["checks"]["state"], "unreadable");
    // `gh` failure -> checks unreadable.
    f.apply_check = json_reply(true, &json!({"mode": "check", "repositories": []}));
    f.gh_runs.insert(repo.into(), failed("gh timeout"));
    f.at(20);
    assert_eq!(f.pass()["detectors"]["checks"]["state"], "unreadable");
}

// spec: specs/ap/watch.md#R26
#[test]
fn r26_runtimes_version_change_help_matching_and_kept_version() {
    assert!(flag_on_line("  --a, --b <x>", "--a"));
    assert!(flag_on_line("  --a, --b <x>", "--b"));
    assert!(!flag_on_line("  --print-timeout <t>", "--print"));
    assert!(!flag_on_line("  see --print for details", "--print"));

    let mut f = F::quiet();
    let before = f.state();
    assert_eq!(before["runtimes"]["claude"]["version"], "claude 1.0.0");
    // Same version and flags digest -> --help is not called.
    f.reset();
    f.at(5);
    f.pass();
    assert!(
        !f.programs()
            .iter()
            .any(|(p, a)| a.contains(&"--help".to_owned()) && p == "claude")
    );
    // Ensure codex is never called.
    assert!(!f.programs().iter().any(|(p, _)| p == "codex"));
    // Version change where one flag is missing and another uses `--a, --b <x>` form:
    f.cli_version.insert("agy".into(), ok("agy 2.0.0\n"));
    f.cli_help.insert(
        ("agy".into(), String::new()),
        ok("  --print, --output-format <fmt>\n  --model <m>\n  --dangerously-skip-permissions\n  --print-timeout <t>\n"),
    );
    f.at(10);
    let v = f.pass();
    let kf = k("runtimes", "cli agy flag --conversation");
    assert_eq!(v["opened"], json!([kf]));
    // Kept version is NOT replaced when a flag is missing.
    assert_eq!(f.state()["runtimes"]["agy"]["version"], "agy 1.0.0");
    // Now `--conversation` is present on `--a, --b <x>` line -> closes and updates kept version.
    f.cli_help.insert(
        ("agy".into(), String::new()),
        ok("  --print, --output-format <fmt>\n  --model <m>\n  --dangerously-skip-permissions\n  --print-timeout <t>\n  --x, --conversation <id>\n"),
    );
    f.at(15);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kf]));
    assert_eq!(f.state()["runtimes"]["agy"]["version"], "agy 2.0.0");
    // `--help` failure -> `cli <name> help`, kept version unchanged.
    f.cli_version.insert("muse".into(), ok("muse 2.0.0\n"));
    f.cli_help
        .insert(("muse".into(), "exec".into()), failed("err"));
    f.at(20);
    let v = f.pass();
    assert_eq!(v["opened"], json!([k("runtimes", "cli muse help")]));
    assert_eq!(f.state()["runtimes"]["muse"]["version"], "muse 1.0.0");
    // Missing CLI (`--version` fails) -> `cli <name> absent`, kept version unchanged.
    f.cli_version.insert("claude".into(), broken("claude"));
    f.at(25);
    let v = f.pass();
    assert!(
        v["opened"]
            .as_array()
            .unwrap()
            .contains(&json!(k("runtimes", "cli claude absent")))
    );
    assert_eq!(f.state()["runtimes"]["claude"]["version"], "claude 1.0.0");
}

struct RunFake {
    t: PathBuf,
    env: Vec<(String, String)>,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
    launches: Vec<Vec<String>>,
}
impl RunFake {
    fn new() -> Self {
        let t = std::env::temp_dir().join(format!(
            "ap-watch-run-flags-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&t);
        for d in [
            "bin",
            "acc_claude",
            "acc_muse/muse",
            "acc_agy/.gemini/antigravity-cli",
            "cwd",
        ] {
            std::fs::create_dir_all(t.join(d)).unwrap();
        }
        std::fs::write(
            t.join("acc_agy/.gemini/antigravity-cli/antigravity-oauth-token"),
            "tok\n",
        )
        .unwrap();
        for (name, text) in [
            ("claude", include_str!("fixtures/watch/fake-claude.sh")),
            ("muse", include_str!("fixtures/watch/fake-muse.sh")),
            ("agy", include_str!("fixtures/watch/fake-agy.sh")),
        ] {
            let fake = t.join("bin").join(name);
            std::fs::write(&fake, text).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            t.join("bin/lines-acc_muse"),
            "{\"payload\":{\"kind\":\"run_terminal\",\"terminal\":\"completed\",\"text\":\"Verdict: accept\",\"reason\":null}}\n",
        )
        .unwrap();
        std::fs::write(t.join("prompt.md"), "review this\n").unwrap();
        std::fs::write(t.join("mcp.json"), "{}\n").unwrap();
        std::fs::write(t.join("schema.json"), "{\"type\":\"object\"}\n").unwrap();
        Self {
            t,
            env: vec![
                ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
                ("HOME".into(), "/root".into()),
                ("LANG".into(), "C.UTF-8".into()),
            ],
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
            launches: vec![],
        }
    }
    fn p(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
}
impl Drop for RunFake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.t);
    }
}
impl System for RunFake {
    fn call(&mut self, program: &str, args: &[String], _input: &Value, _ms: u64) -> Answer {
        if program == "env" {
            return failed("fatal: not a git repository\n");
        }
        assert_eq!(program, "systemctl", "{program} {args:?}");
        let unit = args.last().unwrap();
        match args[0].as_str() {
            "stop" => {
                if let Some(pgid) = self.groups.get(unit) {
                    let _ = Command::new("kill")
                        .args(["-KILL", "--", &format!("-{pgid}")])
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                self.alive.remove(unit);
                ok("")
            }
            "is-active" if self.alive.contains(unit) => ok("active\n"),
            "is-active" => Ok(Reply {
                success: false,
                stdout: b"inactive\n".to_vec(),
                bytes: b"inactive\n".to_vec(),
            }),
            other => panic!("systemctl {other}"),
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
        let pos = argv.iter().rposition(|a| a == "--").unwrap();
        let unit = argv[..pos]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        let child = Command::new(self.t.join("bin").join(&argv[pos + 1]))
            .args(&argv[pos + 2..])
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
        self.env.clone()
    }
}

// spec: specs/ap/watch.md#R26
#[test]
fn r26_a_help_printed_on_standard_error_is_read() {
    // A CLI that prints its `--help` on standard error with exit 0 (the installed Antigravity CLI
    // does): the host's reply then carries the text in `bytes` with an empty `stdout`.
    fn on_stderr(text: &str) -> Answer {
        Ok(Reply {
            success: true,
            stdout: vec![],
            bytes: text.as_bytes().to_vec(),
        })
    }
    let mut f = F::quiet();
    let spec = RUNTIMES.iter().find(|r| r.name == "agy").unwrap();
    let help = spec
        .flags
        .iter()
        .map(|fl| format!("  {fl}  flag\n"))
        .collect::<String>();
    f.cli_version.insert("agy".into(), ok("agy 2.0.0\n"));
    f.cli_help
        .insert(("agy".into(), String::new()), on_stderr(&help));
    f.at(5);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    assert_eq!(f.state()["runtimes"]["agy"]["version"], "agy 2.0.0");
}

// spec: specs/ap/watch.md#R26
#[test]
fn r26_runtimes_flag_lists_match_ap_run_stages() {
    let mut rf = RunFake::new();
    let mut requests = vec![];
    for role in ["implementer", "fixer", "reviewer"] {
        requests.push(json!({
            "role": role,
            "runtime": "claude",
            "account": rf.p("acc_claude"),
            "model": "sonnet",
            "effort": "high",
            "permission_mode": "auto",
            "cwd": rf.p("cwd"),
            "prompt": rf.p("prompt.md"),
            "out": rf.p(&format!("claude-{role}")),
        }));
    }
    requests.push(json!({
        "role": "reviewer",
        "runtime": "claude",
        "account": rf.p("acc_claude"),
        "model": "sonnet",
        "effort": "high",
        "permission_mode": "auto",
        "cwd": rf.p("cwd"),
        "prompt": rf.p("prompt.md"),
        "mcp_config": [rf.p("mcp.json")],
        "answer_schema": rf.p("schema.json"),
        "resume": {"record": rf.p("claude-reviewer.run.json")},
        "out": rf.p("claude-extra"),
    }));
    for role in ["implementer", "fixer", "reviewer"] {
        requests.push(json!({
            "role": role,
            "runtime": "muse",
            "account": rf.p("acc_muse/muse"),
            "model": "muse-wonder-1.0",
            "effort": "high",
            "permission_mode": "bypassPermissions",
            "cwd": rf.p("cwd"),
            "prompt": rf.p("prompt.md"),
            "out": rf.p(&format!("muse-{role}")),
        }));
    }
    requests.push(json!({
        "role": "reviewer",
        "runtime": "agy",
        "account": rf.p("acc_agy"),
        "model": "gemini-4-hydrogen-medium",
        "effort": "medium",
        "permission_mode": "bypassPermissions",
        "cwd": rf.p("cwd"),
        "prompt": rf.p("prompt.md"),
        "out": rf.p("agy-reviewer"),
    }));
    requests.push(json!({
        "role": "reviewer",
        "runtime": "agy",
        "account": rf.p("acc_agy"),
        "model": "gemini-4-hydrogen-medium",
        "effort": "medium",
        "permission_mode": "bypassPermissions",
        "cwd": rf.p("cwd"),
        "prompt": rf.p("prompt.md"),
        "resume": {"record": rf.p("agy-reviewer.run.json")},
        "out": rf.p("agy-resume"),
    }));
    for req in &requests {
        if req.get("answer_schema").is_some() {
            std::fs::write(rf.t.join("bin/mode-acc_claude"), "typed").unwrap();
            std::fs::write(rf.t.join("bin/object"), "{\"ok\":true}").unwrap();
        }
        let (v, exit) = ap::run::execute(&mut rf, req).unwrap();
        assert_eq!(exit, 0, "{req}: {v}");
    }
    let mut seen_flags: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for argv in &rf.launches {
        let pos = argv.iter().rposition(|a| a == "--").unwrap();
        let cli = &argv[pos + 1];
        let rest = &argv[pos + 2..];
        let spec = RUNTIMES
            .iter()
            .find(|s| s.name == cli)
            .unwrap_or_else(|| panic!("unexpected cli {cli}"));
        if cli == "muse" {
            let first_non_flag = rest
                .iter()
                .find(|w| !w.starts_with('-'))
                .map(String::as_str);
            assert_eq!(first_non_flag, Some("exec"), "{rest:?}");
            assert_eq!(spec.subcommands, ["exec"]);
        }
        let flags = seen_flags.entry(cli.clone()).or_default();
        for w in rest {
            if w.starts_with('-') {
                assert!(
                    spec.flags.contains(&w.as_str()),
                    "{cli}: composed flag {w} missing from RUNTIMES"
                );
                flags.insert(w.clone());
            }
        }
    }
    for spec in &RUNTIMES {
        let composed = seen_flags.get(spec.name).unwrap();
        for flag in spec.flags {
            assert!(
                composed.contains(*flag),
                "{}: RUNTIMES flag {flag} not in any composed vector",
                spec.name
            );
        }
    }
}

// spec: specs/ap/watch.md#R27
#[test]
fn r27_drift_inventory_validation_digests_absent_and_unreadable() {
    let mut f = F::quiet();
    let commit = "1234567890abcdef1234567890abcdef12345678";
    f.git_commit = ok(&format!("{commit}\n"));
    let src_ok = "/invented/host/ok.conf";
    let src_drift = "/invented/host/drift.conf";
    let src_abs = "/invented/host/absent.conf";
    let src_bad_dest = "/invented/host/should-not-read-dest";
    let src_untracked = "/invented/host/should-not-read-untracked";
    let src_rel = "relative/source.conf";
    f.files.insert(src_ok.into(), b"same\n".to_vec());
    f.files
        .insert(src_drift.into(), b"SECRET_HOST_BYTES\n".to_vec());
    f.files.insert(src_bad_dest.into(), b"never-read".to_vec());
    f.files.insert(src_untracked.into(), b"never-read".to_vec());
    f.git_cat_file.insert(
        format!("{commit}:deployments/missing-blob.conf"),
        failed("fatal: Not a valid object name\n"),
    );
    let rows = json!([
        {"source": src_ok, "destination": "deployments/ok.conf", "sha256": sha("same\n")},
        {"source": src_drift, "destination": "deployments/drift.conf", "sha256": sha("expected\n")},
        {"source": src_abs, "destination": "deployments/absent.conf", "sha256": sha("expected\n")},
        {"source": src_bad_dest, "destination": "deployments/../escape.conf", "sha256": sha("x")},
        {"source": src_bad_dest, "destination": "/etc/abs.conf", "sha256": sha("x")},
        {"source": src_untracked, "destination": "deployments/missing-blob.conf", "sha256": sha("x")},
        {"source": src_rel, "destination": "deployments/rel-src.conf", "sha256": sha("x")}
    ]);
    f.git_show = ok(&rows.to_string());
    f.reset();
    let v = f.pass();
    let kd = k("drift", "deployments/drift.conf");
    let kd = {
        let _ = kd;
        k("drift", "drift deployments/drift.conf")
    };
    let ka = k("drift", "drift deployments/absent.conf absent");
    let ki1 = k("drift", "inventory deployments/../escape.conf");
    let ki2 = k("drift", "inventory /etc/abs.conf");
    let ki3 = k("drift", "inventory deployments/missing-blob.conf");
    let ki4 = k("drift", "inventory deployments/rel-src.conf");
    let mut opened: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    opened.sort();
    let mut want = vec![kd.clone(), ka.clone(), ki1, ki2, ki3, ki4];
    want.sort();
    assert_eq!(opened, want);
    assert_eq!(f.finding(&kd)["kind"], "drift");
    assert_eq!(f.finding(&kd)["severity"], "act");
    let ev = f.finding(&kd)["evidence"].as_str().unwrap();
    assert!(
        ev.contains(&sha("expected\n"))
            && ev.contains(&sha("SECRET_HOST_BYTES\n"))
            && ev.contains(commit),
        "{ev}"
    );
    assert!(!ev.contains("SECRET_HOST_BYTES"), "{ev}");
    assert_eq!(f.finding(&ka)["severity"], "act");
    // Invalid inventory rows never read host files.
    let reads = f.reads.borrow();
    assert!(
        !reads
            .iter()
            .any(|p| p == src_bad_dest || p == src_untracked || p == src_rel)
    );
    drop(reads);
    // Never runs git fetch or write commands.
    assert!(
        f.programs()
            .iter()
            .filter(|(p, _)| p == "git")
            .all(
                |(_, a)| ["rev-parse", "show", "cat-file", "ls-tree", "for-each-ref"]
                    .contains(&a[2].as_str())
            )
    );
    // Unreadable host file -> drift unreadable.
    f.git_show = ok(&json!([
        {"source": src_ok, "destination": "deployments/ok.conf", "sha256": sha("same\n")}
    ])
    .to_string());
    f.unreadable.insert(src_ok.into());
    f.at(5);
    assert_eq!(f.pass()["detectors"]["drift"]["state"], "unreadable");
    // Non-array inventory -> drift unreadable.
    f.unreadable.clear();
    f.git_show = ok("{\"not\": \"an array\"}");
    f.at(10);
    assert_eq!(f.pass()["detectors"]["drift"]["state"], "unreadable");
}

/// Delivery's clone of the operations repository with its fetched tip, whose `etc/` holds the policy
/// file, and the host's copy with the same bytes (R29).
fn ops(f: &mut F) {
    f.files.insert(
        format!("{ETC_CLONE}/.git/HEAD"),
        b"ref: refs/heads/main\n".to_vec(),
    );
    let policy = b"{\"version\": 1}\n".to_vec();
    f.etc.insert(
        "etc/agent-platform/policy.json".into(),
        ("100644".into(), "blob".into(), policy.clone()),
    );
    f.files
        .insert("/etc/agent-platform/policy.json".into(), policy);
}
fn opened(v: &Value) -> Vec<String> {
    let mut o: Vec<String> = serde_json::from_value(v["opened"].clone()).unwrap();
    o.sort();
    o
}
fn drift_state(v: &Value) -> &str {
    v["detectors"]["drift"]["state"].as_str().unwrap()
}

// spec: specs/ap/watch.md#R28
#[test]
fn r28_unit_files_of_the_installed_commit_are_compared() {
    let mut f = F::quiet();
    // The quiet host compares both unit files of the installed commit, read from delivery's clone.
    f.pass();
    let reads = f.reads.borrow().clone();
    for name in ["ap-watch.service", "ap-watch.timer"] {
        assert!(reads.contains(&format!("{UNIT_DIR}/{name}")), "{name}");
    }
    assert!(
        f.programs()
            .contains(&(INSTALLED_AP.into(), vec!["--version".into()]))
    );
    // A hand edit, an absent copy, and entries that are no unit file (a link, a README, a directory).
    f.files.insert(
        format!("{UNIT_DIR}/ap-watch.service"),
        b"SECRET_UNIT_BYTES\n".to_vec(),
    );
    f.files.remove(&format!("{UNIT_DIR}/ap-watch.timer"));
    for (name, mode, kind) in [
        ("link.timer", "120000", "blob"),
        ("README.md", "100644", "blob"),
        ("sub.service", "040000", "tree"),
        ("exec.service", "100755", "blob"),
    ] {
        f.deploy
            .insert(name.into(), (mode.into(), kind.into(), b"x\n".to_vec()));
    }
    f.reset();
    f.at(5);
    let v = f.pass();
    let changed = k("drift", "drift deploy/ap-watch.service");
    let absent = k("drift", "drift deploy/ap-watch.timer absent");
    let mut want = vec![changed.clone(), absent.clone()];
    want.sort();
    assert_eq!(opened(&v), want, "{v}");
    let found = f.finding(&changed);
    assert_eq!(
        (&found["kind"], &found["severity"]),
        (&json!("drift"), &json!("act"))
    );
    let ev = found["evidence"].as_str().unwrap();
    let theirs = String::from_utf8(f.deploy["ap-watch.service"].2.clone()).unwrap();
    assert_eq!(
        ev,
        format!(
            "expected {}, actual {}, commit {}",
            sha(&theirs),
            sha("SECRET_UNIT_BYTES\n"),
            installed()
        )
    );
    assert!(!found["summary"].as_str().unwrap().contains("SECRET"));
    let ev = f.finding(&absent)["evidence"].as_str().unwrap().to_owned();
    assert!(ev.contains("absent") && ev.contains(&installed()), "{ev}");
    // The entries that are no unit file are not read on the host.
    let reads = f.reads.borrow().clone();
    for name in ["link.timer", "README.md", "sub.service", "exec.service"] {
        assert!(!reads.contains(&format!("{UNIT_DIR}/{name}")), "{name}");
    }
    // The next pass that sees the copies equal again closes both.
    let bytes = f.deploy["ap-watch.service"].2.clone();
    f.files
        .insert(format!("{UNIT_DIR}/ap-watch.service"), bytes.clone());
    f.files.insert(
        format!("{UNIT_DIR}/ap-watch.timer"),
        f.deploy["ap-watch.timer"].2.clone(),
    );
    f.at(10);
    let v = f.pass();
    let mut closed: Vec<String> = serde_json::from_value(v["closed"].clone()).unwrap();
    closed.sort();
    assert_eq!(closed, want);
}

// spec: specs/ap/watch.md#R28
#[test]
fn r28_each_cause_of_unreadable() {
    type Setup = fn(&mut F);
    let causes: [(&str, Setup); 6] = [
        ("no commit", |f| f.ap_version = ok("ap 0.1.0 unknown\n")),
        ("no version", |f| f.ap_version = failed("")),
        ("two words", |f| {
            f.ap_version = ok(&format!("ap {}\n", installed()))
        }),
        ("ls-tree", |f| {
            f.git_fails.insert(format!("{DELIVER_CLONE} ls-tree"));
        }),
        ("show", |f| {
            f.git_fails.insert(format!("{DELIVER_CLONE} show"));
        }),
        ("host file", |f| {
            f.unreadable.insert(format!("{UNIT_DIR}/ap-watch.timer"));
        }),
    ];
    for (what, setup) in causes {
        let mut f = F::quiet();
        setup(&mut f);
        f.at(5);
        let v = f.pass();
        assert_eq!(drift_state(&v), "unreadable", "{what}: {v}");
        assert_eq!(opened(&v), [k("drift", "unreadable")], "{what}");
    }
}

// spec: specs/ap/watch.md#R29
#[test]
fn r29_files_of_etc_at_the_fetched_tip_are_compared() {
    let mut f = F::quiet();
    ops(&mut f);
    f.at(5);
    let v = f.pass();
    assert_eq!(opened(&v), Vec::<String>::new(), "{v}");
    assert!(
        f.reads
            .borrow()
            .contains(&"/etc/agent-platform/policy.json".to_owned())
    );
    // A hand edit of the policy, a file of the tip absent on the host, and entries the watch does
    // not compare: a link, a submodule, a path with a space or a `..`-like component.
    f.files.insert(
        "/etc/agent-platform/policy.json".into(),
        b"{\"SECRET_POLICY\": 2}\n".to_vec(),
    );
    f.etc.insert(
        "etc/agent-platform/extra.conf".into(),
        ("100755".into(), "blob".into(), b"extra\n".to_vec()),
    );
    for (path, mode, kind) in [
        ("etc/link.conf", "120000", "blob"),
        ("etc/module", "160000", "commit"),
        ("etc/a b.conf", "100644", "blob"),
        ("etc/x/../y", "100644", "blob"),
    ] {
        f.etc
            .insert(path.into(), (mode.into(), kind.into(), b"y\n".to_vec()));
        f.files.insert(format!("/{path}"), b"never read\n".to_vec());
    }
    f.reset();
    f.at(10);
    let v = f.pass();
    let changed = k("drift", "drift etc/agent-platform/policy.json");
    let absent = k("drift", "drift etc/agent-platform/extra.conf absent");
    let mut want = vec![changed.clone(), absent];
    for path in ["etc/link.conf", "etc/module", "etc/a b.conf", "etc/x/../y"] {
        want.push(k("drift", &format!("inventory {path}")));
    }
    want.sort();
    assert_eq!(opened(&v), want, "{v}");
    let found = f.finding(&changed);
    assert_eq!(
        (&found["kind"], &found["severity"]),
        (&json!("drift"), &json!("act"))
    );
    let ev = found["evidence"].as_str().unwrap();
    assert_eq!(
        ev,
        format!(
            "expected {}, actual {}, commit {}",
            sha("{\"version\": 1}\n"),
            sha("{\"SECRET_POLICY\": 2}\n"),
            ops_tip()
        )
    );
    for finding in f.findings.values() {
        let text = finding.to_string();
        assert!(
            !text.contains("SECRET_POLICY") && !text.contains("never read"),
            "{text}"
        );
    }
    let reads = f.reads.borrow().clone();
    for path in ["etc/link.conf", "etc/module", "etc/a b.conf", "etc/x/../y"] {
        assert!(!reads.contains(&format!("/{path}")), "{path}");
    }
}

// spec: specs/ap/watch.md#R29
#[test]
fn r29_no_clone_or_no_fetched_tip_reads_nothing() {
    // No clone: `.git/HEAD` is not found; no `git` call reaches the clone.
    let mut f = F::quiet();
    f.etc.insert(
        "etc/agent-platform/policy.json".into(),
        ("100644".into(), "blob".into(), b"{}\n".to_vec()),
    );
    f.at(5);
    let v = f.pass();
    assert_eq!(drift_state(&v), "ok");
    assert!(f.reads.borrow().contains(&format!("{ETC_CLONE}/.git/HEAD")));
    assert!(
        f.programs()
            .iter()
            .all(|(_, a)| !a.contains(&ETC_CLONE.to_owned()))
    );
    // A clone that no fetch of delivery has reached: no ref of the tip, nothing listed.
    let mut f = F::quiet();
    ops(&mut f);
    f.files.remove("/etc/agent-platform/policy.json");
    f.etc_ref = ok("");
    f.at(5);
    let v = f.pass();
    assert_eq!((drift_state(&v), opened(&v)), ("ok", vec![]), "{v}");
    let words: Vec<String> = f
        .programs()
        .iter()
        .filter(|(_, a)| a.contains(&ETC_CLONE.to_owned()))
        .map(|(_, a)| a[2].clone())
        .collect();
    assert_eq!(words, ["for-each-ref"]);
}

// spec: specs/ap/watch.md#R29
#[test]
fn r29_each_cause_of_unreadable() {
    type Setup = fn(&mut F);
    let causes: [(&str, Setup); 7] = [
        ("HEAD", |f| {
            f.unreadable.insert(format!("{ETC_CLONE}/.git/HEAD"));
        }),
        ("for-each-ref", |f| {
            f.git_fails.insert(format!("{ETC_CLONE} for-each-ref"));
        }),
        ("two refs", |f| {
            f.etc_ref = ok(&format!("{}\n{}\n", ops_tip(), installed()));
        }),
        ("no commit", |f| f.etc_ref = ok("main\n")),
        ("ls-tree", |f| {
            f.git_fails.insert(format!("{ETC_CLONE} ls-tree"));
        }),
        ("show", |f| {
            f.git_fails.insert(format!("{ETC_CLONE} show"));
        }),
        ("host file", |f| {
            f.unreadable
                .insert("/etc/agent-platform/policy.json".into());
        }),
    ];
    for (what, setup) in causes {
        let mut f = F::quiet();
        ops(&mut f);
        setup(&mut f);
        f.at(5);
        let v = f.pass();
        assert_eq!(drift_state(&v), "unreadable", "{what}: {v}");
        assert_eq!(opened(&v), [k("drift", "unreadable")], "{what}");
    }
}

// spec: specs/ap/watch.md#R28
// spec: specs/ap/watch.md#R29
#[test]
fn r28_r29_the_clones_are_those_of_the_host_s_request() {
    let request: Value =
        serde_json::from_str(include_str!("../../../deploy/deliver.json")).unwrap();
    let state = request["state"].as_str().unwrap();
    let clone = |repository: &Value| {
        let name = repository.as_str().unwrap().split_once('/').unwrap().1;
        format!("{state}/{name}")
    };
    assert_eq!(clone(&request["repository"]), DELIVER_CLONE);
    assert_eq!(clone(&request["etc"]["repository"]), ETC_CLONE);
    assert_eq!(INSTALLED_AP, "/usr/local/bin/ap");
    assert_eq!(UNIT_DIR, "/etc/systemd/system");
}

// ---- pc down (R30).

fn pc_policy(f: &mut F) {
    set_policy(
        f,
        json!({"hosts": {"pc": {"ssh": "agent@ap-checks", "work": "/invented/work", "busy_load": 0.5}}}),
    );
}
/// A line of the checks log ending at `end`, with the members R30 must never repeat planted.
fn checks_line(end: u64, host: &str, reason: Value) -> String {
    json!({"start": utc(end - 5), "end": utc(end), "seconds": 5.0, "host": host, "reason": reason,
        "dir": "/planted/dir-7", "repository": "planted-owner/planted-repo", "head": "c".repeat(40),
        "command": ["planted-command", "--flag"], "exit": 0, "pc_cpus": 8, "pc_load": 0.1,
        "pc_available_mb": 4096})
    .to_string()
}
/// A line of the door's record arriving at `at` and closed `ms` milliseconds later.
fn door_line(at: u64, ms: u64, verb: Value, exit: Value, reason: Value) -> String {
    json!({"at": utc(at), "run": "/planted/run-9.run.json", "role": "planted-role", "verb": verb,
        "name": "planted-name", "ms": ms, "exit": exit, "reason": reason})
    .to_string()
}
fn put(f: &mut F, path: &str, lines: &[String]) {
    f.files
        .insert(path.into(), format!("{}\n", lines.join("\n")).into_bytes());
}
fn pc_key() -> String {
    k("host", "pc down")
}

// spec: specs/ap/watch.md#R30
#[test]
fn r30_an_unreachable_check_opens_pc_down_and_a_newer_one_on_the_pc_closes_it() {
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(15);
    let now = f.now();
    put(
        &mut f,
        CHECKS_LOG,
        &[checks_line(now - 600, "local", json!("pc_unreachable"))],
    );
    let v = f.pass();
    assert_eq!(v["opened"], json!([pc_key()]), "{v}");
    assert_eq!(v["detectors"]["host"], json!({"state": "ok", "signals": 1}));
    let o = f.finding(&pc_key());
    assert_eq!(o["severity"], "act");
    assert_eq!(o["kind"], "incident");
    assert_eq!(o["detector"], "watch:host");
    let summary = o["summary"].as_str().unwrap();
    assert!(summary.starts_with("pc down: "), "{summary}");
    assert!(summary.contains("`Start-ScheduledTask -TaskName ap-checks-keepalive`"));
    assert!(summary.contains("`tailscale ping ap-checks`"), "{summary}");
    assert_eq!(
        o["evidence"],
        format!(
            "checks.jsonl at {}: host local, reason pc_unreachable",
            utc(now - 600)
        )
    );
    // Nothing of it is kept in state.json.
    let state = f.state();
    let host = state["host"].as_object().unwrap();
    assert!(host.keys().all(|m| !m.contains("pc")), "{host:?}");
    // The same reading seen again within the hour records nothing.
    f.at(20);
    f.reset();
    let v = f.pass();
    assert_eq!(v["opened"], json!([]));
    assert_eq!(f.actions(), vec![]);
    // A newer line on the PC closes it at the next pass.
    f.at(25);
    let now = f.now();
    let lines = [
        checks_line(now - 600, "local", json!("pc_unreachable")),
        checks_line(now - 60, "pc", Value::Null),
    ];
    put(&mut f, CHECKS_LOG, &lines);
    let v = f.pass();
    assert_eq!(v["closed"], json!([pc_key()]), "{v}");
    assert_eq!(f.finding(&pc_key())["state"], "closed");
}

// spec: specs/ap/watch.md#R30
#[test]
fn r30_a_door_check_that_fell_back_is_a_reading_and_a_door_fn_is_not() {
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(30);
    let now = f.now();
    let old_pc = checks_line(now - 1_200, "pc", Value::Null);
    put(&mut f, CHECKS_LOG, &[old_pc]);
    // `fn` lines (even with a `pc_lost` reason) and a check that went elsewhere are no reading.
    let lines = [
        door_line(now - 300, 1_000, json!("fn"), Value::Null, json!("pc_lost")),
        door_line(now - 200, 1_000, Value::Null, Value::Null, json!("pc_lost")),
        door_line(now - 100, 1_000, json!("check"), Value::Null, json!("here")),
        door_line(
            now - 90,
            1_000,
            json!("check"),
            Value::Null,
            json!("client_gone"),
        ),
    ];
    put(&mut f, DOOR_LOG, &lines);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    // A `check` line with `pc_lost` newer than the checks-log line on the PC opens it.
    let mut lines = lines.to_vec();
    lines.push(door_line(
        now - 900,
        60_000,
        json!("check"),
        Value::Null,
        json!("pc_lost"),
    ));
    put(&mut f, DOOR_LOG, &lines);
    f.at(31);
    let v = f.pass();
    assert_eq!(v["opened"], json!([pc_key()]), "{v}");
    // The time of a door line is its arrival plus the whole seconds of `ms`: 900 - 60 = 840 s old.
    assert_eq!(
        f.finding(&pc_key())["evidence"],
        format!("door.jsonl at {}: reason pc_lost", utc(now - 840))
    );
    // At an equal time `reached` wins: the checks line on the PC ends at the door line's time.
    put(
        &mut f,
        CHECKS_LOG,
        &[checks_line(now - 840, "pc", Value::Null)],
    );
    f.at(32);
    let v = f.pass();
    assert_eq!(v["closed"], json!([pc_key()]), "{v}");
    // A door line with an integer exit reached the PC; a later one that ran on it closes as well.
    lines.push(door_line(
        now + 60,
        2_999,
        json!("check"),
        json!(1),
        Value::Null,
    ));
    put(&mut f, DOOR_LOG, &lines);
    put(&mut f, CHECKS_LOG, &[]);
    f.at(33);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    assert_eq!(f.finding(&pc_key())["state"], "closed");
}

// spec: specs/ap/watch.md#R30
// spec: specs/ap/watch.md#R7
#[test]
fn r30_without_a_reading_of_the_last_60_minutes_an_open_pc_down_is_neither_seen_nor_closed() {
    assert_eq!(family("host", "pc down: x").as_deref(), Some("pc down"));
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(70);
    let now = f.now();
    let open = stored(
        "host",
        "pc down: the local PC took no check",
        "act",
        "open",
        now - 7_200,
    );
    f.plant(&pc_key(), open);
    f.reset();
    // No record at all.
    let v = f.pass();
    assert_eq!(v["detectors"]["host"], json!({"state": "ok", "signals": 0}));
    assert_eq!(f.actions(), vec![], "{v}");
    assert_eq!(f.finding(&pc_key())["state"], "open");
    // Only a reading 61 minutes old, among lines that are no reading (garbage included).
    let lines = [
        "not json".to_owned(),
        "[1,2]".to_owned(),
        json!({"end": "yesterday", "host": "pc"}).to_string(),
        checks_line(now - 3_660, "local", json!("pc_unreachable")),
        door_line(now - 3_700, 0, json!("check"), json!(0), Value::Null),
    ];
    put(&mut f, CHECKS_LOG, &lines);
    f.at(71);
    let v = f.pass();
    assert_eq!(f.actions(), vec![], "{v}");
    assert_eq!(f.finding(&pc_key())["state"], "open");
    // A reading exactly 60 minutes old counts, and a later one than the pass counts too.
    f.at(70);
    put(
        &mut f,
        CHECKS_LOG,
        &[checks_line(now - 3_600, "local", json!("pc_lost"))],
    );
    let v = f.pass();
    assert_eq!(v["seen"], json!([pc_key()]), "{v}");
    put(
        &mut f,
        CHECKS_LOG,
        &[checks_line(now + 90, "pc", Value::Null)],
    );
    f.at(71);
    let v = f.pass();
    assert_eq!(v["closed"], json!([pc_key()]), "{v}");
}

// spec: specs/ap/watch.md#R30
// spec: specs/ap/watch.md#R7
#[test]
fn r30_a_policy_without_the_pc_closes_it_and_no_policy_or_a_malformed_one_leaves_it_open() {
    let mut f = F::quiet();
    f.at(10);
    let now = f.now();
    put(
        &mut f,
        CHECKS_LOG,
        &[checks_line(now - 60, "local", json!("pc_unreachable"))],
    );
    put(
        &mut f,
        DOOR_LOG,
        &[door_line(
            now - 60,
            0,
            json!("check"),
            Value::Null,
            json!("pc_lost"),
        )],
    );
    let open = stored(
        "host",
        "pc down: the local PC took no check",
        "act",
        "open",
        now - 7_200,
    );
    f.plant(&pc_key(), open);
    f.reset();
    // No policy file: unknown, `host` reads, nothing is changed and no record is read.
    let v = f.pass();
    assert_eq!(v["detectors"]["host"]["state"], "ok", "{v}");
    assert_eq!(f.actions(), vec![]);
    // A policy that does not parse: the same.
    f.policy = Some(PathBuf::from("/invented/policy.json"));
    f.files
        .insert("/invented/policy.json".into(), b"{not json".to_vec());
    f.at(11);
    let v = f.pass();
    assert_eq!(v["detectors"]["host"]["state"], "ok", "{v}");
    assert!(f.actions().iter().all(|(_, key)| key != &pc_key()));
    assert_eq!(f.finding(&pc_key())["state"], "open");
    // A policy without `hosts.pc` (or whose `hosts.pc` is no object): known, no signal, closed.
    for policy in [
        json!({}),
        json!({"hosts": {}}),
        json!({"hosts": {"pc": null}}),
    ] {
        set_policy(&mut f, policy);
        f.plant(
            &pc_key(),
            stored(
                "host",
                "pc down: the local PC took no check",
                "act",
                "open",
                now - 7_200,
            ),
        );
        f.reset();
        f.at(12);
        let v = f.pass();
        assert!(
            v["closed"].as_array().unwrap().contains(&json!(pc_key())),
            "{v}"
        );
        let reads = f.reads.borrow();
        assert!(
            !reads.iter().any(|p| p == CHECKS_LOG || p == DOOR_LOG),
            "{reads:?}"
        );
    }
    // The pass reads the policy file once.
    let reads = f.reads.borrow();
    assert_eq!(
        reads
            .iter()
            .filter(|p| *p == "/invented/policy.json")
            .count(),
        1
    );
}

// spec: specs/ap/watch.md#R30
#[test]
fn r30_a_busy_pc_that_answered_gives_no_signal_and_closes_an_open_finding() {
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(20);
    let now = f.now();
    for reason in ["pc_busy", "pc_memory_low", "pc_engine_differs"] {
        let lines = [
            checks_line(now - 600, "local", json!("pc_unreachable")),
            checks_line(now - 300, "local", json!(reason)),
        ];
        put(&mut f, CHECKS_LOG, &lines);
        let v = f.pass();
        assert_eq!(v["opened"], json!([]), "{reason}: {v}");
    }
    let door = [
        door_line(now - 600, 0, json!("check"), Value::Null, json!("pc_lost")),
        door_line(now - 300, 0, json!("check"), Value::Null, json!("pc_busy")),
    ];
    put(&mut f, CHECKS_LOG, &[]);
    put(&mut f, DOOR_LOG, &door);
    let v = f.pass();
    assert_eq!(v["opened"], json!([]), "{v}");
    f.plant(
        &pc_key(),
        stored(
            "host",
            "pc down: the local PC took no check",
            "act",
            "open",
            now - 7_200,
        ),
    );
    let v = f.pass();
    assert_eq!(v["closed"], json!([pc_key()]), "{v}");
}

// spec: specs/ap/watch.md#R30
#[test]
fn r30_the_evidence_holds_no_byte_of_a_planted_dir_command_or_repository() {
    let mut f = F::quiet();
    pc_policy(&mut f);
    f.at(20);
    let now = f.now();
    for (path, lines) in [
        (
            CHECKS_LOG,
            vec![checks_line(now - 60, "local", json!("pc_lost"))],
        ),
        (
            DOOR_LOG,
            vec![door_line(
                now - 60,
                0,
                json!("check"),
                Value::Null,
                json!("pc_lost"),
            )],
        ),
    ] {
        f.files.remove(CHECKS_LOG);
        f.files.remove(DOOR_LOG);
        put(&mut f, path, &lines);
        let v = f.pass();
        assert_eq!(f.finding(&pc_key())["state"], "open", "{v}");
        let o = f.finding(&pc_key());
        let text = format!("{} {}", o["summary"], o["evidence"]);
        for planted in [
            "/planted", "planted-", "run-9", "cccccccc", "--flag", "pc_cpus", "pc_load", "4096",
        ] {
            assert!(!text.contains(planted), "{planted} in {text}");
        }
        f.findings.clear();
        f.reset();
    }
}

// spec: specs/ap/watch.md#R30
#[test]
fn r30_a_record_that_exists_and_cannot_be_read_makes_host_unreadable() {
    for path in [CHECKS_LOG, DOOR_LOG] {
        let mut f = F::quiet();
        pc_policy(&mut f);
        f.at(20);
        let now = f.now();
        put(
            &mut f,
            path,
            &[checks_line(now - 60, "local", json!("pc_unreachable"))],
        );
        f.unreadable.insert(path.into());
        f.plant(
            &pc_key(),
            stored(
                "host",
                "pc down: the local PC took no check",
                "act",
                "open",
                now - 7_200,
            ),
        );
        let v = f.pass();
        assert_eq!(v["detectors"]["host"]["state"], "unreadable", "{path}: {v}");
        assert!(v["closed"].as_array().unwrap().is_empty(), "{v}");
        assert!(f.finding(&k("host", "unreadable"))["summary"].is_string());
        assert_eq!(f.finding(&pc_key())["state"], "open");
    }
    // A record that does not exist adds nothing and is no cause of `unreadable`.
    let mut f = F::quiet();
    pc_policy(&mut f);
    let v = f.pass();
    assert_eq!(v["detectors"]["host"]["state"], "ok", "{v}");
}

// ---- credit (R31).

const KEY_FILE: &str = "/etc/agent-platform/credentials/anthropic-api-key";
const KEY_BYTES: &str = "sk-ant-fixture-KEY-4b1e9d";

/// A policy whose member `api` has the stops 160 (standard) and 185 (soak), and its key file of mode 600.
fn credit_policy(f: &mut F) {
    set_policy(
        f,
        json!({"api": {"key_file": KEY_FILE, "stops_usd": {"standard": 160, "soak": 185},
            "day_cap_usd": 40, "jobs": {"drift": {"cap_usd": 60, "models": ["claude-sonnet-5-5"],
            "max_tokens": 8000, "max_requests": 100}}}}),
    );
    f.files
        .insert(KEY_FILE.into(), KEY_BYTES.as_bytes().to_vec());
    f.modes.insert(KEY_FILE.into(), "600 regular file".into());
}
/// A run of role `batch` as `ap api batch` starts and closes it.
fn batch_run(started: u64, status: &str, reserved: i64, settled: Option<i64>) -> Value {
    let mut v = json!({"role": "batch", "runtime": "api", "model": "claude-sonnet-5-5", "job": "drift",
        "status": status, "started_at": utc(started), "reserved_usd_micros": reserved});
    if let Some(s) = settled {
        v["settled_usd_micros"] = json!(s);
    }
    v
}
/// Whether the pass searched the runs of role `batch` (`batch_runs`) or stated the key file.
fn credit_read(f: &F) -> bool {
    let calls = f.calls.borrow();
    calls.iter().any(|(p, a, input)| {
        (p == "ont"
            && a[0] == "search"
            && input["where"]["all"][0]["eq"] == json!(["role", "batch"]))
            || (p == "stat" && a[1] == "%a %F")
    })
}

// spec: specs/ap/watch.md#R31
#[test]
fn credit_stop_at_80_percent() {
    let mut f = F::quiet();
    credit_policy(&mut f);
    let day = 86_400;
    // The ledger of November: settled, running at its reservation, closed without a settlement (at its
    // reservation); a run of October and a run of another role count nothing. One micro-USD under 80 % of 160.
    let runs = [
        (
            "run-settled",
            batch_run(BASE - 10 * day, "closed", 150_000_000, Some(99_999_999)),
        ),
        (
            "run-running",
            batch_run(BASE - 3_600, "running", 20_000_000, None),
        ),
        (
            "run-unsettled",
            batch_run(BASE - 5 * day, "closed", 8_000_000, None),
        ),
        (
            "run-october",
            batch_run(BASE - 20 * day, "closed", 90_000_000, Some(90_000_000)),
        ),
    ];
    for (key, run) in runs {
        f.runs.insert(key.into(), run);
    }
    let mut other = batch_run(BASE - 2 * day, "running", 50_000_000, None);
    other["role"] = json!("implementer");
    f.runs.insert("run-other-role".into(), other);
    f.at(5);
    let v = f.pass();
    assert_eq!(
        v["detectors"]["credit"],
        json!({"state": "ok", "signals": 0}),
        "{v}"
    );
    assert_eq!(v["opened"], json!([]), "{v}");
    // One micro-USD more: exactly 80 % of the standard stop opens `stop standard`, and not `stop soak`.
    f.runs.get_mut("run-settled").unwrap()["settled_usd_micros"] = json!(100_000_000);
    f.at(10);
    let v = f.pass();
    let ks = k("credit", "stop standard");
    assert_eq!(v["opened"], json!([ks]), "{v}");
    let found = f.finding(&ks);
    assert_eq!(
        (&found["kind"], &found["detector"], &found["severity"]),
        (&json!("incident"), &json!("watch:credit"), &json!("act"))
    );
    assert!(
        found["summary"]
            .as_str()
            .unwrap()
            .starts_with("stop standard: ")
    );
    assert_eq!(
        found["evidence"],
        "month 2026-11, counted_usd_micros 128000000, stop_usd_micros 160000000"
    );
    // A pass of the next month counts none of these runs: it closes the finding.
    f.wall.set(epoch("2026-12-01T00:10:00Z").unwrap());
    let v = f.pass();
    assert!(v["closed"].as_array().unwrap().contains(&json!(ks)), "{v}");
    assert!(!v["opened"].as_array().unwrap().contains(&json!(ks)), "{v}");
    assert_eq!(f.finding(&ks)["state"], "closed");
}

// spec: specs/ap/watch.md#R31
#[test]
fn credit_unsettled_batch_after_26_hours() {
    let mut f = F::quiet();
    credit_policy(&mut f);
    f.at(5);
    let now = f.now();
    let h26 = 26 * 3_600;
    let runs = [
        (
            "run-at-26h",
            batch_run(now - h26, "running", 3_000_000, None),
        ),
        (
            "run-past-26h",
            batch_run(now - h26 - 1, "running", 4_000_000, None),
        ),
        (
            "run-settled",
            batch_run(now - 3 * h26, "closed", 5_000_000, Some(1_000_000)),
        ),
        (
            "run-closed",
            batch_run(now - 3 * h26, "closed", 6_000_000, None),
        ),
    ];
    for (key, run) in runs {
        f.runs.insert(key.into(), run);
    }
    let v = f.pass();
    let kp = k("credit", "unsettled run-past-26h");
    assert_eq!(v["opened"], json!([kp]), "{v}");
    assert_eq!(f.finding(&kp)["severity"], "act");
    assert_eq!(
        f.finding(&kp)["evidence"],
        format!(
            "run run-past-26h, job drift, started_at {}, reserved_usd_micros 4000000",
            utc(now - h26 - 1)
        )
    );
    // One second later the run at 26 hours is past them too; the settled run and the one closed without a
    // settlement never are.
    f.wall.set(now + 1);
    let v = f.pass();
    assert_eq!(
        v["opened"],
        json!([k("credit", "unsettled run-at-26h")]),
        "{v}"
    );
    // A run that is closed is no longer unsettled.
    f.runs.get_mut("run-past-26h").unwrap()["status"] = json!("closed");
    f.wall.set(now + 2);
    let v = f.pass();
    assert_eq!(v["closed"], json!([kp]), "{v}");
}

// spec: specs/ap/watch.md#R31
#[test]
fn credit_key_mode() {
    let mut f = F::quiet();
    credit_policy(&mut f);
    let km = k("credit", "key mode");
    f.modes.insert(KEY_FILE.into(), "640 regular file".into());
    f.at(5);
    let v = f.pass();
    assert_eq!(v["opened"], json!([km]), "{v}");
    assert_eq!(f.finding(&km)["severity"], "urgent");
    assert_eq!(f.finding(&km)["evidence"], "mode 640, type regular file");
    // 600: no signal, the finding closes.
    f.modes.insert(KEY_FILE.into(), "600 regular file".into());
    f.at(10);
    let v = f.pass();
    assert_eq!(v["closed"], json!([km]), "{v}");
    // A link (never followed): urgent again.
    f.modes.insert(KEY_FILE.into(), "777 symbolic link".into());
    f.at(15);
    let v = f.pass();
    assert_eq!(v["reopened"], json!([km]), "{v}");
    assert_eq!(f.finding(&km)["evidence"], "mode 777, type symbolic link");
    // Not found: no signal.
    f.modes.remove(KEY_FILE);
    f.at(20);
    let v = f.pass();
    assert_eq!(
        v["detectors"]["credit"],
        json!({"state": "ok", "signals": 0}),
        "{v}"
    );
    assert_eq!(v["closed"], json!([km]), "{v}");
    // A stat that fails other than by not finding the file: unreadable.
    f.stat_fails.insert(KEY_FILE.into());
    f.at(25);
    let v = f.pass();
    assert_eq!(v["detectors"]["credit"]["state"], "unreadable", "{v}");
    // The key file is examined by `stat` alone and never read: no byte of it is in a call, a finding or state.json.
    let stats: Vec<Vec<String>> = f
        .programs()
        .into_iter()
        .filter(|(p, a)| p == "stat" && a.last().is_some_and(|l| l == KEY_FILE))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(stats.len(), 5);
    assert!(stats.iter().all(|a| a == &["-c", "%a %F", KEY_FILE]));
    assert!(!f.reads.borrow().iter().any(|r| r == KEY_FILE));
    for (_, a, input) in f.calls.borrow().iter() {
        assert!(!a.concat().contains(KEY_BYTES) && !input.to_string().contains(KEY_BYTES));
    }
    let all = serde_json::to_string(&f.findings).unwrap();
    assert!(!all.contains(KEY_BYTES));
    assert!(!f.state().to_string().contains(KEY_BYTES));
    assert!(f.state().get("credit").is_none());
}

// spec: specs/ap/watch.md#R31
// spec: specs/ap/watch.md#R20
#[test]
fn credit_unknown_without_api() {
    let mut f = F::quiet();
    let ks = k("credit", "stop standard");
    f.plant(
        &ks,
        stored("credit", "stop standard: x", "act", "open", BASE - 7_200),
    );
    // A ledger that would open `stop standard` and `unsettled` if it were read.
    f.runs.insert(
        "run-old".into(),
        batch_run(BASE - 10 * 86_400, "running", 150_000_000, None),
    );
    // No policy file, then a policy without `api`: every family unknown, the finding neither seen nor closed.
    for policy in [
        None,
        Some(json!({"models": {"allowed": ["sonnet"], "never": []}})),
    ] {
        f.reset();
        if let Some(p) = policy {
            set_policy(&mut f, p);
        }
        f.at(f.now().saturating_sub(BASE) / 60 + 5);
        let v = f.pass();
        assert_eq!(
            v["detectors"]["credit"],
            json!({"state": "ok", "signals": 0}),
            "{v}"
        );
        for list in ["opened", "seen", "reopened", "closed"] {
            assert_eq!(v[list], json!([]), "{list}: {v}");
        }
        assert_eq!(f.finding(&ks)["state"], "open");
        assert!(!credit_read(&f), "credit read a source without api");
    }
    // An `api` of another form: unreadable, which closes nothing.
    set_policy(&mut f, json!({"api": {"key_file": "/invented/key"}}));
    f.at(f.now().saturating_sub(BASE) / 60 + 5);
    let v = f.pass();
    assert_eq!(v["detectors"]["credit"]["state"], "unreadable", "{v}");
    assert_eq!(v["opened"], json!([k("credit", "unreadable")]), "{v}");
    assert_eq!(v["closed"], json!([]), "{v}");
    assert!(!v["seen"].as_array().unwrap().contains(&json!(ks)), "{v}");
    assert_eq!(f.finding(&ks)["state"], "open");
}
