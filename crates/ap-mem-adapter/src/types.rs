use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubjectType {
    Platform,
    Module,
    Host,
    Task,
    Procedure,
    User,
    Term,
    MemoryMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    #[serde(rename = "type")]
    pub kind: SubjectType,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordKind {
    Rationale,
    Incident,
    Lesson,
    Pattern,
    DirectionProposal,
    UserProfile,
    Glossary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordStatus {
    Pending,
    Ready,
    Superseded,
}

/// Stable P1a id; validated even when deserialized from an external envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordId(String);

impl TryFrom<String> for RecordId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > 200
            || value.split('-').any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
        {
            return Err("invalid_record_id");
        }
        Ok(Self(value))
    }
}

impl RecordId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<RecordId> for String {
    fn from(id: RecordId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexRow {
    pub record_id: RecordId,
    pub document_id: RecordId,
    pub status: RecordStatus,
    pub supersedes: Option<RecordId>,
    pub body_hash: String,
}

/// Only these intents may reach the upstream MCP port; never a discovered tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadRequest {
    Recall { subject: Subject, query: String },
    PageSearch { subject: Subject, query: String },
    PageRead { page_id: String },
    TreeRead,
    DocumentRead { record_id: RecordId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn closed_types() {
        // Exhaustive matches make an added enum variant a compile failure here.
        macro_rules! exact_enum {
            ($ty:ident, {$($variant:ident => $name:literal),+ $(,)?}) => {{
                let values = [$($ty::$variant),+];
                assert_eq!(serde_json::to_value(values).unwrap(), json!([$($name),+]));
                for value in values {
                    let name = match value { $($ty::$variant => $name),+ };
                    assert_eq!(serde_json::from_value::<$ty>(json!(name)).unwrap(), value);
                }
            }};
        }
        exact_enum!(SubjectType, {
            Platform => "platform", Module => "module", Host => "host", Task => "task",
            Procedure => "procedure", User => "user", Term => "term", MemoryMap => "memory-map"
        });
        exact_enum!(RecordKind, {
            Rationale => "rationale", Incident => "incident", Lesson => "lesson", Pattern => "pattern",
            DirectionProposal => "direction-proposal", UserProfile => "user-profile", Glossary => "glossary"
        });
        exact_enum!(RecordStatus, { Pending => "pending", Ready => "ready", Superseded => "superseded" });
        assert!(serde_json::from_value::<SubjectType>(json!("class")).is_err());
        assert!(serde_json::from_value::<RecordKind>(json!("status")).is_err());
        assert!(serde_json::from_value::<RecordStatus>(json!("withdrawn")).is_err());
        for id in ["", "-x", "x-", "x--y", "X", "한글", "a/b", &"a".repeat(201)] {
            assert!(serde_json::from_value::<RecordId>(json!(id)).is_err());
        }
        let id: RecordId = serde_json::from_value(json!("ap-record-1")).unwrap();
        assert_eq!(serde_json::to_value(&id).unwrap(), json!("ap-record-1"));
        assert!(
            serde_json::from_value::<Subject>(json!({"type":"task","id":"x","bank":"private"}))
                .is_err()
        );
        let row: Value = json!({"record_id":"x","document_id":"x","status":"ready","supersedes":null,"body_hash":"hash","subject":{}});
        assert!(serde_json::from_value::<IndexRow>(row).is_err());
    }
}
