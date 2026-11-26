//! `ap clause index` and the function `clause` (`specs/ap/clause.md`) against a fake host: the fake answers `git`
//! from invented repositories and `ont` (`index`, `search`, `around`) as the engine answers. The directory each test
//! writes is a temporary one; no test reaches a clone, the policy file or the store of this host. The tests that run
//! the real engine are ignored here and run by `./check` with the engine of the pin, as `route_engine.rs` is.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::clause;
use ap::host::{Answer, Failure, Reply, System};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod roles_file;
const NOW: u64 = 1_800_000_000;
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A new empty directory under the temporary directory, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ap-clause-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
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

/// One invented repository: its files at `origin/main` (path, mode, bytes), the commit each path was last
/// changed in, and whether its fetch fails.
#[derive(Clone, Default)]
struct Repo {
    files: Vec<(String, String, Vec<u8>)>,
    commits: BTreeMap<String, String>,
    fetch_fails: bool,
    size_of: BTreeMap<String, u64>,
}
const REVISION: &str = "abababababababababababababababababababab";
fn commit(n: u8) -> String {
    format!("{n:02x}").repeat(20)
}
impl Repo {
    fn file(mut self, path: &str, text: &str, n: u8) -> Self {
        self.files
            .push((path.into(), "100644".into(), text.as_bytes().to_vec()));
        self.commits.insert(path.into(), commit(n));
        self
    }
    fn answer(&self, sub: &[String]) -> Option<Vec<u8>> {
        let words: Vec<&str> = sub.iter().map(String::as_str).collect();
        match words.as_slice() {
            ["fetch", "--quiet", "origin", "main"] => (!self.fetch_fails).then(Vec::new),
            ["rev-parse", "--verify", "--quiet", "origin/main^{commit}"] => {
                Some(format!("{REVISION}\n").into_bytes())
            }
            ["ls-tree", "-r", "-l", "-z", "--full-tree", rev] if *rev == REVISION => {
                let mut out = vec![];
                for (i, (path, mode, bytes)) in self.files.iter().enumerate() {
                    let kind = if mode == "160000" { "commit" } else { "blob" };
                    let size = self
                        .size_of
                        .get(path)
                        .copied()
                        .unwrap_or(bytes.len() as u64);
                    out.extend(format!("{mode} {kind} object{i}  {size:>7}\t{path}").bytes());
                    out.push(0);
                }
                Some(out)
            }
            ["cat-file", "blob", object] => {
                let i: usize = object.strip_prefix("object")?.parse().ok()?;
                Some(self.files[i].2.clone())
            }
            ["log", "-1", "--format=%H", rev, "--", path] if *rev == REVISION => {
                Some(format!("{}\n", self.commits.get(*path)?).into_bytes())
            }
            _ => None,
        }
    }
}

/// The fake host: repositories by clone, the answer of `ont index`, the clauses `ont search` pages through, the
/// units `ont around` reaches, and every call made.
struct Fake {
    policy: PathBuf,
    repos: BTreeMap<String, Repo>,
    index: Value,
    clauses: Vec<Value>,
    units: Vec<Value>,
    qualified: bool,
    store_fails: bool,
    calls: Vec<(String, Vec<String>, Value)>,
}
impl Fake {
    fn new(dir: &Scratch, repos: &[(&str, Repo)]) -> Self {
        let mut policy = serde_json::Map::new();
        let mut by_clone = BTreeMap::new();
        for (name, repo) in repos {
            let clone = format!("/invented/clones/{name}");
            policy.insert((*name).into(), json!({ "clone": clone }));
            by_clone.insert(clone, repo.clone());
        }
        let file = dir.at("policy.json");
        std::fs::write(&file, json!({ "repositories": policy }).to_string()).unwrap();
        Self {
            policy: file,
            repos: by_clone,
            index: index_answer("ok", 2),
            clauses: vec![],
            units: vec![],
            qualified: true,
            store_fails: false,
            calls: vec![],
        }
    }
    fn git_reads(&self) -> Vec<String> {
        let reads = self.calls.iter().filter(|c| c.0 == "git");
        reads.map(|c| c.1[3..].join(" ")).collect()
    }
    fn ont(&self) -> Vec<&str> {
        let calls = self.calls.iter().filter(|c| c.0 == "ont");
        calls.map(|c| c.1[0].as_str()).collect()
    }
}
fn index_answer(status: &str, n: u64) -> Value {
    json!({"status": status, "result": {"sources": [{"source": "clauses", "status": status,
        "appended": {"initial": n, "update": 0, "refresh": 0, "retraction": 1}, "renewed": 0, "unchanged": 0,
        "records": n}]}})
}
fn reply(success: bool, bytes: Vec<u8>) -> Answer {
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
}
fn object(kind: &str, key: &str, fields: &Value) -> Value {
    let fields: serde_json::Map<String, Value> = (fields.as_object().unwrap().iter())
        .map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect();
    json!({"id": {"object_type": kind, "native_key": key}, "head": {"fields": fields}})
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, _: u64) -> Answer {
        self.calls
            .push((program.into(), args.to_vec(), input.clone()));
        match program {
            "git" => {
                assert_eq!(&args[..2], ["--literal-pathspecs", "-C"]);
                let answer = self.repos.get(&args[2]).and_then(|r| r.answer(&args[3..]));
                match answer {
                    Some(b) => reply(true, b),
                    None => reply(false, b"fatal: invented\n".to_vec()),
                }
            }
            "ont" if self.store_fails => Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            }),
            "ont" if args[0] == "index" => {
                assert_eq!(&args[1..3], ["--source", "clauses"]);
                reply(
                    self.index["status"] == "ok",
                    serde_json::to_vec(&self.index).unwrap(),
                )
            }
            "ont" if args[0] == "search" => {
                assert_eq!(input["object_type"], "clause");
                let want: Vec<Value> = input["where"]["all"].as_array().unwrap().clone();
                let found: Vec<Value> = (self.clauses.iter())
                    .filter(|c| {
                        want.iter().all(|w| {
                            let (k, v) = (w["eq"][0].as_str().unwrap(), &w["eq"][1]);
                            &c["head"]["fields"][k]["value"] == v
                        })
                    })
                    .cloned()
                    .collect();
                let v = json!({"status": "ok", "now": NOW, "result": {"basis": {"qualified": self.qualified},
                    "objects": found, "continuation": null}});
                reply(true, serde_json::to_vec(&v).unwrap())
            }
            "ont" if args[0] == "around" => {
                assert_eq!(input["from"]["object_type"], "clause");
                assert_eq!(input["path"], json!(["clause_units"]));
                let limit = input["limit"].as_u64().unwrap() as usize;
                let shown: Vec<Value> = self.units.iter().take(limit).cloned().collect();
                let v = json!({"status": "ok", "now": NOW, "result": {"basis": {"qualified": false,
                    "matched": self.units.len()}, "objects": shown, "continuation": null}});
                reply(true, serde_json::to_vec(&v).unwrap())
            }
            _ => reply(false, vec![]),
        }
    }
    fn policy_file(&self) -> Option<PathBuf> {
        Some(self.policy.clone())
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(NOW)
    }
}

fn os(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}
fn records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn ap(args: &[&str], env: &[(&str, &str)], stdin: &str) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
    c.args(args)
        .env_remove("ONT_FN_NAME")
        .env_remove("ONT_FN_STORE")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    use std::io::Write;
    // A refusal before `ap` reads its input closes the pipe first: a failed write is no failure here.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}
fn error_of(o: &std::process::Output) -> Value {
    serde_json::from_slice(&o.stderr).unwrap()
}
fn run(fake: &mut Fake, dir: &Scratch, extra: &[&str]) -> (Value, bool) {
    let mut args = vec![
        "--directory".to_owned(),
        dir.path("clause"),
        "--store".into(),
        dir.path("store.sqlite"),
    ];
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
    clause::index(fake, &clause::options(&argv).unwrap()).unwrap()
}
fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}
const SPEC: &str = "# Widgets\n\nPurpose: the invented widgets.\n\n## Requirements\n\n- R1: The board lists\n  every widget\n  once.\n- R2: The board is green.\n## Interface\n\n- R3: A last sentence";
fn alpha() -> Repo {
    Repo::default()
        .file("specs/w.md", SPEC, 1)
        .file(
            "src/w.rs",
            "// spec:\x20specs/w.md#R1\nfn w() {}\n// spec:\x20specs/w.md#R2 and spec:\x20specs/w.md#R1\n",
            2,
        )
}

// spec: specs/ap/clause.md#R1
#[test]
fn r1_the_arguments_of_index() {
    let o = clause::options(&os(&[
        "--repository",
        "a/b",
        "--prune",
        "--repository",
        "c/d",
    ]))
    .unwrap();
    assert!(o.prune && o.repositories == ["a/b", "c/d"] && o.directory.is_none());
    for bad in [
        &["--prune", "--prune"][..],
        &["--directory"],
        &["--wat", "x"],
        &["--instructions", "/a", "--store", "/s", "--directory", "/d"],
        &["--store", "/s", "--store", "/t", "--directory", "/d"],
    ] {
        let e = clause::options(&os(bad)).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{bad:?}");
    }
    for (bad, field) in [
        (
            &["--directory", "relative", "--store", "/s"][..],
            "directory",
        ),
        (&["--store", "/s"], "directory"),
        (
            &[
                "--store",
                "/s",
                "--directory",
                "/var/lib/agent-platform/clause/",
            ],
            "directory",
        ),
        (&["--directory", "/d"], "directory"),
        (&["--repository", ""], "repository"),
        (
            &["--repository", "park-kyungchan/education-platform"],
            "repository",
        ),
        (&["--repository", "not a name"], "repository"),
        (
            &["--repository", "a/b", "--repository", "a/b"],
            "repository",
        ),
    ] {
        let e = clause::options(&os(bad)).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_str()),
            ("invalid_input", field),
            "{bad:?}"
        );
    }
    // The default directory without --store is the host's; a repository the policy does not map is refused
    // before any effect.
    assert!(clause::options(&os(&[])).unwrap().directory.is_none());
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    let argv = os(&[
        "--directory",
        &dir.path("clause"),
        "--store",
        "/s",
        "--repository",
        "o/none",
    ]);
    let e = clause::index(&mut fake, &clause::options(&argv).unwrap()).unwrap_err();
    assert_eq!(e.code, "repository_not_mapped");
    assert!(fake.calls.is_empty() && !dir.at("clause").exists());
    // The verb: no argument, another first argument, an unknown option and an education- repository.
    for args in [
        &["clause"][..],
        &["clause", "answer"],
        &["clause", "--prune"],
        &["clause", "index", "--wat"],
        &["clause", "index", "--instructions", "/root"],
    ] {
        let out = ap(args, &[], "");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(error_of(&out)["error"]["code"], "usage", "{args:?}");
        assert!(out.stdout.is_empty());
    }
    let out = ap(
        &["clause", "index", "--repository", "o/education-x"],
        &[("AP_POLICY_FILE", "/invented/policy.json")],
        "",
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        (
            error_of(&out)["error"]["code"].clone(),
            error_of(&out)["error"]["field"].clone()
        ),
        (json!("invalid_input"), json!("repository"))
    );
    let help = ap(&["clause", "--help"], &[], "");
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).starts_with("Usage: ap clause index "));
}

// spec: specs/ap/clause.md#R2
#[test]
fn r2_a_failing_fetch_keeps_the_previous_file() {
    let dir = Scratch::new();
    let mut broken = alpha();
    broken.fetch_fails = true;
    let mut fake = Fake::new(&dir, &[("o/alpha", alpha()), ("o/broken", broken.clone())]);
    let (line, held) = run(
        &mut fake,
        &dir,
        &["--repository", "o/broken", "--repository", "o/alpha"],
    );
    assert!(held);
    assert_eq!(
        line["kept"],
        json!([{"repository": "o/broken", "reason": "fetch_failed", "previous": false}])
    );
    assert!(!dir.at("clause/o__broken.clauses.jsonl").exists());
    assert_eq!(records(&dir.at("clause/o__alpha.clauses.jsonl")).len(), 3);
    // Nothing of the broken clone is read after its fetch: no listing, no object, no log.
    let reads = fake.git_reads();
    assert_eq!(
        reads.iter().filter(|r| r.contains("broken")).count(),
        0,
        "{reads:?}"
    );
    let broken_calls: Vec<&(String, Vec<String>, Value)> = (fake.calls.iter())
        .filter(|c| c.0 == "git" && c.1[2].ends_with("broken"))
        .collect();
    assert_eq!(broken_calls.len(), 1);
    // A later run whose fetch fails again keeps the file of an earlier run as it was.
    std::fs::write(dir.at("clause/o__broken.clauses.jsonl"), "{\"kept\":1}\n").unwrap();
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/broken"]);
    assert_eq!(line["kept"][0]["previous"], true);
    assert_eq!(
        std::fs::read_to_string(dir.at("clause/o__broken.clauses.jsonl")).unwrap(),
        "{\"kept\":1}\n"
    );
}

// spec: specs/ap/clause.md#R3
#[test]
fn r3_the_clauses_of_a_file() {
    let dir = Scratch::new();
    let long = format!("- R1: {}\n", "word ".repeat(1000));
    let twice = "# T\n\n- R1: First.\n- R1: Again, not indexed.\n- R2: Second.\n";
    let repo = alpha()
        .file("specs/long.md", &long, 3)
        .file("specs/twice.md", twice, 4)
        .file("docs/w.md", "- R1: Not a specification.\n", 5)
        .file("specs/notes.txt", "- R1: Not Markdown.\n", 6)
        .file("specs/big.md", "- R1: Big.\n", 7);
    let mut repo = repo;
    repo.size_of.insert("specs/big.md".into(), 1_000_001);
    let mut fake = Fake::new(&dir, &[("o/alpha", repo)]);
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert_eq!(line["duplicates"], 1);
    let all = records(&dir.at("clause/o__alpha.clauses.jsonl"));
    let mut keys: Vec<&str> = all.iter().map(|r| r["key"].as_str().unwrap()).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "o/alpha:specs/long.md#R1",
            "o/alpha:specs/twice.md#R1",
            "o/alpha:specs/twice.md#R2",
            "o/alpha:specs/w.md#R1",
            "o/alpha:specs/w.md#R2",
            "o/alpha:specs/w.md#R3"
        ]
    );
    let by = |k: &str| all.iter().find(|r| r["key"] == k).unwrap().clone();
    // A clause on several lines ends before a heading directly after it; the next clause ends at the heading.
    let r1 = by("o/alpha:specs/w.md#R1");
    let lines = "- R1: The board lists\n  every widget\n  once.";
    assert_eq!(
        r1,
        json!({"type": "clause", "key": "o/alpha:specs/w.md#R1", "observed_at": NOW, "repository": "o/alpha",
            "path": "specs/w.md", "rule": "R1", "commit": commit(1), "line": 7, "digest": sha(lines),
            "text": "- R1: The board lists every widget once.", "tests": ["src/w.rs:1", "src/w.rs:3"],
            "tests_total": 2})
    );
    assert_eq!(
        by("o/alpha:specs/w.md#R2")["text"],
        "- R2: The board is green."
    );
    assert_eq!(by("o/alpha:specs/w.md#R2")["line"], 10);
    // The last clause runs to the file's end, without a final line feed.
    assert_eq!(
        by("o/alpha:specs/w.md#R3")["digest"],
        sha("- R3: A last sentence")
    );
    // The first line of a rule is indexed; a later one is not.
    assert_eq!(by("o/alpha:specs/twice.md#R1")["text"], "- R1: First.");
    assert_eq!(by("o/alpha:specs/twice.md#R2")["line"], 5);
    assert_eq!(by("o/alpha:specs/twice.md#R1")["commit"], commit(4));
    // A text over 4,000 bytes is cut; its digest is of the whole lines.
    let cut = by("o/alpha:specs/long.md#R1");
    assert_eq!(cut["text"].as_str().unwrap().len(), 4000);
    assert_eq!(cut["digest"], sha(long.trim_end_matches('\n')));
    // Neither a Markdown file outside specs/ nor a large file is read.
    let reads = fake.git_reads();
    assert!(
        !reads
            .iter()
            .any(|r| r.contains("log") && (r.contains("docs/") || r.contains("big.md")))
    );
    let objects: Vec<&String> = reads.iter().filter(|r| r.starts_with("cat-file")).collect();
    assert_eq!(objects.len(), 5, "{objects:?}");
    // The rule of a line (the rule of ci/check-spec-trace.py).
    assert_eq!(clause::rule_of("- R12: x").as_deref(), Some("R12"));
    for no in [
        "- R: x", " - R1: x", "- R1 x", "-R1: x", "- r1: x", "- R1a: x",
    ] {
        assert_eq!(clause::rule_of(no), None, "{no}");
    }
}

// spec: specs/ap/clause.md#R4
#[test]
fn r4_the_tests_that_cite_a_clause() {
    let dir = Scratch::new();
    let many: String = (0..101)
        .map(|i| format!("# spec:\x20specs/w.md#R2 case {i}\n"))
        .collect();
    let repo = alpha()
        .file("tests/a.sh", &many, 3)
        .file("docs/notes.md", "spec:\x20specs/w.md#R1 in Markdown does not count\n", 4)
        .file("ci/x.py", "x = 'spec:\x20specs/w.md#R10'  # spec:\x20specs/w.md#R3\nspec:\x20specs/w.md#Rx spec:\x20specs/.md#R1\n", 5);
    let mut fake = Fake::new(&dir, &[("o/alpha", repo)]);
    run(&mut fake, &dir, &["--repository", "o/alpha"]);
    let all = records(&dir.at("clause/o__alpha.clauses.jsonl"));
    let by = |r: &str| all.iter().find(|x| x["rule"] == r).unwrap().clone();
    assert_eq!(by("R1")["tests"], json!(["src/w.rs:1", "src/w.rs:3"]));
    let r2 = by("R2");
    assert_eq!(r2["tests_total"], 102);
    let tests = r2["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 100);
    // By path, then line (as numbers): the last two lines of tests/a.sh are cut.
    assert_eq!(tests[0], "src/w.rs:3");
    assert_eq!(tests[1], "tests/a.sh:1");
    assert_eq!(tests[99], "tests/a.sh:99");
    assert_eq!(by("R3")["tests"], json!(["ci/x.py:1"]));
    // The engine's specifications under engine/specs/ont/ are clauses, and a file under engine/ cites them as
    // specs/ont/...; a citation of specs/ont/ outside engine/ stays as written.
    let repo = alpha()
        .file(
            "engine/specs/ont/cli.md",
            "# CLI\n\n- R4: The command.\n",
            6,
        )
        .file(
            "engine/specs/other.md",
            "- R1: Not of the engine's specifications.\n",
            7,
        )
        .file(
            "engine/crates/ont/tests/cli.rs",
            "// spec:\x20specs/ont/cli.md#R4\n// spec:\x20specs/w.md#R1\n",
            8,
        )
        .file(
            "engine/scripts/test-install-ont.sh",
            "# spec:\x20specs/ont/cli.md#R4\n",
            9,
        )
        .file("tests/b.rs", "// spec:\x20specs/ont/cli.md#R4\n", 10);
    let mut fake = Fake::new(&dir, &[("o/alpha", repo)]);
    run(&mut fake, &dir, &["--repository", "o/alpha"]);
    let all = records(&dir.at("clause/o__alpha.clauses.jsonl"));
    let engine: Vec<&Value> = all
        .iter()
        .filter(|x| x["path"] == "engine/specs/ont/cli.md")
        .collect();
    assert_eq!(engine.len(), 1);
    assert_eq!(engine[0]["key"], "o/alpha:engine/specs/ont/cli.md#R4");
    assert_eq!(
        engine[0]["tests"],
        json!([
            "engine/crates/ont/tests/cli.rs:1",
            "engine/scripts/test-install-ont.sh:1"
        ])
    );
    assert!(!all.iter().any(|x| x["path"] == "engine/specs/other.md"));
    let w1 = all
        .iter()
        .find(|x| x["path"] == "specs/w.md" && x["rule"] == "R1")
        .unwrap();
    assert!(
        w1["tests"]
            .as_array()
            .unwrap()
            .contains(&json!("engine/crates/ont/tests/cli.rs:2"))
    );
    assert_eq!(
        clause::citations(
            "a spec:\x20specs/a/b.md#R7, spec:\x20specs/c.md#R8x spec:\x20specs/d.txt#R1"
        ),
        [
            ("specs/a/b.md".to_owned(), "R7".to_owned()),
            ("specs/c.md".to_owned(), "R8".to_owned())
        ]
    );
}

// spec: specs/ap/clause.md#R5
#[test]
fn r5_files_their_modes_and_prune() {
    let dir = Scratch::new();
    // An absent directory is made with mode 0700, whatever the umask.
    let mut fake = Fake::new(
        &dir,
        &[
            ("o/alpha", alpha()),
            (
                "o/broken",
                Repo {
                    fetch_fails: true,
                    ..alpha()
                },
            ),
        ],
    );
    run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert_eq!(mode(&dir.at("clause")), 0o700);
    assert_eq!(mode(&dir.at("clause/o__alpha.clauses.jsonl")), 0o600);
    // A directory of another mode is set to 0700; a kept file of mode 0644 is set to 0600.
    std::fs::set_permissions(dir.at("clause"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.at("clause/o__broken.clauses.jsonl"), "").unwrap();
    std::fs::set_permissions(
        dir.at("clause/o__broken.clauses.jsonl"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let clause_dir = dir.at("clause");
    std::fs::write(clause_dir.join("o__gone.clauses.jsonl"), "").unwrap();
    std::fs::write(clause_dir.join("notes.txt"), "kept").unwrap();
    std::fs::set_permissions(
        clause_dir.join("notes.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::fs::create_dir(clause_dir.join("sub.clauses.jsonl")).unwrap();
    std::fs::write(dir.at("elsewhere.jsonl"), "").unwrap();
    std::fs::set_permissions(
        dir.at("elsewhere.jsonl"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        dir.at("elsewhere.jsonl"),
        clause_dir.join("link.clauses.jsonl"),
    )
    .unwrap();
    std::fs::write(clause_dir.join(".ap-route.1.x.sections.jsonl.tmp"), "").unwrap();
    let fresh = clause_dir.join(".ap-clause.1.o__x.clauses.jsonl.tmp");
    std::fs::write(&fresh, "").unwrap();
    // Without --prune nothing is removed.
    let (line, _) = run(
        &mut fake,
        &dir,
        &["--repository", "o/alpha", "--repository", "o/broken"],
    );
    assert_eq!(line["removed"], json!([]));
    assert_eq!(line["kept"][0]["previous"], true);
    assert_eq!(mode(&clause_dir), 0o700);
    assert_eq!(mode(&clause_dir.join("o__broken.clauses.jsonl")), 0o600);
    assert_eq!(mode(&clause_dir.join("o__gone.clauses.jsonl")), 0o600);
    // Another file, and the file a link names, keep their modes.
    assert_eq!(mode(&clause_dir.join("notes.txt")), 0o644);
    assert_eq!(mode(&dir.at("elsewhere.jsonl")), 0o644);
    // With --prune: the file of a repository no longer named, and an old temporary file of the command; the
    // unrelated file, the subdirectory, the link and another command's temporary file stay.
    let old = SystemTime::now() - Duration::from_secs(7200);
    std::fs::File::options()
        .write(true)
        .open(&fresh)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/alpha", "--prune"]);
    assert_eq!(
        line["removed"],
        json!([
            ".ap-clause.1.o__x.clauses.jsonl.tmp",
            "o__broken.clauses.jsonl",
            "o__gone.clauses.jsonl"
        ])
    );
    let mut left: Vec<String> = std::fs::read_dir(&clause_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            ".ap-route.1.x.sections.jsonl.tmp",
            "link.clauses.jsonl",
            "notes.txt",
            "o__alpha.clauses.jsonl",
            "sub.clauses.jsonl"
        ]
    );
    assert!(
        std::fs::symlink_metadata(clause_dir.join("link.clauses.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(mode(&dir.at("elsewhere.jsonl")), 0o644);
    // A directory that is a link, or a file, is refused.
    std::os::unix::fs::symlink(&clause_dir, dir.at("linked")).unwrap();
    for at in ["linked", "elsewhere.jsonl"] {
        let argv = os(&["--directory", &dir.path(at), "--store", "/s"]);
        let e = clause::index(&mut fake, &clause::options(&argv).unwrap()).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", "directory"));
    }
}

// spec: specs/ap/clause.md#R6
#[test]
fn r6_the_index_and_the_answer_line() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[("o/alpha", alpha())]);
    let (line, held) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert!(held);
    assert_eq!(
        line,
        json!({"status": "ok", "directory": dir.path("clause"), "sources": {"clauses": {"records": 3,
            "status": "ok", "code": null, "appended": {"initial": 2, "update": 0, "refresh": 0, "retraction": 1},
            "renewed": 0, "retracted": 1}}, "duplicates": 0, "kept": [], "removed": []})
    );
    let index = fake.calls.iter().find(|c| c.0 == "ont").unwrap();
    assert_eq!(
        index.1,
        [
            "index",
            "--source",
            "clauses",
            "--store",
            &dir.path("store.sqlite")
        ]
    );
    // A source that did not hold.
    fake.index = json!({"status": "source_failed", "result": {"sources": [{"source": "clauses",
        "status": "source_failed", "code": "record_type_mismatch", "appended": {"initial": 0, "update": 0,
        "refresh": 0, "retraction": 0}, "renewed": 0}]}});
    let (line, held) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert!(!held);
    assert_eq!(line["status"], "index_failed");
    assert_eq!(line["sources"]["clauses"]["code"], "record_type_mismatch");
    // An engine that does not answer is a failure of the command.
    fake.store_fails = true;
    let argv = os(&[
        "--directory",
        &dir.path("clause"),
        "--store",
        "/s",
        "--repository",
        "o/alpha",
    ]);
    let e = clause::index(&mut fake, &clause::options(&argv).unwrap()).unwrap_err();
    assert_eq!(e.code, "process_failed");
}

fn clause_object(key: &str, tests: &[&str]) -> Value {
    let (repository, rest) = key.split_once(':').unwrap();
    let (path, rule) = rest.rsplit_once('#').unwrap();
    object(
        "clause",
        key,
        &json!({"repository": repository, "path": path, "rule": rule, "commit": commit(1), "line": 7,
            "digest": sha(key), "text": format!("- {rule}: The sentence."), "tests": tests,
            "tests_total": tests.len()}),
    )
}
fn unit_object(n: usize) -> Value {
    let mut fields = json!({"state": "draft", "class": "contract", "lane": "objective"});
    if n.is_multiple_of(2) {
        fields["wave"] = json!(n);
    }
    object("unit", &format!("u{n:02}"), &fields)
}
fn answer(fake: &mut Fake, p: Value) -> Result<Value, Failure> {
    clause::answer(fake, "/scratch/store.sqlite", &p)
}
const KEY: &str = "o/alpha:specs/w.md#R1";

// spec: specs/ap/clause.md#R7
#[test]
fn r7_ap_with_no_argument_is_the_function_clause() {
    // Parameters it refuses: exit 2 with the field; no store is read.
    let out = ap(&[], &[("ONT_FN_NAME", "clause")], r#"{"key":"x"}"#);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["field"], "ONT_FN_STORE");
    let store = [
        ("ONT_FN_NAME", "clause"),
        ("ONT_FN_STORE", "/invented/store.sqlite"),
    ];
    for (input, field) in [
        ("[", "stdin"),
        (r#"{"key":7}"#, "key"),
        (r#"{"other":1}"#, "other"),
    ] {
        let out = ap(&[], &store, input);
        assert_eq!(out.status.code(), Some(2), "{input}");
        assert_eq!(error_of(&out)["error"]["code"], "invalid_input");
        assert_eq!(error_of(&out)["error"]["field"], field);
    }
    // A store it cannot read (an engine that answers no JSON): exit 3, nothing on standard output.
    let dir = Scratch::new();
    std::fs::create_dir(dir.at("bin")).unwrap();
    std::fs::write(dir.at("bin/ont"), "#!/bin/sh\necho not json\nexit 1\n").unwrap();
    std::fs::set_permissions(dir.at("bin/ont"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", dir.path("bin"));
    let out = ap(
        &[],
        &[
            ("ONT_FN_NAME", "clause"),
            ("ONT_FN_STORE", "/invented/store.sqlite"),
            ("PATH", &path),
        ],
        &json!({"key": KEY}).to_string(),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    // An engine that answers no JSON is unreadable; one that cannot start (a script just written may be busy
    // while another test forks) fails as a process: either is a store the function cannot read.
    let code = error_of(&out)["error"]["code"].clone();
    assert!(
        code == "store_answer_unreadable" || code == "process_failed",
        "{code}"
    );
    // Through the port, the one read refused the same way.
    let mut fake = Fake::new(&dir, &[]);
    fake.store_fails = true;
    assert_eq!(
        answer(&mut fake, json!({"key": KEY})).unwrap_err().code,
        "process_failed"
    );
}

// spec: specs/ap/clause.md#R8
#[test]
fn r8_the_parameter() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    for (p, field) in [
        (json!({}), "key"),
        (json!({"key": ""}), "key"),
        (json!({"key": "k".repeat(513)}), "key"),
        (json!({"key": null}), "key"),
        (json!({"key": KEY, "limit": 3}), "limit"),
        (json!([KEY]), "parameters"),
    ] {
        let e = answer(&mut fake, p.clone()).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field), "{p}");
    }
    assert!(fake.calls.is_empty());
    assert!(answer(&mut fake, json!({"key": "k".repeat(512)})).is_ok());
}

// spec: specs/ap/clause.md#R9
#[test]
fn r9_the_sentence_its_tests_and_the_units_that_name_it() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    fake.clauses = vec![
        clause_object(KEY, &["src/w.rs:1"]),
        clause_object("o/alpha:specs/w.md#R2", &[]),
        clause_object("o/beta:specs/w.md#R1", &[]),
    ];
    fake.units = vec![unit_object(1), unit_object(2)];
    let v = answer(&mut fake, json!({"key": KEY})).unwrap();
    assert_eq!(
        v,
        json!({"clause": {"key": KEY, "repository": "o/alpha", "path": "specs/w.md", "rule": "R1",
            "line": 7, "commit": commit(1), "digest": sha(KEY), "text": "- R1: The sentence.",
            "tests": ["src/w.rs:1"], "tests_total": 1},
            "units": [{"unit": "u01", "state": "draft", "class": "contract", "wave": null},
                {"unit": "u02", "state": "draft", "class": "contract", "wave": 2}],
            "units_total": 2})
    );
    // One search by the parts of the key and one around from that clause, with a limit of 12.
    assert_eq!(fake.ont(), ["search", "around"]);
    let search = &fake.calls[0].2;
    assert_eq!(
        search["where"],
        json!({"all": [{"eq": ["repository", "o/alpha"]}, {"eq": ["path", "specs/w.md"]},
            {"eq": ["rule", "R1"]}]})
    );
    let around = &fake.calls[1].2;
    assert_eq!(around["from"]["where"], search["where"]);
    assert_eq!(around["limit"], 12);
    // Thirteen units: twelve shown, all counted.
    fake.units = (1..=13).map(unit_object).collect();
    let v = answer(&mut fake, json!({"key": KEY})).unwrap();
    assert_eq!(v["units"].as_array().unwrap().len(), 12);
    assert_eq!(v["units_total"], 13);
    assert!(v.get("stale").is_none());
    // A key no current clause has, though units name it, and a key of another form: no read of the units.
    fake.calls.clear();
    for key in [
        "o/alpha:specs/w.md#R9",
        "o/alpha specs/w.md#R1",
        "o/alpha:specs/w.md#X1",
        "o/alpha:#R1",
    ] {
        let v = answer(&mut fake, json!({"key": key})).unwrap();
        assert_eq!(
            v,
            json!({"clause": null, "units": [], "units_total": 0}),
            "{key}"
        );
    }
    assert_eq!(fake.ont(), ["search"]);
    // A search that is not qualified: the answer is given, with stale.
    fake.qualified = false;
    let v = answer(&mut fake, json!({"key": KEY})).unwrap();
    assert_eq!(v["stale"], true);
    assert_eq!(v["clause"]["key"], KEY);
}

// spec: specs/ap/clause.md#R11
#[test]
fn r11_the_service_and_its_timer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let service = std::fs::read_to_string(root.join("deploy/ap-clause.service")).unwrap();
    let route = std::fs::read_to_string(root.join("deploy/ap-route.service")).unwrap();
    assert!(service.lines().any(|l| l
        == "ExecStart=/usr/local/bin/ap clause index --prune --repository park-kyungchan/lifecycle-platform"));
    // The lines of the route service, but its description, its comment and its command.
    let rest = |text: &str| -> Vec<String> {
        (text.lines())
            .filter(|l| {
                !l.starts_with("Description=")
                    && !l.starts_with('#')
                    && !l.starts_with("ExecStart=")
            })
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(rest(&service), rest(&route));
    let timer = std::fs::read_to_string(root.join("deploy/ap-clause.timer")).unwrap();
    for line in [
        "OnCalendar=*:5/10",
        "Persistent=true",
        "WantedBy=timers.target",
    ] {
        assert!(timer.lines().any(|l| l == line), "{line}");
    }
}

// spec: specs/ap/clause.md#R12
#[test]
fn r12_what_the_command_and_the_function_write() {
    let dir = Scratch::new();
    std::fs::create_dir(dir.at("clause")).unwrap();
    let mut fake = Fake::new(&dir, &[("o/alpha", alpha())]);
    run(&mut fake, &dir, &["--repository", "o/alpha", "--prune"]);
    // In the clone: a fetch and reads only. In the store: one index of the one source.
    for (program, args, _) in &fake.calls {
        if program == "git" {
            assert!(
                ["fetch", "rev-parse", "ls-tree", "cat-file", "log"].contains(&args[3].as_str()),
                "{args:?}"
            );
        }
    }
    assert_eq!(fake.ont(), ["index"]);
    // Files only in its directory.
    let mut names: Vec<String> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["clause", "policy.json"]);
    // The function reads only: a search and an around, no action.
    fake.calls.clear();
    fake.clauses = vec![clause_object(KEY, &[])];
    answer(&mut fake, json!({"key": KEY})).unwrap();
    assert_eq!(fake.ont(), ["search", "around"]);
}

/// The real engine: an invented repository with a bare origin, a scratch store whose schema is the repository's
/// with the glob of `clauses` rewritten to a scratch directory and the program of `clause` rewritten to a copy of
/// the built `ap`, and a scratch policy file. Nothing of the host's directory, store or clones is read.
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
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@invalid",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "protocol.file.allow=always"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
impl Bed {
    fn new() -> Self {
        let ont = PathBuf::from(std::env::var("ONT_BIN").expect("ONT_BIN names the engine"));
        let base = std::env::temp_dir().join(format!(
            "ap-clause-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // The engine refuses a store reached through a symbolic link: the root is a real path.
        let root = std::fs::canonicalize(&base).unwrap();
        for d in ["origins", "work", "clones", "bin"] {
            std::fs::create_dir(root.join(d)).unwrap();
        }
        let ap = root.join("bin/ap");
        std::fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
        std::fs::set_permissions(&ap, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut schema: Value =
            serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
        let glob = schema["sources"]["clauses"]["glob"].as_str().unwrap();
        let rest = glob
            .strip_prefix("/var/lib/agent-platform/clause/")
            .unwrap()
            .to_owned();
        schema["sources"]["clauses"]["glob"] = json!(format!("{}/clause/{rest}", root.display()));
        assert_eq!(
            schema["functions"]["clause"]["program"],
            "/usr/local/bin/ap"
        );
        schema["functions"]["clause"]["program"] = json!(ap.to_str().unwrap());
        std::fs::write(root.join("schema.json"), schema.to_string()).unwrap();
        let path = format!(
            "{}:{}",
            ont.parent().unwrap().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let bed = Self { root, ont, path };
        let (code, out) = bed.ont(
            &["schema", "install", bed.at("schema.json").to_str().unwrap()],
            "",
        );
        assert_eq!(code, 0, "{out}");
        roles_file::put(&bed.root, &schema);
        bed
    }
    fn at(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn store(&self) -> String {
        self.at("store.sqlite").to_str().unwrap().to_owned()
    }
    /// A repository `fixture/<name>` with one commit: a working tree, a bare origin and a clone.
    fn repository(&self, name: &str, files: &[(&str, &str)]) {
        let work = self.at(&format!("work/{name}"));
        let origin = self.at(&format!("origins/{name}.git"));
        git(
            &self.root,
            &["init", "--quiet", "--bare", origin.to_str().unwrap()],
        );
        git(&self.root, &["init", "--quiet", work.to_str().unwrap()]);
        git(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        for (path, text) in files {
            let p = work.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "--quiet", "-m", "fixture"]);
        git(&work, &["push", "--quiet", "origin", "HEAD:main"]);
        let clone = self.at(&format!("clones/{name}"));
        git(
            &self.root,
            &[
                "clone",
                "--quiet",
                origin.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        let policy = json!({"repositories": {format!("fixture/{name}"): {"clone": clone.to_str().unwrap()}}});
        std::fs::write(self.at("policy.json"), policy.to_string()).unwrap();
    }
    fn ont(&self, args: &[&str], stdin: &str) -> (i32, Value) {
        let mut child = Command::new(&self.ont)
            .arg("--store")
            .arg(self.store())
            .args(args)
            .env_remove("ONT_ROLE")
            .env_remove("ONT_AGENT")
            .envs(roles_file::fn_role(args))
            .env("PATH", &self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
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
    fn apply(&self, action: &str, parameters: Value) -> Value {
        let body = json!({"action_type": action, "request_id": format!("bed-{action}"), "agent": "bed",
            "session": "bed-1", "role": "lead", "parameters": parameters});
        let (code, v) = self.ont(&["action", "apply"], &body.to_string());
        assert_eq!(code, 0, "{v}");
        v["result"].clone()
    }
}
fn oid(kind: &str, key: &str) -> Value {
    json!({"object_type": kind, "namespace": {"instance": "vps://palantirkc", "account": "root"},
        "native_key": key})
}

// spec: specs/ap/clause.md#R6
// spec: specs/ap/clause.md#R9
#[test]
#[ignore = "runs the real engine: ./check runs it with --ignored"]
fn an_invented_repository_indexed_and_answered_by_the_engine() {
    let bed = Bed::new();
    bed.repository(
        "alpha",
        &[
            (
                "specs/w.md",
                "# Widgets\n\n- R1: The board lists\n  every widget.\n- R2: The board is green.\n",
            ),
            ("src/w.rs", "// spec:\x20specs/w.md#R1\nfn w() {}\n"),
            ("README.md", "- R1: Not a specification.\n"),
        ],
    );
    let out = Command::new(bed.at("bin/ap"))
        .args([
            "clause",
            "index",
            "--directory",
            bed.at("clause").to_str().unwrap(),
        ])
        .args(["--store", &bed.store(), "--repository", "fixture/alpha"])
        .env("PATH", &bed.path)
        .env("AP_POLICY_FILE", bed.at("policy.json"))
        .env_remove("ONT_FN_NAME")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(line["status"], "ok", "{line}");
    assert_eq!(
        line["sources"]["clauses"]["appended"]["initial"], 2,
        "{line}"
    );
    let key = "fixture/alpha:specs/w.md#R1";
    // A unit that names the clause and one that does not exist.
    let request = bed.apply(
        "record_request",
        json!({"words": "Invented words.", "words_verified": true, "reading": "The bed asks.", "kind": "order",
            "topic": "bed", "said_at": "2026-11-01T00:00:00Z"}),
    );
    let origin = request["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"]["object_type"] == "request")
        .unwrap()["id"]
        .clone();
    bed.apply(
        "add_unit",
        json!({"unit": oid("unit", "bed-unit"), "lane": "small", "class": "plain", "origin": origin,
            "objective": "The invented board.", "repository": "fixture/alpha", "write_set": ["src/**"],
            "wave": 3, "clauses": [oid("clause", key), oid("clause", "fixture/alpha:specs/w.md#R9")]}),
    );
    let (code, v) = bed.ont(
        &["fn", "clause"],
        &json!({"parameters": {"key": key}}).to_string(),
    );
    assert_eq!(code, 0, "{v}");
    let answer = &v["answer"];
    assert!(answer.get("stale").is_none(), "{answer}");
    assert_eq!(
        answer["clause"]["text"],
        "- R1: The board lists every widget."
    );
    assert_eq!(answer["clause"]["tests"], json!(["src/w.rs:1"]));
    assert_eq!(answer["clause"]["line"], 3);
    assert_eq!(
        answer["units"],
        json!([{"unit": "bed-unit", "state": "draft", "class": "plain", "wave": 3}])
    );
    assert_eq!(answer["units_total"], 1);
    let (code, v) = bed.ont(
        &["fn", "clause"],
        &json!({"parameters": {"key": "fixture/alpha:specs/w.md#R9"}}).to_string(),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(
        v["answer"],
        json!({"clause": null, "units": [], "units_total": 0})
    );
}
