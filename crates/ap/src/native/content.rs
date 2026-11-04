//! Explicit read-only public content coverage. No install/apply selector is added.
use super::{filesystem as fs, types::*};
use crate::Entry;
use serde::{Deserialize, Serialize};

#[path = "content_inventory.rs"]
mod inventory;
pub use inventory::OWNED;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderContentRequest {
    pub schema_version: u32,
    pub agents_dir: String,
    pub entries: Vec<ContentInput>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentInput {
    pub relative_path: String,
    pub before: Entry,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentSpec {
    pub schema_version: u32,
    pub agents_dir: String,
    pub uid: u32,
    pub environment: Environment,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentArtifact {
    pub relative_path: String,
    pub path: String,
    pub before: Entry,
    pub after: Entry,
    pub changed: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentPlan {
    pub schema_version: u32,
    pub artifacts: Vec<ContentArtifact>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentStatus {
    pub relative_path: String,
    pub path: String,
    pub status: String,
    pub expected_sha256: String,
    pub observed_sha256: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentReport {
    pub schema_version: u32,
    pub outcome: String,
    pub entries: Vec<ContentStatus>,
    pub native_loading: String,
    pub hooks_executed: u32,
    pub p6_complete: bool,
}
fn validate(version: u32, agents: &str) -> Result<()> {
    if version != 1 {
        return Err(err("unsupported_schema", "schema_version"));
    }
    if !fs::valid_path(agents) {
        return Err(err("invalid_path", "agents_dir"));
    }
    Ok(())
}
fn file_or_missing(entry: &Entry) -> Result<()> {
    match entry {
        Entry::Missing => Ok(()),
        Entry::File { text } if text.len() <= fs::LIMIT && !text.contains('\0') => Ok(()),
        Entry::File { .. } => Err(err("unsupported_text", "content")),
        _ => Err(err("unsupported_target", "content")),
    }
}
/// Pure public-content proposals. Every inventory entry must appear exactly once.
/// Proposed replacement bytes are not installation authority for existing files.
pub fn render(request: &RenderContentRequest) -> Result<ContentPlan> {
    validate(request.schema_version, &request.agents_dir)?;
    if request.entries.len() != OWNED.len() {
        return Err(err("content_set_mismatch", "entries"));
    }
    let mut artifacts = Vec::new();
    for (relative, text) in OWNED {
        let matches = request
            .entries
            .iter()
            .filter(|e| e.relative_path == *relative)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(err("content_set_mismatch", "entries"));
        }
        let before = matches[0].before.clone();
        file_or_missing(&before)?;
        let after = Entry::File {
            text: (*text).into(),
        };
        artifacts.push(ContentArtifact {
            relative_path: (*relative).into(),
            path: format!("{}/{relative}", request.agents_dir),
            changed: before != after,
            before,
            after,
        });
    }
    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ContentPlan {
        schema_version: 1,
        artifacts,
    })
}
fn snapshot(path: &str, uid: u32) -> Result<Snapshot> {
    let (parent, name) = fs::split(path)?;
    // Open each existing prefix with the same no-follow/owner/mode rules as S03.
    // Missing parents mean missing content; no directories or locks are created.
    let mut dir = fs::Dir::open("/", uid)?;
    let mut current = String::new();
    for part in parent.split('/').filter(|p| !p.is_empty()) {
        if dir.absent(part)? {
            dir.recheck(uid)?;
            return Ok(Snapshot {
                entry: Entry::Missing,
                stamp: None,
                sha256: None,
            });
        }
        current.push('/');
        current.push_str(part);
        dir = fs::Dir::open(&current, uid)?;
    }
    let value = dir.snapshot(name, uid)?;
    dir.recheck(uid)?;
    file_or_missing(&value.entry)?;
    Ok(value)
}
pub fn doctor(spec: &ContentSpec) -> Result<ContentReport> {
    validate(spec.schema_version, &spec.agents_dir)?;
    if spec.uid != rustix::process::geteuid().as_raw() {
        return Err(err("uid_mismatch", "uid"));
    }
    match &spec.environment {
        Environment::Disposable { root } => {
            if !fs::valid_path(root) || !fs::beneath(&spec.agents_dir, root) {
                return Err(err("invalid_environment", "environment"));
            }
            fs::Dir::private(root, spec.uid)?;
        }
        Environment::CoordinatedNative { evidence } if !evidence.trim().is_empty() => {}
        _ => return Err(err("invalid_environment", "environment")),
    }
    let mut entries = Vec::new();
    for (relative, text) in OWNED {
        let path = format!("{}/{relative}", spec.agents_dir);
        let observed = snapshot(&path, spec.uid)?;
        let status = match &observed.entry {
            Entry::Missing => "missing",
            Entry::File { text: actual } if actual == text => "ready",
            _ => "drift",
        };
        entries.push(ContentStatus {
            relative_path: (*relative).into(),
            path,
            status: status.into(),
            expected_sha256: fs::hash(text.as_bytes()),
            observed_sha256: observed.sha256,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ContentReport {
        schema_version: 1,
        outcome: "diagnosed".into(),
        entries,
        native_loading: "unverified".into(),
        hooks_executed: 0,
        p6_complete: false,
    })
}
