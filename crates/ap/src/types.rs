use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub schema_version: u32,
    pub canonical_path: String,
    pub codex: CodexProfile,
    pub claude: ClaudeProfile,
    pub preserved: Vec<PreservedFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodexProfile {
    pub config_dir: String,
    pub instructions: Entry,
    pub instruction_override: Entry,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeProfile {
    pub config_dir: String,
    pub instructions: Entry,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Missing,
    File { text: String },
    Symlink { target: String },
    Unsupported { reason: UnsupportedShape },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedShape {
    Directory,
    DanglingSymlink,
    SymlinkChain,
    NonUtf8,
    SpecialFile,
}

/// Opaque UTF-8 caller snapshots: never parsed, interpolated or made into write artifacts.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreservedFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenderPlan {
    pub schema_version: u32,
    pub artifacts: Vec<Artifact>,
    pub preserved: Vec<PreservedFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: String,
    pub selector: Selector,
    pub before: Entry,
    pub after: Entry,
    pub changed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    CodexInstructionLink,
    ClaudeCanonicalImport,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenderError {
    pub code: ErrorCode,
    /// Static contract field selector, never input payload or parser diagnostics.
    pub field: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedSchema,
    InvalidPath,
    PathCollision,
    DuplicatePreservedPath,
    UnsupportedTarget,
    InstructionConflict,
    UnsupportedText,
    RendererUnimplemented,
}
