use ap_mem_adapter::git::{BodyHash, CommitId, Git};
use ap_mem_adapter::ports::PortError;
use ap_mem_adapter::types::{RecordId, SubjectType};
use serde_json::json;
use std::fs::{self, DirBuilder};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::var_os("AP_S03_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = parent.join(format!(
            "ap-mem-git-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::create_dir(root.join("ci")).unwrap();
        fs::create_dir_all(root.join("memory/records")).unwrap();
        fs::create_dir(root.join("scratch")).unwrap();
        fs::write(
            root.join("ci/check-memory-records.py"),
            include_bytes!("../../../ci/check-memory-records.py"),
        )
        .unwrap();
        let f = Self(root);
        f.git(&["init", "--initial-branch=main"]);
        f
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .arg("-C")
            .arg(&self.0)
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "owned Git fixture command failed");
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }
    fn commit(&self) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "--no-gpg-sign", "-m", "invented fixture revision"]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn write(&self, id: &str, bytes: &[u8]) {
        fs::write(self.0.join(format!("memory/records/{id}.md")), bytes).unwrap();
    }
    fn adapter(&self) -> Git {
        Git::new(&self.0, self.0.join("scratch"), "refs/heads/main")
    }
    fn clean_scratch(&self) {
        assert_eq!(fs::read_dir(self.0.join("scratch")).unwrap().count(), 0);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let result = fs::remove_dir_all(&self.0);
        if !std::thread::panicking() {
            result.expect("owned Git fixture cleaned");
        }
    }
}
fn record(id: &str, subject: &str, body: &str) -> Vec<u8> {
    let header = json!({"schema_version":1,"record_id":id,"kind":"incident","title":"invented fixture","subject":{"type":"task","id":subject},"scope":"invented scope","owner_ref":"none","owner_ref_reason":"invented fixture reason","sources":[{"date":"2001-02-03","locator":"fixture://source"}]});
    let mut text = format!("---\n{header}\n---\n");
    for heading in [
        "Occurrence",
        "Expected and actual behavior",
        "Boundary",
        "Impact",
        "Cause",
        "Mitigation",
        "Verification",
    ] {
        text.push_str(&format!("\n## {heading}\n{body}\n"));
    }
    text.into_bytes()
}
fn id(value: &str) -> RecordId {
    RecordId::try_from(value.to_owned()).unwrap()
}

#[test]
fn publisher_resolves_prints_tip() {
    let f = Fixture::new();
    f.write(
        "fixture-a",
        &record("fixture-a", "fixture-subject", "invented A"),
    );
    let first = f.commit();
    let git = f.adapter();
    let c = git.pin_main().unwrap();
    assert_eq!(c.as_str(), first);
    let mut printed = Vec::new();
    writeln!(printed, "{c}").unwrap();
    assert_eq!(printed, format!("{first}\n").as_bytes());
    // The configured main, not the current branch or dirty files, selects C.
    f.git(&["checkout", "-b", "unselected"]);
    f.write(
        "fixture-a",
        &record("fixture-a", "wrong-subject", "invented unselected"),
    );
    let other = f.commit();
    assert_ne!(first, other);
    assert_eq!(git.pin_main().unwrap(), c);
    f.git(&["update-ref", "refs/heads/main", &other]);
    assert_eq!(git.pin_main().unwrap().as_str(), other);
    assert_eq!(
        git.originals(&c).unwrap()[&id("fixture-a")].subject.id,
        "fixture-subject"
    );
    f.clean_scratch();
}

#[test]
fn main_originals() {
    let f = Fixture::new();
    let old = record("fixture-a", "fixture-subject", "invented original");
    f.write("fixture-a", &old);
    let first = f.commit();
    let git = f.adapter();
    let c = git.pin_main().unwrap();
    assert_eq!(c.as_str(), first);
    let current = record("fixture-a", "fixture-new-subject", "invented replacement");
    f.write("fixture-a", &current);
    f.write(
        "fixture-b",
        &record("fixture-b", "fixture-other", "invented second"),
    );
    f.commit();
    f.write(
        "fixture-a",
        &record("fixture-a", "dirty-subject", "invented DIRTY"),
    );
    let originals = git.originals(&c).unwrap();
    assert_eq!(originals.len(), 1);
    let original = &originals[&id("fixture-a")];
    assert_eq!(original.bytes, old);
    assert_eq!(original.subject.kind, SubjectType::Task);
    assert_eq!(original.subject.id, "fixture-subject");
    assert_eq!(
        original.tags,
        ["kind:incident", "subject:task:fixture-subject"]
    );
    let next = git.originals(&git.pin_main().unwrap()).unwrap();
    assert_eq!(next.len(), 2);
    assert_eq!(next[&id("fixture-a")].bytes, current);
    assert!(
        !next[&id("fixture-a")]
            .tags
            .iter()
            .any(|t| t.starts_with("status:"))
    );
    f.clean_scratch();
    // Reject Git symlinks/nested non-original files rather than following them.
    f.git(&["reset", "--hard", "HEAD"]);
    std::os::unix::fs::symlink(
        "../../../unowned",
        f.0.join("memory/records/fixture-link.md"),
    )
    .unwrap();
    f.commit();
    assert!(git.originals(&git.pin_main().unwrap()).is_err());
    f.clean_scratch();
}

#[test]
fn checker_write_only() {
    let f = Fixture::new();
    let bytes = record("fixture-a", "fixture-subject", "invented private fixture");
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("## Verification", "### Verification");
    f.write("fixture-a", text.as_bytes());
    f.commit();
    let git = f.adapter();
    // Pinning/printing is still available for invalid originals: it runs no checker.
    let c = git.pin_main().unwrap();
    assert_eq!(c.to_string().len(), 40);
    // A dirty permissive checker cannot replace the trusted pinned checker.
    fs::write(
        f.0.join("ci/check-memory-records.py"),
        "raise SystemExit(0)\n",
    )
    .unwrap();
    assert!(matches!(git.originals(&c), Err(PortError::InvalidResponse)));
    assert_eq!(
        fs::read(f.0.join("memory/records/fixture-a.md")).unwrap(),
        text.as_bytes()
    );
    f.clean_scratch();
}

#[test]
fn exact_utf8() {
    let f = Fixture::new();
    let bytes = record(
        "fixture-utf8",
        "fixture-subject",
        "invented Ω: bytes and trailing spaces  \nnext line",
    );
    f.write("fixture-utf8", &bytes);
    f.commit();
    let git = f.adapter();
    let originals = git.originals(&git.pin_main().unwrap()).unwrap();
    let original = &originals[&id("fixture-utf8")];
    assert_eq!(original.bytes, bytes);
    // Independent Python hashlib oracle; no duplicate SHA256 implementation in Rust.
    let mut child = Command::new("python3")
        .args([
            "-I",
            "-B",
            "-c",
            "import hashlib,sys; print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        original.hash.as_str(),
        String::from_utf8(output.stdout).unwrap().trim()
    );
    for bad in ["", "abcdef", &"g".repeat(64), &"A".repeat(64)] {
        assert!(BodyHash::try_from(bad.to_owned()).is_err());
    }
    for bad in ["main", "--help", &"f".repeat(39), &"a".repeat(41)] {
        assert!(CommitId::try_from(bad.to_owned()).is_err());
    }
    for bad in [
        vec![0xff],
        bytes
            .iter()
            .copied()
            .flat_map(|b| if b == b'\n' { vec![b'\r', b] } else { vec![b] })
            .collect(),
        [b"\xef\xbb\xbf".as_slice(), bytes.as_slice()].concat(),
    ] {
        f.write("fixture-utf8", &bad);
        f.commit();
        assert!(git.originals(&git.pin_main().unwrap()).is_err());
        f.clean_scratch();
    }
}
