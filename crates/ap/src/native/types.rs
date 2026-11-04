use serde::{Deserialize, Serialize};

use crate::{Entry, Selector};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub schema_version: u32,
    pub canonical_path: String,
    pub codex_dir: String,
    pub claude_dir: String,
    pub scope: Scope,
    pub uid: u32,
    pub environment: Environment,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::types::optional_profile"
    )]
    pub agy_dir: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Codex,
    Claude,
    Both,
    Agy,
}
impl Scope {
    pub(crate) fn codex(self) -> bool {
        matches!(self, Self::Codex | Self::Both)
    }
    pub(crate) fn claude(self) -> bool {
        matches!(self, Self::Claude | Self::Both)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Environment {
    Disposable { root: String },
    CoordinatedNative { evidence: String },
}

/// atime deliberately excluded: reading must not invalidate a plan.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub nlink: u64,
    pub len: i64,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub entry: Entry,
    pub stamp: Option<Stamp>,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceFingerprint {
    pub stamp: Stamp,
    pub sha256: String,
}

/// Stable directory identity: child creation legitimately changes directory mtime.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectoryIdentity {
    pub path: String,
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlannedEntry {
    pub path: String,
    pub selector: Selector,
    pub before: Snapshot,
    pub after: Entry,
    pub changed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstallPlan {
    pub schema_version: u32,
    pub spec: Spec,
    pub source: SourceFingerprint,
    pub topology: Vec<DirectoryIdentity>,
    pub entries: Vec<PlannedEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub plan: InstallPlan,
    pub command_dir: String,
    pub writers_quiesced: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverRequest {
    pub command_dir: String,
    pub writers_quiesced: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pending,
    Prepared,
    Applied,
    Unchanged,
    Conflicted,
    Recovering,
    Recovered,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntryReport {
    pub path: String,
    pub selector: Selector,
    pub phase: Phase,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallReport {
    pub schema_version: u32,
    pub outcome: String,
    pub entries: Vec<EntryReport>,
    pub error: Option<NativeError>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeError {
    pub code: String,
    pub field: String,
}
pub(crate) fn err(code: &str, field: &str) -> NativeError {
    NativeError {
        code: code.into(),
        field: field.into(),
    }
}
pub(crate) type Result<T> = std::result::Result<T, NativeError>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    pub schema_version: u32,
    pub plan: InstallPlan,
    pub command_topology: Vec<DirectoryIdentity>,
    pub phases: Vec<Phase>,
    /// Fingerprint of this command's staged inode, durable before replacement.
    pub written: Vec<Option<Snapshot>>,
    /// An interrupted recovery compares against the exact current bytes observed
    /// for that recovery, not the install's old whole-file snapshot.
    pub recovery_before: Vec<Option<Snapshot>>,
    pub recovery_after: Vec<Option<Entry>>,
}
impl State {
    pub fn report(&self, outcome: &str, error: Option<NativeError>) -> InstallReport {
        InstallReport {
            schema_version: 1,
            outcome: outcome.into(),
            error,
            entries: self
                .plan
                .entries
                .iter()
                .zip(&self.phases)
                .map(|(e, p)| EntryReport {
                    path: e.path.clone(),
                    selector: e.selector.clone(),
                    phase: *p,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorEntry {
    pub path: String,
    pub selector: Selector,
    pub status: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub outcome: String,
    pub source: SourceFingerprint,
    pub entries: Vec<DoctorEntry>,
    pub native_loading: String,
    pub hooks_executed: u32,
    pub p6_complete: bool,
    /// New agy-scope consumer coverage; legacy reports retain their frozen bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muse: Option<MuseDisposition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MuseDisposition {
    pub disposition: String,
    pub canonical_path: String,
    pub handoff_requires_task_state: bool,
    pub native_loading: String,
}
