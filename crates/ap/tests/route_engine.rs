//! `ap route index` and the function `route` (`specs/ap/route.md`) against the real engine: invented
//! repositories with a bare origin each, a scratch store whose schema is the repository's
//! `ontology/work.schema.json` with the globs of `sections` and `components` rewritten to a scratch directory
//! and the function's `program` rewritten to a copy of the built `ap` (mode 0755, owned by the caller,
//! `specs/ont/fn.md` R7), and a scratch policy file. Nothing of the host's directory, store or clones is read.
//!
//! The engine admits only uid 0, so these tests are ignored by the plain `cargo test`: `./check` builds them
//! with `cargo test --no-run` and runs the test binary with `--ignored`, the engine in `ONT_BIN`, through
//! `sudo` in CI.
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const HOST_ROUTE: &str = "/var/lib/agent-platform/route/";

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
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "protocol.file.allow=always",
        ])
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
            "ap-route-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // The engine refuses a store reached through a symbolic link: the root is a real path.
        let root = std::fs::canonicalize(&base).unwrap();
        for d in ["origins", "work", "clones", "bin", "agents"] {
            std::fs::create_dir(root.join(d)).unwrap();
        }
        let ap = root.join("bin/ap");
        std::fs::copy(env!("CARGO_BIN_EXE_ap"), &ap).unwrap();
        std::fs::set_permissions(&ap, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut schema: Value =
            serde_json::from_str(include_str!("../../../ontology/work.schema.json")).unwrap();
        for name in ["sections", "components"] {
            let glob = schema["sources"][name]["glob"].as_str().unwrap();
            let rest = glob.strip_prefix(HOST_ROUTE).unwrap().to_owned();
            schema["sources"][name]["glob"] = json!(format!("{}/route/{rest}", root.display()));
        }
        assert_eq!(schema["functions"]["route"]["program"], "/usr/local/bin/ap");
        schema["functions"]["route"]["program"] = json!(ap.to_str().unwrap());
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
        bed
    }
    fn at(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn store(&self) -> String {
        self.at("store.sqlite").to_str().unwrap().to_owned()
    }
    /// A repository `fixture/<name>`: its working tree, a bare origin it pushed `main` to, and a clone.
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
        self.commit(name, files);
        git(
            &self.root,
            &[
                "clone",
                "--quiet",
                origin.to_str().unwrap(),
                self.at(&format!("clones/{name}")).to_str().unwrap(),
            ],
        );
        self.policy();
    }
    /// Writes the files in the working tree of `name`, commits and pushes; the new commit.
    fn commit(&self, name: &str, files: &[(&str, &str)]) -> String {
        let work = self.at(&format!("work/{name}"));
        for (path, text) in files {
            let p = work.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "--quiet", "-m", "fixture"]);
        git(&work, &["push", "--quiet", "origin", "HEAD:main"]);
        git(&work, &["rev-parse", "HEAD"])
    }
    fn policy(&self) {
        let mut repositories = serde_json::Map::new();
        for e in std::fs::read_dir(self.at("clones")).unwrap() {
            let e = e.unwrap();
            let name = e.file_name().into_string().unwrap();
            repositories.insert(
                format!("fixture/{name}"),
                json!({"clone": e.path().to_str().unwrap()}),
            );
        }
        std::fs::write(
            self.at("policy.json"),
            json!({ "repositories": repositories }).to_string(),
        )
        .unwrap();
    }
    fn ont(&self, args: &[&str], stdin: &str) -> (i32, Value) {
        let mut child = Command::new(&self.ont)
            .arg("--store")
            .arg(self.store())
            .args(args)
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
    /// `ap route index` into the scratch directory and store.
    fn index(&self, extra: &[&str]) -> (i32, Value) {
        let out = Command::new(self.at("bin/ap"))
            .args([
                "route",
                "index",
                "--directory",
                self.at("route").to_str().unwrap(),
                "--store",
                &self.store(),
            ])
            .args(extra)
            .env("PATH", &self.path)
            .env("AP_POLICY_FILE", self.at("policy.json"))
            .env_remove("ONT_FN_NAME")
            .output()
            .unwrap();
        let text = if out.stdout.is_empty() {
            &out.stderr
        } else {
            &out.stdout
        };
        let v = serde_json::from_slice(text)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(text)));
        (out.status.code().unwrap_or(-1), v)
    }
    fn route(&self, words: &str) -> Value {
        let (code, v) = self.ont(
            &["fn", "route"],
            &json!({"parameters": {"words": words}}).to_string(),
        );
        assert_eq!(code, 0, "{v}");
        assert!(v["elapsed_ms"].as_u64().unwrap() < 2000, "{v}");
        v["answer"].clone()
    }
    /// The current heads of `kind`: native key and fields.
    fn heads(&self, kind: &str) -> Vec<(String, Value)> {
        let (code, v) = self.ont(
            &["search"],
            &json!({"object_type": kind, "limit": 1000}).to_string(),
        );
        assert_eq!(code, 0, "{v}");
        v["result"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                let fields: serde_json::Map<String, Value> = o["head"]["fields"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, f)| (k.clone(), f["value"].clone()))
                    .collect();
                (
                    o["id"]["native_key"].as_str().unwrap().to_owned(),
                    Value::Object(fields),
                )
            })
            .collect()
    }
}

const README: &str = "# Alpha\n\nThe invented widget lives here.\n\n## Install the widget\n\nRun `widget install --now`.\n";
const MAP: &str = r#"{"components":[{"name":"widget","purpose":"The invented widget and its installer.","paths":["src/"],"entry":["src/main.rs:1-1"],"spec":["docs/guide.md"],"tests":[]}]}"#;

fn three(bed: &Bed) {
    bed.repository(
        "alpha",
        &[
            ("README.md", README),
            ("docs/guide.md", "# Guide\n\nRead the gauge.\n"),
            ("map.json", MAP),
        ],
    );
    bed.repository("beta", &[("NOTES.md", "# Beta\n\nThe gadget notes.\n")]);
    std::fs::write(
        bed.at("agents/AGENTS.md"),
        "# Shared\n\nRoute every gadget question here.\n",
    )
    .unwrap();
}

// spec: specs/ap/route.md#R4
// spec: specs/ap/route.md#R5
// spec: specs/ap/route.md#R12
// spec: specs/ap/route.md#R13
// spec: specs/ap/route.md#R15
// spec: specs/ap/route.md#R17
// spec: specs/ap/route.md#R18
#[test]
#[ignore = "runs the real engine as uid 0: ./check runs it"]
fn engine_the_index_and_the_answer_on_a_scratch_store() {
    let bed = Bed::new();
    three(&bed);
    let first = git(&bed.at("work/alpha"), &["rev-parse", "HEAD"]);
    let agents = bed.at("agents").to_str().unwrap().to_owned();
    let (code, line) = bed.index(&[
        "--prune",
        "--repository",
        "fixture/alpha",
        "--repository",
        "fixture/beta",
        "--instructions",
        &agents,
    ]);
    assert_eq!(code, 0, "{line}");
    assert_eq!(line["status"], "ok");
    assert_eq!(line["sources"]["sections"]["records"], 5);
    assert_eq!(line["sources"]["sections"]["appended"]["initial"], 5);
    assert_eq!(line["sources"]["components"]["appended"]["initial"], 1);
    assert_eq!(line["kept"], json!([]));
    let answer = bed.route("how do I install the widget");
    let top = &answer["answers"][0];
    assert_eq!(
        (
            top["kind"].clone(),
            top["repository"].clone(),
            top["path"].clone(),
            top["heading"].clone()
        ),
        (
            json!("section"),
            json!("fixture/alpha"),
            json!("README.md"),
            json!("Alpha > Install the widget")
        ),
        "{answer}"
    );
    assert_eq!(
        (
            top["first_line"].clone(),
            top["last_line"].clone(),
            top["commit"].clone()
        ),
        (json!(5), json!(7), json!(first))
    );
    assert_eq!(top["matched"], json!(["install", "widget"]));
    assert!(answer.get("stale").is_none());
    let component = answer["answers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "component")
        .unwrap();
    assert_eq!(
        (
            component["path"].clone(),
            component["heading"].clone(),
            component["first_line"].clone()
        ),
        (json!("map.json"), json!("widget"), Value::Null)
    );
    let answer = bed.route("a gadget question");
    let shared = answer["answers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["repository"].is_null())
        .unwrap();
    assert_eq!(shared["path"], format!("{agents}/AGENTS.md"));
    assert_eq!(shared["commit"], Value::Null);
    // A commit that changes another file changes none of README's sections; lines added above a section
    // keep its key and move its lines.
    let key_of = |heading: &str| {
        bed.heads("section")
            .into_iter()
            .find(|(_, f)| f["heading"] == heading)
            .unwrap()
    };
    let (install_key, _) = key_of("Alpha > Install the widget");
    let second = bed.commit(
        "alpha",
        &[("docs/guide.md", "# Guide\n\nRead the gauge twice.\n")],
    );
    let (code, line) = bed.index(&[
        "--repository",
        "fixture/alpha",
        "--repository",
        "fixture/beta",
        "--instructions",
        &agents,
    ]);
    assert_eq!(code, 0, "{line}");
    assert_eq!(
        line["sources"]["sections"]["appended"]["update"], 1,
        "{line}"
    );
    assert_eq!(
        key_of("Alpha > Install the widget").1["commit"],
        json!(first)
    );
    assert_eq!(key_of("Guide").1["commit"], json!(second));
    let third = bed.commit("alpha", &[("README.md", &format!("Above.\n\n{README}"))]);
    let (code, _) = bed.index(&[
        "--repository",
        "fixture/alpha",
        "--repository",
        "fixture/beta",
        "--instructions",
        &agents,
    ]);
    assert_eq!(code, 0);
    let (key, fields) = key_of("Alpha > Install the widget");
    assert_eq!(key, install_key);
    assert_eq!(
        (fields["first_line"].clone(), fields["commit"].clone()),
        (json!(7), json!(third))
    );
    assert_eq!(
        bed.route("install the widget")["answers"][0]["first_line"],
        7
    );
}

// spec: specs/ap/route.md#R4
// spec: specs/ap/route.md#R8
// spec: specs/ap/route.md#R13
#[test]
#[ignore = "runs the real engine as uid 0: ./check runs it"]
fn engine_a_failing_repository_keeps_its_file_and_a_repository_no_longer_named_is_retracted() {
    let bed = Bed::new();
    three(&bed);
    let named = [
        "--repository",
        "fixture/alpha",
        "--repository",
        "fixture/beta",
    ];
    let (code, line) = bed.index(&named);
    assert_eq!(code, 0, "{line}");
    let beta_file = bed.at("route/fixture__beta.sections.jsonl");
    let before = std::fs::read(&beta_file).unwrap();
    // beta's origin is gone: its fetch fails, its file and its sections stay; alpha goes on.
    std::fs::rename(bed.at("origins/beta.git"), bed.at("origins/beta.moved")).unwrap();
    bed.commit("alpha", &[("docs/more.md", "# More\n\nAnother gauge.\n")]);
    let (code, line) = bed.index(&named);
    assert_eq!(code, 0, "{line}");
    assert_eq!(
        line["kept"],
        json!([{"repository": "fixture/beta", "reason": "fetch_failed", "previous": true}])
    );
    assert_eq!(std::fs::read(&beta_file).unwrap(), before);
    assert_eq!(line["sources"]["sections"]["retracted"], 0);
    let repos = |bed: &Bed| -> Vec<String> {
        let mut r: Vec<String> = bed
            .heads("section")
            .iter()
            .map(|(_, f)| f["repository"].as_str().unwrap_or("").to_owned())
            .collect();
        r.sort();
        r.dedup();
        r
    };
    assert_eq!(repos(&bed), ["fixture/alpha", "fixture/beta"]);
    assert!(
        bed.heads("section")
            .iter()
            .any(|(_, f)| f["path"] == "docs/more.md")
    );
    // An unrelated file, a subdirectory and a symbolic link in the directory.
    let dir = bed.at("route");
    std::fs::write(dir.join("notes.txt"), "kept").unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/x.sections.jsonl"), "").unwrap();
    std::fs::write(bed.at("elsewhere.jsonl"), "").unwrap();
    std::os::unix::fs::symlink(bed.at("elsewhere.jsonl"), dir.join("link.sections.jsonl")).unwrap();
    // Without --prune, beta's file stays though beta is no longer named.
    let (code, line) = bed.index(&["--repository", "fixture/alpha"]);
    assert_eq!(code, 0, "{line}");
    assert!(beta_file.exists() && line["removed"] == json!([]));
    // With --prune it is removed and its sections are retracted; nothing else is touched.
    let (code, line) = bed.index(&["--prune", "--repository", "fixture/alpha"]);
    assert_eq!(code, 0, "{line}");
    assert_eq!(
        line["removed"],
        json!([
            "fixture__beta.components.jsonl",
            "fixture__beta.sections.jsonl"
        ])
    );
    assert_eq!(line["sources"]["sections"]["retracted"], 1);
    assert_eq!(repos(&bed), ["fixture/alpha"]);
    assert!(dir.join("notes.txt").exists() && dir.join("sub/x.sections.jsonl").exists());
    assert!(
        std::fs::symlink_metadata(dir.join("link.sections.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(bed.at("elsewhere.jsonl").exists());
    let answer = bed.route("the gadget notes");
    assert!(
        answer["answers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["repository"] != "fixture/beta"),
        "{answer}"
    );
}
