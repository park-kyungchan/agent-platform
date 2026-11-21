//! `ontology-schema/v1` model, semantic checks, pin and value validation.
//! Pure data: no source is read and no store is touched here.
use crate::{Error, Id, Knowledge, Namespace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const FORMAT: &str = "ontology-schema/v1";

/// Every violation code, from this one list.
pub mod codes {
    pub const INVALID_FORMAT: &str = "invalid_format";
    pub const INVALID_VERSION: &str = "invalid_version";
    pub const INVALID_NAMESPACE: &str = "invalid_namespace";
    pub const INVALID_NAME: &str = "invalid_name";
    pub const RESERVED_DOMAIN: &str = "reserved_domain";
    pub const DUPLICATE_SCOPE_KEY: &str = "duplicate_scope_key";
    pub const DUPLICATE_TYPE_NAME: &str = "duplicate_type_name";
    pub const DUPLICATE_DOMAIN: &str = "duplicate_domain";
    pub const DUPLICATE_LINK_NAME: &str = "duplicate_link_name";
    pub const DUPLICATE_CRITERION_CODE: &str = "duplicate_criterion_code";
    pub const INVALID_VALUE_TYPE_DEFINITION: &str = "invalid_value_type_definition";
    pub const UNKNOWN_SOURCE_KIND: &str = "unknown_source_kind";
    pub const UNKNOWN_SOURCE: &str = "unknown_source";
    pub const UNKNOWN_TYPE: &str = "unknown_type";
    pub const UNKNOWN_PROPERTY: &str = "unknown_property";
    pub const UNKNOWN_VALUE_TYPE: &str = "unknown_value_type";
    pub const UNKNOWN_LINK: &str = "unknown_link";
    pub const UNKNOWN_PARAMETER: &str = "unknown_parameter";
    pub const REFERENCE_READ_FORBIDDEN: &str = "reference_read_forbidden";
    pub const UNKNOWN_ACTION: &str = "unknown_action";
    pub const UNKNOWN_SCOPE_KEY: &str = "unknown_scope_key";
    pub const INVALID_PRINCIPAL: &str = "invalid_principal";
    pub const INVALID_GRANT_SCOPE: &str = "invalid_grant_scope";
    pub const AMBIGUOUS_CREATE_KEY: &str = "ambiguous_create_key";
    pub const INVALID_PRIMARY_KEY: &str = "invalid_primary_key";
    pub const INVALID_FROM_POINTER: &str = "invalid_from_pointer";
    pub const ONTOLOGY_PROPERTY_FROM: &str = "ontology_property_from";
    pub const REQUIRED_PROPERTY_WITHOUT_FROM: &str = "required_property_without_from";
    pub const STRUCT_IN_STRUCT: &str = "struct_in_struct";
    pub const INVALID_LINK_BACKING: &str = "invalid_link_backing";
    pub const INVALID_REFERENCE_PROPERTY: &str = "invalid_reference_property";
    pub const INVALID_CRITERION: &str = "invalid_criterion";
    pub const INVALID_RULE: &str = "invalid_rule";
    pub const ACTION_EDITS_SOURCE_OWNED: &str = "action_edits_source_owned";
    pub const UNKNOWN_FIELD: &str = "unknown_field";
    pub const DUPLICATE_FIELD: &str = "duplicate_field";
    pub const MISSING_FIELD: &str = "missing_field";
    pub const INVALID_SCHEMA: &str = "invalid_schema";
    pub const UNDECLARED_PROPERTY: &str = "undeclared_property";
    pub const MISSING_REQUIRED_PROPERTY: &str = "missing_required_property";
    pub const WRONG_VALUE_TYPE: &str = "wrong_value_type";
    pub const SCOPE_DOMAIN_MISMATCH: &str = "scope_domain_mismatch";
    pub const SCOPE_NAMESPACE_MISMATCH: &str = "scope_namespace_mismatch";
    pub const SCOPE_KEYS_MISMATCH: &str = "scope_keys_mismatch";
    pub const ID_NAMESPACE_MISMATCH: &str = "id_namespace_mismatch";
    pub const REF_NAMESPACE_MISMATCH: &str = "ref_namespace_mismatch";
    pub const INVALID_CHANGE_KIND: &str = "invalid_change_kind";
    pub const REFRESH_FIELDS_DIFFER: &str = "refresh_fields_differ";
    pub const VALUE_CONSTRAINT_VIOLATION: &str = "value_constraint_violation";
    pub const INVALID_SOURCE_OPTION: &str = "invalid_source_option";
    pub const INVALID_GLOB: &str = "invalid_glob";
    pub const INVALID_TRANSFORM: &str = "invalid_transform";
    pub const SOURCE_SCOPE_KEYS_UNMAPPED: &str = "source_scope_keys_unmapped";
    pub const UNKNOWN_SOURCE_FIELD: &str = "unknown_source_field";
    pub const UNKNOWN_TEMPLATE_FIELD: &str = "unknown_template_field";
    pub const INVALID_TEMPLATE: &str = "invalid_template";
    pub const TEMPLATE_FIELD_NOT_STRING: &str = "template_field_not_string";
    pub const REFERENCE_ARRAY_FROM_SOURCE_UNSUPPORTED: &str =
        "reference_array_from_source_unsupported";
    pub const RECORDS_SOURCE_OWNER: &str = "records_source_owner";
    pub const INVALID_FUNCTION: &str = "invalid_function";
    pub const INVALID_RULE_TARGET: &str = "invalid_rule_target";
    pub const AMBIGUOUS_RULE_TARGET: &str = "ambiguous_rule_target";
    pub const INVALID_DOCUMENT: &str = "invalid_document";
    pub const UNKNOWN_FUNCTION: &str = "unknown_function";
    pub const ALL: &[&str] = &[
        INVALID_FORMAT,
        INVALID_VERSION,
        INVALID_NAMESPACE,
        INVALID_NAME,
        RESERVED_DOMAIN,
        DUPLICATE_SCOPE_KEY,
        DUPLICATE_TYPE_NAME,
        DUPLICATE_DOMAIN,
        DUPLICATE_LINK_NAME,
        DUPLICATE_CRITERION_CODE,
        INVALID_VALUE_TYPE_DEFINITION,
        UNKNOWN_SOURCE_KIND,
        UNKNOWN_SOURCE,
        UNKNOWN_TYPE,
        UNKNOWN_PROPERTY,
        UNKNOWN_VALUE_TYPE,
        UNKNOWN_LINK,
        UNKNOWN_PARAMETER,
        REFERENCE_READ_FORBIDDEN,
        UNKNOWN_ACTION,
        UNKNOWN_SCOPE_KEY,
        INVALID_PRINCIPAL,
        INVALID_GRANT_SCOPE,
        INVALID_PRIMARY_KEY,
        AMBIGUOUS_CREATE_KEY,
        INVALID_FROM_POINTER,
        ONTOLOGY_PROPERTY_FROM,
        REQUIRED_PROPERTY_WITHOUT_FROM,
        STRUCT_IN_STRUCT,
        INVALID_LINK_BACKING,
        INVALID_REFERENCE_PROPERTY,
        INVALID_CRITERION,
        INVALID_RULE,
        ACTION_EDITS_SOURCE_OWNED,
        UNKNOWN_FIELD,
        DUPLICATE_FIELD,
        MISSING_FIELD,
        INVALID_SCHEMA,
        UNDECLARED_PROPERTY,
        MISSING_REQUIRED_PROPERTY,
        WRONG_VALUE_TYPE,
        VALUE_CONSTRAINT_VIOLATION,
        SCOPE_DOMAIN_MISMATCH,
        SCOPE_NAMESPACE_MISMATCH,
        SCOPE_KEYS_MISMATCH,
        ID_NAMESPACE_MISMATCH,
        REF_NAMESPACE_MISMATCH,
        INVALID_CHANGE_KIND,
        REFRESH_FIELDS_DIFFER,
        INVALID_SOURCE_OPTION,
        INVALID_GLOB,
        INVALID_TRANSFORM,
        SOURCE_SCOPE_KEYS_UNMAPPED,
        UNKNOWN_SOURCE_FIELD,
        UNKNOWN_TEMPLATE_FIELD,
        INVALID_TEMPLATE,
        TEMPLATE_FIELD_NOT_STRING,
        REFERENCE_ARRAY_FROM_SOURCE_UNSUPPORTED,
        RECORDS_SOURCE_OWNER,
        INVALID_FUNCTION,
        INVALID_RULE_TARGET,
        AMBIGUOUS_RULE_TARGET,
        INVALID_DOCUMENT,
        UNKNOWN_FUNCTION,
    ];
}
/// The violations a schema installed before the rule that raises them may
/// keep: the store still opens it, and the affected action refuses at
/// execution (`specs/ont/action.md` R40 to R43, R110).
fn installed_exempt(code: &str) -> bool {
    code == codes::AMBIGUOUS_CREATE_KEY || code == codes::AMBIGUOUS_RULE_TARGET
}
const SOURCE_KINDS: &[&str] = &["markdown_fields", "file_digest", "records"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub code: &'static str,
    pub pointer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
impl Violation {
    fn new(code: &'static str, pointer: String) -> Self {
        Self {
            code,
            pointer,
            message: None,
        }
    }
    fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}
impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.code, self.pointer)
    }
}
fn esc(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
pub fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarType {
    String,
    Integer,
    Boolean,
    Decimal,
    Timestamp,
    Date,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyType {
    Scalar(ScalarType),
    Ref(RefType),
    Array(ArrayType),
    Struct(StructType),
    ValueType(ValueTypeRef),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefType {
    #[serde(rename = "ref")]
    pub target: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrayType {
    pub array: Box<PropertyType>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructType {
    #[serde(rename = "struct")]
    pub struct_: BTreeMap<String, PropertyType>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueTypeRef {
    pub value_type: String,
}

/// A value transform: a named string transform (`first_word`,
/// `parse_integer`) or a single-key object (`segment_after`,
/// `replace_prefix`). Any other shape is kept as `Invalid` so that
/// `schema check` can refuse it with `invalid_transform`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Transform {
    Named(String),
    SegmentAfter(String),
    ReplacePrefix([String; 2]),
    Invalid(Value),
}
impl Transform {
    /// Whether the declared form is one of the four accepted transforms.
    pub fn form_valid(&self) -> bool {
        match self {
            Self::Named(n) => TRANSFORMS.contains(&n.as_str()),
            Self::SegmentAfter(p) => {
                p.len() <= MAX_TRANSFORM_ARG_BYTES && p.starts_with('/') && p.ends_with('/')
            }
            Self::ReplacePrefix([from, to]) => {
                !from.is_empty()
                    && from.len() <= MAX_TRANSFORM_ARG_BYTES
                    && to.len() <= MAX_TRANSFORM_ARG_BYTES
            }
            Self::Invalid(_) => false,
        }
    }
    /// Whether this transform produces a string (`parse_integer` does not).
    pub fn produces_string(&self) -> bool {
        !matches!(self, Self::Named(n) if n == "parse_integer")
    }
}
impl Serialize for Transform {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Named(n) => s.serialize_str(n),
            Self::SegmentAfter(p) => {
                #[derive(Serialize)]
                struct W<'a> {
                    segment_after: &'a str,
                }
                W { segment_after: p }.serialize(s)
            }
            Self::ReplacePrefix(pair) => {
                #[derive(Serialize)]
                struct W<'a> {
                    replace_prefix: &'a [String; 2],
                }
                W {
                    replace_prefix: pair,
                }
                .serialize(s)
            }
            Self::Invalid(v) => v.serialize(s),
        }
    }
}
impl<'de> Deserialize<'de> for Transform {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(match v {
            Value::String(n) => Self::Named(n),
            Value::Object(ref map) if map.len() == 1 => {
                if let Some(Value::String(p)) = map.get("segment_after") {
                    Self::SegmentAfter(p.clone())
                } else if let Some(Value::Array(items)) = map.get("replace_prefix")
                    && items.len() == 2
                    && items.iter().all(|i| i.is_string())
                {
                    Self::ReplacePrefix([
                        items[0].as_str().unwrap_or_default().to_string(),
                        items[1].as_str().unwrap_or_default().to_string(),
                    ])
                } else {
                    Self::Invalid(v)
                }
            }
            other => Self::Invalid(other),
        })
    }
}

/// Largest argument of a `segment_after` / `replace_prefix` transform.
pub const MAX_TRANSFORM_ARG_BYTES: usize = 1_024;
/// Longest `template` string, in bytes.
pub const MAX_TEMPLATE_BYTES: usize = 256;
/// Most placeholders in one `template`.
pub const MAX_TEMPLATE_PLACEHOLDERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub ty: PropertyType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Alternative to `from`: a string with `{field}` placeholders naming
    /// top-level record fields. Mutually exclusive with `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// Rewrites the value `from` or `template` resolves to. Only on a
    /// property of a source-owned type that has `from` or `template`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
}

/// The only named transforms a mapping may name (applied by the indexer).
pub const TRANSFORMS: &[&str] = &["first_word", "parse_integer"];

/// Placeholders of a template, or `invalid_template` when malformed:
/// an unclosed or stray brace, an empty or nested placeholder, or more
/// than `MAX_TEMPLATE_PLACEHOLDERS` placeholders.
pub fn template_placeholders(template: &str) -> Result<Vec<String>, &'static str> {
    if template.len() > MAX_TEMPLATE_BYTES {
        return Err(codes::INVALID_TEMPLATE);
    }
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        match c {
            '{' => {
                let Some(end) = rest.find('}') else {
                    return Err(codes::INVALID_TEMPLATE);
                };
                let name = &rest[1..end];
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    return Err(codes::INVALID_TEMPLATE);
                }
                names.push(name.to_string());
                if names.len() > MAX_TEMPLATE_PLACEHOLDERS {
                    return Err(codes::INVALID_TEMPLATE);
                }
                rest = &rest[end + 1..];
            }
            '}' => return Err(codes::INVALID_TEMPLATE),
            _ => {
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    Ok(names)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueTypeDef {
    pub base: ScalarType,
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,
    /// Whole-value match: the pattern must match the entire string value;
    /// it is compiled as `^(?:pattern)$` (see `compile_pattern`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Value>,
    /// Compiled `pattern`, filled once by `check` and reused by validation.
    #[serde(skip)]
    compiled_pattern: std::sync::OnceLock<Option<regex_lite::Regex>>,
}
impl PartialEq for ValueTypeDef {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.enum_values == other.enum_values
            && self.pattern == other.pattern
            && self.min == other.min
            && self.max == other.max
    }
}
impl ValueTypeDef {
    fn compiled(&self) -> Option<&regex_lite::Regex> {
        self.compiled_pattern
            .get_or_init(|| {
                self.pattern.as_deref().and_then(|p| {
                    if p.len() > MAX_PATTERN_BYTES {
                        None
                    } else {
                        compile_pattern(p).ok()
                    }
                })
            })
            .as_ref()
    }
    fn pattern_matches(&self, text: &str) -> bool {
        self.pattern.is_none() || self.compiled().is_some_and(|re| re.is_match(text))
    }
}

/// Maximum `pattern` length in bytes; a longer pattern is refused with
/// `invalid_value_type_definition`.
const MAX_PATTERN_BYTES: usize = 1_024;

/// Compile a value-type `pattern` for whole-value matching: the pattern
/// must match the whole value, so it is wrapped as `^(?:pattern)$`. The
/// bare pattern must compile on its own first, so a pattern that compiles
/// only inside the wrapper (for example `a)|(b`) is refused. Matching a
/// value is linear in the value length; a value's length is bounded by the
/// property's `max_bytes` when declared and otherwise by the store's
/// limits (packet O2).
fn compile_pattern(pattern: &str) -> Result<regex_lite::Regex, regex_lite::Error> {
    regex_lite::Regex::new(pattern)?;
    regex_lite::Regex::new(&format!("^(?:{pattern})$"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDef {
    pub kind: String,
    #[serde(flatten)]
    pub options: BTreeMap<String, Value>,
}

/// One token of a glob segment pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobToken {
    Char(char),
    /// `?`: one character inside a segment.
    One,
    /// `*`: any run of characters inside a segment.
    Many,
}
/// One segment of an absolute glob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobSegment {
    /// A name without wildcards.
    Literal(String),
    /// A segment with `*` or `?`.
    Pattern(Vec<GlobToken>),
    /// `**`: any number of whole segments (zero or more).
    AnyDirs,
}
impl GlobSegment {
    /// Whether a single path component matches (`AnyDirs` matches any).
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Self::Literal(l) => l == name,
            Self::AnyDirs => true,
            Self::Pattern(tokens) => {
                let chars: Vec<char> = name.chars().collect();
                tokens_match(tokens, &chars)
            }
        }
    }
}
fn tokens_match(tokens: &[GlobToken], name: &[char]) -> bool {
    // Iterative wildcard match with one backtrack point.
    let (mut t, mut n) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match tokens.get(t) {
            Some(GlobToken::Many) => {
                star = Some((t, n));
                t += 1;
            }
            Some(GlobToken::One) => {
                t += 1;
                n += 1;
            }
            Some(GlobToken::Char(c)) if *c == name[n] => {
                t += 1;
                n += 1;
            }
            _ => match star {
                Some((st, sn)) => {
                    t = st + 1;
                    n = sn + 1;
                    star = Some((st, sn + 1));
                }
                None => return false,
            },
        }
    }
    while matches!(tokens.get(t), Some(GlobToken::Many)) {
        t += 1;
    }
    t == tokens.len()
}

/// An absolute glob: `*` and `?` inside one path segment, `**` as a whole
/// segment for any number of directories. No brace, class or escape syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    text: String,
    segments: Vec<GlobSegment>,
}
/// Longest glob text, in bytes.
pub const MAX_GLOB_BYTES: usize = 4_096;
/// Most `**` segments in one glob.
pub const MAX_GLOB_ANY_DIRS: usize = 8;
impl Glob {
    /// Refuses a relative glob, an empty segment, `.` or `..`, a NUL, any of
    /// `{ } [ ] \`, `**` mixed with other characters, and a final `**`.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        if text.len() > MAX_GLOB_BYTES || !text.starts_with('/') || text.contains('\0') {
            return Err("invalid_glob");
        }
        let mut segments = Vec::new();
        for part in text[1..].split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err("invalid_glob");
            }
            if part.contains(['{', '}', '[', ']', '\\']) {
                return Err("invalid_glob");
            }
            if part.contains("**") {
                if part != "**" {
                    return Err("invalid_glob");
                }
                segments.push(GlobSegment::AnyDirs);
            } else if part.contains(['*', '?']) {
                segments.push(GlobSegment::Pattern(
                    part.chars()
                        .map(|c| match c {
                            '*' => GlobToken::Many,
                            '?' => GlobToken::One,
                            c => GlobToken::Char(c),
                        })
                        .collect(),
                ));
            } else {
                segments.push(GlobSegment::Literal(part.to_string()));
            }
        }
        if matches!(segments.last(), Some(GlobSegment::AnyDirs))
            || segments
                .iter()
                .filter(|s| matches!(s, GlobSegment::AnyDirs))
                .count()
                > MAX_GLOB_ANY_DIRS
        {
            return Err("invalid_glob");
        }
        Ok(Self {
            text: text.to_string(),
            segments,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub fn segments(&self) -> &[GlobSegment] {
        &self.segments
    }
    /// Whether an absolute path matches the whole glob.
    pub fn matches(&self, path: &str) -> bool {
        let Some(rest) = path.strip_prefix('/') else {
            return false;
        };
        let parts: Vec<&str> = rest.split('/').collect();
        segments_match(&self.segments, &parts)
    }
}
/// Time proportional to segments times parts: on a mismatch the matcher
/// backs up only to the most recent `**`.
fn segments_match(segments: &[GlobSegment], parts: &[&str]) -> bool {
    let (mut i, mut j) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while j < parts.len() {
        match segments.get(i) {
            Some(GlobSegment::AnyDirs) => {
                star = Some((i, j));
                i += 1;
                continue;
            }
            Some(seg) if seg.matches(parts[j]) => {
                i += 1;
                j += 1;
                continue;
            }
            _ => {}
        }
        match star {
            Some((si, sj)) => {
                i = si + 1;
                j = sj + 1;
                star = Some((si, sj + 1));
            }
            None => return false,
        }
    }
    segments[i..]
        .iter()
        .all(|s| matches!(s, GlobSegment::AnyDirs))
}

/// Which compiled file source a `SourceDef` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSourceKind {
    MarkdownFields,
    FileDigest,
    /// Every line of every matched file is one JSON record that carries
    /// its own time (see `RecordsOptions`).
    Records,
}
/// The parsed options of a `markdown_fields` or `file_digest` source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSourceSpec {
    pub kind: FileSourceKind,
    /// `glob` (one string or 1 to 16 globs) for `markdown_fields`;
    /// `paths` for `file_digest`.
    pub globs: Vec<Glob>,
    pub exclude: Vec<Glob>,
    /// Lines parsed for `fields`; `markdown_fields` only (default 40).
    pub head_lines: usize,
    pub complete_listing: bool,
    pub fresh_for_s: u64,
}
/// The options only a `records` source has: the top-level member that holds
/// the record's time (whole seconds since 1970) and, optionally, the member
/// that must name the owning object type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordsOptions {
    pub time_member: String,
    pub type_member: Option<String>,
}

/// An option that is missing, mistyped or out of range: the refusal code
/// and the JSON pointer below the source (`/glob`, `/paths/2`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOptionError {
    pub code: &'static str,
    pub pointer: String,
}
const MAX_HEAD_LINES: u64 = 10_000;
const MAX_FRESH_FOR_S: u64 = 315_360_000;
const MAX_GLOB_LIST: usize = 256;
/// Most globs in the array form of a `markdown_fields` `glob` option.
pub const MAX_SOURCE_GLOBS: usize = 16;

impl FileSourceKind {
    /// The record fields (first pointer segments) this kind produces.
    pub fn record_fields(self) -> &'static [&'static str] {
        match self {
            Self::MarkdownFields => &[
                "path", "dir", "dir_path", "stem", "stem_id", "sha256", "size", "mtime", "line1",
                "fields",
            ],
            Self::FileDigest => &["path", "sha256", "size", "mtime", "mode"],
            // The members of a record are the file's own, not a fixed list.
            Self::Records => &[],
        }
    }
    /// Whether `record_fields()` is the complete list of members a mapping
    /// may point into.
    pub fn has_fixed_fields(self) -> bool {
        !matches!(self, Self::Records)
    }
    /// The record fields of `record_fields()` whose value is a string for
    /// this kind; the only fields a template placeholder may name.
    pub fn string_record_fields(self) -> &'static [&'static str] {
        match self {
            Self::MarkdownFields => &[
                "path", "dir", "dir_path", "stem", "stem_id", "sha256", "line1",
            ],
            Self::FileDigest => &["path", "sha256"],
            Self::Records => &[],
        }
    }
}

/// A member name of a `records` option: 1 to 128 bytes, no NUL.
const MAX_MEMBER_NAME: usize = 128;

impl SourceDef {
    /// The options of a `records` source; `Ok(None)` for any other kind.
    pub fn records_options(&self) -> Result<Option<RecordsOptions>, SourceOptionError> {
        if self.kind != "records" {
            return Ok(None);
        }
        let member = |key: &str| -> Result<Option<String>, SourceOptionError> {
            match self.options.get(key) {
                None => Ok(None),
                Some(Value::String(s))
                    if !s.is_empty() && s.len() <= MAX_MEMBER_NAME && !s.contains('\0') =>
                {
                    Ok(Some(s.clone()))
                }
                Some(_) => Err(SourceOptionError {
                    code: codes::INVALID_SOURCE_OPTION,
                    pointer: format!("/{key}"),
                }),
            }
        };
        let Some(time_member) = member("time_member")? else {
            return Err(SourceOptionError {
                code: codes::INVALID_SOURCE_OPTION,
                pointer: "/time_member".to_string(),
            });
        };
        let type_member = member("type_member")?;
        if type_member.as_deref() == Some(time_member.as_str()) {
            return Err(SourceOptionError {
                code: codes::INVALID_SOURCE_OPTION,
                pointer: "/type_member".to_string(),
            });
        }
        Ok(Some(RecordsOptions {
            time_member,
            type_member,
        }))
    }

    /// `Ok(None)` for a kind this engine does not know.
    pub fn file_spec(&self) -> Result<Option<FileSourceSpec>, SourceOptionError> {
        let kind = match self.kind.as_str() {
            "markdown_fields" => FileSourceKind::MarkdownFields,
            "file_digest" => FileSourceKind::FileDigest,
            "records" => FileSourceKind::Records,
            _ => return Ok(None),
        };
        let err = |code: &'static str, pointer: &str| SourceOptionError {
            code,
            pointer: pointer.to_string(),
        };
        let allowed: &[&str] = match kind {
            FileSourceKind::MarkdownFields => &[
                "glob",
                "exclude",
                "head_lines",
                "complete_listing",
                "fresh_for_s",
            ],
            FileSourceKind::FileDigest => &["paths", "exclude", "complete_listing", "fresh_for_s"],
            FileSourceKind::Records => &[
                "glob",
                "exclude",
                "complete_listing",
                "fresh_for_s",
                "time_member",
                "type_member",
            ],
        };
        if let Some(key) = self.options.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(err(codes::INVALID_SOURCE_OPTION, &format!("/{}", esc(key))));
        }
        let glob_list =
            |value: &Value, key: &str, min: usize| -> Result<Vec<Glob>, SourceOptionError> {
                let Some(items) = value.as_array() else {
                    return Err(err(codes::INVALID_SOURCE_OPTION, &format!("/{key}")));
                };
                if items.len() < min || items.len() > MAX_GLOB_LIST {
                    return Err(err(codes::INVALID_SOURCE_OPTION, &format!("/{key}")));
                }
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let pointer = format!("/{key}/{i}");
                    let Some(text) = item.as_str() else {
                        return Err(err(codes::INVALID_SOURCE_OPTION, &pointer));
                    };
                    out.push(Glob::parse(text).map_err(|_| err(codes::INVALID_GLOB, &pointer))?);
                }
                Ok(out)
            };
        if kind == FileSourceKind::Records {
            self.records_options()?;
        }
        let globs = match kind {
            FileSourceKind::MarkdownFields | FileSourceKind::Records => {
                match self.options.get("glob") {
                    Some(Value::String(text)) => {
                        vec![Glob::parse(text).map_err(|_| err(codes::INVALID_GLOB, "/glob"))?]
                    }
                    Some(Value::Array(items)) => {
                        if items.is_empty() || items.len() > MAX_SOURCE_GLOBS {
                            return Err(err(codes::INVALID_SOURCE_OPTION, "/glob"));
                        }
                        let mut out = Vec::new();
                        for (i, item) in items.iter().enumerate() {
                            let pointer = format!("/glob/{i}");
                            let Some(text) = item.as_str() else {
                                return Err(err(codes::INVALID_SOURCE_OPTION, &pointer));
                            };
                            out.push(
                                Glob::parse(text)
                                    .map_err(|_| err(codes::INVALID_GLOB, &pointer))?,
                            );
                        }
                        out
                    }
                    _ => return Err(err(codes::INVALID_SOURCE_OPTION, "/glob")),
                }
            }
            FileSourceKind::FileDigest => match self.options.get("paths") {
                Some(v) => glob_list(v, "paths", 1)?,
                None => return Err(err(codes::INVALID_SOURCE_OPTION, "/paths")),
            },
        };
        let exclude = match self.options.get("exclude") {
            Some(v) => glob_list(v, "exclude", 0)?,
            None => Vec::new(),
        };
        let head_lines = match self.options.get("head_lines") {
            None => 40,
            Some(v) => match v.as_u64() {
                Some(n) if (1..=MAX_HEAD_LINES).contains(&n) => n as usize,
                _ => return Err(err(codes::INVALID_SOURCE_OPTION, "/head_lines")),
            },
        };
        let complete_listing = match self.options.get("complete_listing") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(err(codes::INVALID_SOURCE_OPTION, "/complete_listing")),
        };
        let fresh_for_s = match self.options.get("fresh_for_s").and_then(Value::as_u64) {
            Some(n) if (1..=MAX_FRESH_FOR_S).contains(&n) => n,
            _ => return Err(err(codes::INVALID_SOURCE_OPTION, "/fresh_for_s")),
        };
        Ok(Some(FileSourceSpec {
            kind,
            globs,
            exclude,
            head_lines,
            complete_listing,
            fresh_for_s,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Ontology,
    Source(String),
}
impl Serialize for Owner {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Ontology => s.serialize_str("ontology"),
            Self::Source(name) => {
                #[derive(Serialize)]
                struct W<'a> {
                    source: &'a str,
                }
                W { source: name }.serialize(s)
            }
        }
    }
}
impl<'de> Deserialize<'de> for Owner {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct OwnerMap {
            source: String,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum W {
            Text(String),
            Map(OwnerMap),
        }
        match W::deserialize(d)? {
            W::Text(t) if t == "ontology" => Ok(Self::Ontology),
            W::Text(t) => Err(serde::de::Error::custom(format!(
                "invalid owner {t:?}, expected \"ontology\""
            ))),
            W::Map(m) => Ok(Self::Source(m.source)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryKey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Alternative to `from`; mutually exclusive with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// A string-producing transform only (a key is a string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub owner: Owner,
    pub primary_key: PrimaryKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropertyDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    ManyToOne,
    ManyToMany,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkNames {
    pub forward: String,
    pub reverse: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkType {
    pub from: String,
    pub via: String,
    pub to: String,
    pub cardinality: Cardinality,
    pub names: LinkNames,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub none: Option<Vec<Condition>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Present(String),
    Exists(String),
    Fresh(String),
    Eq([Value; 2]),
    Ne([Value; 2]),
    In([Value; 2]),
    Lt([Value; 2]),
    Le([Value; 2]),
    Gt([Value; 2]),
    Ge([Value; 2]),
    Linked(LinkedConditionWire),
    Actor(Value),
    All(Vec<Condition>),
    Any(Vec<Condition>),
    #[serde(rename = "none")]
    NoneOf(Vec<Condition>),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedConditionWire {
    pub from: String,
    pub link: String,
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<ConditionGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub none: Option<Vec<Condition>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRule {
    pub object_type: String,
    pub property: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modify_object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_or_modify_object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_reference: Option<ReferenceRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_reference: Option<ReferenceRule>,
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// `$<parameter>`: the reference parameter whose object the rule edits
    /// (`specs/ont/action.md` R108).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}
impl ActionRule {
    /// The object type a rule of `specs/ont/action.md` R108 edits through one
    /// parameter: `modify_object`, `delete_object`, `set_reference`,
    /// `clear_reference`, and `create_or_modify_object` of a type whose key
    /// is not a pointer (`$action_id`). `None` for every other rule.
    pub fn targeted_type<'a>(&'a self, schema: &Schema) -> Option<&'a str> {
        if let Some(t) = &self.create_or_modify_object {
            let pointer_key = schema
                .object_types
                .get(t)
                .and_then(|ty| ty.primary_key.from.as_deref())
                .is_some_and(|p| p.starts_with('/'));
            return (!pointer_key).then_some(t.as_str());
        }
        self.modify_object
            .as_deref()
            .or(self.delete_object.as_deref())
            .or(self.set_reference.as_ref().map(|r| r.object_type.as_str()))
            .or(self
                .clear_reference
                .as_ref()
                .map(|r| r.object_type.as_str()))
    }
    fn object_target(&self) -> Option<(&'static str, &str)> {
        [
            ("create_object", &self.create_object),
            ("modify_object", &self.modify_object),
            ("create_or_modify_object", &self.create_or_modify_object),
            ("delete_object", &self.delete_object),
        ]
        .into_iter()
        .find_map(|(f, v)| v.as_deref().map(|x| (f, x)))
    }
    fn kind_count(&self) -> usize {
        [
            self.create_object.is_some(),
            self.modify_object.is_some(),
            self.create_or_modify_object.is_some(),
            self.delete_object.is_some(),
            self.set_reference.is_some(),
            self.clear_reference.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, PropertyDef>,
    #[serde(default)]
    pub submission_criteria: Vec<Criterion>,
    #[serde(default)]
    pub rules: Vec<ActionRule>,
}

impl ActionType {
    /// The declared reference parameters to `object_type`, in name order
    /// (an array of references is none).
    pub fn reference_parameters<'a>(&'a self, object_type: &str) -> Vec<&'a str> {
        self.parameters
            .iter()
            .filter(|(_, p)| matches!(&p.ty, PropertyType::Ref(r) if r.target == object_type))
            .map(|(name, _)| name.as_str())
            .collect()
    }
    /// The parameter whose object a rule of `specs/ont/action.md` R108 edits:
    /// the one `target` names, or else the only declared reference parameter
    /// to `object_type`. `Err(())` when the rule names no target and the
    /// action declares more than one such parameter (`ambiguous_rule_target`);
    /// `Ok(None)` when it declares none. Neither a name nor the order of names
    /// chooses.
    #[allow(clippy::result_unit_err)]
    pub fn rule_target_parameter<'a>(
        &'a self,
        rule: &'a ActionRule,
        object_type: &str,
    ) -> Result<Option<&'a str>, ()> {
        if let Some(t) = &rule.target {
            return Ok(Some(t.strip_prefix('$').unwrap_or(t)));
        }
        match self.reference_parameters(object_type).as_slice() {
            [] => Ok(None),
            [one] => Ok(Some(one)),
            _ => Err(()),
        }
    }
    /// A pointer key comes from exactly one required reference to its created
    /// type, and one create/upsert rule for that type. Never use value/name order.
    pub fn create_key_parameter(&self, object_type: &str, pointer: &str) -> Option<&str> {
        if pointer != "/native_key"
            || self
                .rules
                .iter()
                .filter(|r| {
                    r.create_object.as_deref() == Some(object_type)
                        || r.create_or_modify_object.as_deref() == Some(object_type)
                })
                .count()
                != 1
        {
            return None;
        }
        let mut candidates = self.parameters.iter().filter(|(_, p)| {
            p.required && matches!(&p.ty, PropertyType::Ref(r) if r.target == object_type)
        });
        let (name, _) = candidates.next()?;
        candidates.next().is_none().then_some(name.as_str())
    }
}

/// Most bytes a function's answer may declare (`answer_bytes`).
pub const MAX_ANSWER_BYTES: u64 = 1_048_576;
/// Longest time limit a function may declare (`timeout_ms`).
pub const MAX_TIMEOUT_MS: u64 = 600_000;
/// Longest `program` path, in bytes.
pub const MAX_PROGRAM_BYTES: usize = 4096;

/// A function a domain declares (`specs/ont/fn.md`): its signature and the
/// program the engine starts. It is read from any JSON value, so that
/// [`check`] reports each malformed member as one `invalid_function`
/// diagnostic instead of one parse error; a member at fault keeps its
/// default here and is listed in the faults that `check` reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunctionDef {
    pub description: Option<String>,
    pub parameters: BTreeMap<String, FunctionParameter>,
    /// The absolute path of the program.
    pub program: String,
    /// The bound on the bytes of the program's standard output.
    pub answer_bytes: u64,
    /// The time limit of one call, in milliseconds.
    pub timeout_ms: u64,
    /// JSON pointers, relative to the entry, of the members at fault.
    faults: Vec<String>,
}

/// One declared parameter of a function: a type as an action parameter
/// declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameter {
    pub description: Option<String>,
    pub ty: PropertyType,
    pub required: bool,
}

const FUNCTION_MEMBERS: &[&str] = &[
    "description",
    "parameters",
    "program",
    "answer_bytes",
    "timeout_ms",
];
const FUNCTION_PARAMETER_MEMBERS: &[&str] = &["type", "required", "description"];

fn description_member(value: Option<&Value>) -> Result<Option<String>, ()> {
    match value {
        None => Ok(None),
        Some(Value::String(s)) if s.len() <= 300 => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

fn bounded_member(value: Option<&Value>, max: u64) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .filter(|n| (1..=max).contains(n))
}

/// An absolute path of at most [`MAX_PROGRAM_BYTES`] bytes without NUL.
pub fn valid_program(path: &str) -> bool {
    path.starts_with('/') && path.len() <= MAX_PROGRAM_BYTES && !path.contains('\0')
}

impl FunctionDef {
    /// Read an entry of `functions`, noting each member at fault.
    pub fn from_value(value: &Value) -> Self {
        let mut def = Self::default();
        let Some(entry) = value.as_object() else {
            def.faults.push(String::new());
            return def;
        };
        for key in entry.keys() {
            if !FUNCTION_MEMBERS.contains(&key.as_str()) {
                def.faults.push(format!("/{}", esc(key)));
            }
        }
        match description_member(entry.get("description")) {
            Ok(d) => def.description = d,
            Err(()) => def.faults.push("/description".into()),
        }
        match entry.get("parameters") {
            Some(Value::Object(parameters)) => {
                for (name, declared) in parameters {
                    let at = format!("/parameters/{}", esc(name));
                    if !valid_name(name) || name == "now" {
                        def.faults.push(at.clone());
                    }
                    if let Some(p) = Self::parameter(declared, &at, &mut def.faults) {
                        def.parameters.insert(name.clone(), p);
                    }
                }
            }
            _ => def.faults.push("/parameters".into()),
        }
        match entry.get("program") {
            Some(Value::String(p)) if valid_program(p) => def.program = p.clone(),
            _ => def.faults.push("/program".into()),
        }
        match bounded_member(entry.get("answer_bytes"), MAX_ANSWER_BYTES) {
            Some(n) => def.answer_bytes = n,
            None => def.faults.push("/answer_bytes".into()),
        }
        match bounded_member(entry.get("timeout_ms"), MAX_TIMEOUT_MS) {
            Some(n) => def.timeout_ms = n,
            None => def.faults.push("/timeout_ms".into()),
        }
        def
    }

    fn parameter(value: &Value, at: &str, faults: &mut Vec<String>) -> Option<FunctionParameter> {
        let Some(declared) = value.as_object() else {
            faults.push(at.to_string());
            return None;
        };
        for key in declared.keys() {
            if !FUNCTION_PARAMETER_MEMBERS.contains(&key.as_str()) {
                faults.push(format!("{at}/{}", esc(key)));
            }
        }
        let description = description_member(declared.get("description"))
            .map_err(|()| faults.push(format!("{at}/description")))
            .ok()
            .flatten();
        let ty = declared
            .get("type")
            .and_then(|t| serde_json::from_value::<PropertyType>(t.clone()).ok());
        if ty.is_none() {
            faults.push(format!("{at}/type"));
        }
        let required = declared.get("required").and_then(Value::as_bool);
        if required.is_none() {
            faults.push(format!("{at}/required"));
        }
        // A parsed type is kept even when `required` is at fault, so that
        // `check` still reports a fault of the type: one diagnostic per
        // fault. A fault recorded here refuses the schema either way.
        Some(FunctionParameter {
            description,
            ty: ty?,
            required: required.unwrap_or(false),
        })
    }

    /// The pointers, relative to the entry, of the members at fault.
    pub fn faults(&self) -> &[String] {
        &self.faults
    }

    /// The declared form of this entry (members at fault left out).
    pub fn to_value(&self) -> Value {
        let mut entry = serde_json::Map::new();
        if let Some(d) = &self.description {
            entry.insert("description".into(), Value::String(d.clone()));
        }
        let parameters: serde_json::Map<String, Value> = self
            .parameters
            .iter()
            .map(|(name, p)| {
                let mut declared = serde_json::Map::new();
                declared.insert(
                    "type".into(),
                    serde_json::to_value(&p.ty).unwrap_or(Value::Null),
                );
                declared.insert("required".into(), Value::Bool(p.required));
                if let Some(d) = &p.description {
                    declared.insert("description".into(), Value::String(d.clone()));
                }
                (name.clone(), Value::Object(declared))
            })
            .collect();
        entry.insert("parameters".into(), Value::Object(parameters));
        entry.insert("program".into(), Value::String(self.program.clone()));
        entry.insert("answer_bytes".into(), Value::from(self.answer_bytes));
        entry.insert("timeout_ms".into(), Value::from(self.timeout_ms));
        Value::Object(entry)
    }
}

impl<'de> Deserialize<'de> for FunctionDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_value(&Value::deserialize(deserializer)?))
    }
}

impl Serialize for FunctionDef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaGrant {
    pub principal: String,
    #[serde(default)]
    pub scope: BTreeMap<String, String>,
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub derive: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    /// The role whose calls the grant is of; without it, a grant of every
    /// call (`specs/ont/schema-language.md` R28, R29).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The functions of the schema the grant lets a call run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
}

/// A deterministic generated table over one type in this domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDef {
    pub object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(default)]
    pub args: Vec<String>,
    pub order: Vec<ViewOrder>,
    pub columns: Vec<ViewColumn>,
    #[serde(default = "view_limit")]
    pub limit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
fn view_limit() -> u64 {
    200
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewOrder {
    pub property: String,
    pub direction: ViewDirection,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewDirection {
    Asc,
    Desc,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewColumn {
    pub header: String,
    pub property: String,
}

/// One entry of a schema's `views` map: a table view over one object type,
/// or a document of ordered document blocks (`specs/ont/render.md` R1, R51).
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Table(ViewDef),
    Document(DocumentDef),
}
impl View {
    /// The arguments the view's filters may use.
    pub fn args(&self) -> &[String] {
        match self {
            View::Table(v) => &v.args,
            View::Document(d) => &d.args,
        }
    }
}

/// A document: 1 to 20 document blocks, each a heading and a table or a
/// list over one object type; the arguments are declared once for all.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentDef {
    pub blocks: Vec<DocumentBlock>,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One part of a document: `{"heading", "table"}` or `{"heading", "list"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentBlock {
    pub heading: String,
    pub body: BlockBody,
}
#[derive(Debug, Clone, PartialEq)]
pub enum BlockBody {
    Table(BlockTable),
    List(BlockList),
}
impl BlockBody {
    pub fn object_type(&self) -> &str {
        match self {
            BlockBody::Table(t) => &t.object_type,
            BlockBody::List(l) => &l.object_type,
        }
    }
    pub fn filter(&self) -> Option<&Value> {
        match self {
            BlockBody::Table(t) => t.filter.as_ref(),
            BlockBody::List(l) => l.filter.as_ref(),
        }
    }
}

/// A table of a document block; members as in a table view, without
/// `args` and `description`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockTable {
    pub object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    pub order: Vec<ViewOrder>,
    pub columns: Vec<ViewColumn>,
    #[serde(default = "view_limit")]
    pub limit: u64,
}

/// A list of a document block: one line per object, the cell texts of
/// `properties` joined by `: ` (`specs/ont/render.md` R54).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockList {
    pub object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    pub order: Vec<ViewOrder>,
    pub properties: Vec<String>,
    #[serde(default = "view_limit")]
    pub limit: u64,
}

/// A present member never takes `null` for "absent": the error stays the
/// one the member's own type gives.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// Every member either form of a view may hold; `blocks` chooses the form.
/// `filter` keeps a written `null` apart from an absent member: a table view
/// reads `null` as no filter, a document refuses the member in any form.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename = "ViewDef")]
struct ViewMembers {
    #[serde(default, deserialize_with = "present")]
    object_type: Option<String>,
    #[serde(default, deserialize_with = "present")]
    filter: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    args: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    order: Option<Vec<ViewOrder>>,
    #[serde(default, deserialize_with = "present")]
    columns: Option<Vec<ViewColumn>>,
    #[serde(default, deserialize_with = "present")]
    limit: Option<u64>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, deserialize_with = "present")]
    blocks: Option<Vec<DocumentBlock>>,
}
const DOCUMENT_MEMBERS: &[&str] = &["blocks", "args", "description"];

const VIEW_MEMBERS: &[&str] = &[
    "object_type",
    "filter",
    "args",
    "order",
    "columns",
    "limit",
    "description",
    "blocks",
];
fn view_from_members<E: serde::de::Error>(m: ViewMembers) -> Result<View, E> {
    let Some(blocks) = m.blocks else {
        // A table view: the members and the order of missing-member errors
        // of the derived form.
        return Ok(View::Table(ViewDef {
            object_type: m
                .object_type
                .ok_or_else(|| E::missing_field("object_type"))?,
            filter: m.filter.flatten(),
            args: m.args.unwrap_or_default(),
            order: m.order.ok_or_else(|| E::missing_field("order"))?,
            columns: m.columns.ok_or_else(|| E::missing_field("columns"))?,
            limit: m.limit.unwrap_or_else(view_limit),
            description: m.description,
        }));
    };
    for (member, written) in [
        ("object_type", m.object_type.is_some()),
        ("filter", m.filter.is_some()),
        ("order", m.order.is_some()),
        ("columns", m.columns.is_some()),
        ("limit", m.limit.is_some()),
    ] {
        if written {
            return Err(E::unknown_field(member, DOCUMENT_MEMBERS));
        }
    }
    Ok(View::Document(DocumentDef {
        blocks,
        args: m.args.unwrap_or_default(),
        description: m.description,
    }))
}
/// A map is a table view or a document; any other input is judged by the
/// derived `ViewDef`, so its answers (a sequence form among them) stay those
/// of a table view.
impl<'de> Deserialize<'de> for View {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ViewVisitor;
        impl<'de> serde::de::Visitor<'de> for ViewVisitor {
            type Value = View;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("struct ViewDef")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<View, A::Error> {
                ViewDef::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
                    .map(View::Table)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<View, A::Error> {
                view_from_members(ViewMembers::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?)
            }
        }
        deserializer.deserialize_struct("ViewDef", VIEW_MEMBERS, ViewVisitor)
    }
}
impl Serialize for View {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            View::Table(v) => v.serialize(serializer),
            View::Document(d) => d.serialize(serializer),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BlockMembers {
    heading: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    table: Option<BlockTable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    list: Option<BlockList>,
}
impl<'de> Deserialize<'de> for DocumentBlock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let m = BlockMembers::deserialize(deserializer)?;
        let body = match (m.table, m.list) {
            (Some(t), None) => BlockBody::Table(t),
            (None, Some(l)) => BlockBody::List(l),
            (Some(_), Some(_)) => {
                return Err(D::Error::unknown_field("list", &["heading", "table"]));
            }
            (None, None) => return Err(D::Error::missing_field("table")),
        };
        Ok(DocumentBlock {
            heading: m.heading,
            body,
        })
    }
}
impl Serialize for DocumentBlock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (table, list) = match &self.body {
            BlockBody::Table(t) => (Some(t.clone()), None),
            BlockBody::List(l) => (None, Some(l.clone())),
        };
        BlockMembers {
            heading: self.heading.clone(),
            table,
            list,
        }
        .serialize(serializer)
    }
}

/// A schema's `views` map. Iterating `&Views` yields the table views only,
/// each with its one object type; a document has none, so readers that list
/// views by type (`describe`) do not see it. `get`, `keys` and `all` reach
/// every entry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Views(BTreeMap<String, View>);
type TableEntry<'a> = (&'a String, &'a ViewDef);
fn table_entry<'a>((name, view): (&'a String, &'a View)) -> Option<TableEntry<'a>> {
    match view {
        View::Table(v) => Some((name, v)),
        View::Document(_) => None,
    }
}
impl Views {
    pub fn get(&self, name: &str) -> Option<&View> {
        self.0.get(name)
    }
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
    /// The names of every entry, table views and documents.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }
    /// Every entry, table views and documents, by name.
    pub fn all(&self) -> std::collections::btree_map::Iter<'_, String, View> {
        self.0.iter()
    }
    /// The table views, by name.
    pub fn tables(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
impl From<BTreeMap<String, View>> for Views {
    fn from(map: BTreeMap<String, View>) -> Self {
        Self(map)
    }
}
impl<'a> IntoIterator for &'a Views {
    type Item = TableEntry<'a>;
    type IntoIter = std::iter::FilterMap<
        std::collections::btree_map::Iter<'a, String, View>,
        fn((&'a String, &'a View)) -> Option<TableEntry<'a>>,
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0
            .iter()
            .filter_map(table_entry as fn((&'a String, &'a View)) -> Option<TableEntry<'a>>)
    }
}

/// Validate the search filter grammar without depending on the CLI crate.
fn check_view_filter(
    f: &Value,
    args: &[String],
    schema: &Schema,
    props: &BTreeMap<String, PropertyDef>,
    depth: usize,
    leaves: &mut usize,
) -> bool {
    let Some(map) = f.as_object().filter(|m| m.len() == 1) else {
        return false;
    };
    if depth > 8 {
        return false;
    }
    let Some((op, arg)) = map.iter().next() else {
        return false;
    };
    if matches!(op.as_str(), "all" | "any" | "none") {
        return arg.as_array().is_some_and(|a| {
            a.iter()
                .all(|f| check_view_filter(f, args, schema, props, depth + 1, leaves))
        });
    }
    *leaves += 1;
    if *leaves > 64 {
        return false;
    }
    if op == "unresolved" {
        return arg.as_str().is_some_and(|p| props.contains_key(p));
    }
    if !matches!(
        op.as_str(),
        "eq" | "ne" | "in" | "lt" | "le" | "gt" | "ge" | "prefix"
    ) {
        return false;
    }
    let Some(a) = arg.as_array().filter(|a| a.len() == 2) else {
        return false;
    };
    let Some(def) = a[0].as_str().and_then(|p| props.get(p)) else {
        return false;
    };
    let base = match &def.ty {
        PropertyType::Scalar(s) => Some(*s),
        PropertyType::ValueType(r) => schema.value_types.get(&r.value_type).map(|d| d.base),
        _ => None,
    };
    if op == "prefix" && base != Some(ScalarType::String) {
        return false;
    }
    if matches!(op.as_str(), "lt" | "le" | "gt" | "ge")
        && !matches!(
            base,
            Some(
                ScalarType::String | ScalarType::Integer | ScalarType::Timestamp | ScalarType::Date
            )
        )
    {
        return false;
    }
    fn declared(v: &Value, args: &[String]) -> bool {
        match v {
            Value::String(s) => s
                .strip_prefix('$')
                .is_none_or(|n| args.iter().any(|a| a == n)),
            Value::Array(a) => a.iter().all(|v| declared(v, args)),
            Value::Object(m) => m.values().all(|v| declared(v, args)),
            _ => true,
        }
    }
    if op == "in" && a[1].as_array().is_none_or(|a| a.len() > 100) {
        return false;
    }
    if !declared(&a[1], args) {
        return false;
    }
    fn has_arg(v: &Value) -> bool {
        match v {
            Value::String(s) => s.starts_with('$'),
            Value::Array(a) => a.iter().any(has_arg),
            Value::Object(m) => m.values().any(has_arg),
            _ => false,
        }
    }
    let valid = |v: &Value| {
        if has_arg(v) {
            (base == Some(ScalarType::String)
                || (op == "eq" && matches!(def.ty, PropertyType::Ref(_))))
                && v.is_string()
        } else {
            validate_value(schema, &def.ty, v) != Err(codes::WRONG_VALUE_TYPE)
        }
    };
    if op == "in" {
        a[1].as_array().is_some_and(|a| a.iter().all(valid))
    } else {
        valid(&a[1])
    }
}
/// The members a table view and a document block share: the selection of
/// one object type, its order and limit, and what is printed of each object.
struct Selection<'a> {
    object_type: &'a str,
    filter: Option<&'a Value>,
    order: &'a [ViewOrder],
    limit: u64,
    printed: Printed<'a>,
}
enum Printed<'a> {
    Columns(&'a [ViewColumn]),
    Properties(&'a [String]),
}
/// The checks of a table view (R1 to R5) over one selection, with pointers
/// under `p`; `args` are the arguments its filter may use.
fn check_selection(
    schema: &Schema,
    sel: &Selection<'_>,
    args: &[String],
    p: &str,
    out: &mut Vec<Violation>,
) {
    let mut bad = |code, field: &str| out.push(Violation::new(code, format!("{p}{field}")));
    if !(1..=1000).contains(&sel.limit) {
        bad(codes::INVALID_SCHEMA, "/limit");
    }
    if sel.order.is_empty() {
        bad(codes::INVALID_SCHEMA, "/order");
    }
    match sel.printed {
        Printed::Columns([]) => bad(codes::INVALID_SCHEMA, "/columns"),
        Printed::Properties(properties) if !(1..=4).contains(&properties.len()) => {
            bad(codes::INVALID_DOCUMENT, "/properties")
        }
        _ => {}
    }
    let Some(ty) = schema.object_types.get(sel.object_type) else {
        bad(codes::UNKNOWN_TYPE, "/object_type");
        return;
    };
    let printable = |property: &str| {
        property == "$key" || property == "$observed_at" || ty.properties.contains_key(property)
    };
    match sel.printed {
        Printed::Columns(columns) => {
            for col in columns {
                if col.header.contains(['|', '\n', '\r']) {
                    bad(codes::INVALID_SCHEMA, "/columns");
                }
                if !printable(&col.property) {
                    bad(codes::UNKNOWN_PROPERTY, "/columns");
                }
            }
        }
        Printed::Properties(properties) => {
            if properties.iter().any(|p| !printable(p)) {
                bad(codes::UNKNOWN_PROPERTY, "/properties");
            }
        }
    }
    for order in sel.order {
        if order.property != "$key" && !ty.properties.contains_key(&order.property) {
            bad(codes::UNKNOWN_PROPERTY, "/order");
        }
    }
    if sel
        .filter
        .is_some_and(|f| !check_view_filter(f, args, schema, &ty.properties, 0, &mut 0))
    {
        bad(codes::INVALID_SCHEMA, "/filter");
    }
}
/// At most this many document blocks in one document (R52).
pub const DOCUMENT_BLOCKS_MAX: usize = 20;
/// At most this many bytes in a document block's heading (R52).
pub const HEADING_BYTES_MAX: usize = 200;
fn check_views(schema: &Schema) -> Vec<Violation> {
    let mut out = Vec::new();
    for (name, view) in schema.views.all() {
        let p = format!("/views/{}", esc(name));
        if !valid_name(name) {
            out.push(Violation::new(codes::INVALID_NAME, p.clone()));
        }
        let mut args = BTreeSet::new();
        if view
            .args()
            .iter()
            .any(|a| !valid_name(a) || !args.insert(a))
        {
            out.push(Violation::new(codes::INVALID_SCHEMA, format!("{p}/args")));
        }
        match view {
            View::Table(v) => {
                let sel = Selection {
                    object_type: &v.object_type,
                    filter: v.filter.as_ref(),
                    order: &v.order,
                    limit: v.limit,
                    printed: Printed::Columns(&v.columns),
                };
                check_selection(schema, &sel, &v.args, &p, &mut out);
            }
            View::Document(d) => {
                if !(1..=DOCUMENT_BLOCKS_MAX).contains(&d.blocks.len()) {
                    out.push(Violation::new(
                        codes::INVALID_DOCUMENT,
                        format!("{p}/blocks"),
                    ));
                }
                for (i, block) in d.blocks.iter().enumerate() {
                    let bp = format!("{p}/blocks/{i}");
                    let h = &block.heading;
                    if h.is_empty()
                        || h.len() > HEADING_BYTES_MAX
                        || h.chars().any(char::is_control)
                    {
                        out.push(Violation::new(
                            codes::INVALID_DOCUMENT,
                            format!("{bp}/heading"),
                        ));
                    }
                    let (sel, at) = match &block.body {
                        BlockBody::Table(t) => (
                            Selection {
                                object_type: &t.object_type,
                                filter: t.filter.as_ref(),
                                order: &t.order,
                                limit: t.limit,
                                printed: Printed::Columns(&t.columns),
                            },
                            "table",
                        ),
                        BlockBody::List(l) => (
                            Selection {
                                object_type: &l.object_type,
                                filter: l.filter.as_ref(),
                                order: &l.order,
                                limit: l.limit,
                                printed: Printed::Properties(&l.properties),
                            },
                            "list",
                        ),
                    };
                    check_selection(schema, &sel, &d.args, &format!("{bp}/{at}"), &mut out);
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    #[serde(default, skip_serializing_if = "Views::is_empty")]
    pub views: Views,
    pub format: String,
    pub domain: String,
    pub version: u64,
    pub namespace: Namespace,
    #[serde(default)]
    pub scope_keys: Vec<String>,
    #[serde(default)]
    pub value_types: BTreeMap<String, ValueTypeDef>,
    #[serde(default)]
    pub sources: BTreeMap<String, SourceDef>,
    #[serde(default)]
    pub object_types: BTreeMap<String, ObjectType>,
    #[serde(default)]
    pub link_types: BTreeMap<String, LinkType>,
    #[serde(default)]
    pub action_types: BTreeMap<String, ActionType>,
    #[serde(default)]
    pub functions: BTreeMap<String, FunctionDef>,
    #[serde(default)]
    pub grants: Vec<SchemaGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaPin {
    pub domain: String,
    pub version: u64,
    pub sha256: String,
}
impl SchemaPin {
    fn for_bytes(domain: &str, version: u64, bytes: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(bytes);
        Self {
            domain: domain.into(),
            version,
            sha256: format!("{:x}", h.finalize()),
        }
    }
}

/// A checked schema paired with the pin of its exact bytes. Installed bytes
/// may lack a resolvable create key; new admission uses `Schema::check_bytes`.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedSchema {
    schema: Schema,
    pin: SchemaPin,
}
impl CheckedSchema {
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn pin(&self) -> &SchemaPin {
        &self.pin
    }
}

impl Schema {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        serde_json::from_slice(bytes).map_err(|_| Error::SchemaViolation)
    }
    pub fn check(&self, other_domains_types: &BTreeSet<String>) -> Vec<Violation> {
        check(self, other_domains_types)
    }
    /// Strictly parse, check, and pin exact bytes for admission of a new schema.
    pub fn check_bytes(
        bytes: &[u8],
        other_domains_types: &BTreeSet<String>,
    ) -> Result<CheckedSchema, Vec<Violation>> {
        Self::check_bytes_inner(bytes, other_domains_types, false)
    }

    /// Reload previously installed bytes, exempting only the create-key rule
    /// and the rule-target rule (`specs/ont/action.md` R40, R110).
    /// Never use for admission of a new install; execution must resolve its key.
    pub fn check_installed_bytes(
        bytes: &[u8],
        other_domains_types: &BTreeSet<String>,
    ) -> Result<CheckedSchema, Vec<Violation>> {
        Self::check_bytes_inner(bytes, other_domains_types, true)
    }

    fn check_bytes_inner(
        bytes: &[u8],
        other_domains_types: &BTreeSet<String>,
        installed: bool,
    ) -> Result<CheckedSchema, Vec<Violation>> {
        if let Some(pointer) = duplicate_key_pointer(bytes) {
            return Err(vec![Violation::new(codes::DUPLICATE_FIELD, pointer)]);
        }
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let schema: Self = serde_path_to_error::deserialize(&mut de).map_err(|e| {
            let pointer = e
                .path()
                .iter()
                .map(|seg| match seg {
                    serde_path_to_error::Segment::Seq { index } => format!("/{index}"),
                    serde_path_to_error::Segment::Map { key } => format!("/{}", esc(key)),
                    serde_path_to_error::Segment::Enum { variant } => {
                        format!("/{}", esc(variant))
                    }
                    _ => String::new(),
                })
                .collect::<String>();
            let msg = e.inner().to_string();
            let code = if (pointer.contains("/submission_criteria/") || pointer.contains("/rules/"))
                && (pointer.ends_with("/present") || msg.contains("unknown field `present`"))
            {
                codes::INVALID_RULE
            } else if msg.contains("unknown field") {
                codes::UNKNOWN_FIELD
            } else if msg.contains("duplicate field") {
                codes::DUPLICATE_FIELD
            } else if msg.contains("missing field") {
                codes::MISSING_FIELD
            } else {
                codes::INVALID_SCHEMA
            };
            vec![Violation::new(code, pointer)]
        })?;
        let mut violations = check(&schema, other_domains_types);
        if installed {
            violations.retain(|v| !installed_exempt(v.code));
        }
        if violations.is_empty() {
            Ok(CheckedSchema {
                pin: SchemaPin::for_bytes(&schema.domain, schema.version, bytes),
                schema,
            })
        } else {
            Err(violations)
        }
    }

    /// Validate one value against a declared property type, including a
    /// value type's `pattern` and other constraints.
    pub fn validate_value(&self, ty: &PropertyType, v: &Value) -> Result<(), Violation> {
        validate_value(self, ty, v).map_err(|code| Violation::new(code, String::new()))
    }

    pub(crate) fn validate_fields(
        &self,
        object_type: &str,
        fields: &BTreeMap<String, Knowledge>,
    ) -> Result<(), Violation> {
        let Some(ty) = self.object_types.get(object_type) else {
            return Err(Violation::new(codes::UNKNOWN_TYPE, String::new()));
        };
        for name in fields.keys() {
            if !ty.properties.contains_key(name) {
                return Err(Violation::new(
                    codes::UNDECLARED_PROPERTY,
                    format!("/fields/{}", esc(name)),
                ));
            }
        }
        for (name, prop) in &ty.properties {
            match fields.get(name) {
                None if prop.required => {
                    return Err(Violation::new(
                        codes::MISSING_REQUIRED_PROPERTY,
                        format!("/fields/{}", esc(name)),
                    ));
                }
                Some(Knowledge::Value(v)) => {
                    validate_value(self, &prop.ty, v)
                        .map_err(|code| Violation::new(code, format!("/fields/{}", esc(name))))?;
                    if let (Some(limit), Value::String(s)) = (prop.max_bytes, v)
                        && s.len() as u64 > limit
                    {
                        return Err(Violation::new(
                            codes::VALUE_CONSTRAINT_VIOLATION,
                            format!("/fields/{}", esc(name)),
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Nesting depth limit of the duplicate scan, the same limit serde_json
/// applies; beyond it the scan stops and leaves the refusal to the parser.
const MAX_SCAN_DEPTH: usize = 128;

/// One pass over the JSON token stream: the pointer of the first duplicate
/// key inside any object, if there is one. serde's map readers are last-wins,
/// so duplicates are refused here before deserialization.
fn duplicate_key_pointer(bytes: &[u8]) -> Option<String> {
    struct Scan<'a> {
        b: &'a [u8],
        i: usize,
    }
    impl Scan<'_> {
        fn ws(&mut self) {
            while self.b.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                self.i += 1;
            }
        }
        fn string(&mut self) -> Option<String> {
            if self.b.get(self.i) != Some(&b'"') {
                return None;
            }
            self.i += 1;
            let mut out = String::new();
            loop {
                let c = *self.b.get(self.i)?;
                self.i += 1;
                match c {
                    b'"' => return Some(out),
                    b'\\' => {
                        let e = *self.b.get(self.i)?;
                        self.i += 1;
                        match e {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'b' => out.push('\u{0008}'),
                            b'f' => out.push('\u{000C}'),
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'u' => {
                                let hex = |s: &Scan<'_>| -> Option<u32> {
                                    let h = std::str::from_utf8(s.b.get(s.i..s.i + 4)?).ok()?;
                                    u32::from_str_radix(h, 16).ok()
                                };
                                let mut cp = hex(self)?;
                                self.i += 4;
                                if (0xD800..0xDC00).contains(&cp)
                                    && self.b.get(self.i) == Some(&b'\\')
                                    && self.b.get(self.i + 1) == Some(&b'u')
                                {
                                    self.i += 2;
                                    let lo = hex(self)?;
                                    self.i += 4;
                                    cp =
                                        0x10000 + ((cp - 0xD800) << 10) + lo.saturating_sub(0xDC00);
                                }
                                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                            }
                            _ => return None,
                        }
                    }
                    _ => {
                        // Copy the full UTF-8 sequence verbatim.
                        let start = self.i - 1;
                        let len = if c < 0x80 {
                            1
                        } else if c >= 0xF0 {
                            4
                        } else if c >= 0xE0 {
                            3
                        } else {
                            2
                        };
                        let end = (start + len).min(self.b.len());
                        out.push_str(std::str::from_utf8(&self.b[start..end]).ok()?);
                        self.i = end;
                    }
                }
            }
        }
        fn value(&mut self, pointer: &str, depth: usize) -> Option<Option<String>> {
            if depth > MAX_SCAN_DEPTH {
                return Some(None);
            }
            self.ws();
            match *self.b.get(self.i)? {
                b'{' => {
                    self.i += 1;
                    let mut keys = BTreeSet::new();
                    self.ws();
                    if self.b.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        return Some(None);
                    }
                    loop {
                        self.ws();
                        let key = self.string()?;
                        let kp = format!("{pointer}/{}", esc(&key));
                        if !keys.insert(key) {
                            return Some(Some(kp));
                        }
                        self.ws();
                        if self.b.get(self.i) != Some(&b':') {
                            return None;
                        }
                        self.i += 1;
                        if let Some(d) = self.value(&kp, depth + 1)? {
                            return Some(Some(d));
                        }
                        self.ws();
                        match self.b.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b'}') => {
                                self.i += 1;
                                return Some(None);
                            }
                            _ => return None,
                        }
                    }
                }
                b'[' => {
                    self.i += 1;
                    self.ws();
                    if self.b.get(self.i) == Some(&b']') {
                        self.i += 1;
                        return Some(None);
                    }
                    let mut idx = 0usize;
                    loop {
                        if let Some(d) = self.value(&format!("{pointer}/{idx}"), depth + 1)? {
                            return Some(Some(d));
                        }
                        idx += 1;
                        self.ws();
                        match self.b.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b']') => {
                                self.i += 1;
                                return Some(None);
                            }
                            _ => return None,
                        }
                    }
                }
                b'"' => {
                    self.string()?;
                    Some(None)
                }
                _ => {
                    while self.b.get(self.i).is_some_and(|c| {
                        !matches!(c, b',' | b'}' | b']') && !c.is_ascii_whitespace()
                    }) {
                        self.i += 1;
                    }
                    Some(None)
                }
            }
        }
    }
    let mut scan = Scan { b: bytes, i: 0 };
    scan.value("", 0).flatten()
}

/// Check one template: syntax and length (`invalid_template`), then each
/// distinct placeholder name against the record fields when they are
/// known: an undeclared field is `unknown_template_field`, a declared
/// field that is never a string is `template_field_not_string`.
/// `fields` is `(record_fields, string_record_fields)` of the source kind.
fn check_template(
    template: &str,
    fields: Option<(&[&str], &[&str])>,
    pointer: String,
    out: &mut Vec<Violation>,
) {
    match template_placeholders(template) {
        Err(code) => out.push(Violation::new(code, pointer)),
        Ok(names) => {
            if let Some((known, strings)) = fields {
                let mut seen = BTreeSet::new();
                for name in &names {
                    if !seen.insert(name.as_str()) {
                        continue;
                    }
                    if !known.contains(&name.as_str()) {
                        out.push(Violation::new(
                            codes::UNKNOWN_TEMPLATE_FIELD,
                            pointer.clone(),
                        ));
                    } else if !strings.contains(&name.as_str()) {
                        out.push(Violation::new(
                            codes::TEMPLATE_FIELD_NOT_STRING,
                            pointer.clone(),
                        ));
                    }
                }
            }
        }
    }
}

/// Semantic checks of DESIGN section 3 "Format and checks".
pub fn check(schema: &Schema, other_domains_types: &BTreeSet<String>) -> Vec<Violation> {
    let mut out: Vec<Violation> = check_views(schema);
    let v = |code: &'static str, pointer: String, out: &mut Vec<Violation>| {
        out.push(Violation::new(code, pointer));
    };
    if schema.format != FORMAT {
        v(codes::INVALID_FORMAT, "/format".into(), &mut out);
    }
    if !valid_name(&schema.domain) {
        v(codes::INVALID_NAME, "/domain".into(), &mut out);
    }
    if schema.domain == crate::scope::PROGRAM_DOMAIN {
        v(codes::RESERVED_DOMAIN, "/domain".into(), &mut out);
    }
    if schema.version == 0 {
        v(codes::INVALID_VERSION, "/version".into(), &mut out);
    }
    for (field, value) in [
        ("instance", &schema.namespace.instance),
        ("account", &schema.namespace.account),
    ] {
        if value.is_empty() || value.len() > crate::scope::NAMESPACE_MAX_BYTES {
            v(
                codes::INVALID_NAMESPACE,
                format!("/namespace/{field}"),
                &mut out,
            );
        }
    }
    let mut seen_keys = BTreeSet::new();
    for (i, k) in schema.scope_keys.iter().enumerate() {
        if !valid_name(k) {
            v(codes::INVALID_NAME, format!("/scope_keys/{i}"), &mut out);
        }
        if !seen_keys.insert(k.clone()) {
            v(
                codes::DUPLICATE_SCOPE_KEY,
                format!("/scope_keys/{i}"),
                &mut out,
            );
        }
    }
    for (name, def) in &schema.value_types {
        let p = format!("/value_types/{}", esc(name));
        if !valid_name(name) {
            v(codes::INVALID_NAME, p.clone(), &mut out);
        }
        if let Some(vals) = &def.enum_values
            && (vals.is_empty() || vals.iter().any(|x| !scalar_matches(def.base, x)))
        {
            v(
                codes::INVALID_VALUE_TYPE_DEFINITION,
                format!("{p}/enum"),
                &mut out,
            );
        }
        if def.pattern.is_some() && (def.base != ScalarType::String || def.compiled().is_none()) {
            v(
                codes::INVALID_VALUE_TYPE_DEFINITION,
                format!("{p}/pattern"),
                &mut out,
            );
        }
        for (field, bound) in [("min", &def.min), ("max", &def.max)] {
            if let Some(bound) = bound
                && !min_max_matches(def.base, bound)
            {
                v(
                    codes::INVALID_VALUE_TYPE_DEFINITION,
                    format!("{p}/{field}"),
                    &mut out,
                );
            }
        }
        if let (Some(a), Some(b)) = (&def.min, &def.max)
            && min_max_matches(def.base, a)
            && min_max_matches(def.base, b)
            && compare_values(a, b) == Some(std::cmp::Ordering::Greater)
        {
            v(
                codes::INVALID_VALUE_TYPE_DEFINITION,
                format!("{p}/max"),
                &mut out,
            );
        }
    }
    for (name, src) in &schema.sources {
        if !valid_name(name) {
            v(
                codes::INVALID_NAME,
                format!("/sources/{}", esc(name)),
                &mut out,
            );
        }
        if !SOURCE_KINDS.contains(&src.kind.as_str()) {
            v(
                codes::UNKNOWN_SOURCE_KIND,
                format!("/sources/{}/kind", esc(name)),
                &mut out,
            );
        }
        if let Err(e) = src.file_spec() {
            v(
                e.code,
                format!("/sources/{}{}", esc(name), e.pointer),
                &mut out,
            );
        }
        if src.kind == "records"
            && schema
                .object_types
                .values()
                .filter(|t| t.owner == Owner::Source(name.clone()))
                .count()
                != 1
        {
            v(
                codes::RECORDS_SOURCE_OWNER,
                format!("/sources/{}", esc(name)),
                &mut out,
            );
        }
    }
    for (name, ty) in &schema.object_types {
        let base = format!("/object_types/{}", esc(name));
        if !valid_name(name) {
            v(codes::INVALID_NAME, base.clone(), &mut out);
        }
        if other_domains_types.contains(name) {
            v(codes::DUPLICATE_TYPE_NAME, base.clone(), &mut out);
        }
        if let Owner::Source(s) = &ty.owner
            && !schema.sources.contains_key(s)
        {
            v(
                codes::UNKNOWN_SOURCE,
                format!("{base}/owner/source"),
                &mut out,
            );
        }
        let file_spec = match &ty.owner {
            Owner::Source(s) => schema
                .sources
                .get(s)
                .and_then(|src| src.file_spec().ok().flatten()),
            Owner::Ontology => None,
        };
        let known_fields: Option<(&[&str], &[&str])> = file_spec
            .as_ref()
            .filter(|spec| spec.kind.has_fixed_fields())
            .map(|spec| (spec.kind.record_fields(), spec.kind.string_record_fields()));
        if let Some(spec) = &file_spec {
            if !schema.scope_keys.is_empty() {
                v(
                    codes::SOURCE_SCOPE_KEYS_UNMAPPED,
                    format!("{base}/owner/source"),
                    &mut out,
                );
            }
            let first = |from: &str| -> Option<String> {
                from.strip_prefix('/').map(|r| {
                    r.split('/')
                        .next()
                        .unwrap_or("")
                        .replace("~1", "/")
                        .replace("~0", "~")
                })
            };
            let known = spec.kind.record_fields();
            let fixed = spec.kind.has_fixed_fields();
            if let Some(pk) = &ty.primary_key.from
                && pk != "$action_id"
                && fixed
                && first(pk).is_some_and(|f| !known.contains(&f.as_str()))
            {
                v(
                    codes::UNKNOWN_SOURCE_FIELD,
                    format!("{base}/primary_key/from"),
                    &mut out,
                );
            }
            for (pname, prop) in &ty.properties {
                if let Some(from) = &prop.from
                    && fixed
                    && first(from).is_some_and(|f| !known.contains(&f.as_str()))
                {
                    v(
                        codes::UNKNOWN_SOURCE_FIELD,
                        format!("{base}/properties/{}/from", esc(pname)),
                        &mut out,
                    );
                }
            }
        }
        let pk = &ty.primary_key;
        match (&pk.from, &pk.template) {
            (Some(_), Some(_)) => v(
                codes::INVALID_TEMPLATE,
                format!("{base}/primary_key/template"),
                &mut out,
            ),
            (None, None) => v(
                codes::INVALID_PRIMARY_KEY,
                format!("{base}/primary_key/from"),
                &mut out,
            ),
            _ => {}
        }
        if let Some(f) = &pk.from
            && f != "$action_id"
            && !(f.starts_with('/') && f.len() > 1)
        {
            v(
                codes::INVALID_PRIMARY_KEY,
                format!("{base}/primary_key/from"),
                &mut out,
            );
        }
        if matches!(ty.owner, Owner::Ontology)
            && pk
                .from
                .as_deref()
                .is_some_and(|p| p.starts_with('/') && p != "/native_key")
        {
            v(
                codes::AMBIGUOUS_CREATE_KEY,
                format!("{base}/primary_key/from"),
                &mut out,
            );
        }
        if let Some(tpl) = &pk.template {
            check_template(
                tpl,
                known_fields,
                format!("{base}/primary_key/template"),
                &mut out,
            );
            if matches!(ty.owner, Owner::Ontology) {
                v(
                    codes::INVALID_TEMPLATE,
                    format!("{base}/primary_key/template"),
                    &mut out,
                );
            }
        }
        if let Some(t) = &ty.primary_key.transform
            && (!t.form_valid()
                || !t.produces_string()
                || !matches!(ty.owner, Owner::Source(_))
                || ty.primary_key.from.as_deref() == Some("$action_id")
                || (ty.primary_key.from.is_none() && ty.primary_key.template.is_none()))
        {
            v(
                codes::INVALID_TRANSFORM,
                format!("{base}/primary_key/transform"),
                &mut out,
            );
        }
        if let Some(title) = &ty.title
            && !ty.properties.contains_key(title)
        {
            v(codes::UNKNOWN_PROPERTY, format!("{base}/title"), &mut out);
        }
        for (pname, prop) in &ty.properties {
            let pp = format!("{base}/properties/{}", esc(pname));
            if !valid_name(pname) {
                v(codes::INVALID_NAME, pp.clone(), &mut out);
            }
            check_description(prop, &pp, &mut out);
            check_type(schema, &prop.ty, &format!("{pp}/type"), false, &mut out);
            if matches!(ty.owner, Owner::Source(_))
                && matches!(&prop.ty, PropertyType::Array(a)
                    if matches!(&*a.array, PropertyType::Ref(_)))
                && (prop.from.is_some() || prop.template.is_some())
            {
                v(
                    codes::REFERENCE_ARRAY_FROM_SOURCE_UNSUPPORTED,
                    if prop.from.is_some() {
                        format!("{pp}/from")
                    } else {
                        format!("{pp}/template")
                    },
                    &mut out,
                );
            }
            if prop.from.is_some() && prop.template.is_some() {
                v(codes::INVALID_TEMPLATE, format!("{pp}/template"), &mut out);
            }
            if let Some(tpl) = &prop.template {
                check_template(tpl, known_fields, format!("{pp}/template"), &mut out);
            }
            if let Some(t) = &prop.transform {
                let integer = matches!(t, Transform::Named(n) if n == "parse_integer");
                let compatible = match &prop.ty {
                    PropertyType::Ref(_) => t.produces_string(),
                    PropertyType::Scalar(s) => {
                        if integer {
                            *s == ScalarType::Integer
                        } else {
                            *s == ScalarType::String
                        }
                    }
                    PropertyType::ValueType(r) => schema
                        .value_types
                        .get(&r.value_type)
                        .map(|d| d.base)
                        .is_some_and(|base| {
                            if integer {
                                base == ScalarType::Integer
                            } else {
                                base == ScalarType::String
                            }
                        }),
                    _ => false,
                };
                if !t.form_valid()
                    || !compatible
                    || (prop.from.is_none() && prop.template.is_none())
                    || !matches!(ty.owner, Owner::Source(_))
                {
                    v(
                        codes::INVALID_TRANSFORM,
                        format!("{pp}/transform"),
                        &mut out,
                    );
                }
            }
            match (&ty.owner, &prop.from, &prop.template) {
                (Owner::Ontology, Some(_), _) => v(
                    codes::ONTOLOGY_PROPERTY_FROM,
                    format!("{pp}/from"),
                    &mut out,
                ),
                (Owner::Ontology, None, Some(_)) => {
                    v(codes::INVALID_TEMPLATE, format!("{pp}/template"), &mut out)
                }
                (Owner::Source(_), None, None) if prop.required => {
                    v(codes::REQUIRED_PROPERTY_WITHOUT_FROM, pp.clone(), &mut out)
                }
                (_, Some(f), _) if !f.starts_with('/') => {
                    v(codes::INVALID_FROM_POINTER, format!("{pp}/from"), &mut out)
                }
                _ => {}
            }
        }
    }
    for (name, def) in &schema.functions {
        let base = format!("/functions/{}", esc(name));
        if !valid_name(name) {
            v(codes::INVALID_NAME, base.clone(), &mut out);
        }
        for fault in def.faults() {
            v(codes::INVALID_FUNCTION, format!("{base}{fault}"), &mut out);
        }
        for (pname, param) in &def.parameters {
            // One diagnostic for a type at fault, whatever its depth.
            let pointer = format!("{base}/parameters/{}/type", esc(pname));
            let mut type_faults = Vec::new();
            check_type(schema, &param.ty, &pointer, false, &mut type_faults);
            if !type_faults.is_empty() {
                v(codes::INVALID_FUNCTION, pointer, &mut out);
            }
        }
    }
    // Link types.
    let mut link_names: BTreeMap<String, &LinkType> = BTreeMap::new();
    for (name, lt) in &schema.link_types {
        let base = format!("/link_types/{}", esc(name));
        if !valid_name(name) {
            v(codes::INVALID_NAME, base.clone(), &mut out);
        }
        for (field, target) in [("from", &lt.from), ("to", &lt.to)] {
            if !schema.object_types.contains_key(target) {
                v(codes::UNKNOWN_TYPE, format!("{base}/{field}"), &mut out);
            }
        }
        for (field, lname) in [
            ("forward", &lt.names.forward),
            ("reverse", &lt.names.reverse),
        ] {
            if !valid_name(lname) {
                v(
                    codes::INVALID_NAME,
                    format!("{base}/names/{field}"),
                    &mut out,
                );
            }
            if link_names.insert(lname.clone(), lt).is_some() {
                v(
                    codes::DUPLICATE_LINK_NAME,
                    format!("{base}/names/{field}"),
                    &mut out,
                );
            }
        }
        if let Some(from_ty) = schema.object_types.get(&lt.from) {
            match from_ty.properties.get(&lt.via) {
                None => v(codes::UNKNOWN_PROPERTY, format!("{base}/via"), &mut out),
                Some(prop) => {
                    let (backing, cardinality_ok) = match &prop.ty {
                        PropertyType::Ref(r) => {
                            (r.target == lt.to, lt.cardinality == Cardinality::ManyToOne)
                        }
                        PropertyType::Array(a) => (
                            matches!(&*a.array, PropertyType::Ref(r) if r.target == lt.to),
                            lt.cardinality == Cardinality::ManyToMany,
                        ),
                        _ => (false, false),
                    };
                    if !backing {
                        v(codes::INVALID_LINK_BACKING, format!("{base}/via"), &mut out);
                    } else if !cardinality_ok {
                        v(
                            codes::INVALID_LINK_BACKING,
                            format!("{base}/cardinality"),
                            &mut out,
                        );
                    }
                }
            }
        }
    }
    // Action types.
    for (name, at) in &schema.action_types {
        let base = format!("/action_types/{}", esc(name));
        if !valid_name(name) {
            v(codes::INVALID_NAME, base.clone(), &mut out);
        }
        for (pname, param) in &at.parameters {
            let pp = format!("{base}/parameters/{}", esc(pname));
            if !valid_name(pname) || pname == "now" {
                v(codes::INVALID_NAME, pp.clone(), &mut out);
            }
            check_description(param, &pp, &mut out);
            check_type(schema, &param.ty, &format!("{pp}/type"), false, &mut out);
            if param.from.is_some() {
                v(codes::INVALID_FROM_POINTER, format!("{pp}/from"), &mut out);
            }
            if param.template.is_some() {
                v(codes::INVALID_TEMPLATE, format!("{pp}/template"), &mut out);
            }
            if param.transform.is_some() {
                v(
                    codes::INVALID_TRANSFORM,
                    format!("{pp}/transform"),
                    &mut out,
                );
            }
        }
        check_action_reads(schema, name, at, &base, &mut out);
        let mut seen_codes = BTreeSet::new();
        for (ci, crit) in at.submission_criteria.iter().enumerate() {
            let cp = format!("{base}/submission_criteria/{ci}");
            if !valid_name(&crit.code) {
                v(codes::INVALID_NAME, format!("{cp}/code"), &mut out);
            }
            if !seen_codes.insert(crit.code.clone()) {
                v(
                    codes::DUPLICATE_CRITERION_CODE,
                    format!("{cp}/code"),
                    &mut out,
                );
            }
            let arms: Vec<(&str, &Vec<Condition>)> =
                [("all", &crit.all), ("any", &crit.any), ("none", &crit.none)]
                    .into_iter()
                    .filter_map(|(n, a)| a.as_ref().map(|x| (n, x)))
                    .collect();
            if arms.len() != 1 {
                v(codes::INVALID_CRITERION, cp.clone(), &mut out);
                continue;
            }
            for (i, cond) in arms[0].1.iter().enumerate() {
                let ptr = format!("{cp}/{}/{i}", arms[0].0);
                check_condition(schema, at, cond, None, true, &ptr, &mut out);
            }
        }
        for (ri, rule) in at.rules.iter().enumerate() {
            let rp = format!("{base}/rules/{ri}");
            if rule.kind_count() != 1 {
                v(codes::INVALID_RULE, rp.clone(), &mut out);
                continue;
            }
            // The object a rule edits through one parameter (R108, R109).
            match rule.targeted_type(schema) {
                None => {
                    if rule.target.is_some() {
                        v(codes::INVALID_RULE_TARGET, format!("{rp}/target"), &mut out);
                    }
                }
                Some(t) => match &rule.target {
                    Some(target) => {
                        let named = target
                            .strip_prefix('$')
                            .is_some_and(|name| at.reference_parameters(t).contains(&name));
                        if !named {
                            v(codes::INVALID_RULE_TARGET, format!("{rp}/target"), &mut out);
                        }
                    }
                    None => {
                        if at.reference_parameters(t).len() > 1 {
                            v(codes::AMBIGUOUS_RULE_TARGET, rp.clone(), &mut out);
                        }
                    }
                },
            }
            if let Some((field, target)) = rule.object_target() {
                let tp = format!("{rp}/{field}");
                match schema.object_types.get(target) {
                    None => v(codes::UNKNOWN_TYPE, tp, &mut out),
                    Some(ty) => {
                        if matches!(ty.owner, Owner::Source(_)) {
                            v(codes::ACTION_EDITS_SOURCE_OWNED, tp, &mut out);
                        }
                        if matches!(ty.owner, Owner::Ontology)
                            && (rule.create_object.is_some()
                                || rule.create_or_modify_object.is_some())
                            && let Some(pointer) = ty
                                .primary_key
                                .from
                                .as_deref()
                                .filter(|p| p.starts_with('/'))
                            && at.create_key_parameter(target, pointer).is_none()
                        {
                            v(codes::AMBIGUOUS_CREATE_KEY, base.clone(), &mut out);
                        }
                        for key in rule.set.keys() {
                            if !ty.properties.contains_key(key) {
                                v(
                                    codes::UNKNOWN_PROPERTY,
                                    format!("{rp}/set/{}", esc(key)),
                                    &mut out,
                                );
                            }
                        }
                    }
                }
            }
            for (key, val) in &rule.set {
                let ptr = format!("{rp}/set/{}", esc(key));
                scan_params(schema, at, val, &ptr, true, &mut out);
                if let Some((_, target)) = rule.object_target()
                    && let Some(prop) = schema
                        .object_types
                        .get(target)
                        .and_then(|t| t.properties.get(key))
                {
                    check_rule_expression(schema, at, val, &prop.ty, &ptr, &mut out);
                    if prop.required
                        && (rule.create_object.is_some() || rule.create_or_modify_object.is_some())
                    {
                        check_required_input(at, val, &ptr, &mut out);
                    }
                }
            }
            for (field, refr) in [
                ("set_reference", &rule.set_reference),
                ("clear_reference", &rule.clear_reference),
            ]
            .into_iter()
            .filter_map(|(f, r)| r.as_ref().map(|x| (f, x)))
            {
                let tp = format!("{rp}/{field}");
                match schema.object_types.get(&refr.object_type) {
                    None => v(codes::UNKNOWN_TYPE, format!("{tp}/object_type"), &mut out),
                    Some(ty) => {
                        if matches!(ty.owner, Owner::Source(_)) {
                            v(
                                codes::ACTION_EDITS_SOURCE_OWNED,
                                format!("{tp}/object_type"),
                                &mut out,
                            );
                        }
                        match ty.properties.get(&refr.property) {
                            None => v(codes::UNKNOWN_PROPERTY, format!("{tp}/property"), &mut out),
                            Some(p) if !is_reference_type(&p.ty) => v(
                                codes::INVALID_REFERENCE_PROPERTY,
                                format!("{tp}/property"),
                                &mut out,
                            ),
                            _ => {}
                        }
                    }
                }
                if let Some(val) = &refr.value {
                    let ptr = format!("{tp}/value");
                    scan_params(schema, at, val, &ptr, true, &mut out);
                    if let Some(prop) = schema
                        .object_types
                        .get(&refr.object_type)
                        .and_then(|t| t.properties.get(&refr.property))
                    {
                        let ty = match &prop.ty {
                            PropertyType::Array(a) => &a.array,
                            ty => ty,
                        };
                        check_rule_expression(schema, at, val, ty, &ptr, &mut out);
                    }
                }
            }
            if rule
                .clear_reference
                .as_ref()
                .is_some_and(|r| r.value.is_some())
            {
                v(
                    codes::INVALID_RULE,
                    format!("{rp}/clear_reference/value"),
                    &mut out,
                );
            }
        }
    }
    for (gi, g) in schema.grants.iter().enumerate() {
        let gp = format!("/grants/{gi}");
        if g.principal.is_empty() {
            v(
                codes::INVALID_PRINCIPAL,
                format!("{gp}/principal"),
                &mut out,
            );
        }
        // A grant scope the policy could not build (an empty key or value)
        // would be skipped silently by every consumer: refuse it here.
        if g.scope
            .iter()
            .any(|(k, val)| k.is_empty() || val.is_empty())
        {
            v(codes::INVALID_GRANT_SCOPE, format!("{gp}/scope"), &mut out);
        }
        for key in g.scope.keys() {
            if !schema.scope_keys.contains(key) {
                v(
                    codes::UNKNOWN_SCOPE_KEY,
                    format!("{gp}/scope/{}", esc(key)),
                    &mut out,
                );
            }
        }
        for (field, types) in [("read", &g.read), ("derive", &g.derive)] {
            for (i, t) in types.iter().enumerate() {
                if !schema.object_types.contains_key(t) {
                    v(codes::UNKNOWN_TYPE, format!("{gp}/{field}/{i}"), &mut out);
                }
            }
        }
        for (i, a) in g.actions.iter().enumerate() {
            if !schema.action_types.contains_key(a) {
                v(codes::UNKNOWN_ACTION, format!("{gp}/actions/{i}"), &mut out);
            }
        }
        if g.role.as_deref().is_some_and(|r| !valid_name(r)) {
            v(codes::INVALID_NAME, format!("{gp}/role"), &mut out);
        }
        for (i, f) in g.functions.iter().enumerate() {
            if !schema.functions.contains_key(f) {
                v(
                    codes::UNKNOWN_FUNCTION,
                    format!("{gp}/functions/{i}"),
                    &mut out,
                );
            }
        }
    }
    out
}

fn check_type(
    schema: &Schema,
    ty: &PropertyType,
    pointer: &str,
    in_struct: bool,
    out: &mut Vec<Violation>,
) {
    match ty {
        PropertyType::Scalar(_) => {}
        PropertyType::Ref(r) => {
            if !schema.object_types.contains_key(&r.target) {
                out.push(Violation::new(codes::UNKNOWN_TYPE, pointer.into()));
            }
        }
        PropertyType::ValueType(vt) => {
            if !schema.value_types.contains_key(&vt.value_type) {
                out.push(Violation::new(codes::UNKNOWN_VALUE_TYPE, pointer.into()));
            }
        }
        PropertyType::Array(a) => check_type(
            schema,
            &a.array,
            &format!("{pointer}/array"),
            in_struct,
            out,
        ),
        PropertyType::Struct(s) => {
            if in_struct {
                out.push(Violation::new(codes::STRUCT_IN_STRUCT, pointer.into()));
            }
            for (fname, fty) in &s.struct_ {
                let fp = format!("{pointer}/struct/{}", esc(fname));
                if !valid_name(fname) {
                    out.push(Violation::new(codes::INVALID_NAME, fp.clone()));
                }
                check_type(schema, fty, &fp, true, out);
            }
        }
    }
}

fn is_reference_type(ty: &PropertyType) -> bool {
    match ty {
        PropertyType::Ref(_) => true,
        PropertyType::Array(a) => matches!(&*a.array, PropertyType::Ref(_)),
        _ => false,
    }
}

fn min_max_matches(base: ScalarType, v: &Value) -> bool {
    match base {
        ScalarType::Boolean => false,
        ScalarType::Integer | ScalarType::Timestamp => v.as_i64().is_some() || v.as_u64().is_some(),
        ScalarType::Decimal => v.as_str().is_some_and(|s| parse_decimal(s).is_some()),
        ScalarType::Date => v.as_str().is_some_and(valid_date),
        ScalarType::String => v.is_string(),
    }
}

fn check_description(def: &PropertyDef, pointer: &str, out: &mut Vec<Violation>) {
    if def.description.as_ref().is_some_and(|s| s.len() > 300) {
        out.push(Violation::new(
            codes::INVALID_SCHEMA,
            format!("{pointer}/description"),
        ));
    }
}
fn parameter_property<'a>(
    schema: &'a Schema,
    at: &ActionType,
    name: &str,
    prop: &str,
) -> Option<&'a PropertyType> {
    let PropertyType::Ref(r) = &at.parameters.get(name)?.ty else {
        return None;
    };
    Some(&schema.object_types.get(&r.target)?.properties.get(prop)?.ty)
}
fn scalar_base(schema: &Schema, ty: &PropertyType) -> Option<ScalarType> {
    match ty {
        PropertyType::Scalar(s) => Some(*s),
        PropertyType::ValueType(r) => schema.value_types.get(&r.value_type).map(|d| d.base),
        _ => None,
    }
}
// Patterned application times have a deliberately bounded schema contract:
// the canonical full UTC shape, or an unrestricted string pattern. Other
// constraints are still checked against the actual value when it is applied.
fn now_type_supported(schema: &Schema, ty: &PropertyType) -> bool {
    if scalar_base(schema, ty) != Some(ScalarType::String) {
        return false;
    }
    let PropertyType::ValueType(r) = ty else {
        return true;
    };
    schema.value_types.get(&r.value_type).is_some_and(|def| {
        def.pattern.as_ref().is_none_or(|p| {
            let pattern = p.strip_prefix('^').unwrap_or(p);
            let pattern = pattern
                .strip_suffix('$')
                .unwrap_or(pattern)
                .replace("\\d", "[0-9]");
            matches!(
                pattern.as_str(),
                ".*" | "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z"
            )
        })
    })
}
fn check_required_input(at: &ActionType, value: &Value, pointer: &str, out: &mut Vec<Violation>) {
    match value {
        Value::String(s) => {
            if let Some(name) = s
                .strip_prefix('$')
                .map(|p| p.split('.').next().unwrap_or(p))
                && at.parameters.get(name).is_some_and(|p| !p.required)
            {
                out.push(Violation::new(codes::INVALID_RULE, pointer.into()));
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                check_required_input(at, v, &format!("{pointer}/{i}"), out);
            }
        }
        Value::Object(m) => {
            for (k, v) in m {
                check_required_input(at, v, &format!("{pointer}/{}", esc(k)), out);
            }
        }
        _ => {}
    }
}
fn check_action_reads(
    schema: &Schema,
    name: &str,
    at: &ActionType,
    base: &str,
    out: &mut Vec<Violation>,
) {
    fn walk(
        schema: &Schema,
        name: &str,
        at: &ActionType,
        value: &Value,
        pointer: &str,
        out: &mut Vec<Violation>,
    ) {
        match value {
            Value::String(s) => {
                if let Some((param, _)) = s.strip_prefix('$').and_then(|p| p.split_once('.'))
                    && let Some(PropertyDef {
                        ty: PropertyType::Ref(r),
                        ..
                    }) = at.parameters.get(param)
                    && schema
                        .grants
                        .iter()
                        .any(|g| g.actions.iter().any(|a| a == name) && !g.read.contains(&r.target))
                {
                    out.push(Violation::new(
                        codes::REFERENCE_READ_FORBIDDEN,
                        pointer.into(),
                    ));
                }
            }
            Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    walk(schema, name, at, v, &format!("{pointer}/{i}"), out);
                }
            }
            Value::Object(m) => {
                for (k, v) in m {
                    walk(schema, name, at, v, &format!("{pointer}/{}", esc(k)), out);
                }
            }
            _ => {}
        }
    }
    for (i, r) in at.rules.iter().enumerate() {
        for (key, value) in &r.set {
            walk(
                schema,
                name,
                at,
                value,
                &format!("{base}/rules/{i}/set/{}", esc(key)),
                out,
            );
        }
        if let Some(value) = r.set_reference.as_ref().and_then(|r| r.value.as_ref()) {
            walk(
                schema,
                name,
                at,
                value,
                &format!("{base}/rules/{i}/set_reference/value"),
                out,
            );
        }
    }
    for (i, c) in at.submission_criteria.iter().enumerate() {
        for (arm, conditions) in [("all", &c.all), ("any", &c.any), ("none", &c.none)] {
            if let Some(conditions) = conditions {
                let value = serde_json::to_value(conditions).unwrap_or(Value::Null);
                walk(
                    schema,
                    name,
                    at,
                    &value,
                    &format!("{base}/submission_criteria/{i}/{arm}"),
                    out,
                );
            }
        }
    }
}

// Only the new expressions are statically typed; existing literal/parameter
// rule values retain their runtime checks and their existing schema pins.
fn check_rule_expression(
    schema: &Schema,
    at: &ActionType,
    value: &Value,
    ty: &PropertyType,
    pointer: &str,
    out: &mut Vec<Violation>,
) {
    match value {
        Value::String(s) if s == "$now" && !now_type_supported(schema, ty) => {
            out.push(Violation::new(codes::INVALID_RULE, pointer.into()));
        }
        Value::String(s) => {
            if let Some((name, prop)) = s.strip_prefix('$').and_then(|s| s.split_once('.'))
                && let Some(source) = parameter_property(schema, at, name, prop)
                && source != ty
                && !(scalar_base(schema, source).is_some()
                    && scalar_base(schema, source) == scalar_base(schema, ty))
            {
                out.push(Violation::new(codes::INVALID_RULE, pointer.into()));
            }
        }
        Value::Array(values) => {
            for (i, value) in values.iter().enumerate() {
                let inner = match ty {
                    PropertyType::Array(a) => &a.array,
                    _ => ty,
                };
                check_rule_expression(schema, at, value, inner, &format!("{pointer}/{i}"), out);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let inner = match ty {
                    PropertyType::Struct(s) => s.struct_.get(key).unwrap_or(ty),
                    _ => ty,
                };
                check_rule_expression(
                    schema,
                    at,
                    value,
                    inner,
                    &format!("{pointer}/{}", esc(key)),
                    out,
                );
            }
        }
        _ => {}
    }
}

fn param_name(s: &str) -> Option<&str> {
    s.strip_prefix('$')
}
fn scan_params(
    schema: &Schema,
    at: &ActionType,
    val: &Value,
    pointer: &str,
    allow_now: bool,
    out: &mut Vec<Violation>,
) {
    match val {
        Value::String(s) => {
            if let Some(p) = param_name(s) {
                if allow_now && p == "now" {
                    return;
                }
                let name = p.split_once('.').map_or(p, |(n, _)| n);
                if !at.parameters.contains_key(name) {
                    out.push(Violation::new(codes::UNKNOWN_PARAMETER, pointer.into()));
                } else if let Some((_, prop)) = p.split_once('.')
                    && parameter_property(schema, at, name, prop).is_none()
                {
                    out.push(Violation::new(codes::UNKNOWN_PROPERTY, pointer.into()));
                }
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                scan_params(schema, at, item, &format!("{pointer}/{i}"), allow_now, out);
            }
        }
        Value::Object(map) => {
            for (k, item) in map {
                scan_params(
                    schema,
                    at,
                    item,
                    &format!("{pointer}/{}", esc(k)),
                    allow_now,
                    out,
                );
            }
        }
        _ => {}
    }
}

fn check_condition(
    schema: &Schema,
    at: &ActionType,
    cond: &Condition,
    context: Option<&str>,
    allow_now: bool,
    pointer: &str,
    out: &mut Vec<Violation>,
) {
    let pair = match cond {
        Condition::Eq(p) => Some(("eq", p)),
        Condition::Ne(p) => Some(("ne", p)),
        Condition::In(p) => Some(("in", p)),
        Condition::Lt(p) => Some(("lt", p)),
        Condition::Le(p) => Some(("le", p)),
        Condition::Gt(p) => Some(("gt", p)),
        Condition::Ge(p) => Some(("ge", p)),
        _ => None,
    };
    if let Some((kind, pair)) = pair {
        if allow_now && kind != "in" && pair.iter().any(|v| v.as_str() == Some("$now")) {
            // With an explicit clock operand, unprefixed strings are literals.
            // Legacy comparisons without $now keep their bare-property syntax.
            for (i, operand) in pair.iter().enumerate() {
                let ptr = format!("{pointer}/{kind}/{i}");
                scan_params(schema, at, operand, &ptr, true, out);
                let string_typed = match operand.as_str() {
                    Some("$now") => true,
                    Some(s) => match s.strip_prefix('$') {
                        Some(p) => {
                            let ty = match p.split_once('.') {
                                Some((name, prop)) => parameter_property(schema, at, name, prop),
                                None => at.parameters.get(p).map(|p| &p.ty),
                            };
                            ty.and_then(|ty| scalar_base(schema, ty)) == Some(ScalarType::String)
                        }
                        None => {
                            if at.parameters.contains_key(s) {
                                out.push(
                                    Violation::new(codes::INVALID_CRITERION, ptr.clone())
                                        .with_message(format!(
                                            "Write ${s} to use the declared parameter."
                                        )),
                                );
                            }
                            true
                        }
                    },
                    None => false,
                };
                if !string_typed {
                    out.push(Violation::new(codes::INVALID_CRITERION, ptr));
                }
            }
            return;
        }
        if let (Some(ctx), Value::String(first)) = (context, &pair[0])
            && !first.starts_with('$')
        {
            let known = schema
                .object_types
                .get(ctx)
                .is_some_and(|t| t.properties.contains_key(first));
            if !known {
                out.push(Violation::new(
                    codes::UNKNOWN_PROPERTY,
                    format!("{pointer}/{kind}/0"),
                ));
            }
        }
        if context.is_none()
            && let Value::String(first) = &pair[0]
            && !first.starts_with('$')
        {
            // A bare name is a property of the one reference parameter's
            // object; with any other count the explicit form is required.
            let refs: Vec<&PropertyType> = at
                .parameters
                .values()
                .map(|p| &p.ty)
                .filter(|ty| matches!(ty, PropertyType::Ref(_)))
                .collect();
            if refs.len() != 1 {
                out.push(Violation::new(
                    codes::INVALID_CRITERION,
                    format!("{pointer}/{kind}/0"),
                ));
            } else if let PropertyType::Ref(r) = refs[0] {
                let known = schema
                    .object_types
                    .get(&r.target)
                    .is_some_and(|t| t.properties.contains_key(first));
                if !known {
                    out.push(Violation::new(
                        codes::UNKNOWN_PROPERTY,
                        format!("{pointer}/{kind}/0"),
                    ));
                }
            }
        }
        for (i, operand) in pair.iter().enumerate() {
            scan_params(
                schema,
                at,
                operand,
                &format!("{pointer}/{kind}/{i}"),
                false,
                out,
            );
        }
        return;
    }
    match cond {
        Condition::Present(s) => {
            let valid = s
                .strip_prefix('$')
                .is_some_and(|p| match p.split_once('.') {
                    Some((name, prop)) => parameter_property(schema, at, name, prop).is_some(),
                    None => at.parameters.contains_key(p),
                });
            if !valid {
                out.push(Violation::new(
                    codes::INVALID_RULE,
                    format!("{pointer}/present"),
                ));
            }
        }
        Condition::Exists(s) | Condition::Fresh(s) => {
            let kind = if matches!(cond, Condition::Exists(_)) {
                "exists"
            } else {
                "fresh"
            };
            if let Some(p) = param_name(s)
                && !at.parameters.contains_key(p)
            {
                out.push(Violation::new(
                    codes::UNKNOWN_PARAMETER,
                    format!("{pointer}/{kind}"),
                ));
            }
        }
        Condition::Actor(val) => {
            scan_params(schema, at, val, &format!("{pointer}/actor"), false, out)
        }
        Condition::Linked(link) => {
            if let Some(p) = param_name(&link.from)
                && !at.parameters.contains_key(p)
            {
                out.push(Violation::new(
                    codes::UNKNOWN_PARAMETER,
                    format!("{pointer}/linked/from"),
                ));
            }
            let lt = schema
                .link_types
                .values()
                .find(|l| l.names.forward == link.link || l.names.reverse == link.link);
            let Some(lt) = lt else {
                out.push(Violation::new(
                    codes::UNKNOWN_LINK,
                    format!("{pointer}/linked/link"),
                ));
                return;
            };
            // Resolve the type reached by following the named link.
            let from_ty = param_name(&link.from)
                .and_then(|p| at.parameters.get(p))
                .and_then(|param| match &param.ty {
                    PropertyType::Ref(r) => Some(r.target.as_str()),
                    _ => None,
                });
            let target = match from_ty {
                Some(ft) if lt.names.forward == link.link && lt.from == ft => Some(lt.to.as_str()),
                Some(ft) if lt.names.reverse == link.link && lt.to == ft => Some(lt.from.as_str()),
                _ => None,
            };
            if let Some(group) = &link.condition {
                let arms: Vec<(&str, &Vec<Condition>)> = [
                    ("all", &group.all),
                    ("any", &group.any),
                    ("none", &group.none),
                ]
                .into_iter()
                .filter_map(|(n, a)| a.as_ref().map(|x| (n, x)))
                .collect();
                if arms.len() != 1 {
                    out.push(Violation::new(
                        codes::INVALID_CRITERION,
                        format!("{pointer}/linked/where"),
                    ));
                    return;
                }
                for (i, c) in arms[0].1.iter().enumerate() {
                    let ptr = format!("{pointer}/linked/where/{}/{i}", arms[0].0);
                    check_condition(schema, at, c, target, false, &ptr, out);
                }
            }
        }
        Condition::All(cs) => {
            for (i, c) in cs.iter().enumerate() {
                check_condition(
                    schema,
                    at,
                    c,
                    context,
                    allow_now,
                    &format!("{pointer}/all/{i}"),
                    out,
                );
            }
        }
        Condition::Any(cs) => {
            for (i, c) in cs.iter().enumerate() {
                check_condition(
                    schema,
                    at,
                    c,
                    context,
                    allow_now,
                    &format!("{pointer}/any/{i}"),
                    out,
                );
            }
        }
        Condition::NoneOf(cs) => {
            for (i, c) in cs.iter().enumerate() {
                check_condition(
                    schema,
                    at,
                    c,
                    context,
                    allow_now,
                    &format!("{pointer}/none/{i}"),
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Accepted scalar grammars. `decimal`: `-?digits[.digits]` with ASCII
/// digits only, at least one digit on each side of the dot when a dot is
/// present; no leading `+`, no exponent (so `+1`, `.5` and `1.` are refused).
/// `date`: `YYYY-MM-DD` with ASCII digits and `-` separators only; no sign
/// inside any field. `timestamp` and `integer`: a JSON integer (`i64`/`u64`),
/// never a float or a string.
fn scalar_matches(base: ScalarType, v: &Value) -> bool {
    match base {
        ScalarType::String => v.is_string(),
        ScalarType::Integer | ScalarType::Timestamp => v.as_i64().is_some() || v.as_u64().is_some(),
        ScalarType::Boolean => v.is_boolean(),
        ScalarType::Decimal => v.as_str().is_some_and(|s| parse_decimal(s).is_some()),
        ScalarType::Date => v.as_str().is_some_and(valid_date),
    }
}

fn validate_value(schema: &Schema, ty: &PropertyType, v: &Value) -> Result<(), &'static str> {
    match ty {
        PropertyType::Scalar(s) => {
            if scalar_matches(*s, v) {
                Ok(())
            } else {
                Err(codes::WRONG_VALUE_TYPE)
            }
        }
        PropertyType::Ref(r) => {
            // The namespace of a referenced Id is not compared with the
            // schema namespace here; packet O2 sets that rule in the store,
            // which knows the installed namespaces.
            let id: Id = serde_json::from_value(v.clone()).map_err(|_| codes::WRONG_VALUE_TYPE)?;
            if id.validate().is_err() || id.object_type != r.target {
                return Err(codes::WRONG_VALUE_TYPE);
            }
            Ok(())
        }
        PropertyType::Array(a) => {
            let Value::Array(items) = v else {
                return Err(codes::WRONG_VALUE_TYPE);
            };
            for item in items {
                validate_value(schema, &a.array, item)?;
            }
            Ok(())
        }
        PropertyType::Struct(s) => {
            let Value::Object(map) = v else {
                return Err(codes::WRONG_VALUE_TYPE);
            };
            for (fname, fty) in &s.struct_ {
                match map.get(fname) {
                    Some(fv) => validate_value(schema, fty, fv)?,
                    None => return Err(codes::WRONG_VALUE_TYPE),
                }
            }
            if map.keys().any(|k| !s.struct_.contains_key(k)) {
                return Err(codes::WRONG_VALUE_TYPE);
            }
            Ok(())
        }
        PropertyType::ValueType(vt) => {
            let Some(def) = schema.value_types.get(&vt.value_type) else {
                return Err(codes::WRONG_VALUE_TYPE);
            };
            if !scalar_matches(def.base, v) {
                return Err(codes::WRONG_VALUE_TYPE);
            }
            if let Some(vals) = &def.enum_values
                && !vals.contains(v)
            {
                return Err(codes::VALUE_CONSTRAINT_VIOLATION);
            }
            if let (ScalarType::String, Value::String(s)) = (def.base, v)
                && !def.pattern_matches(s)
            {
                return Err(codes::VALUE_CONSTRAINT_VIOLATION);
            }
            if let Some(min) = &def.min
                && compare_values(v, min) == Some(std::cmp::Ordering::Less)
            {
                return Err(codes::VALUE_CONSTRAINT_VIOLATION);
            }
            if let Some(max) = &def.max
                && compare_values(v, max) == Some(std::cmp::Ordering::Greater)
            {
                return Err(codes::VALUE_CONSTRAINT_VIOLATION);
            }
            Ok(())
        }
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if let (Some(xi), Some(yi)) = (x.as_i64(), y.as_i64()) {
                Some(xi.cmp(&yi))
            } else {
                x.as_f64()?.partial_cmp(&y.as_f64()?)
            }
        }
        (Value::String(x), Value::String(y)) => {
            Some(compare_decimal(x, y).unwrap_or_else(|| x.cmp(y)))
        }
        _ => None,
    }
}

/// The order of two decimals in the decimal spelling (`-?digits[.digits]`),
/// of any length: `"0999"` is greater than `"100"`, `"1.0"` equals `"1"`,
/// `"-0"` equals `"0"`. `None` when either is not such a decimal. The one
/// decimal order of the engine: value-type bounds and action criteria.
pub fn compare_decimal(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    Some(cmp_decimal(&parse_decimal(a)?, &parse_decimal(b)?))
}

/// Whether a declared type is `decimal`: the scalar, or a value type of
/// `schema` whose base is `decimal`.
pub fn is_decimal_type(schema: &Schema, ty: &PropertyType) -> bool {
    match ty {
        PropertyType::Scalar(ScalarType::Decimal) => true,
        PropertyType::ValueType(vt) => schema
            .value_types
            .get(&vt.value_type)
            .is_some_and(|d| d.base == ScalarType::Decimal),
        _ => false,
    }
}

type DecimalParts = (bool, String, String);
fn parse_decimal(s: &str) -> Option<DecimalParts> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    if rest.starts_with('+') {
        return None;
    }
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, f),
        None => (rest, ""),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if rest.contains('.') && (frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some((
        neg,
        int.trim_start_matches('0').to_string(),
        frac.trim_end_matches('0').to_string(),
    ))
}
fn cmp_decimal(a: &DecimalParts, b: &DecimalParts) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let zero = |d: &DecimalParts| d.1.is_empty() && d.2.is_empty();
    if zero(a) && zero(b) {
        return Ordering::Equal;
    }
    match (a.0, b.0) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let mag =
        a.1.len()
            .cmp(&b.1.len())
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| {
                let n = a.2.len().max(b.2.len());
                let fa = format!("{:0<n$}", a.2);
                let fb = format!("{:0<n$}", b.2);
                fa.cmp(&fb)
            });
    if a.0 { mag.reverse() } else { mag }
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        s[0..4].parse::<i64>(),
        s[5..7].parse::<u32>(),
        s[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    if !(1..=12).contains(&m) || d == 0 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    d <= days[(m - 1) as usize]
}

/// Several checked domains held together; type names are unique across it.
#[derive(Debug, Clone, Default)]
pub struct SchemaSet {
    domains: BTreeMap<String, Schema>,
    pins: BTreeMap<String, SchemaPin>,
}
impl SchemaSet {
    /// Compose strictly checked new schemas or compatibility-checked installed
    /// schemas. Recheck every rule except the create-key admission rule, which
    /// check_bytes already enforces for new schemas. Existing domains may retain
    /// that violation during an upgrade; actions still resolve keys at execution.
    pub fn new(checked: Vec<CheckedSchema>) -> Result<Self, Vec<Violation>> {
        Self::build(
            checked
                .into_iter()
                .map(|c| (c.schema, Some(c.pin)))
                .collect(),
        )
    }
    fn build(entries: Vec<(Schema, Option<SchemaPin>)>) -> Result<Self, Vec<Violation>> {
        let types_by_domain: Vec<(String, BTreeSet<String>)> = entries
            .iter()
            .map(|(s, _)| (s.domain.clone(), s.object_types.keys().cloned().collect()))
            .collect();
        let mut violations = Vec::new();
        let mut view_domains = BTreeMap::new();
        for (schema, _) in &entries {
            for name in schema.views.keys() {
                if view_domains.insert(name, &schema.domain).is_some() {
                    violations.push(Violation::new(
                        codes::DUPLICATE_TYPE_NAME,
                        format!("/views/{}", esc(name)),
                    ));
                }
            }
        }
        let mut domains: BTreeMap<String, Schema> = BTreeMap::new();
        let mut pins = BTreeMap::new();
        for (schema, pin) in entries {
            if domains.contains_key(&schema.domain) {
                violations.push(Violation::new(codes::DUPLICATE_DOMAIN, "/domain".into()));
                continue;
            }
            let others: BTreeSet<String> = types_by_domain
                .iter()
                .filter(|(d, _)| d != &schema.domain)
                .flat_map(|(_, ts)| ts.iter().cloned())
                .collect();
            violations.extend(
                check(&schema, &others)
                    .into_iter()
                    .filter(|v| !installed_exempt(v.code)),
            );
            if let Some(p) = pin {
                pins.insert(schema.domain.clone(), p);
            }
            domains.insert(schema.domain.clone(), schema);
        }
        if violations.is_empty() {
            Ok(Self { domains, pins })
        } else {
            Err(violations)
        }
    }
    pub fn schema(&self, domain: &str) -> Option<&Schema> {
        self.domains.get(domain)
    }
    /// This set with one domain replaced (or added) by a checked schema.
    pub fn with_domain(&self, checked: CheckedSchema) -> Result<Self, Vec<Violation>> {
        let mut entries: Vec<(Schema, Option<SchemaPin>)> = self
            .domains
            .iter()
            .filter(|(d, _)| *d != &checked.schema.domain)
            .map(|(d, s)| (s.clone(), self.pins.get(d).cloned()))
            .collect();
        entries.push((checked.schema, Some(checked.pin)));
        Self::build(entries)
    }
    pub fn pin(&self, domain: &str) -> Option<&SchemaPin> {
        self.pins.get(domain)
    }
    pub fn pins(&self) -> &BTreeMap<String, SchemaPin> {
        &self.pins
    }
    pub fn object_type(&self, name: &str) -> Option<&ObjectType> {
        self.domains.values().find_map(|s| s.object_types.get(name))
    }
    pub fn schema_of(&self, object_type: &str) -> Option<&Schema> {
        self.domains
            .values()
            .find(|s| s.object_types.contains_key(object_type))
    }
    pub fn properties(&self, object_type: &str) -> Option<&BTreeMap<String, PropertyDef>> {
        self.object_type(object_type).map(|t| &t.properties)
    }
    pub fn owner(&self, object_type: &str) -> Option<Owner> {
        self.object_type(object_type).map(|t| t.owner.clone())
    }
    pub fn link_types_from<'a>(&'a self, object_type: &str) -> Vec<(&'a String, &'a LinkType)> {
        self.domains
            .values()
            .flat_map(|s| &s.link_types)
            .filter(|(_, l)| l.from == object_type)
            .collect()
    }
    pub fn link_types_to<'a>(&'a self, object_type: &str) -> Vec<(&'a String, &'a LinkType)> {
        self.domains
            .values()
            .flat_map(|s| &s.link_types)
            .filter(|(_, l)| l.to == object_type)
            .collect()
    }
    pub fn validate_fields(
        &self,
        object_type: &str,
        fields: &BTreeMap<String, Knowledge>,
    ) -> Result<(), Violation> {
        match self.schema_of(object_type) {
            Some(s) => s.validate_fields(object_type, fields),
            None => Err(Violation::new(codes::UNKNOWN_TYPE, String::new())),
        }
    }
    /// Validate one value against a property type in the owning schema.
    pub fn validate_value(
        &self,
        object_type: &str,
        ty: &PropertyType,
        v: &Value,
    ) -> Result<(), Violation> {
        match self.schema_of(object_type) {
            Some(s) => s.validate_value(ty, v),
            None => Err(Violation::new(codes::UNKNOWN_TYPE, String::new())),
        }
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn glob_syntax_is_refused_or_matched() {
        // spec: specs/ont/index.md#R4
        for bad in [
            "rel/*.md", "/a/{x,y}", "/a/[ab]", "/a/**", "/a/x**/f", "/a//f", "/a/./f", "/a/../f",
            "/a/\\f", "/a/f/",
        ] {
            assert!(Glob::parse(bad).is_err(), "{bad}");
        }
        let g = Glob::parse("/w/*/STATE.md").unwrap();
        assert!(g.matches("/w/a/STATE.md"));
        assert!(!g.matches("/w/a/b/STATE.md"));
        assert!(!g.matches("/w/STATE.md"));
        let g = Glob::parse("/w/**/f?.md").unwrap();
        assert!(g.matches("/w/f1.md"));
        assert!(g.matches("/w/a/b/c/f1.md"));
        assert!(!g.matches("/w/a/f10.md"));
        let g = Glob::parse("/w/a*b*c").unwrap();
        assert!(g.matches("/w/abc") && g.matches("/w/aXXbYYc") && !g.matches("/w/aXXbYY"));
    }

    #[test]
    fn double_star_matching_is_polynomial_and_capped() {
        // spec: specs/ont/index.md#R4
        // Eight `**` against a 64-segment path that does not match: the
        // time bound (5 s) is generous; the old matcher took exponential time.
        let glob = format!("/{}x", "**/".repeat(8));
        let g = Glob::parse(&glob).unwrap();
        let path = format!("/{}", vec!["a"; 64].join("/"));
        let start = std::time::Instant::now();
        assert!(!g.matches(&path));
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(g.matches("/a/b/x"));
        assert!(Glob::parse(&format!("/{}x", "**/".repeat(9))).is_err());
    }

    #[test]
    fn source_options_are_parsed_strictly() {
        let def = |v: Value| -> SourceDef { serde_json::from_value(v).unwrap() };
        let spec = def(json!({"kind":"markdown_fields","glob":"/a/*/S.md","fresh_for_s":5}))
            .file_spec()
            .unwrap()
            .unwrap();
        assert_eq!(
            (spec.head_lines, spec.complete_listing, spec.fresh_for_s),
            (40, false, 5)
        );
        assert!(
            def(json!({"kind":"unknown_kind","x":1}))
                .file_spec()
                .unwrap()
                .is_none()
        );
        let err = def(json!({"kind":"markdown_fields","glob":"/a/*/S.md"}))
            .file_spec()
            .unwrap_err();
        assert_eq!(
            (err.code, err.pointer.as_str()),
            ("invalid_source_option", "/fresh_for_s")
        );
    }
}
