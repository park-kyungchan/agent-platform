use crate::{Result, util};
use serde_json::{Value, json};
use std::path::Path;
pub fn preflight(d: &Path, target: &str) -> Result<Value> {
    let names = util::names(d)?;
    let artifacts: Vec<_> = names
        .iter()
        .filter(|n| {
            n.starts_with("desktop-native-request-") || n.starts_with("desktop-native-handoff-")
        })
        .cloned()
        .collect();
    let primary: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("desktop-update-result-"))
        .collect();
    let mut results = primary.clone();
    results.extend(
        names
            .iter()
            .filter(|n| n.starts_with("desktop-update-recovery-")),
    );
    results.sort();
    let mut reasons = vec![];
    if !artifacts.is_empty() {
        reasons.push("durable-native-request-or-handoff-requires-recovery".to_owned())
    }
    if primary.is_empty() {
        reasons.push("no-proven-prior-preflight-result".to_owned())
    }
    let mut evidence = vec![];
    for name in results {
        let mut e = json!({"name":name,"sha256":null,"operationId":null});
        let check = (|| -> Result<bool> {
            if !util::matches(
                r"^desktop-update-(?:result|recovery)-[A-Za-z0-9-]+\.json$",
                name,
            ) {
                return Err("Invalid result file".into());
            }
            let b = util::bounded(d.join(name), 2 * 1024 * 1024)?;
            if b.is_empty() {
                return Err("Invalid result file".into());
            }
            e["sha256"] = json!(util::hash(&b));
            let v: Value = serde_json::from_slice(&b)?;
            e["operationId"] = v["operationId"].clone();
            let id = v["operationId"].as_str().unwrap_or("");
            let started = util::seconds(v["startedAtUtc"].as_str().unwrap_or(""));
            let completed = util::seconds(v["completedAtUtc"].as_str().unwrap_or(""));
            Ok(v["schemaVersion"] == 1
                && util::run_id(id)
                && (!name.starts_with("desktop-update-result-")
                    || *name == format!("desktop-update-result-{id}.json"))
                && v["executionBoundary"] == "Native mutation not executed"
                && v["status"] == "blocked"
                && ["canonicalSha", "after", "receipt", "verification"]
                    .iter()
                    .all(|k| v.get(k).is_some_and(Value::is_null))
                && started.is_finite()
                && completed.is_finite()
                && completed >= started
                && (v["before"].is_null() || v["before"]["targetId"] == target))
        })();
        match check {
            Ok(true) => {}
            Ok(false) => reasons.push(format!("unknown-or-effectful-prior-result:{name}")),
            Err(_) => reasons.push(format!("unreadable-or-invalid-prior-result:{name}")),
        }
        evidence.push(e)
    }
    Ok(
        json!({"schemaVersion":1,"observedAtUtc":util::now(),"targetId":target,"eligible":reasons.is_empty(),"reasons":reasons,"requestOrHandoffArtifacts":artifacts,"priorResults":evidence,"executionBoundary":"Read-only prior evidence inspection; native invocation still requires fresh adapter preflight"}),
    )
}
pub fn rejected(d: &Path, target: &str) -> Result<Value> {
    let names = util::names(d)?;
    let proof = d.canonicalize()?.join("native-launch-rejection-proof.json");
    let mut reasons = vec![];
    let mut candidate = Value::Null;
    if !names
        .iter()
        .any(|n| n == "native-launch-rejection-proof.json")
    {
        reasons.push("no-same-run-native-rejection-proof".to_owned())
    } else {
        let requests: Vec<_> = names
            .iter()
            .filter(|n| n.starts_with("desktop-native-request-"))
            .collect();
        if requests.len() != 1
            || !util::matches(
                r"^desktop-native-request-[A-Za-z0-9-]+\.json$",
                requests.first().map(|x| x.as_str()).unwrap_or(""),
            )
        {
            reasons.push("exactly-one-original-native-request-required".into())
        }
        if names.iter().any(|n| {
            n.starts_with("desktop-native-handoff-")
                || n.starts_with("desktop-native-rejected-launch-retry-")
        }) {
            reasons.push("existing-native-handoff-or-retry-requires-recovery".into())
        }
        if reasons.is_empty() {
            let check = (|| -> Result<Value> {
                let b = util::bounded(&proof, 2 * 1024 * 1024)?;
                let p: Value = serde_json::from_slice(&b)?;
                let req: Value = util::json(d.join(requests[0]))?;
                let id = req["operationId"].as_str().unwrap_or("");
                let result = d
                    .canonicalize()?
                    .join(format!("desktop-update-result-{id}.json"));
                if !util::run_id(id)
                    || **requests[0] != format!("desktop-native-request-{id}.json")
                    || req["schemaVersion"] != 1
                    || req["targetId"] != target
                    || req["mode"] != "closed-script"
                    || req["correlationId"] != id
                    || req["scope"]["channel"] != "stable"
                    || p["schema"] != "vps-operations.hermes-native-script-binding-check.v1"
                    || p["readOnly"] != true
                    || p["correlationId"] != id
                    || p["nativeRequestStartedAt"] != req["startedAtUtc"]
                    || p["verification"]["passed"] != true
                    || p["failedResult"]["path"] != result.to_string_lossy().as_ref()
                {
                    return Err("Invalid proof scope".into());
                }
                let prior = util::bounded(result, 2 * 1024 * 1024)?;
                let v: Value = serde_json::from_slice(&prior)?;
                if p["failedNativeResultSha256"] != util::hash(&prior)
                    || v["operationId"] != id
                    || v["status"] != "failed"
                    || v["error"]["code"] != "native-coordinator-failed"
                    || v["executionBoundary"]
                        != "Native updater invoked; verification is separately recorded"
                    || !v["receipt"].is_null()
                    || !v["after"].is_null()
                {
                    return Err("Invalid original rejection".into());
                }
                Ok(json!({"operationId":id,"proofRef":proof,"proofSha256":util::hash(&b)}))
            })();
            match check {
                Ok(v) => candidate = v,
                Err(_) => reasons.push("invalid-or-unbound-native-rejection-evidence".into()),
            }
        }
    }
    Ok(
        json!({"schemaVersion":1,"observedAtUtc":util::now(),"candidate":candidate,"reasons":reasons,"executionBoundary":"Read-only dispatch nomination; native adapter proof and live preflight remain required"}),
    )
}
