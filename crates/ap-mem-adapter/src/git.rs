//! Pinned Git originals for the write route. No store, upstream, or reader policy.
use crate::ports::PortError;
use crate::types::{RecordId, RecordKind, Subject};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! digest {
    ($name:ident, $length:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);
        impl TryFrom<String> for $name {
            type Error = PortError;
            fn try_from(value: String) -> Result<Self, PortError> {
                if value.len() != $length
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(PortError::InvalidResponse);
                }
                Ok(Self(value))
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
digest!(CommitId, 40);
digest!(BodyHash, 64);

// The existing RecordId owns identity/validation; ordering only enables the row's map.
impl PartialOrd for RecordId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RecordId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

#[derive(Clone)]
pub struct Original {
    pub bytes: Vec<u8>,
    pub hash: BodyHash,
    pub subject: Subject,
    pub tags: Vec<String>,
}
#[derive(Deserialize)]
struct Header {
    record_id: RecordId,
    kind: RecordKind,
    subject: Subject,
}

pub struct Git {
    repository: PathBuf,
    scratch: PathBuf,
    main_ref: String,
}
impl Git {
    /// All three values are trusted launch configuration, never MCP arguments.
    pub fn new(
        repository: impl Into<PathBuf>,
        scratch: impl Into<PathBuf>,
        main_ref: &str,
    ) -> Self {
        Self {
            repository: repository.into(),
            scratch: scratch.into(),
            main_ref: main_ref.into(),
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .arg("--no-optional-locks")
            .arg("-c")
            .arg("core.fsmonitor=false")
            .arg("-C")
            .arg(&self.repository)
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        command
    }
    fn git(&self, args: &[&str]) -> Result<Vec<u8>, PortError> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|_| PortError::Unavailable)?;
        if !output.status.success() {
            return Err(PortError::Unavailable);
        }
        Ok(output.stdout)
    }
    /// Resolve the configured main once; Display supplies the publisher's printed C.
    pub fn pin_main(&self) -> Result<CommitId, PortError> {
        if !self.main_ref.starts_with("refs/") || !self.main_ref.ends_with("/main") {
            return Err(PortError::InvalidResponse);
        }
        self.git(&["check-ref-format", &self.main_ref])?;
        let raw = self.git(&[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", self.main_ref),
        ])?;
        let text = std::str::from_utf8(&raw).map_err(|_| PortError::InvalidResponse)?;
        CommitId::try_from(text.trim_end_matches('\n').to_owned())
    }
    /// Validate a complete pinned tree only on the publisher/write route.
    pub fn originals(&self, commit: &CommitId) -> Result<BTreeMap<RecordId, Original>, PortError> {
        let temporary = Temporary::new(&self.scratch)?;
        let records = temporary.0.join("records");
        DirBuilder::new()
            .mode(0o700)
            .create(&records)
            .map_err(|_| PortError::Unavailable)?;
        let tree = self.git(&[
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            commit.as_str(),
            "--",
            "memory/records",
        ])?;
        let mut files = Vec::new();
        for entry in tree.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
            let text = std::str::from_utf8(entry).map_err(|_| PortError::InvalidResponse)?;
            let (metadata, path) = text.split_once('\t').ok_or(PortError::InvalidResponse)?;
            let fields: Vec<_> = metadata.split_ascii_whitespace().collect();
            if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") || fields[1] != "blob"
            {
                return Err(PortError::InvalidResponse);
            }
            let name = path
                .strip_prefix("memory/records/")
                .ok_or(PortError::InvalidResponse)?;
            let id = RecordId::try_from(
                name.strip_suffix(".md")
                    .ok_or(PortError::InvalidResponse)?
                    .to_owned(),
            )
            .map_err(|_| PortError::InvalidResponse)?;
            let bytes = self.git(&["cat-file", "blob", fields[2]])?;
            write_private(&records.join(name), &bytes)?;
            files.push((id, bytes));
        }
        // Use the trusted main commit's exact checker, never a dirty worktree copy.
        let checker = self.git(&[
            "cat-file",
            "blob",
            &format!("{commit}:ci/check-memory-records.py"),
        ])?;
        let checker_path = temporary.0.join("check.py");
        write_private(&checker_path, &checker)?;
        let status = Command::new("python3")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(["-I", "-B"])
            .arg(&checker_path)
            .arg(&records)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| PortError::Unavailable)?;
        if !status.success() {
            return Err(PortError::InvalidResponse);
        }
        let mut originals = BTreeMap::new();
        for (id, bytes) in files {
            let text = std::str::from_utf8(&bytes).map_err(|_| PortError::InvalidResponse)?;
            let header = text
                .strip_prefix("---\n")
                .and_then(|s| s.split_once("\n---\n"))
                .ok_or(PortError::InvalidResponse)?
                .0;
            let header: Header =
                serde_json::from_str(header).map_err(|_| PortError::InvalidResponse)?;
            if id != header.record_id {
                return Err(PortError::InvalidResponse);
            }
            let kind = serde_json::to_value(header.kind).map_err(|_| PortError::InvalidResponse)?;
            let subject = serde_json::to_value(header.subject.kind)
                .map_err(|_| PortError::InvalidResponse)?;
            let tags = vec![
                format!("kind:{}", kind.as_str().ok_or(PortError::InvalidResponse)?),
                format!(
                    "subject:{}:{}",
                    subject.as_str().ok_or(PortError::InvalidResponse)?,
                    header.subject.id
                ),
            ];
            let original = Original {
                hash: body_hash(&bytes)?,
                bytes,
                subject: header.subject,
                tags,
            };
            if originals.insert(id, original).is_some() {
                return Err(PortError::InvalidResponse);
            }
        }
        Ok(originals)
    }
}
fn body_hash(bytes: &[u8]) -> Result<BodyHash, PortError> {
    // Coreutils SHA256 avoids a second crypto implementation or a manifest change.
    let mut child = Command::new("sha256sum")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| PortError::Unavailable)?;
    let written = child.stdin.take().unwrap().write_all(bytes);
    let output = child
        .wait_with_output()
        .map_err(|_| PortError::Unavailable)?;
    written.map_err(|_| PortError::Unavailable)?;
    if !output.status.success() {
        return Err(PortError::Unavailable);
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| PortError::InvalidResponse)?;
    BodyHash::try_from(
        text.split_ascii_whitespace()
            .next()
            .ok_or(PortError::InvalidResponse)?
            .to_owned(),
    )
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), PortError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|_| PortError::Unavailable)
}
struct Temporary(PathBuf);
impl Temporary {
    fn new(parent: &Path) -> Result<Self, PortError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = parent.join(format!(
            "ap-mem-git-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| PortError::Unavailable)?;
        Ok(Self(path))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let result = fs::remove_dir_all(&self.0);
        if !std::thread::panicking() {
            result.expect("owned Git scratch cleaned");
        }
    }
}
