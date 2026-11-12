//! Recorded wire shapes and explicitly advanced traces; never a serializer of state.
#![allow(dead_code)] // This module is shared by separately compiled integration targets.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const BANK: &str = "fixture-bank";
const STAMP: &str = "2001-02-03T04:05:06Z";
const PREFIX: &str = "/v1/default/banks/fixture-bank";

#[derive(Clone, Debug)]
pub struct WireRequest {
    pub method: String,
    pub path: String,
    pub body: Value,
    pub authorized: bool,
    pub session: Option<String>,
}
impl WireRequest {
    pub fn new(method: &str, path: &str, body: Value) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            body,
            authorized: true,
            session: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RecordedReply {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub id: String,
    pub original_text: String,
    pub tags: Vec<String>,
    pub body_hash: Option<String>,
    pub observation_scopes: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Fact {
    pub id: String,
    pub document_id: String,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct MentalModel {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
enum Status {
    Pending,
    Completed,
}
#[derive(Clone, Debug)]
struct Operation {
    id: String,
    accepted: Vec<Document>, // Never echoed by a list/default lookup.
    status: Status,
    child: Option<(String, Status, Option<String>)>,
}
/// The controller selects recorded transitions; there is no clock or completion race.
pub enum TraceStep {
    Retained {
        operation_id: String,
        child_id: String,
        facts: Vec<Fact>,
    },
    ParentPendingChildError {
        operation_id: String,
        child_id: String,
    },
    Observations(Vec<Fact>),
    RefreshCompleted,
    MaintenanceCompleted,
}
#[derive(Default)]
pub struct Replay {
    documents: BTreeMap<String, Document>,
    facts: BTreeMap<String, Fact>,
    operations: BTreeMap<String, Operation>,
    by_document: BTreeMap<String, BTreeSet<String>>,
    models: Vec<MentalModel>,
    observations: Vec<Fact>,
    absent: bool,
    refresh: bool,
    maintenance: bool,
    initialized: bool,
    upstream_defaults: bool,
}
impl Replay {
    /// The enabled/empty-overrides snapshot measured in 161, distinct from 006.
    pub fn upstream_defaults() -> Self {
        Self {
            upstream_defaults: true,
            ..Self::default()
        }
    }
    pub fn with_models(models: Vec<MentalModel>) -> Self {
        Self {
            models,
            ..Self::default()
        }
    }
    pub fn absent() -> Self {
        Self {
            absent: true,
            ..Self::default()
        }
    }
    pub fn active_operations(&self) -> usize {
        self.operations
            .values()
            .filter(|o| o.status == Status::Pending)
            .count()
    }
    pub fn accepted_documents(&self, id: &str) -> &[Document] {
        &self.operations[id].accepted
    }
    pub fn advance(&mut self, step: TraceStep) {
        match step {
            TraceStep::Retained {
                operation_id,
                child_id,
                facts,
            } => {
                let operation = self
                    .operations
                    .get_mut(&operation_id)
                    .expect("declared retain");
                assert_eq!(
                    operation.status,
                    Status::Pending,
                    "trace cannot complete twice"
                );
                for doc in &operation.accepted {
                    self.facts.retain(|_, f| f.document_id != doc.id);
                    self.documents.insert(doc.id.clone(), doc.clone());
                    assert!(
                        facts
                            .iter()
                            .all(|f| operation.accepted.iter().any(|d| d.id == f.document_id))
                    );
                }
                for fact in facts {
                    self.facts.insert(fact.id.clone(), fact);
                }
                operation.status = Status::Completed;
                operation.child = Some((child_id, Status::Completed, None));
            }
            TraceStep::ParentPendingChildError {
                operation_id,
                child_id,
            } => {
                let operation = self
                    .operations
                    .get_mut(&operation_id)
                    .expect("declared retain");
                assert_eq!(operation.status, Status::Pending);
                operation.child = Some((
                    child_id,
                    Status::Pending,
                    Some("fixture extraction error".into()),
                ));
            }
            TraceStep::Observations(facts) => self.observations = facts,
            TraceStep::RefreshCompleted => self.refresh = true,
            TraceStep::MaintenanceCompleted => self.maintenance = true,
        }
    }
    pub fn apply(&mut self, request: WireRequest) -> RecordedReply {
        if request.authorized && !self.absent {
            let route = request.path.split('?').next().unwrap();
            if route == "/mcp/fixture-bank/" && request.body["method"] == "initialize" {
                self.initialized = true;
            }
            if request.method == "POST" && route == format!("{PREFIX}/memories") {
                let id = request.body["operation_id"]
                    .as_str()
                    .expect("fixture operation id");
                // 010/012/054: changed bytes under the same identity keep the first intent.
                if !self.operations.contains_key(id) {
                    let documents: Vec<_> = request.body["items"]
                        .as_array()
                        .expect("fixture items")
                        .iter()
                        .map(|v| {
                            if let Some(m) = v["metadata"].as_object() {
                                assert!(
                                    m.keys().all(|k| k == "body_hash"),
                                    "uncaptured metadata keys"
                                );
                            }
                            Document {
                                id: v["document_id"].as_str().unwrap().into(),
                                original_text: v["content"].as_str().unwrap().into(),
                                tags: strings(&v["tags"]),
                                body_hash: v["metadata"]["body_hash"].as_str().map(str::to_owned),
                                observation_scopes: strings(&v["observation_scopes"]),
                            }
                        })
                        .collect();
                    for doc in &documents {
                        self.by_document
                            .entry(doc.id.clone())
                            .or_default()
                            .insert(id.into());
                    }
                    self.operations.insert(
                        id.into(),
                        Operation {
                            id: id.into(),
                            accepted: documents,
                            status: Status::Pending,
                            child: None,
                        },
                    );
                }
            }
        }
        // DELETE projects the pre-delete count, then applies the measured synchronous removal.
        let reply = project(self, &request);
        if request.authorized && !self.absent && request.method == "DELETE" && reply.status == 200 {
            let id = request
                .path
                .strip_prefix(&format!("{PREFIX}/documents/"))
                .unwrap();
            self.documents.remove(id);
            self.facts.retain(|_, f| f.document_id != id);
        }
        reply
    }
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().into()).collect())
        .unwrap_or_default()
}
fn text(status: &Status) -> &'static str {
    match status {
        Status::Pending => "pending",
        Status::Completed => "completed",
    }
}
fn query(path: &str) -> BTreeMap<&str, &str> {
    path.split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .map(|p| p.split_once('=').unwrap_or((p, "")))
                .collect()
        })
        .unwrap_or_default()
}

/// The ONLY constructor of nominal wire values, including errors and SSE bodies.
/// Unsupported, uncaptured variants panic rather than inventing a successful answer.
pub fn project(replay: &Replay, request: &WireRequest) -> RecordedReply {
    let mut status = 200;
    let mut headers = BTreeMap::from([("content-type".into(), "application/json".into())]);
    let (path, _) = request.path.split_once('?').unwrap_or((&request.path, ""));
    let params = query(&request.path);
    let limit = params
        .get("limit")
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(100);
    let offset = params
        .get("offset")
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(0);
    let doc_value = |doc: &Document, mcp: bool| {
        let hash = doc
            .body_hash
            .as_ref()
            .map(|h| json!({"body_hash":h}))
            .unwrap_or(Value::Null);
        let mut retain = json!({"event_date":null,"observation_scopes":doc.observation_scopes});
        if !hash.is_null() {
            retain["metadata"] = hash.clone();
        }
        let count = replay
            .facts
            .values()
            .filter(|f| f.document_id == doc.id)
            .count();
        let mut value = json!({"id":doc.id,"bank_id":BANK,"original_text":doc.original_text,"content_hash":"fixture-content-hash","created_at":STAMP,"updated_at":STAMP,"memory_unit_count":count,"nodes_by_fact_type":{"world":count,"experience":0,"observation":0},"tags":doc.tags,"document_metadata":hash,"retain_params":retain,"observation_scopes":null});
        if !mcp {
            value["attachments"] = Value::Null;
        }
        value
    };
    let fact_value = |f: &Fact, observation: bool| {
        let doc = replay.documents.get(&f.document_id);
        let hash = doc
            .and_then(|d| d.body_hash.as_ref())
            .map(|h| json!({"body_hash":h}))
            .unwrap_or(json!({}));
        let tags = doc
            .map(|d| d.tags.clone())
            .unwrap_or(vec!["fixture-observation-tag".into()]);
        json!({"id":f.id,"text":f.text,"context":"fixture-context","date":STAMP,"fact_type":if observation {"observation"} else {"world"},"document_id":if observation {None} else {Some(&f.document_id)},"mentioned_at":if observation {Some(STAMP)} else {None},"occurred_start":null,"occurred_end":null,"entities":"fixture-entities","chunk_id":if observation {None} else {Some(format!("{}-chunk",f.id))},"proof_count":1,"tags":tags,"metadata":hash,"consolidated_at":null,"consolidation_failed_at":null,"state":"active","invalidation_reason":null,"invalidated_at":null,"edited_at":null,"updated_at":STAMP,"source_memory_ids":if observation {vec!["fixture-source"]} else {vec![]}})
    };
    let operation_value = |o: &Operation, child: bool| {
        let id = if child {
            &o.child.as_ref().unwrap().0
        } else {
            &o.id
        };
        let state = if child {
            &o.child.as_ref().unwrap().1
        } else {
            &o.status
        };
        json!({"id":id,"task_type":if child {"retain"} else {"batch_retain"},"operation_id":id,"operation_type":if child {"retain"} else {"batch_retain"},"items_count":o.accepted.len(),"document_id":o.accepted[0].id,"filename":null,"mental_model_id":null,"details":null,"created_at":STAMP,"updated_at":STAMP,"status":text(state),"error_message":null,"retry_count":0,"next_retry_at":null,"progress":null})
    };
    let recall_value = |mcp: bool| {
        let mut chunks = serde_json::Map::new();
        let results: Vec<_> = replay.facts.values().map(|f| {
            let d = &replay.documents[&f.document_id];
            let chunk = format!("{}-chunk",f.id);
            chunks.insert(chunk.clone(), if mcp {json!({"chunk_text":d.original_text,"chunk_index":0,"truncated":false})} else {json!({"id":chunk,"text":d.original_text,"chunk_index":0,"truncated":false,"attachments":null})});
            let mut value = json!({"id":f.id,"text":f.text,"entities":null,"context":"fixture-context","occurred_start":null,"occurred_end":null,"mentioned_at":null,"document_id":d.id,"metadata":d.body_hash.as_ref().map(|h| json!({"body_hash":h})).unwrap_or(json!({})),"chunk_id":chunk,"tags":d.tags,"source_fact_ids":null,"scores":{"final":0.25,"reranker":0.25,"semantic":0.25,"keyword":0.25}});
            value[if mcp {"fact_type"} else {"type"}] = json!("world");
            if !mcp { value["attachments"] = Value::Null; }
            value
        }).collect();
        json!({"results":results,"trace":null,"entities":null,"chunks":chunks,"source_facts":null,"source_facts_truncated":if mcp {Some(false)} else {None}})
    };
    let body = if !request.authorized {
        status = 401;
        json!({"detail":"fixture authentication refusal"})
    } else if replay.absent {
        status = 404;
        json!({"detail":"fixture absent bank"})
    } else if path == "/mcp/fixture-bank/" && request.method == "POST" {
        let method = request.body["method"].as_str().unwrap();
        if method != "initialize" {
            assert!(
                replay.initialized && request.session.as_deref() == Some("fixture-session"),
                "uncaptured missing-session request"
            );
        }
        let result = match method {
            "initialize" => {
                json!({"protocolVersion":"2025-03-26","capabilities":{"experimental":{},"logging":{},"prompts":{"listChanged":false},"resources":{"subscribe":false,"listChanged":false},"tools":{"listChanged":false},"extensions":{"io.modelcontextprotocol/ui":{}}},"serverInfo":{"name":"fixture-server","version":"0.0.0"}})
            }
            "tools/list" => {
                serde_json::from_str::<Value>(r#"{"tools":[{"name":"recall","description":"fixture-description","inputSchema":{"additionalProperties":false,"properties":{"query":{"type":"string"},"max_tokens":{"default":1,"type":"integer"},"budget":{"default":"fixture-description","type":"string"},"types":{"anyOf":[{"items":{"type":"string"},"type":"array"},{"type":"null"}],"default":null},"prefer_observations":{"anyOf":[{"type":"boolean"},{"type":"null"}],"default":null},"tags":{"anyOf":[{"items":{"type":"string"},"type":"array"},{"type":"null"}],"default":null},"tags_match":{"default":"fixture-description","enum":["any","all","any_strict","all_strict","exact"],"type":"string"},"tag_groups":{"anyOf":[{"items":{"additionalProperties":true,"type":"object"},"type":"array"},{"type":"null"}],"default":null},"query_timestamp":{"anyOf":[{"type":"string"},{"type":"null"}],"default":null},"min_scores":{"anyOf":[{"additionalProperties":true,"type":"object"},{"type":"null"}],"default":null},"temporal_window":{"anyOf":[{"additionalProperties":true,"type":"object"},{"type":"null"}],"default":null},"rerank":{"anyOf":[{"additionalProperties":true,"type":"object"},{"type":"null"}],"default":null},"include_chunks":{"default":false,"type":"boolean"},"max_chunk_tokens":{"default":1,"type":"integer"},"include_entities":{"default":false,"type":"boolean"},"max_entity_tokens":{"default":1,"type":"integer"},"include_source_facts":{"default":false,"type":"boolean"},"max_source_facts_tokens":{"default":1,"type":"integer"}},"required":["query"],"type":"object"},"outputSchema":{"additionalProperties":true,"type":"object"},"annotations":{"readOnlyHint":true,"openWorldHint":false},"_meta":{"fastmcp":{"tags":[]}}},{"name":"get_document","description":"fixture-description","inputSchema":{"additionalProperties":false,"properties":{"document_id":{"type":"string","description":"fixture-description"}},"required":["document_id"],"type":"object"},"outputSchema":{"additionalProperties":true,"type":"object"},"annotations":{"readOnlyHint":true,"openWorldHint":false},"_meta":{"fastmcp":{"tags":[]}}}]}"#).unwrap()
            }
            "tools/call" => {
                let args = &request.body["params"]["arguments"];
                let value = match request.body["params"]["name"].as_str().unwrap() {
                    "recall" => {
                        assert!(args["query"].is_string(), "fixed recall query argument");
                        recall_value(true)
                    },
                    "get_document" => doc_value(
                        &replay.documents[args["document_id"].as_str().unwrap()],
                        true,
                    ),
                    _ => panic!("uncaptured RPC tool"),
                };
                json!({"content":[{"type":"text","text":serde_json::to_string(&value).unwrap()}],"structuredContent":value,"isError":false})
            }
            _ => panic!("uncaptured RPC method"),
        };
        headers.insert("content-type".into(), "text/event-stream".into());
        headers.insert("mcp-session-id".into(), "fixture-session".into());
        json!(format!(
            "event: message\ndata: {}\n\n",
            json!({"jsonrpc":"2.0","id":request.body["id"],"result":result})
        ))
    } else {
        let route = path.strip_prefix(PREFIX).expect("fixed fixture bank only");
        match (request.method.as_str(), route) {
            ("GET", "/config") => {
                let mut config = serde_json::from_str::<Value>(r#"{"bank_id":"fixture-bank","config":{"recall_budget_max":0,"recall_budget_fixed_mid":0,"observation_scope_limits":null,"mental_model_delta_use_decision_model":false,"store_document_text":false,"retain_custom_instructions":null,"llm_gemini_safety_settings":null,"enable_auto_consolidation":false,"recall_budget_adaptive_low":0.25,"retain_max_attachments_per_chunk":0,"consolidation_max_memories_per_round":0,"retain_strategies":null,"recall_max_tokens":0,"retain_chunk_batch_size":0,"recall_chunks_max_tokens":0,"retain_chunk_size":0,"default_observation_scopes":null,"consolidation_use_decision_model":false,"enable_reranking":false,"reflect_source_facts_max_tokens":0,"recall_budget_min":0,"disposition_literalism":null,"recall_decision_model_dedup":false,"enable_observations":false,"consolidation_source_facts_max_tokens":0,"entities_allow_free_form":false,"reflect_use_decision_model":false,"mental_model_refresh_use_decision_model":false,"observations_mission":null,"recall_budget_adaptive_high":0.25,"max_observations_per_scope":0,"recall_budget_adaptive_mid":0.25,"recall_include_chunks":false,"memory_defense":null,"default_timezone":"fixture-value","consolidation_source_facts_max_tokens_per_observation":0,"recall_budget_fixed_low":0,"entity_labels":null,"mental_model_min_refresh_interval_seconds":0,"disposition_skepticism":null,"enable_text_search":false,"enable_temporal_retrieval":false,"consolidation_llm_batch_size":0,"retain_default_strategy":null,"retain_mission":null,"recall_budget_function":"fixture-value","consolidation_strategies":null,"enable_graph_retrieval":false,"recall_decision_model_prune":false,"retain_extraction_mode":"fixture-value","reflect_default_options":null,"recall_budget_fixed_high":0,"retain_structured_chunk_size":null,"knowledge_page_default_trigger":null,"disposition_empathy":null,"reflect_mission":null,"recall_prefer_observations":false,"audit_log_enabled":false,"mcp_enabled_tools":null,"consolidation_llm_parallelism":0},"overrides":{"enable_observations":false,"enable_auto_consolidation":false}}"#).unwrap();
                config["config"]["enable_observations"] = json!(replay.upstream_defaults);
                config["config"]["enable_auto_consolidation"] = json!(replay.upstream_defaults);
                if replay.upstream_defaults {
                    config["overrides"] = json!({});
                }
                config
            }
            ("GET", "/stats") => {
                let mut stats = json!({"bank_id":"fixture-bank","total_nodes":0,"total_links":0,"total_documents":0,"nodes_by_fact_type":{"observation":0,"world":0},"links_by_link_type":{"semantic":0,"entity":0},"links_by_fact_type":{},"links_breakdown":{},"pending_operations":0,"failed_operations":0,"operations_by_status":{"completed":0},"last_consolidated_at":"fixture-value","last_memory_write_at":"fixture-value","pending_consolidation":0,"failed_consolidation":0,"total_observations":0});
                stats["total_documents"] = json!(replay.documents.len());
                stats["total_observations"] = json!(replay.observations.len());
                stats
            }
            ("POST", "/memories") => {
                let o = &replay.operations[request.body["operation_id"].as_str().unwrap()];
                json!({"success":true,"bank_id":BANK,"items_count":o.accepted.len(),"async":true,"operation_id":o.id,"operation_ids":null,"usage":null})
            }
            ("GET", "/mental-models") => {
                let tags: Vec<_> = params
                    .get("tags")
                    .map(|s| s.split(',').collect())
                    .unwrap_or_default();
                let models: Vec<_> = replay
                    .models
                    .iter()
                    .filter(
                        |m| match params.get("tags_match").copied().unwrap_or("any") {
                            "exact" => {
                                m.tags.iter().map(String::as_str).collect::<BTreeSet<_>>()
                                    == tags.iter().copied().collect()
                            }
                            "all" => tags.iter().all(|t| m.tags.iter().any(|x| x == t)),
                            "any" => {
                                tags.is_empty()
                                    || tags.iter().any(|t| m.tags.iter().any(|x| x == t))
                            }
                            _ => panic!("uncaptured tags_match"),
                        },
                    )
                    .collect();
                let items: Vec<_> = models.iter().skip(offset).take(limit).map(|m| {
                    assert_eq!(params.get("detail").copied().unwrap_or("metadata"),"metadata","only empty full-detail page recorded");
                    json!({"id":m.id,"bank_id":BANK,"name":m.name,"source_query":null,"content":null,"tags":m.tags,"max_tokens":null,"trigger":null,"last_refreshed_at":STAMP,"last_memory_seen_at":null,"last_refresh_failed_at":null,"last_full_refreshed_at":null,"created_at":STAMP,"is_knowledge_page":false,"reflect_response":null,"is_stale":false})
                }).collect();
                json!({"items":items,"total":models.len(),"limit":limit,"offset":offset})
            }
            ("GET", "/memories/list") => {
                let observations = params.get("type").copied() == Some("observation");
                let facts: Vec<_> = if observations {
                    replay.observations.iter().collect()
                } else {
                    replay
                        .facts
                        .values()
                        .filter(|f| {
                            params
                                .get("document_id")
                                .is_none_or(|id| *id == f.document_id)
                        })
                        .collect()
                };
                let items: Vec<_> = facts
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|f| fact_value(f, observations))
                    .collect();
                json!({"items":items,"total":facts.len(),"limit":limit,"offset":offset})
            }
            ("GET", "/documents") => {
                let items: Vec<_> = replay.documents.values().skip(offset).take(limit).map(|d| {
                    assert!(d.body_hash.is_none(),"document-summary metadata variant not captured");
                    json!({"id":d.id,"bank_id":BANK,"content_hash":"fixture-content-hash","created_at":STAMP,"updated_at":STAMP,"text_length":d.original_text.len(),"memory_unit_count":replay.facts.values().filter(|f| f.document_id==d.id).count(),"retain_params":{"event_date":null,"observation_scopes":d.observation_scopes},"document_metadata":null,"tags":d.tags,"matched_fields":[]})
                }).collect();
                json!({"items":items,"total":replay.documents.len(),"limit":limit,"offset":offset,"total_is_estimate":false})
            }
            ("POST", "/memories/recall") => recall_value(false),
            ("GET", "/operations") => {
                let mut items = Vec::new();
                for o in replay.operations.values().filter(|o| {
                    params.get("document_id").is_none_or(|id| {
                        replay
                            .by_document
                            .get(*id)
                            .is_some_and(|ids| ids.contains(&o.id))
                    })
                }) {
                    items.push(operation_value(o, false));
                    if o.child.is_some() {
                        items.push(operation_value(o, true));
                    }
                }
                for (present, kind) in [
                    (replay.refresh, "refresh_mental_model"),
                    (replay.maintenance, "graph_maintenance"),
                ] {
                    if present {
                        let mut v = json!({"id":"fixture-background-id","task_type":kind,"operation_id":"fixture-background-operation","operation_type":kind,"items_count":1,"document_id":null,"filename":null,"mental_model_id":null,"details":null,"created_at":STAMP,"updated_at":STAMP,"status":"completed","error_message":null,"retry_count":0,"next_retry_at":null,"progress":null});
                        if kind == "refresh_mental_model" {
                            v["mental_model_id"] = json!("fixture-model-id");
                            v["details"] = json!({"operation_type":kind,"outcome":"fixture-outcome","failure_reason":null});
                        }
                        items.push(v);
                    }
                }
                json!({"bank_id":BANK,"total":items.len(),"limit":limit,"offset":offset,"operations":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>()})
            }
            (method, r) if r.starts_with("/documents/") => {
                let id = r.strip_prefix("/documents/").unwrap();
                if let Some(base) = id.strip_suffix("/chunks") {
                    assert!(
                        !replay.documents.contains_key(base),
                        "positive chunk shape not captured"
                    );
                }
                if let Some(d) = replay.documents.get(id) {
                    match method {
                        "GET" => doc_value(d, false),
                        "DELETE" => {
                            json!({"success":true,"message":"fixture deletion complete","document_id":id,"memory_units_deleted":replay.facts.values().filter(|f| f.document_id==id).count()})
                        }
                        _ => panic!("uncaptured document method"),
                    }
                } else {
                    status = 404;
                    json!({"detail":"fixture absent document"})
                }
            }
            ("GET", r) if r.starts_with("/operations/") => {
                let id = r.strip_prefix("/operations/").unwrap();
                let (o, child) = replay
                    .operations
                    .values()
                    .find_map(|o| {
                        if o.id == id {
                            Some((o, false))
                        } else if o.child.as_ref().is_some_and(|c| c.0 == id) {
                            Some((o, true))
                        } else {
                            None
                        }
                    })
                    .expect("uncaptured operation404");
                let payload = params.get("include_payload").copied() == Some("true");
                assert!(
                    (!child && o.status == Status::Pending)
                        || (child && o.child.as_ref().unwrap().1 == Status::Completed && payload),
                    "uncaptured lookup variant"
                );
                let mut value = json!({"operation_id":id,"status":if child {"completed"} else {"pending"},"operation_type":if child {"retain"} else {"batch_retain"},"id":id,"task_type":if child {"retain"} else {"batch_retain"},"mental_model_id":null,"created_at":STAMP,"updated_at":STAMP,"completed_at":null,"error_message":null,"retry_count":0,"next_retry_at":null,"progress":null,"result_metadata":{"is_parent":true,"document_id":o.accepted[0].id,"items_count":o.accepted.len(),"total_tokens":0,"num_sub_batches":1},"details":null,"child_operations":null,"task_payload":null});
                if child {
                    value["completed_at"] = json!(STAMP);
                    value["result_metadata"] = json!({"document_id":o.accepted[0].id,"items_count":o.accepted.len(),"document_ids":o.accepted.iter().map(|d| &d.id).collect::<Vec<_>>(),"unit_ids_count":replay.facts.values().filter(|f| o.accepted.iter().any(|d| d.id==f.document_id)).count(),"document_events":o.accepted.iter().map(|d| json!({"kind":"fixture-event","fact_id":null,"created_at":STAMP,"document_id":d.id,"facts_added":1,"facts_removed":0,"observations_created":{"ids":[],"count":0},"observations_removed":{"ids":[],"count":0},"observations_updated":{"ids":[],"count":0}})).collect::<Vec<_>>(),"sub_batch_index":0,"total_sub_batches":1,"parent_operation_id":o.id,"extraction_errors_count":0});
                    value["task_payload"] = json!({"type":"retain","bank_id":BANK,"contents":o.accepted.iter().map(|d| json!({"tags":d.tags,"content":d.original_text,"event_date":null,"document_id":d.id,"update_mode":"replace","observation_scopes":d.observation_scopes})).collect::<Vec<_>>(),"operation_id":id});
                } else {
                    let c = o
                        .child
                        .as_ref()
                        .expect("pending-child-error trace required");
                    value["child_operations"] = json!([{"operation_id":c.0,"status":text(&c.1),"sub_batch_index":0,"items_count":o.accepted.len(),"error_message":c.2}]);
                }
                value
            }
            _ => panic!("uncaptured wire route"),
        }
    };
    RecordedReply {
        status,
        headers,
        body,
    }
}

pub struct Shape {
    pub recording: &'static str,
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/replay/shapes.json")).unwrap()
    })
}
/// Exact object keys, primitive/null variants, whole array alternatives, status and SSE.
/// Only the recorded `chunks` dictionaries permit dynamic keys; JSON-in-text is checked too.
pub fn assert_recorded_keys(reply: &RecordedReply, shape: &Shape) -> Result<(), String> {
    let input = &oracle()["shapes"][shape.recording];
    if input["status"].as_u64() != Some(u64::from(reply.status)) {
        return Err("status differs from recording".into());
    }
    fn check(value: &Value, node: usize, at: &str) -> Result<(), String> {
        let shape = &oracle()["nodes"][node];
        let fail = || Err(format!("recorded_shape_mismatch@{at}"));
        if let Some(kind) = shape.as_str() {
            let ok = match kind {
                "null" => value.is_null(),
                "string" => value.is_string(),
                "boolean" => value.is_boolean(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                _ => false,
            };
            return if ok { Ok(()) } else { fail() };
        }
        if let Some(fields) = shape.get("object") {
            let Some(object) = value.as_object() else {
                return fail();
            };
            let fields = fields.as_object().unwrap();
            if object.keys().collect::<BTreeSet<_>>() != fields.keys().collect() {
                return fail();
            }
            for (key, node) in fields {
                check(
                    &object[key],
                    node.as_u64().unwrap() as usize,
                    &format!("{at}/{key}"),
                )?;
            }
        } else if let Some(options) = shape.get("array").or_else(|| shape.get("map")) {
            let entries: Vec<_> = if shape.get("map").is_some() {
                let Some(v) = value.as_object() else {
                    return fail();
                };
                v.values().collect()
            } else {
                let Some(v) = value.as_array() else {
                    return fail();
                };
                v.iter().collect()
            };
            for entry in entries {
                if !options
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| check(entry, n.as_u64().unwrap() as usize, at).is_ok())
                {
                    return fail();
                }
            }
        } else if let Some(node) = shape.get("json") {
            let Some(s) = value.as_str() else {
                return fail();
            };
            let v = serde_json::from_str(s).map_err(|_| format!("invalid_json@{at}"))?;
            check(&v, node.as_u64().unwrap() as usize, at)?;
        } else if let Some(node) = shape.get("sse") {
            let Some(s) = value.as_str() else {
                return fail();
            };
            let lines: Vec<_> = s.lines().filter_map(|l| l.strip_prefix("data: ")).collect();
            if lines.len() != 1 || !s.starts_with("event: message\n") {
                return fail();
            }
            let v = serde_json::from_str(lines[0]).map_err(|_| format!("invalid_sse@{at}"))?;
            check(&v, node.as_u64().unwrap() as usize, at)?;
        } else {
            return fail();
        }
        Ok(())
    }
    if oracle()["nodes"][input["body"].as_u64().unwrap() as usize]
        .get("sse")
        .is_some()
        && (reply.headers.get("content-type").map(String::as_str) != Some("text/event-stream")
            || !reply.headers.contains_key("mcp-session-id"))
    {
        return Err("missing_sse_session_headers".into());
    }
    check(
        &reply.body,
        input["body"].as_u64().expect("known recording") as usize,
        "$",
    )
}
