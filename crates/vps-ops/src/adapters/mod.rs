pub mod backend;
pub mod desktop;
pub mod handoff;
pub mod lifecycle;
pub mod ssh;
use crate::{Result, domain, runner::Runner, util};
use serde_json::Value;
use std::path::Path;
pub fn observe(
    r: &mut dyn Runner,
    t: &ssh::HermesTarget,
    operation: &str,
    dir: &Path,
) -> Result<Value> {
    let source = include_str!("../../protocol/hermes-source.py")
        .replace("__PROFILE__", &serde_json::to_string(&t.profile)?)
        .replace("__TARGET__", &serde_json::to_string(&t.gateway_service)?)
        .replace(
            "__EXPECTED__",
            &serde_json::to_string(&serde_json::to_string(&t.expected_profiles)?)?,
        );
    let result = ssh::execute(
        r,
        t,
        operation,
        &source,
        dir,
        ssh::Limits {
            timeout_ms: 180_000,
            ..Default::default()
        },
    )?;
    if result["exitCode"] != 0 {
        return Err("Remote native observation failed; inspect saved execution receipt".into());
    }
    let v: Value = serde_json::from_str(
        result["stdout"]
            .as_str()
            .ok_or("Missing observation stdout")?,
    )?;
    domain::validate_observation(&v)?;
    Ok(v)
}
pub fn remote(
    r: &mut dyn Runner,
    t: &ssh::HermesTarget,
    operation: &str,
    source: &str,
    dir: &Path,
) -> Result<Value> {
    let receipt = ssh::execute(
        r,
        t,
        &format!("backend-{operation}-{}", util::id()),
        source,
        dir,
        ssh::Limits {
            timeout_ms: if operation == "gateway-owner-handoff" {
                600_000
            } else {
                120_000
            },
            max_stdout_bytes: 16 * 1024 * 1024,
            ..Default::default()
        },
    )?;
    if receipt["exitCode"] != 0 {
        return Err(format!("Backend {operation} transport {} (exit {}; remote outcome {}); inspect its saved execution receipt; automatic replay prohibited",receipt["reason"].as_str().unwrap_or("unknown"),receipt["exitCode"],receipt["remoteOutcome"].as_str().unwrap_or("uncertain")).into());
    }
    Ok(serde_json::from_str(
        receipt["stdout"].as_str().ok_or("Missing backend stdout")?,
    )?)
}
