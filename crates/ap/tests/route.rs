//! `ap route index` and the function `route` (`specs/ap/route.md`) against a fake host: the fake answers `git`
//! from invented repositories and `ont` (`index`, `search`) as the engine answers. The directory each test
//! writes is a temporary one; no test reaches a clone, the policy file or the store of this host. The tests
//! that run the real engine are `route_engine.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use ap::host::{Answer, Reply, System};
use ap::route;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const NOW: u64 = 1_800_000_000;
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A new empty directory under the temporary directory, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ap-route-test-{}-{}",
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

/// One invented repository: its files at `origin/main` (path, mode, bytes), the commit each path was
/// last changed in, and whether its fetch fails.
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

/// The fake host: repositories by clone, the answer of `ont index`, the objects `ont search` pages
/// through, and every call made.
struct Fake {
    policy: PathBuf,
    repos: BTreeMap<String, Repo>,
    index: Value,
    objects: BTreeMap<String, Vec<Value>>,
    qualified: bool,
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
            index: index_answer("ok", 2, 1),
            objects: BTreeMap::new(),
            qualified: true,
            calls: vec![],
        }
    }
    fn git_calls(&self) -> usize {
        self.calls.iter().filter(|c| c.0 == "git").count()
    }
}
fn index_answer(status: &str, sections: u64, components: u64) -> Value {
    let one = |name: &str, n: u64| {
        json!({"source": name, "status": status, "appended": {"initial": n, "update": 0, "refresh": 0, "retraction": 1},
            "renewed": 0, "unchanged": 0, "records": n})
    };
    json!({"status": status, "result": {"sources": [one("sections", sections), one("components", components)]}})
}
fn reply(success: bool, bytes: Vec<u8>) -> Answer {
    Ok(Reply {
        success,
        stdout: bytes.clone(),
        bytes,
    })
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
            "ont" if args[0] == "index" => reply(
                self.index["status"] == "ok",
                serde_json::to_vec(&self.index).unwrap(),
            ),
            "ont" if args[0] == "search" => {
                let all = self
                    .objects
                    .get(input["object_type"].as_str().unwrap())
                    .cloned()
                    .unwrap_or_default();
                let from: usize = input["continuation"]
                    .as_str()
                    .map_or(0, |c| c.parse().unwrap());
                let limit = input["limit"].as_u64().unwrap() as usize;
                let page: Vec<Value> = all.iter().skip(from).take(limit).cloned().collect();
                let next = (from + limit < all.len()).then(|| (from + limit).to_string());
                let v = json!({"status": "ok", "now": NOW, "result": {"basis": {"qualified": self.qualified},
                    "objects": page, "continuation": next}});
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
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    use std::io::Write;
    // A refusal before `ap` reads its input closes the pipe first: a failed write is no failure here.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}
fn error_of(o: &std::process::Output) -> Value {
    serde_json::from_slice(&o.stderr).unwrap()
}

// spec: specs/ap/route.md#R1
#[test]
fn r1_the_arguments_of_index() {
    let o = route::options(&os(&[
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
        &["--instructions", "/a", "--instructions", "/b"],
        &["--store", "/s", "--store", "/t", "--directory", "/d"],
    ] {
        let e = route::options(&os(bad)).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("usage", "args"), "{bad:?}");
    }
    let e = route::options(&os(&["--instructions", "relative"])).unwrap_err();
    assert_eq!(
        (e.code, e.field.as_str()),
        ("invalid_input", "instructions")
    );
    let e = route::options(&os(&["--directory", "relative", "--store", "/s"])).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "directory"));
    let e = route::options(&os(&["--repository", ""])).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "repository"));
    let out = ap(&["route", "index", "--wat"], &[], "");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["code"], "usage");
}

// spec: specs/ap/route.md#R2
#[test]
fn r2_a_scratch_store_needs_a_scratch_directory_and_back() {
    for args in [
        &["--store", "/tmp/s.sqlite"][..],
        &[
            "--store",
            "/tmp/s.sqlite",
            "--directory",
            "/var/lib/agent-platform/route",
        ],
        &[
            "--store",
            "/tmp/s.sqlite",
            "--directory",
            "/var/lib/agent-platform/route/",
        ],
        &["--directory", "/tmp/route"],
    ] {
        let e = route::options(&os(args)).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_str()),
            ("invalid_input", "directory"),
            "{args:?}"
        );
    }
    assert!(
        route::options(&os(&[
            "--directory",
            "/tmp/route",
            "--store",
            "/tmp/s.sqlite"
        ]))
        .is_ok()
    );
    assert!(route::options(&os(&["--directory", "/var/lib/agent-platform/route"])).is_ok());
    let out = ap(
        &["route", "index", "--store", "/tmp/never-made.sqlite"],
        &[],
        "",
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["field"], "directory");
}

// spec: specs/ap/route.md#R3
#[test]
fn r3_an_education_repository_is_refused_before_any_fetch() {
    let dir = Scratch::new();
    for name in ["park-kyungchan/education-platform", "someone/education-x"] {
        let e = route::options(&os(&["--repository", "a/b", "--repository", name])).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", "repository"));
        // The installed command refuses it with nothing done: no directory is made, no Git runs.
        let target = dir.path("never");
        let out = ap(
            &[
                "route",
                "index",
                "--directory",
                &target,
                "--store",
                "/tmp/x.sqlite",
                "--repository",
                name,
            ],
            &[],
            "",
        );
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(error_of(&out)["error"]["field"], "repository");
        assert!(!Path::new(&target).exists());
    }
    for bad in ["noslash", "a/b/c", "a/..", "a_b/c", "/b"] {
        let e = route::options(&os(&["--repository", bad])).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_str()),
            ("invalid_input", "repository"),
            "{bad}"
        );
    }
    let e = route::options(&os(&["--repository", "a/b", "--repository", "a/b"])).unwrap_err();
    assert_eq!(e.code, "invalid_input");
    // A repository the policy does not map is refused before any effect.
    let mut fake = Fake::new(&dir, &[("a/b", Repo::default())]);
    let o = route::options(&os(&[
        "--repository",
        "a/c",
        "--directory",
        &dir.path("route"),
        "--store",
        "/s",
    ]))
    .unwrap();
    let e = route::index(&mut fake, &o).unwrap_err();
    assert_eq!(e.code, "repository_not_mapped");
    assert_eq!(fake.calls.len(), 0);
    assert!(!dir.at("route").exists());
}

fn alpha() -> Repo {
    Repo::default()
        .file("README.md", "# Alpha\n\nThe invented widget.\n\n## Use\n\nRun `widget go`.\n", 1)
        .file("docs/notes.md", "No heading here, only words.\n", 2)
        .file("src/main.rs", "fn main() {}\n", 3)
        .file(
            "map.json",
            r#"{"components":[{"name":"widget","purpose":"The invented widget.","paths":["src/"],"entry":["src/main.rs:1-1"],"spec":[],"tests":["tests/w.rs"]}]}"#,
            4,
        )
}

fn run(fake: &mut Fake, dir: &Scratch, extra: &[&str]) -> (Value, bool) {
    let mut args = vec![
        "--directory".to_owned(),
        dir.path("route"),
        "--store".into(),
        dir.path("store.sqlite"),
    ];
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
    route::index(fake, &route::options(&argv).unwrap()).unwrap()
}

// spec: specs/ap/route.md#R4
// spec: specs/ap/route.md#R5
#[test]
fn r4_r5_a_repository_at_origin_main_and_the_commit_of_each_file() {
    let dir = Scratch::new();
    let mut repo = alpha();
    repo.files
        .push(("link.md".into(), "120000".into(), b"README.md".to_vec()));
    repo.files.push(("sub.md".into(), "160000".into(), vec![]));
    repo.files
        .push(("tool.md".into(), "100755".into(), b"# Tool\n".to_vec()));
    repo.commits.insert("tool.md".into(), commit(5));
    let mut fake = Fake::new(&dir, &[("o/alpha", repo)]);
    let (line, held) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert!(held, "{line}");
    let git: Vec<Vec<String>> = fake
        .calls
        .iter()
        .filter(|c| c.0 == "git")
        .map(|c| c.1[3..].to_vec())
        .collect();
    assert_eq!(git[0], ["fetch", "--quiet", "origin", "main"]);
    assert!(git.iter().all(|g| {
        !g.iter()
            .any(|w| w == "checkout" || w == "reset" || w == "merge")
    }));
    assert!(git.contains(&vec![
        "log".into(),
        "-1".into(),
        "--format=%H".into(),
        REVISION.into(),
        "--".into(),
        "README.md".into()
    ]));
    let sections = records(&dir.at("route/o__alpha.sections.jsonl"));
    let paths: Vec<&str> = sections
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["README.md", "README.md", "docs/notes.md", "tool.md"]
    );
    let first = &sections[0];
    assert_eq!(first["repository"], "o/alpha");
    assert_eq!(first["commit"], commit(1));
    assert_eq!(
        first["digest"],
        ap_digest(b"# Alpha\n\nThe invented widget.\n\n## Use\n\nRun `widget go`.\n")
    );
    assert_eq!(sections[2]["commit"], commit(2));
    assert_eq!(sections[3]["commit"], commit(5));
    // A fetch that fails keeps the previous files as they were; the other repository goes on.
    let before = std::fs::read(dir.at("route/o__alpha.sections.jsonl")).unwrap();
    let mut failing = alpha();
    failing.fetch_fails = true;
    let mut fake = Fake::new(
        &dir,
        &[
            ("o/alpha", failing),
            ("o/beta", Repo::default().file("B.md", "# B\n", 9)),
        ],
    );
    let (line, _) = run(
        &mut fake,
        &dir,
        &["--repository", "o/alpha", "--repository", "o/beta"],
    );
    assert_eq!(
        line["kept"],
        json!([{"repository": "o/alpha", "reason": "fetch_failed", "previous": true}])
    );
    assert_eq!(
        std::fs::read(dir.at("route/o__alpha.sections.jsonl")).unwrap(),
        before
    );
    assert_eq!(records(&dir.at("route/o__beta.sections.jsonl")).len(), 1);
    // A repository whose fetch fails and that had no file is kept with none.
    let failing = Repo {
        fetch_fails: true,
        ..Repo::default()
    };
    let mut fake = Fake::new(&dir, &[("o/gamma", failing)]);
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/gamma"]);
    assert_eq!(line["kept"][0]["previous"], false);
    assert!(!dir.at("route/o__gamma.sections.jsonl").exists());
}

fn ap_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let d = sha256(bytes);
    d.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
/// SHA-256 by the standard's steps, so the test does not take the digest from the code under test.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend(((data.len() as u64) * 8).to_be_bytes());
    for chunk in m.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

// spec: specs/ap/route.md#R6
#[test]
fn r6_the_instructions_are_read_on_disk() {
    let dir = Scratch::new();
    let agents = dir.at("agents");
    std::fs::create_dir_all(agents.join("skills/lead")).unwrap();
    std::fs::create_dir_all(agents.join(".hidden")).unwrap();
    std::fs::write(agents.join("AGENTS.md"), "# Shared\n\nWords.\n").unwrap();
    std::fs::write(agents.join("skills/lead/SKILL.md"), "# Lead\n").unwrap();
    std::fs::write(agents.join("skills/lead/notes.txt"), "# Not markdown\n").unwrap();
    std::fs::write(agents.join(".hidden/x.md"), "# Hidden\n").unwrap();
    std::os::unix::fs::symlink(agents.join("AGENTS.md"), agents.join("link.md")).unwrap();
    let mut fake = Fake::new(&dir, &[]);
    let root = agents.to_str().unwrap().to_owned();
    let (line, held) = run(&mut fake, &dir, &["--instructions", &root]);
    assert!(held && line["kept"] == json!([]), "{line}");
    let all = records(&dir.at("route/instructions.sections.jsonl"));
    let paths: Vec<String> = all
        .iter()
        .map(|r| r["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        paths,
        [
            format!("{root}/AGENTS.md"),
            format!("{root}/skills/lead/SKILL.md")
        ]
    );
    for r in &all {
        assert!(
            r.get("repository").is_none() && r.get("commit").is_none(),
            "{r}"
        );
        assert_eq!(r["type"], "section");
    }
    assert_eq!(all[0]["digest"], ap_digest(b"# Shared\n\nWords.\n"));
    // A directory that cannot be read keeps the previous file.
    let (line, _) = run(&mut fake, &dir, &["--instructions", &dir.path("absent")]);
    assert_eq!(line["kept"][0]["reason"], "instructions_unreadable");
    assert_eq!(line["kept"][0]["previous"], true);
    assert_eq!(
        records(&dir.at("route/instructions.sections.jsonl")).len(),
        2
    );
}

// spec: specs/ap/route.md#R7
#[test]
fn r7_the_components_of_map_json() {
    let dir = Scratch::new();
    let mut fake = Fake::new(
        &dir,
        &[
            ("o/alpha", alpha()),
            ("o/bare", Repo::default().file("A.md", "# A\n", 6)),
        ],
    );
    run(
        &mut fake,
        &dir,
        &["--repository", "o/alpha", "--repository", "o/bare"],
    );
    let c = records(&dir.at("route/o__alpha.components.jsonl"));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["key"], route::key("o/alpha", "map.json", "widget", 1));
    let shown: BTreeMap<_, _> = c[0]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| *k != "key")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        Value::Object(shown.into_iter().collect()),
        json!({"type": "component", "observed_at": NOW, "repository": "o/alpha", "name": "widget",
            "purpose": "The invented widget.", "paths": ["src/"], "entry": ["src/main.rs:1-1"], "spec": [],
            "tests": ["tests/w.rs"], "commit": commit(4)})
    );
    // No map.json: zero components, an empty file.
    assert_eq!(
        std::fs::read(dir.at("route/o__bare.components.jsonl")).unwrap(),
        b""
    );
    // A map.json that cannot be read keeps the repository's files.
    for map in [
        r#"not json"#,
        r#"{"components":{}}"#,
        r#"{"components":[{"purpose":"x"}]}"#,
        r#"{"components":[{"name":"a"},{"name":"a"}]}"#,
        r#"{"components":[{"name":"a","paths":"src/"}]}"#,
    ] {
        let repo = Repo::default()
            .file("A.md", "# A\n", 6)
            .file("map.json", map, 7);
        let mut fake = Fake::new(&dir, &[("o/bare", repo)]);
        let (line, _) = run(&mut fake, &dir, &["--repository", "o/bare"]);
        assert_eq!(line["kept"][0]["reason"], "map_unreadable", "{map}");
    }
}

// spec: specs/ap/route.md#R8
#[test]
fn r8_files_written_whole_and_pruned() {
    let dir = Scratch::new();
    let route_dir = dir.at("route");
    // An absent directory is made with mode 0700, whatever the umask.
    let mut fake = Fake::new(&dir, &[("o/alpha", alpha())]);
    run(&mut fake, &dir, &["--repository", "o/alpha"]);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode(&route_dir), 0o700);
    std::fs::remove_dir_all(&route_dir).unwrap();
    // A directory of mode 0755 is set to 0700, and a kept file of mode 0644 to 0600.
    std::fs::create_dir(&route_dir).unwrap();
    std::fs::set_permissions(&route_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(route_dir.join("o__gone.sections.jsonl"), "").unwrap();
    std::fs::write(route_dir.join("o__gone.components.jsonl"), "").unwrap();
    std::fs::write(route_dir.join("instructions.sections.jsonl"), "").unwrap();
    std::fs::set_permissions(
        route_dir.join("o__gone.sections.jsonl"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::fs::write(route_dir.join("notes.txt"), "kept").unwrap();
    std::fs::set_permissions(
        route_dir.join("notes.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::fs::create_dir(route_dir.join("sub.sections.jsonl")).unwrap();
    std::fs::write(dir.at("elsewhere.jsonl"), "").unwrap();
    std::os::unix::fs::symlink(
        dir.at("elsewhere.jsonl"),
        route_dir.join("link.sections.jsonl"),
    )
    .unwrap();
    let fresh_tmp = route_dir.join(".ap-route.1.x.sections.jsonl.tmp");
    std::fs::write(&fresh_tmp, "").unwrap();
    // Without --prune nothing is removed.
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert_eq!(line["removed"], json!([]));
    assert!(route_dir.join("o__gone.sections.jsonl").exists());
    assert_eq!(mode(&route_dir.join("o__alpha.sections.jsonl")), 0o600);
    assert_eq!(mode(&route_dir), 0o700);
    assert_eq!(mode(&route_dir.join("o__gone.sections.jsonl")), 0o600);
    assert_eq!(mode(&route_dir.join("notes.txt")), 0o644);
    // With --prune: the files of a repository no longer named, and the instructions' file when no
    // --instructions is given; the other entries stay.
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/alpha", "--prune"]);
    assert_eq!(
        line["removed"],
        json!([
            "instructions.sections.jsonl",
            "o__gone.components.jsonl",
            "o__gone.sections.jsonl"
        ])
    );
    let mut left: Vec<String> = std::fs::read_dir(&route_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            ".ap-route.1.x.sections.jsonl.tmp",
            "link.sections.jsonl",
            "notes.txt",
            "o__alpha.components.jsonl",
            "o__alpha.sections.jsonl",
            "sub.sections.jsonl"
        ]
    );
    assert!(
        std::fs::symlink_metadata(route_dir.join("link.sections.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(dir.at("elsewhere.jsonl").exists());
    // An old temporary file of ap is removed by --prune.
    let old = SystemTime::now() - Duration::from_secs(7200);
    std::fs::File::options()
        .write(true)
        .open(&fresh_tmp)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let (line, _) = run(&mut fake, &dir, &["--repository", "o/alpha", "--prune"]);
    assert_eq!(line["removed"], json!([".ap-route.1.x.sections.jsonl.tmp"]));
    // A directory that is a link is refused.
    std::os::unix::fs::symlink(&route_dir, dir.at("linked")).unwrap();
    let argv = os(&["--directory", &dir.path("linked"), "--store", "/s"]);
    let e = route::index(&mut fake, &route::options(&argv).unwrap()).unwrap_err();
    assert_eq!((e.code, e.field.as_str()), ("invalid_input", "directory"));
}

fn heads(text: &str) -> Vec<(String, usize, usize, usize)> {
    route::sections(&route::lines(text), "FILE.md")
        .into_iter()
        .map(|s| (s.heading, s.first_line, s.last_line, s.ordinal))
        .collect()
}
fn h(heading: &str, first: usize, last: usize, ordinal: usize) -> (String, usize, usize, usize) {
    (heading.into(), first, last, ordinal)
}

// spec: specs/ap/route.md#R9
#[test]
fn r9_headings_levels_and_fences() {
    let text = "# Top\nintro\n## A\na\n### A1 ###\na1\n## B\n```sh\n# not a heading\n```\n~~~~\n# no\n~~~~~\n    # indented code\n#hashtag\n## C #\n";
    assert_eq!(
        heads(text),
        [
            h("Top", 1, 16, 1),
            h("Top > A", 3, 6, 1),
            h("Top > A > A1", 5, 6, 1),
            h("Top > B", 7, 15, 1),
            h("Top > C", 16, 16, 1)
        ]
    );
    // A heading as the last line, without a final newline.
    assert_eq!(
        heads("# One\ntext\n## Last"),
        [h("One", 1, 3, 1), h("One > Last", 3, 3, 1)]
    );
    // A deeper heading first, then a higher one: the deeper one ends before it.
    assert_eq!(
        heads("### Deep\nx\n# High\n"),
        [h("Deep", 1, 2, 1), h("High", 3, 3, 1)]
    );
    // A backtick fence whose info string holds a backtick is no fence; a fence closes only with as many.
    assert_eq!(
        heads("``` a`b\n# Yes\n"),
        [h("FILE.md", 1, 1, 1), h("Yes", 2, 2, 1)]
    );
    assert_eq!(
        heads("````\n```\n# no\n````\n# Yes\n"),
        [h("FILE.md", 1, 4, 1), h("Yes", 5, 5, 1)]
    );
    // An unclosed fence runs to the end.
    assert_eq!(heads("# A\n```\n# no\n"), [h("A", 1, 3, 1)]);
    // Empty heading text and a closing sequence only.
    assert_eq!(heads("#\n# ###\n"), [h("", 1, 1, 1), h("", 2, 2, 2)]);
}

// spec: specs/ap/route.md#R10
#[test]
fn r10_the_text_above_the_first_heading_and_a_file_without_one() {
    assert_eq!(
        heads("preface\n\n# A\n"),
        [h("FILE.md", 1, 2, 1), h("A", 3, 3, 1)]
    );
    assert_eq!(heads("\n\n# A\n"), [h("A", 3, 3, 1)]);
    assert_eq!(heads("only words\nand more\n"), [h("FILE.md", 1, 2, 1)]);
    assert_eq!(heads(""), []);
    assert_eq!(heads("\n  \n"), []);
}

// spec: specs/ap/route.md#R11
#[test]
fn r11_the_bounds() {
    let many = "é".repeat(3000); // 6,000 bytes
    let text = route::text_of(&[&many]);
    assert!(
        text.len() <= route::TEXT_BYTES
            && text.len() >= route::TEXT_BYTES - 1
            && text.chars().all(|c| c == 'é')
    );
    assert_eq!(route::text_of(&["  a \t b\u{7}c ", "", " d  "]), "a b c d");
    let long = format!("# {}\n", "ü".repeat(800));
    let [s] = route::sections(&route::lines(&long), "F.md")
        .try_into()
        .unwrap();
    assert!(s.heading.len() <= route::HEADING_BYTES && s.heading.len() >= route::HEADING_BYTES - 1);
    assert_eq!(route::cut("abc", 2), "ab");
    // A file over 1,000,000 bytes, and a path over 1,024 bytes, are not indexed.
    let dir = Scratch::new();
    let long_path = format!("{}.md", "p".repeat(1100));
    let mut repo = Repo::default()
        .file("big.md", "# Big\n", 1)
        .file(&long_path, "# Long\n", 2)
        .file("ok.md", "# Ok\n", 3);
    repo.size_of.insert("big.md".into(), 1_000_001);
    let mut fake = Fake::new(&dir, &[("o/alpha", repo)]);
    run(&mut fake, &dir, &["--repository", "o/alpha"]);
    let all = records(&dir.at("route/o__alpha.sections.jsonl"));
    assert_eq!(
        all.iter()
            .map(|r| r["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ok.md"]
    );
    // The members of a section's line.
    let mut keys: Vec<&String> = all[0].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "commit",
            "digest",
            "first_line",
            "heading",
            "key",
            "last_line",
            "observed_at",
            "path",
            "repository",
            "text",
            "type"
        ]
    );
    assert_eq!(all[0]["text"], "# Ok");
}

// spec: specs/ap/route.md#R12
#[test]
fn r12_keys_stay_when_lines_move() {
    let text = "# A\nx\n## B\ny\n## B\nz\n";
    let keyed = |t: &str| -> Vec<(String, usize)> {
        route::sections(&route::lines(t), "F.md")
            .into_iter()
            .map(|s| {
                (
                    route::key("o/r", "F.md", &s.heading, s.ordinal),
                    s.first_line,
                )
            })
            .collect()
    };
    let before = keyed(text);
    let after = keyed(&format!("new line\n\nanother\n{text}"));
    assert_eq!(before.len(), 3);
    assert_eq!(after.len(), 4);
    for (k, line) in &before {
        assert!(
            after.iter().any(|(k2, l2)| k2 == k && *l2 == line + 3),
            "{k}"
        );
    }
    // Two equal headings: ordinals 1 and 2, two keys.
    assert_ne!(before[1].0, before[2].0);
    assert_eq!(before[1].0, route::key("o/r", "F.md", "A > B", 1));
    assert_eq!(before[2].0, route::key("o/r", "F.md", "A > B", 2));
    // The key is the hex SHA-256 of the JSON array, 64 bytes.
    assert_eq!(
        route::key("o/r", "F.md", "A", 1),
        ap_digest(br#"["o/r","F.md","A",1]"#)
    );
    assert_eq!(route::key("instructions", "/x/A.md", "A", 1).len(), 64);
}

// spec: specs/ap/route.md#R13
#[test]
fn r13_the_index_and_the_answer_line() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[("o/alpha", alpha())]);
    let (line, held) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert!(held);
    let ont: Vec<&(String, Vec<String>, Value)> =
        fake.calls.iter().filter(|c| c.0 == "ont").collect();
    assert_eq!(ont.len(), 1);
    assert_eq!(
        ont[0].1,
        [
            "index",
            "--source",
            "sections",
            "--source",
            "components",
            "--store",
            &dir.path("store.sqlite")
        ]
    );
    assert_eq!(line["status"], "ok");
    assert_eq!(line["directory"], dir.path("route"));
    assert_eq!(line["sources"]["sections"]["records"], 3);
    assert_eq!(line["sources"]["components"]["records"], 1);
    assert_eq!(line["sources"]["sections"]["appended"]["initial"], 2);
    assert_eq!(line["sources"]["sections"]["retracted"], 1);
    assert_eq!(line["sources"]["sections"]["renewed"], 0);
    // A source that does not hold: the line says so and the run is not held.
    fake.index = index_answer("source_failed", 0, 0);
    let (line, held) = run(&mut fake, &dir, &["--repository", "o/alpha"]);
    assert!(!held && line["status"] == "index_failed");
    // An engine that refuses: a failure of the run.
    fake.index = json!({"status": "refused", "code": "index_busy"});
    let argv = os(&["--directory", &dir.path("route"), "--store", "/s"]);
    let e = route::index(&mut fake, &route::options(&argv).unwrap()).unwrap_err();
    assert_eq!(e.code, "store_refused");
    assert!(fake.git_calls() > 0);
}

// spec: specs/ap/route.md#R14
#[test]
fn r14_route_without_index_is_a_usage_refusal() {
    for args in [&["route"][..], &["route", "answer"], &["route", "--prune"]] {
        let out = ap(args, &[], "");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(
            error_of(&out),
            json!({"error": {"code": "usage", "field": "args", "detail": null}})
        );
        assert!(out.stdout.is_empty());
    }
    let help = ap(&["route", "--help"], &[], "");
    assert_eq!(help.status.code(), Some(0));
}

// spec: specs/ap/route.md#R15
// spec: specs/ap/verbs.md#R4
#[test]
fn r15_ap_with_no_argument_answers_only_the_function_it_knows() {
    for env in [&[][..], &[("ONT_FN_NAME", "other")], &[("ONT_FN_NAME", "")]] {
        let out = ap(&[], env, "{}");
        assert_eq!(out.status.code(), Some(2), "{env:?}");
        assert_eq!(
            out.stderr,
            b"{\"error\":{\"code\":\"usage\",\"field\":\"args\"}}\n"
        );
        assert!(out.stdout.is_empty());
    }
    // The function without a store, and with parameters it refuses: exit 2 with the field.
    let out = ap(&[], &[("ONT_FN_NAME", "route")], r#"{"words":"x"}"#);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["field"], "ONT_FN_STORE");
    let out = ap(
        &[],
        &[
            ("ONT_FN_NAME", "route"),
            ("ONT_FN_STORE", "/invented/store.sqlite"),
        ],
        "[",
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(error_of(&out)["error"]["field"], "stdin");
}

fn section(
    repository: Option<&str>,
    path: &str,
    heading: &str,
    lines: (u64, u64),
    text: &str,
) -> Value {
    let mut fields = json!({"path": path, "heading": heading, "first_line": lines.0, "last_line": lines.1,
        "digest": "0".repeat(64), "text": text});
    if let Some(r) = repository {
        fields["repository"] = json!(r);
        fields["commit"] = json!(commit(7));
    }
    object(
        &fields,
        &format!("{repository:?}{path}{heading}{}", lines.0),
    )
}
fn object(fields: &Value, key: &str) -> Value {
    let wrapped: serde_json::Map<String, Value> = fields
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), json!({"kind": "value", "value": v})))
        .collect();
    json!({"id": {"native_key": key}, "head": {"fields": wrapped}})
}
fn answer(fake: &mut Fake, p: Value) -> Value {
    route::answer(fake, "/invented/store.sqlite", &p).unwrap()
}

// spec: specs/ap/route.md#R16
#[test]
fn r16_the_parameters() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    let many: Vec<Value> = (0..30)
        .map(|i| {
            section(
                Some("o/r"),
                &format!("f{i:02}.md"),
                "Widget",
                (1, 2),
                &format!("widget {i}"),
            )
        })
        .collect();
    fake.objects.insert("section".into(), many);
    assert_eq!(
        answer(&mut fake, json!({"words": "widget"}))["answers"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        answer(&mut fake, json!({"words": "widget", "limit": null}))["answers"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        answer(&mut fake, json!({"words": "widget", "limit": 20}))["answers"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        answer(&mut fake, json!({"words": "widget", "limit": 1}))["answers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for (p, field) in [
        (json!({"words": "x", "limit": 0}), "limit"),
        (json!({"words": "x", "limit": 21}), "limit"),
        (json!({"words": "x", "limit": "3"}), "limit"),
        (json!({"words": 3}), "words"),
        (json!({}), "words"),
        (json!({"words": "x", "other": 1}), "other"),
        (json!([]), "parameters"),
    ] {
        let e = route::answer(&mut fake, "/s", &p).unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("invalid_input", field), "{p}");
    }
}

// spec: specs/ap/route.md#R17
#[test]
fn r17_every_page_and_stale() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    // 120 sections: three pages of 50; the best one is on the last page.
    let mut all: Vec<Value> = (0..119)
        .map(|i| {
            section(
                Some("o/r"),
                &format!("f{i:03}.md"),
                "Other",
                (1, 1),
                "nothing here",
            )
        })
        .collect();
    all.push(section(Some("o/r"), "z.md", "Gadget", (1, 1), "the gadget"));
    fake.objects.insert("section".into(), all);
    let v = answer(&mut fake, json!({"words": "gadget"}));
    assert_eq!(v["answers"][0]["path"], "z.md");
    assert!(v.get("stale").is_none());
    let searches: Vec<&Value> = fake
        .calls
        .iter()
        .filter(|c| c.0 == "ont" && c.1[0] == "search")
        .map(|c| &c.2)
        .collect();
    assert_eq!(searches.len(), 4); // three pages of sections, one of components
    assert!(fake.calls.iter().all(|c| {
        c.1.ends_with(&["--store".into(), "/invented/store.sqlite".into()])
    }));
    fake.qualified = false;
    let v = answer(&mut fake, json!({"words": "gadget"}));
    assert_eq!(v["stale"], true);
    assert_eq!(v["answers"][0]["path"], "z.md");
}

// spec: specs/ap/route.md#R18
#[test]
fn r18_the_shape_of_the_answer() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    fake.objects.insert(
        "section".into(),
        vec![
            section(
                Some("o/r"),
                "docs/widget.md",
                "Widget > Use",
                (3, 9),
                "Run the widget.",
            ),
            section(None, "/x/agents/AGENTS.md", "Rules", (1, 4), "No widget."),
            section(Some("o/r"), "other.md", "Other", (1, 1), "nothing"),
        ],
    );
    fake.objects.insert("component".into(), vec![object(&json!({"repository": "o/r", "name": "widget",
        "purpose": "The widget.", "paths": ["src/"], "entry": [], "spec": [], "tests": [], "commit": commit(8)}), "c")]);
    let v = answer(&mut fake, json!({"words": "Widget rules"}));
    let answers = v["answers"].as_array().unwrap();
    assert_eq!(answers.len(), 3, "{v}");
    let by_kind = |k: &str| answers.iter().find(|a| a["kind"] == k).unwrap();
    let c = by_kind("component");
    assert_eq!(
        (
            c["path"].clone(),
            c["first_line"].clone(),
            c["last_line"].clone(),
            c["heading"].clone(),
            c["repository"].clone(),
            c["commit"].clone()
        ),
        (
            json!("map.json"),
            Value::Null,
            Value::Null,
            json!("widget"),
            json!("o/r"),
            json!(commit(8))
        )
    );
    let s = answers
        .iter()
        .find(|a| a["path"] == "docs/widget.md")
        .unwrap();
    let mut keys: Vec<&String> = s.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "commit",
            "first_line",
            "heading",
            "kind",
            "last_line",
            "matched",
            "path",
            "repository",
            "score"
        ]
    );
    assert_eq!(
        (
            s["first_line"].clone(),
            s["last_line"].clone(),
            s["matched"].clone()
        ),
        (json!(3), json!(9), json!(["widget"]))
    );
    let i = answers
        .iter()
        .find(|a| a["path"] == "/x/agents/AGENTS.md")
        .unwrap();
    assert_eq!(
        (
            i["repository"].clone(),
            i["commit"].clone(),
            i["matched"].clone()
        ),
        (Value::Null, Value::Null, json!(["widget", "rules"]))
    );
    assert!(
        answers
            .windows(2)
            .all(|w| w[0]["score"].as_f64() >= w[1]["score"].as_f64())
    );
    // A candidate that matches no word is not answered; nothing matched is an empty list.
    assert_eq!(
        answer(&mut fake, json!({"words": "zebra"}))["answers"],
        json!([])
    );
    assert_eq!(
        answer(&mut fake, json!({"words": ""}))["answers"],
        json!([])
    );
    // The answer stays under 60,000 bytes: long headings drop the last answers.
    let long = "Widget ".repeat(400);
    let big: Vec<Value> = (0..20)
        .map(|i| {
            section(
                Some("o/r"),
                &format!("{}{i:02}.md", "d/".repeat(500)),
                &long,
                (1, 1),
                &format!("widget {i}"),
            )
        })
        .collect();
    fake.objects.insert("section".into(), big);
    let v = answer(&mut fake, json!({"words": "widget", "limit": 20}));
    let n = v["answers"].as_array().unwrap().len();
    assert!(
        v.to_string().len() <= route::ANSWER_BYTES && (1..20).contains(&n),
        "{n}"
    );
}

// spec: specs/ap/route.md#R19
#[test]
fn r19_the_words_and_their_stems() {
    assert_eq!(
        route::words(
            "Run `ap unit run` in the ws-program's worktree: ./check, a/b.md and C:D; X y"
        ),
        [
            "run",
            "ap",
            "unit",
            "run",
            "ws-program",
            "worktree",
            "check",
            "a/b.md",
            "c:d"
        ]
    );
    assert_eq!(
        route::words("The ws-program's ./check: a/b.md, C:D; X y"),
        ["ws-program", "check", "a/b.md", "c:d"]
    );
    assert_eq!(route::words("-_./:edge:/._-"), ["edge"]);
    assert_eq!(route::words("the and of a i"), Vec::<String>::new());
    for (word, stem) in [
        ("refusing", "refus"),
        ("refused", "refus"),
        ("refuses", "refus"),
        ("units", "unit"),
        ("is", "is"),
        ("bus", "bus"),
        ("ws-programs", "ws-programs"),
        ("v14s", "v14s"),
        ("check", "check"),
    ] {
        assert_eq!(route::stem(word), stem, "{word}");
    }
}

// spec: specs/ap/route.md#R20
#[test]
fn r20_a_heading_match_weighs_more_and_ties_go_by_path() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    fake.objects.insert(
        "section".into(),
        vec![
            section(Some("o/r"), "b.md", "Notes", (1, 1), "the gadget is here"),
            section(Some("o/r"), "c.md", "Gadget", (1, 1), "words of c"),
            section(Some("o/r"), "gadget", "Notes", (1, 1), "words of g"),
            section(Some("o/r"), "a.md", "Notes", (1, 1), "a gadget too"),
            section(Some("o/r"), "d.md", "Notes", (1, 1), "nothing"),
        ],
    );
    let v = answer(&mut fake, json!({"words": "gadgets"}));
    let paths: Vec<&str> = v["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["c.md", "gadget", "a.md", "b.md"]);
    let s: Vec<f64> = v["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["score"].as_f64().unwrap())
        .collect();
    let idf = (1.0f64 + 5.0 / 4.0).ln();
    let r = |x: f64| (x * 1000.0).round() / 1000.0;
    assert_eq!(s, [r(3.0 * idf), r(2.0 * idf), r(idf), r(idf)]);
    // A component's name weighs as a heading, its purpose as text.
    fake.objects.insert(
        "component".into(),
        vec![object(
            &json!({"repository": "o/r", "name": "gadget", "purpose": "x",
        "paths": [], "entry": [], "spec": [], "tests": [], "commit": commit(1)}),
            "c",
        )],
    );
    let v = answer(&mut fake, json!({"words": "gadget"}));
    assert_eq!(v["answers"][0]["path"], "c.md");
    assert_eq!(v["answers"][1]["kind"], "component");
}

// spec: specs/ap/route.md#R21
#[test]
fn r21_equal_texts_and_nested_sections_are_answered_once() {
    let dir = Scratch::new();
    let mut fake = Fake::new(&dir, &[]);
    let copies = vec![
        section(
            Some("o/agent-platform"),
            "content/AGENTS.md",
            "Rules",
            (1, 9),
            "The gadget rule.",
        ),
        section(
            None,
            "/x/agents/AGENTS.md",
            "Rules",
            (1, 9),
            "The gadget rule.",
        ),
        section(Some("o/r"), "other.md", "Gadget", (1, 2), ""),
        section(Some("o/r"), "other2.md", "Gadget", (1, 2), ""),
    ];
    fake.objects.insert("section".into(), copies);
    let paths = |v: &Value| -> Vec<String> {
        v["answers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["path"].as_str().unwrap().to_owned())
            .collect()
    };
    // Equal texts: one answer, the instructions' copy, whether it ranked first or the content/ copy did.
    for question in ["gadget", "gadget content/AGENTS.md"] {
        let v = answer(&mut fake, json!({"words": question}));
        let got = paths(&v);
        assert!(
            got.contains(&"/x/agents/AGENTS.md".to_owned())
                && !got.contains(&"content/AGENTS.md".to_owned()),
            "{v}"
        );
        // Two sections without text are both answered.
        assert!(
            got.contains(&"other.md".to_owned()) && got.contains(&"other2.md".to_owned()),
            "{v}"
        );
    }
    // A section within a section of the same file ranked higher is dropped; the reverse keeps both.
    fake.objects.insert(
        "section".into(),
        vec![
            section(
                Some("o/r"),
                "guide.md",
                "Gadget guide",
                (1, 40),
                "gadget one",
            ),
            section(
                Some("o/r"),
                "guide.md",
                "Gadget guide > Steps",
                (10, 20),
                "gadget two",
            ),
        ],
    );
    let v = answer(&mut fake, json!({"words": "gadget guide steps"}));
    assert_eq!(v["answers"].as_array().unwrap().len(), 2, "{v}");
    assert_eq!(v["answers"][0]["first_line"], 10);
    let v = answer(&mut fake, json!({"words": "gadget guide"}));
    let a = v["answers"].as_array().unwrap();
    assert_eq!(a.len(), 1, "{v}");
    assert_eq!(a[0]["first_line"], 1);
}

// spec: specs/ap/route.md#R22
#[test]
fn r22_the_timer_and_its_service() {
    let service = include_str!("../../../deploy/ap-route.service");
    let timer = include_str!("../../../deploy/ap-route.timer");
    let exec = service
        .lines()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .unwrap();
    assert_eq!(
        exec,
        "/usr/local/bin/ap route index --prune --repository park-kyungchan/agent-platform --repository park-kyungchan/ontology-platform --repository park-kyungchan/agent-vps-operations --instructions /root/.agents"
    );
    assert!(!service.contains("education"));
    assert!(timer.lines().any(|l| l == "OnCalendar=*:0/10"));
    let options = route::options(
        &exec
            .split(' ')
            .skip(3)
            .map(OsString::from)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(options.prune && options.repositories.len() == 3 && options.store.is_none());
    let readme = include_str!("../../../deploy/README.md");
    let (_, section) = readme.split_once("\n## Routes\n").unwrap();
    let section = section
        .split_once("\n## ")
        .map_or(section, |(head, _)| head);
    assert!(section.contains("ap-route.service") && section.contains("ap-route.timer"));
    assert!(
        !section
            .lines()
            .any(|l| l.starts_with("install -m 0644 deploy/"))
    );
}
