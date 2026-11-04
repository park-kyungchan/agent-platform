pub mod types;
use crate::{Result, util};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
pub use types::*;
pub const DOMAIN_VERSION: &str = "1.0.0";
pub const PROJECT_ID: &str = "vps-operations";
pub const OBJECT_NAMES: [&str; 8] = [
    "VpsHost",
    "ContainerRuntime",
    "GatewayRuntime",
    "SharedVolume",
    "Observation",
    "VerificationResult",
    "AcceptanceDecision",
    "OperationRequest",
];
fn schemas() -> &'static Value {
    static SCHEMAS: OnceLock<Value> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        serde_json::from_str(include_str!("schemas.json")).expect("embedded domain schemas")
    })
}
/// Closed schema vocabulary used by this domain. Unknown schema keywords are never inputs.
fn shape(v: &Value, s: &Value, path: &str) -> Result<()> {
    let fail = || format!("Invalid infrastructure record: {path} schema validation failed");
    if let Some(c) = s.get("const")
        && v != c
        && !(v.is_number() && c.is_number() && v.as_f64() == c.as_f64())
    {
        return Err(fail().into());
    }
    if let Some(a) = s["anyOf"].as_array() {
        return if a.iter().any(|x| shape(v, x, path).is_ok()) {
            Ok(())
        } else {
            Err(fail().into())
        };
    }
    if let Some(e) = s["enum"].as_array()
        && !e.contains(v)
    {
        return Err(fail().into());
    }
    match s["type"].as_str() {
        Some("null") if !v.is_null() => return Err(fail().into()),
        Some("boolean") if !v.is_boolean() => return Err(fail().into()),
        Some("string") => {
            let t = v.as_str().ok_or_else(fail)?;
            if s["maxLength"]
                .as_u64()
                .is_some_and(|max| t.chars().count() > max as usize)
                || t.chars().count() < (s["minLength"].as_u64().unwrap_or(0) as usize)
                || s["pattern"].as_str().is_some_and(|p| !util::matches(p, t))
                || s["format"] == "date-time" && !util::seconds(t).is_finite()
            {
                return Err(fail().into());
            }
        }
        Some("integer" | "number") => {
            let n = v.as_f64().ok_or_else(fail)?;
            if !n.is_finite()
                || s["type"] == "integer" && n.fract() != 0.
                || s["minimum"].as_f64().is_some_and(|m| n < m)
                || s["maximum"].as_f64().is_some_and(|m| n > m)
            {
                return Err(fail().into());
            }
        }
        Some("array") => {
            let a = v.as_array().ok_or_else(fail)?;
            if a.len() < (s["minItems"].as_u64().unwrap_or(0) as usize) {
                return Err(fail().into());
            }
            for (i, x) in a.iter().enumerate() {
                if s["uniqueItems"] == true && a[..i].contains(x) {
                    return Err(fail().into());
                }
                shape(x, &s["items"], &format!("{path}/{i}"))?
            }
        }
        Some("object") => {
            let o = v.as_object().ok_or_else(fail)?;
            for k in rows(&s["required"]) {
                if !o.contains_key(k.as_str().ok_or("Invalid schema")?) {
                    return Err(fail().into());
                }
            }
            for (k, x) in o {
                let p = &s["properties"][k];
                if p.is_null() {
                    if s["additionalProperties"] == false {
                        return Err(fail().into());
                    }
                    continue;
                }
                shape(x, p, &format!("{path}/{k}"))?
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn rows(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn unique(v: &Value, key: &str) -> bool {
    let mut set = BTreeSet::new();
    rows(v).iter().all(|x| set.insert(x[key].to_string()))
}
fn credential(s: &str) -> bool {
    util::matches(
        r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{8,}|\bsk-[A-Za-z0-9_-]{10,}",
        s,
    ) || util::matches(
        r"(?i)\b(?:password|token|secret|api[_-]?key)\s*[:=]\s*[A-Za-z0-9._~+/=-]{8,}",
        s,
    )
}
fn errors(v: &Value) -> bool {
    rows(v)
        .iter()
        .any(|x| credential(x["message"].as_str().unwrap_or("")))
}
pub fn validate_observation(v: &Value) -> Result<InfrastructureObservation> {
    shape(v, &schemas()["observationSchema"], "")?;
    if !unique(&v["runtimes"], "containerId")
        || !unique(&v["inventory"]["containers"], "containerId")
        || rows(&v["runtimes"]).iter().any(|r| {
            !unique(&r["native"]["profiles"], "profile")
                || errors(&r["native"]["adapterErrors"])
                || credential(r["native"]["error"].as_str().unwrap_or(""))
        })
    {
        return Err(
            "Invalid infrastructure record: duplicate identity or unsanitized error".into(),
        );
    }
    {
        let mut normalized = v.clone();
        util::normalize_integers(&mut normalized);
        Ok(serde_json::from_value(normalized)?)
    }
}
pub fn validate_operation(v: &Value) -> Result<OperationRequestProperties> {
    shape(
        v,
        &schemas()["domainPropertySchemas"]["OperationRequest"],
        "",
    )?;
    if ["authorized", "executed"].contains(&v["status"].as_str().unwrap_or(""))
        && v["authority"].is_null()
    {
        return Err("missing-authority".into());
    }
    if ["native-gateway-stop", "native-gateway-restart"].contains(&v["kind"].as_str().unwrap_or(""))
    {
        let p = &v["target"]["processIdentity"];
        let l = &v["target"]["lockIdentity"];
        if p.is_null() || l.is_null() || v["constraints"]["exactLockInodeRequired"] != true {
            return Err("missing-process-incarnation-or-exact-lock".into());
        }
        let hs = rows(&l["holders"]);
        if p["containerId"] != v["target"]["containerId"]
            || l["held"] != true
            || hs.len() != 1
            || !hs.iter().any(|h| {
                h["pid"] == p["hostPid"]
                    && h["localPid"] == p["localPid"]
                    && h["startTicks"] == p["startTicks"]
                    && h["namespace"] == p["pidNamespace"]
                    && h["containerId"] == p["containerId"]
                    && h["matchingFdCount"].as_u64().unwrap_or(0) > 0
            })
        {
            return Err("unproven-exact-holder".into());
        }
        if rows(&v["preserveProcesses"]).iter().any(|h| {
            h["hostPid"] == p["hostPid"]
                && h["startTicks"] == p["startTicks"]
                && h["pidNamespace"] == p["pidNamespace"]
        }) {
            return Err("preserved-process-targeted".into());
        }
    }
    {
        let mut normalized = v.clone();
        util::normalize_integers(&mut normalized);
        Ok(serde_json::from_value(normalized)?)
    }
}
pub fn validate_properties(name: &str, v: &Value) -> Result<()> {
    if !OBJECT_NAMES.contains(&name) {
        return Err("unknown-infrastructure-object".into());
    }
    if name == "OperationRequest" {
        validate_operation(v)?;
        return Ok(());
    }
    shape(v, &schemas()["domainPropertySchemas"][name], "")?;
    match name {
        "Observation" => {
            validate_observation(&v["observation"])?;
            if v["observedAtUtc"] != v["observation"]["observedAtUtc"] {
                return Err("observation-time-mismatch".into());
            }
        }
        "GatewayRuntime" if (!unique(&v["profiles"], "profile") || errors(&v["adapterErrors"])) => {
            return Err("duplicate-profile-or-unsanitized-error".into());
        }
        "VerificationResult" => {
            if !unique(&v["criteria"], "id") {
                return Err("duplicate-identity".into());
            }
            let refs = rows(&v["observationRefs"]);
            let criteria = rows(&v["criteria"]);
            for c in criteria {
                let r = rows(&c["observationRefs"]);
                if c["status"] != "unverified" && r.is_empty()
                    || r.iter().any(|x| !refs.contains(x))
                {
                    return Err("unsupported-criterion".into());
                }
            }
            if v["status"] == "passed" && criteria.iter().any(|c| c["status"] != "pass")
                || v["status"] == "failed" && !criteria.iter().any(|c| c["status"] == "fail")
            {
                return Err("contradictory-verification".into());
            }
        }
        "AcceptanceDecision" if v["status"] == "accepted" && v["authority"].is_null() => {
            return Err("missing-authority".into());
        }
        _ => {}
    }
    Ok(())
}
pub fn create_object(name: &str, id: &str, p: Value, time: &str) -> Result<Value> {
    validate_properties(name, &p)?;
    let v = json!({"schemaVersion":"1.0.0","kind":"object","id":id,"objectType":format!("infrastructure:{name}"),"objectTypeVersion":DOMAIN_VERSION,"version":1,"projectId":PROJECT_ID,"scope":"project","createdAt":time,"updatedAt":time,"properties":p});
    validate_object(&v)?;
    Ok(v)
}
pub fn validate_object(v: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!("envelope.json"))?;
    shape(v, &schema, "")?;
    if util::seconds(v["updatedAt"].as_str().unwrap_or(""))
        < util::seconds(v["createdAt"].as_str().unwrap_or(""))
    {
        return Err("negative-object-duration".into());
    }
    let n = v["objectType"]
        .as_str()
        .and_then(|s| s.strip_prefix("infrastructure:"))
        .ok_or("unknown-infrastructure-object")?;
    if v["schemaVersion"] != "1.0.0"
        || v["kind"] != "object"
        || v["objectTypeVersion"] != DOMAIN_VERSION
        || v["version"].as_u64().unwrap_or(0) == 0
        || v["id"].as_str().unwrap_or("").is_empty()
        || v["projectId"].as_str().unwrap_or("").is_empty()
        || !["project", "shared"].contains(&v["scope"].as_str().unwrap_or(""))
        || !["createdAt", "updatedAt"]
            .iter()
            .all(|k| util::seconds(v[k].as_str().unwrap_or("")).is_finite())
    {
        return Err("domain-envelope-invalid".into());
    }
    validate_properties(n, &v["properties"])
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub control_plane_passed: bool,
    pub all_profiles_on: bool,
    pub checks: BTreeMap<String, bool>,
    pub profile_results: Vec<Value>,
    pub adapter_failures: Vec<Value>,
    pub all_adapters_healthy: bool,
}
pub fn evaluate(v: &Value, now: &str, max_age: f64, max_status: f64) -> Result<Evaluation> {
    validate_observation(v)?;
    let time = util::seconds(now);
    if !time.is_finite()
        || !max_age.is_finite()
        || max_age < 0.
        || !max_status.is_finite()
        || max_status < 0.
    {
        return Err("Invalid deterministic verification time window.".into());
    }
    let runtimes = rows(&v["runtimes"]);
    let candidates: Vec<_> = runtimes
        .iter()
        .filter(|r| r["service"] == v["targetService"])
        .collect();
    let null = Value::Null;
    let t = if candidates.len() == 1 {
        candidates[0]
    } else {
        &null
    };
    let holders = rows(&v["lock"]["holders"]);
    let h = if holders.len() == 1 {
        &holders[0]
    } else {
        &null
    };
    let native = &t["native"];
    let identity = &native["identify"];
    let observed = util::seconds(v["observedAtUtc"].as_str().unwrap_or(""));
    let age = time - observed;
    let expected = v
        .get("expectedProfiles")
        .cloned()
        .unwrap_or(json!([v["profile"]]));
    let profiles=native.get("profiles").map(|x|rows(x).to_vec()).unwrap_or_else(||if t.is_null(){vec![]}else{vec![json!({"profile":v["profile"],"nativeOn":native["profileOn"],"liveServed":rows(&identity["served_profiles"]).contains(&v["profile"]),"nativeOwnerPid":native["nativeOwnerPid"]})]});
    let all = rows(&expected).iter().all(|name| {
        profiles
            .iter()
            .find(|p| &p["profile"] == name)
            .is_some_and(|p| {
                p["nativeOn"] == true
                    && p["liveServed"] == true
                    && !h.is_null()
                    && p["nativeOwnerPid"] == h["localPid"]
                    && rows(&identity["served_profiles"]).contains(name)
            })
    });
    let entries = [
        ("currentObservation", age >= -5. && age <= max_age),
        (
            "sharedVolumeConsistent",
            !t.is_null() && runtimes.iter().all(|r| r["dataSource"] == t["dataSource"]),
        ),
        (
            "exactSingletonLockHeld",
            v["lock"]["held"] == true
                && holders.len() == 1
                && h["matchingFdCount"].as_u64().unwrap_or(0) > 0,
        ),
        (
            "intendedRuntimeOwnsLock",
            !t.is_null()
                && !h.is_null()
                && h["containerId"] == t["containerId"]
                && h["namespace"] == t["namespace"],
        ),
        (
            "freshIdentityMatchesHolder",
            identity["kind"] == "hermes-gateway"
                && !h.is_null()
                && identity["pid"] == h["localPid"]
                && identity["start_time"] == h["startTicks"],
        ),
        (
            "freshIdentityServesProfile",
            rows(&identity["served_profiles"]).contains(&v["profile"]),
        ),
        (
            "nativeServingConfirmed",
            native["nativeServing"] == true
                && !h.is_null()
                && native["nativeOwnerPid"] == h["localPid"],
        ),
        ("nativeProfileOn", native["profileOn"] == true),
        ("allExpectedProfilesNativeOn", all),
        (
            "settledRuntimeState",
            ["running", "degraded"]
                .contains(&native["status"]["gateway_state"].as_str().unwrap_or(""))
                && native["status"]["desired_state"] == "running",
        ),
        (
            "liveStatusMatchesHolder",
            !h.is_null()
                && native["status"]["answering_pid"] == h["localPid"]
                && (observed - native["status"]["answered_at"].as_f64().unwrap_or(0.)).abs()
                    < max_status,
        ),
        (
            "oneRootGatewaySlotUp",
            runtimes
                .iter()
                .filter(|r| r["rootSlot"]["up"] == true)
                .count()
                == 1
                && !h.is_null()
                && t["rootSlot"]["localPid"] == h["localPid"],
        ),
        (
            "preservedSessionAlive",
            v["preservedSession"]["alive"].as_bool().unwrap_or(true),
        ),
    ];
    let checks: BTreeMap<String, bool> = entries.into_iter().map(|(k, v)| (k.into(), v)).collect();
    let failures: Vec<Value> = rows(&native["adapters"])
        .iter()
        .filter(|a| {
            a["configured"] != false
                && (a["credentialRejected"] == true
                    || ["fatal", "failed", "retrying", "disconnected"]
                        .contains(&a["state"].as_str().unwrap_or("")))
        })
        .cloned()
        .collect();
    let healthy = !t.is_null()
        && failures.is_empty()
        && rows(&native["adapterErrors"]).is_empty()
        && native["status"]["gateway_state"] == "running";
    Ok(Evaluation {
        control_plane_passed: checks.values().all(|x| *x),
        all_profiles_on: all,
        checks,
        profile_results: profiles,
        adapter_failures: failures,
        all_adapters_healthy: healthy,
    })
}
pub fn verification(
    e: &Evaluation,
    host: &str,
    reference: &str,
    time: &str,
    limitations: Vec<String>,
) -> Value {
    json!({"hostId":host,"observationRefs":[reference],"verificationClass":"current-runtime","verifiedAtUtc":time,"status":if e.control_plane_passed{"passed"}else{"failed"},"criteria":e.checks.iter().map(|(id,pass)|json!({"id":id,"status":if *pass{"pass"}else{"fail"},"description":id,"observationRefs":[reference]})).collect::<Vec<_>>(),"adapterHealth":if e.all_adapters_healthy{"healthy"}else{"degraded"},"limitations":limitations})
}

pub fn registry() -> Value {
    serde_json::from_str(include_str!("registry.json")).expect("embedded domain registry")
}
