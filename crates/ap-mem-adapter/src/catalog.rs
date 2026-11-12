use crate::types::{ReadRequest, RecordId, Subject};
use serde::Deserialize;
use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "Engineering memory reads only. Resolve the subject and form an English query; preserve Korean proper nouns. Start with mem_recall, then use a focused page or exact document when needed. mem_tree_read supplies filtered navigation. Only ready records and pages without pending subjects are returned. unresolved_ready_count counts current ready records with unavailable Git metadata; a positive count means partial memory. Busy or unavailable means no current-memory answer; do not fall back to another endpoint. Preserve attribution and verify operational claims at their native owner. Publication is outside MCP.";

pub fn tools() -> Value {
    let string = |max| json!({"type":"string","minLength":1,"maxLength":max});
    let subject = json!({"type":"object","additionalProperties":false,
        "properties":{"type":{"type":"string","enum":["platform","module","host","task","procedure","user","term","memory-map"]},"id":string(200)},"required":["type","id"]});
    let tool = |name, description, properties, required| {
        json!({
        "name":name,"description":description,"inputSchema":{
            "type":"object","additionalProperties":false,"properties":properties,"required":required}})
    };
    let query = json!({"subject":subject,"query":string(2048)});
    json!({"tools":[
        tool("mem_recall","Recall ready engineering records for one subject.",query.clone(),json!(["subject","query"])),
        tool("mem_page_search","Find visible subject pages.",query,json!(["subject","query"])),
        tool("mem_page_read","Read a visible page by id.",json!({"page_id":string(200)}),json!(["page_id"])),
        tool("mem_tree_read","Read filtered memory navigation.",json!({}),json!([])),
        tool("mem_document_read","Read one ready record by id.",json!({"record_id":{"type":"string","maxLength":200,"pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"}}),json!(["record_id"]))
    ]})
}

pub fn tools_list(id: Value) -> Value {
    json!({"id":id,"jsonrpc":"2.0","result":tools()})
}

pub fn initialize() -> Value {
    json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},
        "serverInfo":{"name":"ap-mem-adapter","version":env!("CARGO_PKG_VERSION")},
        "instructions":INSTRUCTIONS})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    subject: Subject,
    query: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageArgs {
    page_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentArgs {
    record_id: RecordId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

fn bounded(s: &str, max: usize) -> Result<(), &'static str> {
    if s.trim().is_empty() || s.chars().count() > max {
        Err("invalid_arguments")
    } else {
        Ok(())
    }
}

fn decode<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, &'static str> {
    serde_json::from_value(args).map_err(|_| "invalid_arguments")
}

/// Closed dispatch; no argument can select an upstream tool, path or bank.
pub fn parse_call(name: &str, args: Value) -> Result<ReadRequest, &'static str> {
    Ok(match name {
        "mem_recall" | "mem_page_search" => {
            let a: QueryArgs = decode(args)?;
            bounded(&a.query, 2048)?;
            bounded(&a.subject.id, 200)?;
            if name == "mem_recall" {
                ReadRequest::Recall {
                    subject: a.subject,
                    query: a.query,
                }
            } else {
                ReadRequest::PageSearch {
                    subject: a.subject,
                    query: a.query,
                }
            }
        }
        "mem_page_read" => {
            let a: PageArgs = decode(args)?;
            bounded(&a.page_id, 200)?;
            ReadRequest::PageRead { page_id: a.page_id }
        }
        "mem_tree_read" => {
            let _: EmptyArgs = decode(args)?;
            ReadRequest::TreeRead
        }
        "mem_document_read" => {
            let a: DocumentArgs = decode(args)?;
            ReadRequest::DocumentRead {
                record_id: a.record_id,
            }
        }
        _ => return Err("unknown_tool"),
    })
}
