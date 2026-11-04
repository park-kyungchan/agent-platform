use super::ssh::{self, HermesTarget};
use crate::{Result, domain, runner::Runner, util};
use serde_json::{Value, json};
use std::{fs, path::Path};
pub fn apply(
    r: &mut dyn Runner,
    t: &HermesTarget,
    request: &Value,
    run_dir: &Path,
    root: &Path,
) -> Result<Value> {
    domain::validate_operation(request)?;
    if request["status"] != "authorized"
        || request["authority"].is_null()
        || request["hostId"] != t.host_id
        || request["target"]["service"] != t.gateway_service
    {
        return Err("Authorized exact host/service request required".into());
    }
    let state = root.join("state/operations");
    fs::create_dir_all(&state)?;
    let key = util::hash(format!(
        "{}\0{}",
        request["hostId"].as_str().unwrap_or(""),
        request["idempotencyKey"].as_str().unwrap_or("")
    ));
    let path = state.join(format!("{key}.json"));
    let mut normalized = request.clone();
    util::normalize_integers(&mut normalized);
    let req_hash = util::hash(serde_json::to_vec(&normalized)?);
    if path.exists() {
        let mut previous: Value = util::json(&path)?;
        if previous["requestHash"] != req_hash {
            return Err("Idempotency key already belongs to a different request".into());
        }
        if previous["status"] != "executed" {
            return Err("Previous request did not establish execution completion; reconcile its original receipts and current native state; automatic replay prohibited".into());
        }
        previous["replayed"] = json!(true);
        return Ok(previous);
    }
    let lock = state.join(format!("{key}.lock"));
    let pointer = run_dir.join("native-operation-execution.json");
    util::record(&lock,&json!({"schemaVersion":1,"requestHash":req_hash,"idempotencyKey":request["idempotencyKey"],"status":"uncertain","receiptPath":pointer,"automaticReplayPermitted":false,"verificationStillRequired":true})).map_err(|_|"Previous request unresolved; inspect original receipt pointer; automatic replay prohibited")?;
    let source = include_str!("../../protocol/lifecycle-source.py")
        .replace(
            "__REQUEST__",
            &serde_json::to_string(&serde_json::to_string(request)?)?,
        )
        .replace(
            "__CONFIG__",
            &serde_json::to_string(&serde_json::to_string(t)?)?,
        );
    let receipt = ssh::execute(
        r,
        t,
        "native-operation",
        &source,
        run_dir,
        ssh::Limits {
            timeout_ms: 180_000,
            ..Default::default()
        },
    )?;
    let mut details = json!({"exitCode":receipt["exitCode"],"reason":receipt["reason"],"remoteOutcome":receipt["remoteOutcome"]});
    let mut status = if receipt["remoteOutcome"] == "not-started" {
        "failed"
    } else {
        "uncertain"
    };
    if receipt["exitCode"] == 0 {
        match serde_json::from_str(receipt["stdout"].as_str().unwrap_or("")) {
            Ok(v) => {
                details = v;
                status = "executed"
            }
            Err(_) => {
                details = json!({"reason":"invalid-native-result","remoteOutcome":"uncertain"})
            }
        }
    }
    let result = json!({"schemaVersion":1,"requestHash":req_hash,"idempotencyKey":request["idempotencyKey"],"completedAtUtc":receipt["completedAtUtc"],"status":status,"receiptPath":pointer,"details":details,"automaticReplayPermitted":false,"verificationStillRequired":true});
    util::immutable(&path, &result)?;
    fs::remove_file(lock)?;
    util::sync_dir(&state)?;
    if status != "executed" {
        return Err(format!("Native action completion unresolved; original receipt retained at {}; automatic replay prohibited",path.display()).into());
    }
    Ok(result)
}
