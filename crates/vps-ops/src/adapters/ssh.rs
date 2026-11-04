use crate::{
    Result,
    runner::{CommandSpec, Runner},
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTarget {
    pub address: String,
    pub user: String,
    pub host_key_alias: String,
    pub port: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HermesTarget {
    pub schema_version: u8,
    pub host_id: String,
    pub ssh: SshTarget,
    pub project: String,
    pub gateway_service: String,
    pub profile: String,
    pub expected_profiles: Vec<String>,
    pub native_home: String,
    pub native_uid: u32,
    pub native_gid: u32,
    pub native_python: String,
    pub native_repository: String,
    pub gateway_slot: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub timeout_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub termination_grace_ms: u64,
    pub reap_timeout_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout_ms: 120_000,
            max_stdout_bytes: 8 * 1024 * 1024,
            max_stderr_bytes: 65536,
            termination_grace_ms: 1000,
            reap_timeout_ms: 1000,
        }
    }
}
pub fn execute(
    r: &mut dyn Runner,
    t: &HermesTarget,
    operation: &str,
    source: &str,
    dir: &Path,
    limits: Limits,
) -> Result<Value> {
    if !util::matches("^[a-zA-Z0-9_.-]+$", operation) {
        return Err("Invalid operation identifier".into());
    }
    if [
        limits.timeout_ms,
        limits.max_stdout_bytes as u64,
        limits.max_stderr_bytes as u64,
        limits.termination_grace_ms,
        limits.reap_timeout_ms,
    ]
    .iter()
    .any(|n| *n == 0 || *n > 2147483647)
    {
        return Err("Execution limits must be positive bounded integers".into());
    }
    let started = util::now();
    let p = dir.join(format!("{operation}-execution.json"));
    let mut receipt = json!({"schemaVersion":1,"operation":operation,"startedAtUtc":started,"completedAtUtc":null,"exitCode":null,"stdout":"","stderr":"","stage":"spawn","reason":"launch-pending","ownedChildCreated":null,"transportExitCode":null,"transportSignal":null,"reaped":false,"cleanup":{"termSent":false,"killSent":false,"signalFailed":false,"streamCancellationRequested":false,"streamsCancelled":false},"output":{"stdoutBytesObserved":0,"stderrBytesObserved":0,"stdoutComplete":false,"stderrComplete":false},"limits":limits,"remoteOutcome":"uncertain","automaticReplayPermitted":false,"state":"launch-pending"});
    util::save(&p, &receipt)?;
    let mut spec = CommandSpec::new(
        "ssh",
        vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=yes".into(),
            "-o".into(),
            format!("HostKeyAlias={}", t.ssh.host_key_alias),
            "-o".into(),
            "ConnectTimeout=10".into(),
            "-p".into(),
            t.ssh.port.to_string(),
            format!("{}@{}", t.ssh.user, t.ssh.address),
            "python3 -".into(),
        ],
    );
    spec.stdin = source.as_bytes().to_vec();
    spec.timeout_ms = limits.timeout_ms;
    spec.max_stdout_bytes = limits.max_stdout_bytes;
    spec.max_stderr_bytes = limits.max_stderr_bytes;
    spec.termination_grace_ms = limits.termination_grace_ms;
    spec.reap_timeout_ms = limits.reap_timeout_ms;
    let o = r.run(spec)?;
    let ok = o.exit_code == 0 && o.reason == "completed";
    let outcome = if !o.created {
        "not-started"
    } else if ok {
        "unverified"
    } else {
        "uncertain"
    };
    let interrupted = !["completed", "nonzero-exit", "spawn-failed"].contains(&o.reason.as_str());
    receipt["completedAtUtc"] = json!(util::now());
    receipt["exitCode"] = json!(o.exit_code);
    receipt["state"] = json!("settled");
    receipt["stdout"] = json!(if ok { o.stdout } else { String::new() });
    receipt["stderr"] = json!(if ok {
        String::new()
    } else {
        format!(
            "SSH transport {}; remote outcome {outcome}; automatic replay prohibited",
            o.reason
        )
    });
    receipt["stage"] = json!(if !o.created {
        "spawn"
    } else if ok {
        "completed"
    } else if o.reason.ends_with("limit-exceeded") || o.reason == "stream-read-failed" {
        "stream"
    } else {
        "transport"
    });
    receipt["reason"] = json!(o.reason);
    receipt["ownedChildCreated"] = json!(o.created);
    receipt["transportExitCode"] = json!(o.transport_exit_code);
    receipt["transportSignal"] = json!(o.transport_signal);
    receipt["reaped"] = json!(o.reaped);
    receipt["remoteOutcome"] = json!(outcome);
    receipt["cleanup"] = json!({"termSent":o.term_sent,"killSent":o.kill_sent,"signalFailed":o.signal_failed,"streamCancellationRequested":interrupted,"streamsCancelled":interrupted});
    receipt["output"] = json!({"stdoutBytesObserved":o.stdout_bytes_observed,"stderrBytesObserved":o.stderr_bytes_observed,"stdoutComplete":o.stdout_complete,"stderrComplete":o.stderr_complete});
    util::save(&p, &receipt)?;
    Ok(receipt)
}
