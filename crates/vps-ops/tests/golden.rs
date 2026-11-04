use serde_json::{Value, json};
use vps_ops::{
    Result,
    adapters::{backend, desktop, ssh::HermesTarget},
    domain,
    host::{canary, report},
    update::{arguments, release},
    util,
};
fn fixture(name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(format!("{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn domain_evaluation_golden() {
    for c in fixture("domain-evaluation").as_array().unwrap() {
        let actual = domain::evaluate(&c["input"], "2026-10-06T12:00:00Z", 60., 60.).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            c["expected"],
            "{}",
            c["kind"]
        );
    }
}
#[test]
fn backend_evaluation_and_identity_golden() {
    for c in fixture("backend").as_array().unwrap() {
        let target: HermesTarget = serde_json::from_value(c["target"].clone()).unwrap();
        assert_eq!(
            backend::evaluate(
                &target,
                &c["policy"],
                c["sha"].as_str().unwrap(),
                &c["input"],
                c["now"].as_str().unwrap()
            ),
            c["expected"],
            "{}",
            c["kind"]
        );
        assert_eq!(
            serde_json::to_value(backend::identity(&c["input"], &c["policy"]).unwrap()).unwrap(),
            c["identity"],
            "{}",
            c["kind"]
        );
    }
}
#[test]
fn prepared_evidence_golden() {
    let c = fixture("backend-prepared");
    assert_eq!(
        backend::prepared_candidate(&c["policy"], c["sha"].as_str().unwrap(), &c["snapshot"])
            .unwrap(),
        c["expected"]
    );
    for path in [
        "/candidate/actual_image_id",
        "/candidate/validation_key",
        "/candidate/validation_evidence/doctor/actualSha256",
    ] {
        let mut s = c["snapshot"].clone();
        *s.pointer_mut(path).unwrap() = json!("changed");
        assert!(backend::prepared_candidate(&c["policy"], c["sha"].as_str().unwrap(), &s).is_err());
    }
}
#[test]
fn native_job_correlation_golden() {
    for c in fixture("jobs").as_array().unwrap() {
        assert_eq!(
            backend::correlate(&c["job"], &[c["tx"].clone()]).unwrap(),
            if c["correlated"].is_null() {
                None
            } else {
                Some(c["correlated"].clone())
            }
        );
        assert_eq!(
            backend::completed(&c["job"], Some(&c["tx"])).unwrap(),
            c["complete"].as_bool().unwrap()
        );
        assert_eq!(
            backend::prepare_may_retry(&c["job"], Some(&c["tx"]), false).unwrap(),
            c["retry"].as_bool().unwrap()
        );
    }
    let c = &fixture("jobs")[0];
    assert!(backend::correlate(&c["job"], &[c["tx"].clone(), c["tx"].clone()]).is_err());
}
#[test]
fn desktop_receipt_identity_golden() {
    for c in fixture("desktop").as_array().unwrap() {
        let p = &c["provenance"];
        let receipt = desktop::normalize_receipt(
            &c["raw"],
            p["archivePath"].as_str().unwrap(),
            p["sha256"].as_str().unwrap(),
            p["byteLength"].as_u64().unwrap() as usize,
        );
        assert_eq!(receipt, c["normalized"], "normalized {}", c["kind"]);
        assert_eq!(
            desktop::verify_receipt(&receipt, &c["snapshot"], &c["expected"]),
            c["verification"],
            "verification {}",
            c["kind"]
        );
        assert_eq!(
            desktop::pre_update_eligibility(&c["target"], &c["snapshot"]).unwrap(),
            c["eligibility"],
            "eligibility {}",
            c["kind"]
        );
        assert_eq!(
            serde_json::to_value(desktop::identity(&c["snapshot"], None)).unwrap(),
            c["identity"],
            "identity {}",
            c["kind"]
        );
    }
}
#[test]
fn desktop_native_command_golden() {
    for c in fixture("desktop-command").as_array().unwrap() {
        let actual = desktop::build_command(&c["target"], c["id"].as_str().unwrap()).unwrap();
        assert_eq!(actual, c["expected"]);
    }
}
#[test]
fn update_argument_golden() {
    for c in fixture("arguments").as_array().unwrap() {
        let a: Vec<String> = serde_json::from_value(c["args"].clone()).unwrap();
        match arguments::parse(&a) {
            Ok((command, options)) => {
                assert_eq!(json!({"command":command,"options":options}), c["expected"])
            }
            Err(e) => assert_eq!(e.to_string(), c["error"].as_str().unwrap(), "{}", c["args"]),
        }
    }
}
#[test]
fn sanitizer_golden() {
    for c in fixture("sanitizer").as_array().unwrap() {
        assert_eq!(
            desktop::sanitize_output(c["input"].as_str().unwrap()),
            c["expected"].as_str().unwrap()
        );
    }
}
struct CanaryFixture {
    states: std::collections::VecDeque<Value>,
    command: Value,
    calls: Vec<String>,
}
impl canary::Backend for CanaryFixture {
    fn observe(&mut self) -> Result<Value> {
        self.calls.push("observe".into());
        Ok(self.states.pop_front().unwrap())
    }
    fn change(&mut self, a: &str) -> Result<Value> {
        self.calls.push(a.into());
        Ok(self.command.clone())
    }
}
#[test]
fn fixed_canary_status_and_effect_golden() {
    for c in fixture("canary").as_array().unwrap() {
        let mut b = CanaryFixture {
            states: [c["before"].clone(), c["after"].clone()].into(),
            command: c["command"].clone(),
            calls: vec![],
        };
        let args: Vec<String> = serde_json::from_value(c["args"].clone()).unwrap();
        let (v, code) = canary::dispatch(
            &args,
            &mut b,
            "2026-10-08T00:00:00+00:00",
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(v, c["expected"], "{}", c["kind"]);
        assert_eq!(code, c["code"].as_i64().unwrap() as i32);
        assert_eq!(json!(b.calls), c["calls"]);
    }
}
#[test]
fn main_report_byte_golden() {
    let v = fixture("report");
    let input = report::ReportInput {
        old: v["old"].as_str().unwrap().into(),
        new: v["new"].as_str().unwrap().into(),
        commits: v["commits"].as_str().unwrap().into(),
        changed: v["changed"].as_str().unwrap().into(),
        shortstat: v["shortstat"].as_str().unwrap().into(),
        count: v["count"].as_u64().unwrap(),
        time: v["time"].as_str().unwrap().into(),
    };
    assert_eq!(report::render(&input), include_str!("fixtures/report.md"));
}
#[test]
fn canonical_release_refs() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    assert_eq!(
        release::parse_refs(
            &format!("{a}\trefs/tags/v0.22.0\n{b}\trefs/tags/v0.22.0^{{}}\n"),
            "v0.22.0"
        )
        .unwrap(),
        b
    );
    for tag in ["v01.2.3", "v0.22.0-rc1", "v1.2", "v1000.2.3"] {
        assert!(release::validate_tag(tag).is_err())
    }
    assert!(
        release::parse_refs(
            &format!("{a}\trefs/tags/v0.22.0\n{a}\trefs/tags/v0.22.0\n"),
            "v0.22.0"
        )
        .is_err()
    );
}
#[test]
fn all_domain_objects_and_boundaries() {
    let f = fixture("domain");
    for name in domain::OBJECT_NAMES {
        let object = domain::create_object(
            name,
            &format!("vps-operations:object:{name}:test"),
            f["properties"][name].clone(),
            "2026-10-06T12:00:00Z",
        )
        .unwrap();
        domain::validate_object(&object).unwrap();
        let mut wrong = object.clone();
        wrong["objectTypeVersion"] = json!("0.0.0");
        assert!(domain::validate_object(&wrong).is_err());
        let mut wrong = object.clone();
        wrong["unexpected"] = json!(true);
        assert!(domain::validate_object(&wrong).is_err());
    }
    let mut r = f["operation"].clone();
    r["target"]["processIdentity"]["startTicks"] = json!(999);
    assert!(domain::validate_operation(&r).is_err());
    r = f["operation"].clone();
    let preserved = r["target"]["processIdentity"].clone();
    r["preserveProcesses"]
        .as_array_mut()
        .unwrap()
        .push(preserved);
    assert!(domain::validate_operation(&r).is_err());
    r = f["operation"].clone();
    r["constraints"]["broadContainerRestart"] = json!(true);
    assert!(domain::validate_operation(&r).is_err());
    let mut o = f["observation"].clone();
    o["ports"][0]["port"] = json!(70000);
    assert!(domain::validate_observation(&o).is_err());
    assert_eq!(domain::registry()["actionTypes"], json!([]));
    assert_eq!(
        domain::registry()["objectTypes"].as_array().unwrap().len(),
        8
    );
}
#[test]
fn legacy_compose_replacement_preserves_first_image_behavior() {
    let input =
        "services:\n  ingress:\n    image: traefik:fixture\n  hermes:\n    image: hermes:old\n";
    assert_eq!(
        vps_ops::host::release_update::replace_first_image(
            input,
            "nousresearch/hermes-agent@sha256:fixture"
        )
        .unwrap(),
        "services:\n  ingress:\n    image: nousresearch/hermes-agent@sha256:fixture\n  hermes:\n    image: hermes:old\n"
    );
    assert!(vps_ops::host::release_update::replace_first_image("services: {}", "image").is_err());
}
#[test]
fn native_poll_does_not_admit_exit_code_alone() {
    for code in [None, Some(0), Some(1)] {
        for terminal in [false, true] {
            for marker in [false, true] {
                let actual = desktop::poll_state(code, true, terminal, marker);
                let expected = if code == Some(1) {
                    "native-failed"
                } else if terminal && !marker && code == Some(0) {
                    "settled"
                } else {
                    "waiting"
                };
                assert_eq!(actual, expected)
            }
        }
    }
    assert!(util::digest(&canary::unit_sha()));
}
#[test]
fn rejected_launch_proof_golden() {
    for c in fixture("rejected-proof").as_array().unwrap() {
        assert_eq!(
            json!(desktop::verify_rejected_proof(
                &c["proof"],
                &c["request"],
                &c["prior"],
                &c["current"]
            )),
            c["expected"],
            "{}",
            c["kind"]
        );
    }
}
#[test]
fn desktop_recovery_scope_golden() {
    for c in fixture("desktop-recovery").as_array().unwrap() {
        assert_eq!(
            desktop::recovery_request(
                c["requests"].as_array().unwrap(),
                &c["target"],
                c["checkpoints"].as_array().unwrap()
            )
            .unwrap(),
            c["expected"],
            "{}",
            c["kind"]
        );
    }
}
#[test]
fn handoff_requires_exact_deployed_sibling_and_native_slot() {
    let f = fixture("backend-prepared");
    let target: HermesTarget =
        serde_json::from_value(fixture("backend")[0]["target"].clone()).unwrap();
    let candidate =
        backend::prepared_candidate(&f["policy"], f["sha"].as_str().unwrap(), &f["snapshot"])
            .unwrap();
    let s = &f["snapshot"];
    assert!(
        vps_ops::adapters::handoff::plan(&target, &f["policy"], &candidate, s)
            .unwrap()
            .is_none()
    );
    let mut s = s.clone();
    let mut owner = s["gatewayOwner"].clone();
    owner["containerId"] = json!("standby");
    owner["namespace"] = json!("pid:[100]");
    s["gatewayOwner"] = owner.clone();
    s["gatewayLock"]["holders"] = json!([owner.clone()]);
    s["containers"][0]["rootSlot"] =
        json!({"up":true,"localPid":370,"raw":"fixture","identity":owner});
    s["containers"][1]["rootSlot"] =
        json!({"up":false,"localPid":null,"raw":"down","identity":null});
    let plan = vps_ops::adapters::handoff::plan(&target, &f["policy"], &candidate, &s)
        .unwrap()
        .unwrap();
    assert_eq!(plan["from"]["containerId"], "standby");
    assert_eq!(plan["intended"]["containerId"], "owner");
    s["gatewayOwner"]["startTicks"] = json!(999);
    assert!(vps_ops::adapters::handoff::plan(&target, &f["policy"], &candidate, &s).is_err());
}
#[test]
fn schema_integer_numbers_match_json_number_semantics() {
    let mut v = fixture("domain")["observation"].clone();
    v["schemaVersion"] = json!(1.0);
    v["lock"]["device"] = json!(2049.0);
    assert!(domain::validate_observation(&v).is_ok());
}
