#![cfg(target_os = "linux")]
use ap_kpi_producer::{
    self as kpi, Error, Result,
    capture::{self, *},
    filesystem::{self, Evidence},
    git::{self, RepositoryRequest},
    guardrails::{self, *},
    journal,
    model::*,
    process::{Call, Output, Runner},
    strict,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "ap-kpi-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().into()
    }
    fn write(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.path(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(bytes: Vec<u8>) -> Result<Output> {
    Ok(Output {
        stdout: bytes,
        exit_code: Some(0),
        signal: None,
    })
}
fn response(v: Value) -> Result<Output> {
    success(serde_json::to_vec(&v).unwrap())
}
struct Fake {
    outputs: VecDeque<Result<Output>>,
    calls: Vec<Call>,
}
impl Fake {
    fn new(outputs: Vec<Result<Output>>) -> Self {
        Self {
            outputs: outputs.into(),
            calls: vec![],
        }
    }
}
impl Runner for Fake {
    fn run(&mut self, c: &Call) -> Result<Output> {
        assert!(!c.args.iter().any(|s| matches!(
            s.as_str(),
            "fetch" | "clone" | "checkout" | "POST" | "PATCH" | "DELETE"
        )));
        if c.program == "gh" && c.args.first().is_some_and(|s| s == "api") {
            assert_eq!(&c.args[..3], ["api", "--method", "GET"]);
        }
        self.calls.push(c.clone());
        self.outputs.pop_front().expect("unexpected source command")
    }
}
fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| p.to_string()).collect()
}
fn req() -> RepositoryRequest {
    RepositoryRequest {
        manifest_key: "synthetic-repo".into(),
        github_id: 7,
        full_name: "synthetic-owner/synthetic-repo".into(),
        local_root: K::NotPerformed,
        backend: "api_only".into(),
    }
}
fn tree(paths: &[&str]) -> Value {
    json!({"sha":"b".repeat(40),"truncated":false,"tree":paths.iter().map(|p|json!({"path":p,"type":"blob","mode":"100644","sha":"c".repeat(40)})).collect::<Vec<_>>()})
}
fn git_responses(paths: &[&str]) -> Vec<Result<Output>> {
    vec![
        response(json!({"id":7,"full_name":"synthetic-owner/synthetic-repo"})),
        response(json!({"ref":"refs/heads/main","object":{"type":"commit","sha":"a".repeat(40)}})),
        response(json!({"sha":"a".repeat(40),"tree":{"sha":"b".repeat(40)}})),
        response(tree(paths)),
    ]
}
const NOW: u64 = 1793836800; // 2026-11-05T00:00Z
fn collected(outputs: Vec<Result<Output>>, paths: &[&str]) -> (git::Collected, Fake) {
    let mut f = Fake::new(outputs);
    let c = git::collect(&mut f, &req(), &set(paths), 0, &mut || Ok(NOW)).unwrap();
    (c, f)
}
fn billing_contract() -> Contract {
    Contract::Billing {
        account: "synthetic-account".into(),
        month: Month {
            year: 2026,
            month: 11,
        },
        linux_skus: set(&["synthetic-linux"]),
        limit: 3000,
    }
}
fn disk_contract() -> Contract {
    Contract::Disk {
        minimum: DiskMinimum::Exact {
            quantity: 15,
            unit: DiskUnit::Gib,
        },
    }
}
fn recall_contract() -> Contract {
    Contract::Recall {
        baseline: Baseline {
            supported: 1,
            total: 3,
            evaluation_set_sha256: "c".repeat(64),
            result_revision: "synthetic-baseline/v1".into(),
            artifact_sha256: "d".repeat(64),
        },
    }
}
fn defects_contract() -> Contract {
    Contract::Defects {
        week_start: NOW - 3 * 86400,
        limit: 0,
    }
}
fn billing() -> Value {
    let (start, end) = Month {
        year: 2026,
        month: 11,
    }
    .bounds()
    .unwrap();
    json!({"source_revision":"synthetic-billing/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"billing","account":"synthetic-account","month":{"year":2026,"month":11},"period_start":start,"period_end":end,"cutoff":NOW,"settlement_lag_seconds":0,"population_complete":true,"pages":[{"number":1,"has_more":false,"items":[]}]}})
}
fn evaluate(v: &Value, c: &Contract) -> Evaluation {
    guardrails::evaluate(Some(&serde_json::to_vec(v).unwrap()), c, NOW).unwrap()
}
fn manifest() -> Vec<u8> {
    serde_json::to_vec(&json!({"repos":{"synthetic-repo":{"groups":[{"disposition":"DELETE_NOW","files":["kept","gone","gone"]},{"disposition":"KEEP","files":["ignored"]}]}}})).unwrap()
}
fn prepared(s: &Scratch, id: &str) -> Prepared {
    let manifest = manifest();
    let root = s.0.to_str().unwrap().to_string();
    let request = Request {
        schema_version: "ws-replan-kpi-capture-request/v2".into(),
        measurement_id: id.into(),
        manifest_path: s.write("manifest.json", &manifest),
        definition_contract_path: s.path("definition.json"),
        build_evidence_path: s.path("build.json"),
        journal_path: s.path("snapshots-v2.jsonl"),
        evidence_root: root,
        repositories: vec![req()],
        billing_recording_path: RecordingInput::State(Unattempted {
            kind: "not_performed".into(),
        }),
        parity_recording_path: RecordingInput::State(Unattempted {
            kind: "not_performed".into(),
        }),
        recall_recording_path: RecordingInput::State(Unattempted {
            kind: "not_performed".into(),
        }),
        defects_recording_path: RecordingInput::State(Unattempted {
            kind: "not_performed".into(),
        }),
        disk_target: "/".into(),
        limits: Limits {
            process_timeout_seconds: 2,
            capture_timeout_seconds: 30,
            process_output_bytes: 1 << 20,
            evidence_bytes: 1 << 20,
        },
    };
    let definition = DefinitionContract {
        revision: DEFINITION.into(),
        algorithm_revision: ALGORITHM.into(),
        manifest_sha256: kpi::digest(&manifest),
        repositories: vec![req().identity()],
        parity_tool_ids: set(&["tool-a", "tool-b"]),
        billing: billing_contract(),
        disk: disk_contract(),
        recall: recall_contract(),
        defects: defects_contract(),
    };
    let build = BuildEvidence {
        source_commit: "a".repeat(40),
        source_tree: "b".repeat(40),
        binary_sha256: "f".repeat(64),
        clean: true,
    };
    let definition_bytes = strict::bytes(&definition).unwrap();
    let build_bytes = strict::bytes(&build).unwrap();
    s.write("definition.json", &definition_bytes);
    s.write("build.json", &build_bytes);
    let request_bytes = strict::bytes(&request).unwrap();
    Prepared {
        request,
        definition,
        build,
        manifest,
        request_bytes,
        definition_bytes,
        build_bytes,
    }
}
fn snapshot(s: &Scratch, id: &str) -> (Snapshot, Evidence) {
    let p = prepared(s, id);
    let mut outputs = vec![
        success(b"git version synthetic\n".to_vec()),
        success(b"gh version synthetic\n".to_vec()),
    ];
    outputs.extend(git_responses(&["kept"]));
    let mut f = Fake::new(outputs);
    let got = capture::collect(p, &mut f, &mut || Ok(NOW), &mut || Ok(20 * GIB)).unwrap();
    assert!(f.outputs.is_empty());
    got
}

#[test]
fn recursive_duplicate_keys_are_rejected() {
    assert!(strict::value(br#"{"a":{"b":1,"b":2}}"#, 1000).is_err());
}
#[test]
fn duplicate_keys_in_array_members_are_rejected() {
    assert!(strict::value(br#"[{"a":1,"a":2}]"#, 1000).is_err());
}
#[test]
fn typed_integer_does_not_accept_decimal_boolean_or_negative() {
    for v in [b"1.0".as_slice(), b"true", b"-1"] {
        assert!(strict::parse::<u64>(v, 100).is_err());
    }
}
#[test]
fn knowledge_wrappers_reject_hidden_values() {
    assert!(strict::parse::<K<u64>>(br#"{"kind":"unknown","value":0}"#, 100).is_err());
}
#[test]
fn nul_membership_preserves_all_special_names() {
    let paths = [
        "한글",
        "line\nbreak",
        "tab\tname",
        "a\\b",
        "a\"b",
        "with space",
    ];
    let mut bytes = Vec::new();
    for p in paths {
        bytes.extend_from_slice(p.as_bytes());
        bytes.push(0);
    }
    assert_eq!(git::nul_paths(&bytes).unwrap(), set(&paths));
}
#[test]
fn malformed_path_outputs_never_become_empty_tree() {
    for b in [b"path".as_slice(), b"a\0a\0", b"\xff\0", b"\0", b"a\0\0"] {
        assert!(git::nul_paths(b).is_err());
    }
    assert!(git::nul_paths(b"").unwrap().is_empty());
}
#[test]
fn manifest_population_is_unique_non_keep() {
    let p = git::population(&manifest()).unwrap();
    assert_eq!(p["synthetic-repo"], set(&["kept", "gone"]));
}
#[test]
fn failed_tree_cannot_count_every_path_absent() {
    let mut o = git_responses(&[]);
    o[3] = Ok(Output {
        stdout: serde_json::to_vec(&tree(&[])).unwrap(),
        exit_code: Some(17),
        signal: None,
    });
    let (c, _) = collected(o, &["one", "two"]);
    assert_eq!(c.repository.removed, K::Unknown);
    assert_eq!(c.repository.tree_read.status, Status::Failed);
    assert_eq!(c.repository.tree_read.exit_code, K::Value(17));
    assert_eq!(
        git::aggregate(&[c.repository], NOW, NOW)
            .unwrap()
            .value
            .get()
            .map(|v| v.removed),
        None
    );
}
#[test]
fn successful_complete_empty_tree_can_count_absence() {
    let (c, _) = collected(git_responses(&[]), &["one", "two"]);
    assert_eq!(c.repository.removed, K::Value(2));
    assert!(c.repository.source_qualified);
}
#[test]
fn denied_identity_is_not_repository_deletion() {
    let (c, f) = collected(
        vec![Ok(Output {
            stdout: vec![],
            exit_code: Some(1),
            signal: None,
        })],
        &["one"],
    );
    assert_eq!(c.repository.removed, K::Unknown);
    assert_eq!(f.calls.len(), 1);
}
#[test]
fn foreign_repository_identity_refuses_collection() {
    let mut o = git_responses(&[]);
    o[0] = response(json!({"id":8,"full_name":"synthetic-owner/synthetic-repo"}));
    let (c, f) = collected(o, &["one"]);
    assert!(!c.repository.source_qualified);
    assert_eq!(f.calls.len(), 1);
}
#[test]
fn commit_tree_identity_is_resolved_not_echoed() {
    let mut o = git_responses(&[]);
    let mut v = tree(&[]);
    v["sha"] = json!("a".repeat(40));
    o[3] = response(v);
    let (c, _) = collected(o, &["one"]);
    assert_eq!(c.repository.tree_sha, K::Value("b".repeat(40)));
    assert_eq!(c.repository.removed, K::Unknown);
}
#[test]
fn truncated_tree_refuses_qualification() {
    let mut o = git_responses(&[]);
    let mut v = tree(&[]);
    v["truncated"] = json!(true);
    o[3] = response(v);
    let (c, _) = collected(o, &["one"]);
    assert_eq!(c.repository.qualification, Qualification::IncompleteSource);
}
#[test]
fn api_leaf_population_includes_gitlinks_and_symlinks() {
    let mut v = tree(&["symbolic"]);
    v["tree"][0]["mode"] = json!("120000");
    v["tree"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"module","type":"commit","mode":"160000","sha":"d".repeat(40)}));
    v["tree"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"directory","type":"tree","mode":"040000","sha":"e".repeat(40)}));
    assert_eq!(
        git::api_paths(&v, &"b".repeat(40)).unwrap(),
        set(&["symbolic", "module"])
    );
}
#[test]
fn duplicate_api_paths_are_invalid() {
    let mut v = tree(&["a"]);
    let entry = v["tree"][0].clone();
    v["tree"].as_array_mut().unwrap().push(entry);
    assert!(git::api_paths(&v, &"b".repeat(40)).is_err());
}
#[test]
fn endpoint_is_pinned_once_and_artifacts_do_not_disclose_paths() {
    let (c, f) = collected(
        git_responses(&["한글"]),
        &["한글", "private-synthetic-name"],
    );
    assert_eq!(c.repository.commit_sha, K::Value("a".repeat(40)));
    assert_eq!(
        f.calls
            .iter()
            .filter(|c| c.args.last().unwrap().contains("git/ref/"))
            .count(),
        1
    );
    for (_, b) in c.artifacts {
        let txt = String::from_utf8(b).unwrap();
        assert!(!txt.contains("한글") && !txt.contains("private-synthetic-name"));
    }
}
#[test]
fn original_response_digest_is_of_exact_bytes() {
    let mut o = git_responses(&[]);
    let bytes = b"{  \"id\":7, \"full_name\":\"synthetic-owner/synthetic-repo\" }\n".to_vec();
    o[0] = success(bytes.clone());
    let (c, _) = collected(o, &["one"]);
    let a: Value = serde_json::from_slice(&c.artifacts[0].1).unwrap();
    assert_eq!(a["identity_response_sha256"], kpi::digest(&bytes));
}
#[test]
fn failed_component_retains_other_repository_fact() {
    let (good, _) = collected(git_responses(&[]), &["one"]);
    let (mut bad, _) = collected(vec![Err(Error("process_timeout"))], &["two"]);
    bad.repository.manifest_key = "second".into();
    let v = git::aggregate(&[good.repository.clone(), bad.repository], NOW, NOW).unwrap();
    assert!(matches!(v.value, K::Unknown));
    assert_eq!(good.repository.removed, K::Value(1));
}

#[test]
fn complete_empty_month_is_zero_but_missing_is_unknown() {
    assert_eq!(evaluate(&billing(), &billing_contract()).value, K::Value(0));
    assert_eq!(
        guardrails::evaluate(None, &billing_contract(), NOW)
            .unwrap()
            .value,
        K::Unknown
    );
}
#[test]
fn failed_and_unattempted_sources_are_distinct() {
    for (kind, value, q) in [
        ("failed", K::Unknown, Qualification::FailedSource),
        (
            "not_performed",
            K::NotPerformed,
            Qualification::NotPerformed,
        ),
    ] {
        let v = json!({"source_revision":"synthetic/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":kind}});
        let e = evaluate(&v, &billing_contract());
        assert_eq!(e.value, value);
        assert_eq!(e.qualification, q);
    }
}
#[test]
fn billing_missing_fields_pages_and_identity_never_qualify_zero() {
    for field in ["account", "month", "period_start", "cutoff"] {
        let mut v = billing();
        v["outcome"].as_object_mut().unwrap().remove(field);
        assert!(!evaluate(&v, &billing_contract()).source_qualified);
    }
    for (change, value) in [
        ("account", json!("wrong")),
        ("pages", json!([])),
        ("population_complete", json!(false)),
        ("settlement_lag_seconds", Value::Null),
    ] {
        let mut v = billing();
        v["outcome"][change] = value;
        let e = evaluate(&v, &billing_contract());
        assert_eq!(e.value, K::Unknown);
    }
}
#[test]
fn billing_unfinished_pages_and_wrong_sku_refuse_qualification() {
    let mut v = billing();
    v["outcome"]["pages"][0]["has_more"] = json!(true);
    assert_eq!(
        evaluate(&v, &billing_contract()).qualification,
        Qualification::IncompleteSource
    );
    v["outcome"]["pages"][0]["has_more"] = json!(false);
    v["outcome"]["pages"][0]["items"] =
        json!([{"id":"u","sku":"wrong","unit":"linux_minutes","quantity":0}]);
    assert_eq!(
        evaluate(&v, &billing_contract()).qualification,
        Qualification::InvalidSource
    );
}
#[test]
fn billing_fractions_duplicates_and_cost_units_are_invalid() {
    for items in [
        json!([{"id":"u","sku":"synthetic-linux","unit":"linux_minutes","quantity":1.5}]),
        json!([{"id":"u","sku":"synthetic-linux","unit":"USD","quantity":0}]),
        json!([{"id":"u","sku":"synthetic-linux","unit":"linux_minutes","quantity":0},{"id":"u","sku":"synthetic-linux","unit":"linux_minutes","quantity":0}]),
    ] {
        let mut v = billing();
        v["outcome"]["pages"][0]["items"] = items;
        assert_eq!(
            evaluate(&v, &billing_contract()).qualification,
            Qualification::InvalidSource
        );
    }
}
#[test]
fn unknown_or_nonzero_settlement_never_becomes_zero() {
    for lag in [Value::Null, json!(1)] {
        let mut v = billing();
        v["outcome"]["settlement_lag_seconds"] = lag;
        assert_eq!(
            evaluate(&v, &billing_contract()).qualification,
            Qualification::SettlementLag
        );
    }
}
#[test]
fn disk_exact_unit_and_unresolved_bounds_preserve_bytes() {
    let v = json!({"source_revision":"disk/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"disk","observed_at":NOW,"numerator":15500000000u64,"denominator":1,"unit":"bytes"}});
    let a = evaluate(&v, &disk_contract());
    assert_eq!(a.value, K::Value(15500000000));
    assert_eq!(a.qualification, Qualification::Qualified);
    let b = evaluate(
        &v,
        &Contract::Disk {
            minimum: DiskMinimum::UnresolvedGbOrGib { quantity: 15 },
        },
    );
    assert!(b.source_qualified);
    assert_eq!(b.qualification, Qualification::UnresolvedDiskUnit);
    assert_eq!(disk_contract().kind(), "disk");
    assert_eq!(
        DiskMinimum::Exact {
            quantity: 15,
            unit: DiskUnit::Gib
        }
        .bounds()
        .unwrap(),
        (16106127360, 16106127360)
    );
}
#[test]
fn disk_fraction_must_resolve_exact_bytes() {
    let mut v = json!({"source_revision":"disk/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"disk","observed_at":NOW,"numerator":1,"denominator":3,"unit":"bytes"}});
    assert_eq!(
        evaluate(&v, &disk_contract()).qualification,
        Qualification::InvalidSource
    );
    v["outcome"]["denominator"] = json!(0);
    assert!(!evaluate(&v, &disk_contract()).source_qualified);
}
#[test]
fn recall_requires_executed_matching_set() {
    let mut v = json!({"source_revision":"recall/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"recall","observed_at":NOW,"evaluation_set_sha256":"c".repeat(64),"result_revision":"executed/v2","supported":2,"total":3}});
    assert_eq!(evaluate(&v, &recall_contract()).value, K::Value(2));
    v["outcome"]["evaluation_set_sha256"] = json!("e".repeat(64));
    assert!(!evaluate(&v, &recall_contract()).source_qualified);
}
#[test]
fn fixed_incident_still_counts_escaped() {
    let start = NOW - 3 * 86400;
    let v = json!({"source_revision":"incidents/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"defects","week_start":start,"week_end":start+604800,"cutoff":NOW,"population_complete":true,"incidents":[{"id":"incident-a","escaped_at":NOW-100,"fixed_at":NOW-1}]}});
    assert_eq!(evaluate(&v, &defects_contract()).value, K::Value(1));
}
#[test]
fn baseline_does_not_become_a_fresh_executed_result() {
    let v = json!({"source_revision":"baseline/v1","capture_start":NOW,"capture_end":NOW,"outcome":{"kind":"baseline","observed_at":NOW,"value":1,"total":3}});
    let e = evaluate(&v, &recall_contract());
    assert_eq!(e.qualification, Qualification::UnverifiedBaseline);
    assert_eq!(e.value, K::Unknown);
}
#[test]
fn source_time_not_import_time_controls_freshness() {
    let mut v = billing();
    v["capture_end"] = json!(NOW + 86401);
    assert_eq!(
        guardrails::evaluate(
            Some(&serde_json::to_vec(&v).unwrap()),
            &billing_contract(),
            NOW + 86401
        )
        .unwrap()
        .qualification,
        Qualification::StaleSource
    );
    v["capture_end"] = json!(NOW);
    assert_eq!(
        guardrails::evaluate(
            Some(&serde_json::to_vec(&v).unwrap()),
            &billing_contract(),
            NOW + 86400
        )
        .unwrap()
        .qualification,
        Qualification::Qualified
    );
}
#[test]
fn future_and_reversed_capture_cannot_qualify() {
    let mut v = billing();
    v["capture_end"] = json!(NOW + 1);
    assert_eq!(
        evaluate(&v, &billing_contract()).qualification,
        Qualification::FutureSource
    );
    v["capture_end"] = json!(NOW - 1);
    assert_eq!(
        evaluate(&v, &billing_contract()).qualification,
        Qualification::InvalidSource
    );
}

#[test]
fn complete_capture_pipeline_emits_partial_without_defaults() {
    let s = Scratch::new();
    let (v, e) = snapshot(&s, "capture-a");
    assert_eq!(v.capture_status, "partial");
    assert_eq!(v.kpi_legacy_removal.value.get().unwrap().removed, 1);
    assert_eq!(v.kpi_legacy_removal.value.get().unwrap().total, 2);
    assert_eq!(v.guard_ci_minutes_month.value, K::NotPerformed);
    assert_eq!(v.guard_recall_regression.value, K::NotPerformed);
    assert!(matches!(v.kpi_mcp_parity.value, K::NotPerformed));
    assert_eq!(v.guard_disk_free_gib.free_gib.get().unwrap().unit, "GiB");
    assert!(!PathBuf::from(s.path("snapshots-v2.jsonl")).exists());
    e.verify().unwrap();
    let bytes = journal::row(&v).unwrap();
    let parsed: Snapshot = strict::parse(&bytes, LINE_LIMIT).unwrap();
    parsed.validate().unwrap();
}
#[test]
fn prepare_checks_binary_definition_and_manifest_population() {
    let s = Scratch::new();
    let p = prepared(&s, "capture-a");
    assert!(capture::prepare(&p.request_bytes, &"f".repeat(64)).is_ok());
    assert_eq!(
        capture::prepare(&p.request_bytes, &"e".repeat(64)).err(),
        Some(Error("binary_pin_mismatch"))
    );
}
#[test]
fn missing_owner_recording_is_unknown_with_reason() {
    let s = Scratch::new();
    let mut p = prepared(&s, "capture-a");
    p.request.billing_recording_path = RecordingInput::Path(s.path("missing.json"));
    p.request_bytes = strict::bytes(&p.request).unwrap();
    let mut o = vec![
        success(b"git synthetic".to_vec()),
        success(b"gh synthetic".to_vec()),
    ];
    o.extend(git_responses(&["kept"]));
    let (v, _) =
        capture::collect(p, &mut Fake::new(o), &mut || Ok(NOW), &mut || Ok(20 * GIB)).unwrap();
    assert_eq!(v.guard_ci_minutes_month.value, K::Unknown);
    assert_eq!(
        v.guard_ci_minutes_month.qualification,
        Qualification::MissingSource
    );
}
#[test]
fn disk_failure_is_explicit_and_does_not_erase_removal() {
    let s = Scratch::new();
    let p = prepared(&s, "capture-a");
    let mut o = vec![
        success(b"git synthetic".to_vec()),
        success(b"gh synthetic".to_vec()),
    ];
    o.extend(git_responses(&["kept"]));
    let (v, _) = capture::collect(p, &mut Fake::new(o), &mut || Ok(NOW), &mut || {
        Err(Error("disk_stat_failed"))
    })
    .unwrap();
    assert_eq!(v.guard_disk_free_gib.value, K::Unknown);
    assert_eq!(
        v.guard_disk_free_gib.qualification,
        Qualification::FailedSource
    );
    assert!(v.kpi_legacy_removal.source_qualified);
}
#[test]
fn append_readback_binds_row_and_prefix() {
    let s = Scratch::new();
    let (v, e) = snapshot(&s, "capture-a");
    e.verify().unwrap();
    let path = s.path("snapshots-v2.jsonl");
    let receipt = journal::append(&path, &v).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(receipt.row_sha256, kpi::digest(&bytes));
    assert_eq!(receipt.journal_prefix_sha256, kpi::digest(&bytes));
    assert_eq!(receipt.line_number, 1);
    let again = journal::append(&path, &v).unwrap();
    assert_eq!(again.row_sha256, receipt.row_sha256);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
#[test]
fn conflicting_id_and_truncated_tail_never_rewrite_bytes() {
    let s = Scratch::new();
    let (mut v, _) = snapshot(&s, "capture-a");
    let path = s.path("snapshots-v2.jsonl");
    journal::append(&path, &v).unwrap();
    let before = std::fs::read(&path).unwrap();
    v.producer.git_version = "git different".into();
    assert_eq!(
        journal::append(&path, &v).err(),
        Some(Error("measurement_id_conflict"))
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, &before[..before.len() - 1]).unwrap();
    let truncated = std::fs::read(&path).unwrap();
    assert_eq!(
        journal::append(&path, &v).err(),
        Some(Error("journal_truncated"))
    );
    assert_eq!(std::fs::read(&path).unwrap(), truncated);
}
#[test]
fn second_row_extends_prefix_without_changing_first() {
    let s = Scratch::new();
    let (a, _) = snapshot(&s, "capture-a");
    let (b, _) = snapshot(&s, "capture-b");
    let path = s.path("snapshots-v2.jsonl");
    let first = journal::append(&path, &a).unwrap();
    let before = std::fs::read(&path).unwrap();
    let second = journal::append(&path, &b).unwrap();
    let after = std::fs::read(&path).unwrap();
    assert!(after.starts_with(&before));
    assert_eq!(second.line_number, 2);
    assert_eq!(
        first.journal_prefix_sha256,
        kpi::digest(&after[..first.journal_prefix_bytes as usize])
    );
}
#[test]
fn v1_filename_and_v1_definition_are_refused() {
    let s = Scratch::new();
    let (mut v, _) = snapshot(&s, "capture-a");
    assert_eq!(
        journal::append(&s.path("snapshots.jsonl"), &v).err(),
        Some(Error("invalid_journal_name"))
    );
    v.definition.algorithm_revision = "producer-v1-rendered-paths".into();
    assert!(journal::row(&v).is_err());
}
#[test]
fn output_schema_rejects_unknown_fields_and_inconsistent_totals() {
    let s = Scratch::new();
    let (v, _) = snapshot(&s, "capture-a");
    let mut value = serde_json::to_value(&v).unwrap();
    value["surprise"] = json!(1);
    assert!(strict::parse::<Snapshot>(&serde_json::to_vec(&value).unwrap(), LINE_LIMIT).is_err());
    let mut v = v;
    v.kpi_legacy_removal.value = K::Value(LegacyValue {
        removed: 0,
        total: 2,
        unit: "fraction".into(),
    });
    assert!(v.validate().is_err());
}
#[test]
fn nofollow_source_and_journal_do_not_follow_links() {
    let s = Scratch::new();
    let path = s.write("original", b"safe");
    symlink(&path, s.path("linked")).unwrap();
    assert!(filesystem::read(&s.path("linked"), 10).is_err());
    let dir = s.path("directory");
    std::fs::create_dir(&dir).unwrap();
    symlink(&dir, s.path("alias")).unwrap();
    assert!(filesystem::read(&s.path("alias/file"), 10).is_err());
    let (v, _) = snapshot(&s, "capture-a");
    symlink(&path, s.path("snapshots-v2.jsonl")).unwrap();
    assert!(journal::append(&s.path("snapshots-v2.jsonl"), &v).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"safe");
}
#[test]
fn fifo_source_is_rejected_without_a_writer() {
    let s = Scratch::new();
    let dir = filesystem::directory(s.0.to_str().unwrap()).unwrap();
    rustix::fs::mknodat(
        &dir,
        "fifo",
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        filesystem::read(&s.path("fifo"), 10).err(),
        Some(Error("not_regular_file"))
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
#[test]
fn artifact_replacement_size_and_hash_are_verified() {
    let s = Scratch::new();
    let mut e = Evidence::create(s.0.to_str().unwrap(), "capture-a", 100).unwrap();
    let a = e.retain("record.json", b"safe").unwrap();
    assert!(e.retain("too-large", &[0; 101]).is_err());
    std::fs::write(s.path("capture-a/record.json"), b"evil").unwrap();
    assert_eq!(e.read(&a).err(), Some(Error("evidence_mismatch")));
    assert!(e.verify().is_err());
}
#[test]
fn evidence_directory_replacement_is_rejected() {
    let s = Scratch::new();
    let e = Evidence::create(s.0.to_str().unwrap(), "capture-a", 100).unwrap();
    std::fs::rename(s.path("capture-a"), s.path("old-capture-a")).unwrap();
    std::fs::create_dir(s.path("capture-a")).unwrap();
    assert_eq!(e.verify().err(), Some(Error("evidence_replaced")));
}
#[test]
fn journal_busy_is_bounded_and_does_not_append() {
    use rustix::fs::{FlockOperation, flock};
    let s = Scratch::new();
    let (v, _) = snapshot(&s, "capture-a");
    let path = s.write("snapshots-v2.jsonl", b"");
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    flock(&f, FlockOperation::LockExclusive).unwrap();
    assert_eq!(
        journal::append(&path, &v).err(),
        Some(Error("journal_busy"))
    );
    assert!(std::fs::read(&path).unwrap().is_empty());
}

#[test]
fn native_git_quoting_cannot_change_exact_utf8_membership() {
    let s = Scratch::new();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args([
                "-C",
                s.0.to_str().unwrap(),
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    };
    run(&["init", "--quiet", "--initial-branch=main"]);
    let paths = [
        "한글",
        "line\nbreak",
        "tab\tname",
        "quote\"name",
        "back\\slash",
        "space name",
    ];
    for p in paths {
        s.write(p, b"synthetic");
    }
    run(&["add", "--all"]);
    run(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "synthetic tree",
    ]);
    let tree = String::from_utf8(run(&["rev-parse", "HEAD^{tree}"])).unwrap();
    let tree = tree.trim();
    let before = run(&["show-ref"]);
    let exact = run(&[
        "-c",
        "core.quotePath=true",
        "ls-tree",
        "-r",
        "-z",
        "--full-tree",
        "--name-only",
        tree,
    ]);
    assert_eq!(git::nul_paths(&exact).unwrap(), set(&paths));
    let rendered = run(&[
        "-c",
        "core.quotePath=true",
        "ls-tree",
        "-r",
        "--name-only",
        tree,
    ]);
    let rendered = String::from_utf8(rendered).unwrap();
    assert!(!rendered.lines().any(|p| p == "한글"));
    assert_eq!(before, run(&["show-ref"]));
}

#[test]
fn local_backend_checks_status_and_does_not_fallback_after_a_failed_read() {
    let mut request = req();
    request.backend = "local_then_api".into();
    request.local_root = K::Value("/synthetic/repository".into());
    let mut outputs = git_responses(&[]);
    outputs.pop();
    outputs.push(success(format!("{}\n", "b".repeat(40)).into_bytes()));
    outputs.push(Ok(Output {
        stdout: b"plausible\0".to_vec(),
        exit_code: Some(4),
        signal: None,
    }));
    let mut f = Fake::new(outputs);
    let got = git::collect(&mut f, &request, &set(&["plausible"]), 0, &mut || Ok(NOW)).unwrap();
    assert_eq!(got.repository.removed, K::Unknown);
    assert_eq!(got.repository.tree_read.reason, "git_exit_nonzero");
    assert!(f.outputs.is_empty());
    assert_eq!(f.calls.len(), 5);
}

#[test]
fn local_immutable_oid_read_succeeds_with_exact_paths() {
    let mut request = req();
    request.backend = "local_then_api".into();
    request.local_root = K::Value("/synthetic/repository".into());
    let mut outputs = git_responses(&[]);
    outputs.pop();
    outputs.push(success(format!("{}\n", "b".repeat(40)).into_bytes()));
    outputs.push(success("한글\0".as_bytes().to_vec()));
    let mut f = Fake::new(outputs);
    let got = git::collect(
        &mut f,
        &request,
        &set(&["한글", "absent"]),
        0,
        &mut || Ok(NOW),
    )
    .unwrap();
    assert_eq!(got.repository.removed, K::Value(1));
    assert_eq!(got.repository.backend, "local_git");
    assert!(f.calls.last().unwrap().args.contains(&"-z".to_owned()));
    assert_eq!(
        f.calls.last().unwrap().args.last().unwrap(),
        &"b".repeat(40)
    );
}

#[test]
fn hung_pipe_filling_and_orphaned_native_commands_are_bounded() {
    use kpi::process::NativeRunner;
    use std::time::{Duration, Instant};
    for (alias, expected) in [
        ("!sleep 10", Some("process_timeout")),
        ("!yes synthetic", Some("process_output_limit")),
        ("!sleep 10 &", None),
    ] {
        let mut runner = NativeRunner {
            per_process: Duration::from_millis(100),
            deadline: Instant::now() + Duration::from_secs(2),
            output_limit: 32768,
        };
        let start = Instant::now();
        let result = runner.run(&Call::new(
            "git",
            vec![
                "-c".into(),
                format!("alias.kpi-fixture={alias}"),
                "kpi-fixture".into(),
            ],
        ));
        if let Some(code) = expected {
            assert_eq!(result.err(), Some(Error(code)));
        } else {
            assert!(result.unwrap().success());
        }
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

fn pipeline(p: Prepared) -> (Snapshot, Evidence) {
    let mut o = vec![
        success(b"git synthetic".to_vec()),
        success(b"gh synthetic".to_vec()),
    ];
    o.extend(git_responses(&["kept"]));
    capture::collect(p, &mut Fake::new(o), &mut || Ok(NOW), &mut || Ok(20 * GIB)).unwrap()
}
fn tool_recording(s: &Scratch, qualified: bool) -> String {
    let gate = |kind: &str| {
        let bytes = strict::bytes(&Gate {
            gate: kind.into(),
            candidate_commit: "e".repeat(40),
            passed: true,
            owner_revision: "synthetic-owner/v1".into(),
        })
        .unwrap();
        let name = format!("{kind}-gate.json");
        s.write(&name, &bytes);
        K::Value(ArtifactRef::new(&name, &bytes))
    };
    let tools = vec![
        ToolStatus {
            id: "tool-a".into(),
            bounded_implemented: true,
            production_qualified: qualified,
            parity: gate("parity"),
            synthetic_tests: gate("synthetic_tests"),
            independent_review: gate("independent_review"),
            ci: gate("ci"),
            rollback: gate("rollback"),
        },
        ToolStatus {
            id: "tool-b".into(),
            bounded_implemented: false,
            production_qualified: false,
            parity: K::NotPerformed,
            synthetic_tests: K::NotPerformed,
            independent_review: K::NotPerformed,
            ci: K::NotPerformed,
            rollback: K::NotPerformed,
        },
    ];
    s.write(
        "tool-status.json",
        &strict::bytes(&ToolRecording {
            source_revision: "tool-owner/v2".into(),
            observed_at: NOW,
            population_complete: true,
            tools,
        })
        .unwrap(),
    )
}

#[test]
fn parity_qualifies_only_owner_gates_on_one_candidate() {
    let s = Scratch::new();
    let mut p = prepared(&s, "capture-a");
    p.request.parity_recording_path = RecordingInput::Path(tool_recording(&s, true));
    p.request_bytes = strict::bytes(&p.request).unwrap();
    let (v, _) = pipeline(p);
    let t = v.kpi_mcp_parity.value.get().unwrap();
    assert_eq!((t.qualified, t.total), (1, 2));
    assert_eq!(t.bounded_implemented, K::Value(1));
    let s = Scratch::new();
    let mut p = prepared(&s, "capture-a");
    p.request.parity_recording_path = RecordingInput::Path(tool_recording(&s, true));
    s.write("ci-gate.json", b"{}");
    p.request_bytes = strict::bytes(&p.request).unwrap();
    let (v, _) = pipeline(p);
    assert!(matches!(v.kpi_mcp_parity.value, K::Unknown));
    assert_eq!(v.kpi_mcp_parity.qualification, Qualification::InvalidSource);
}

#[test]
fn complete_owner_sources_can_produce_complete_evidence_even_with_failed_bound() {
    let s = Scratch::new();
    let mut p = prepared(&s, "capture-a");
    p.request.parity_recording_path = RecordingInput::Path(tool_recording(&s, true));
    p.request.billing_recording_path = RecordingInput::Path(s.write(
        "owner-billing.json",
        &serde_json::to_vec(&billing()).unwrap(),
    ));
    let recall = Recording {
        source_revision: "executed-recall/v2".into(),
        capture_start: NOW,
        capture_end: NOW,
        outcome: Outcome::Recall {
            observed_at: NOW,
            evaluation_set_sha256: "c".repeat(64),
            result_revision: "result/v2".into(),
            supported: 2,
            total: 3,
        },
    };
    p.request.recall_recording_path =
        RecordingInput::Path(s.write("owner-recall.json", &strict::bytes(&recall).unwrap()));
    let start = NOW - 3 * 86400;
    let defects = Recording {
        source_revision: "incident-ledger/v2".into(),
        capture_start: NOW,
        capture_end: NOW,
        outcome: Outcome::Defects {
            week_start: start,
            week_end: start + 604800,
            cutoff: NOW,
            population_complete: true,
            incidents: vec![Incident {
                id: "escaped-incident".into(),
                escaped_at: NOW - 100,
                fixed_at: Some(NOW - 1),
            }],
        },
    };
    p.request.defects_recording_path =
        RecordingInput::Path(s.write("owner-defects.json", &strict::bytes(&defects).unwrap()));
    p.request_bytes = strict::bytes(&p.request).unwrap();
    let (v, e) = pipeline(p);
    assert_eq!(v.capture_status, "complete");
    assert_eq!(v.guard_escaped_defects_week.value, K::Value(1));
    assert_eq!(v.guard_recall_regression.total, K::Value(3));
    e.verify().unwrap();
    assert!(journal::row(&v).is_ok());
}

#[test]
fn concurrent_native_lock_writers_keep_whole_rows() {
    let s = Scratch::new();
    let (a, _) = snapshot(&s, "capture-a");
    let (b, _) = snapshot(&s, "capture-b");
    let path = s.path("snapshots-v2.jsonl");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [a, b]
        .into_iter()
        .map(|v| {
            let barrier = barrier.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let result = journal::append(&path, &v);
                (v, result)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    // Both owned writers have exited, so any native lock-busy cause is resolved.
    for (v, result) in results {
        if let Err(e) = result {
            assert_eq!(e, Error("journal_busy"));
            journal::append(&path, &v).unwrap();
        }
    }
    let bytes = std::fs::read(&path).unwrap();
    let lines = bytes.split_inclusive(|b| *b == b'\n').collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    for line in lines {
        strict::parse::<Snapshot>(line, LINE_LIMIT)
            .unwrap()
            .validate()
            .unwrap();
    }
}
