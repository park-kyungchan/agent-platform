use crate::{
    Result,
    adapters::{self, ssh::HermesTarget},
    domain, host,
    runner::Runner,
    update, util,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
fn output(v: &Value, code: i32) -> Result<Output> {
    Ok(Output {
        code,
        stdout: format!("{}\n", serde_json::to_string_pretty(v)?),
        stderr: String::new(),
    })
}
pub fn dispatch(r: &mut dyn Runner, args: &[String], root: &Path) -> Result<Output> {
    let command = args.first().map(String::as_str).unwrap_or("doctor");
    let tail = args.get(1..).unwrap_or(&[]);
    if command == "hostctl" {
        let v = host::hostctl::dispatch(r, tail.first().map(String::as_str).unwrap_or(""))?;
        return Ok(Output {
            code: v.code,
            stdout: v.stdout,
            stderr: v.stderr,
        });
    }
    if command == "canary" {
        let (v, c) = host::canary::live(r, tail);
        return Ok(Output {
            code: c,
            stdout: format!("{}\n", util::python_json(&v, true)?),
            stderr: String::new(),
        });
    }
    if command == "main-report" {
        return match host::report::run(r, tail) {
            Ok(()) => Ok(Output {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
            Err(e) => Ok(Output {
                code: 1,
                stdout: String::new(),
                stderr: format!("{e}\n"),
            }),
        };
    }
    if command == "release-update" {
        return match host::release_update::run(r, &Default::default()) {
            Ok(s) => Ok(Output {
                code: 0,
                stdout: s,
                stderr: String::new(),
            }),
            Err(e) => {
                let stdout = e
                    .downcast_ref::<host::release_update::ReleaseFailure>()
                    .map(|f| f.stdout.clone())
                    .unwrap_or_default();
                let message = e.to_string();
                Ok(Output {
                    code: 1,
                    stdout,
                    stderr: if message.starts_with("Update failed;") {
                        format!("{message}\n")
                    } else {
                        format!("ERROR: {message}\n")
                    },
                })
            }
        };
    }
    let name = format!("{}-{command}", util::now().replace([':', '.'], "-"));
    let dir = root.join("runs").join(&name);
    let work = (|| -> Result<Output> {
        if command == "update" {
            let (v, c) = update::cli::run(r, tail, root)?;
            return output(&v, c);
        }
        let target: HermesTarget = util::json(root.join("architecture/targets.json"))?;
        if command == "apply" {
            if tail.is_empty()
                || tail.get(1).map(String::as_str) != Some("--confirm-exact-target")
                || tail.get(2).is_none()
            {
                return Err(
                    "Usage: apply <authorized-request.json> --confirm-exact-target <container-id>"
                        .into(),
                );
            }
            let request: Value = util::json(&tail[0])?;
            domain::validate_operation(&request)?;
            if request["status"] != "authorized"
                || request["authority"].is_null()
                || request["target"]["containerId"] != tail[2]
            {
                return Err("Explicit authorized request and exact target confirmation required; a schema does not establish actual user permission".into());
            }
            return output(
                &adapters::lifecycle::apply(r, &target, &request, &dir, root)?,
                0,
            );
        }
        if command == "verify" {
            let file = tail.first().ok_or("Usage: verify <observation.json>")?;
            let v: Value = util::json(file)?;
            let observation = v
                .get("observation")
                .or_else(|| v["properties"].get("observation"))
                .unwrap_or(&v);
            let e = domain::evaluate(observation, &util::now(), 60., 60.)?;
            return output(
                &serde_json::to_value(&e)?,
                if !e.control_plane_passed {
                    2
                } else if e.all_adapters_healthy {
                    0
                } else {
                    3
                },
            );
        }
        if !["doctor", "inventory", "plan"].contains(&command) {
            return Err("Commands: inventory, doctor, verify <observation.json>, plan <start|stop|restart>, apply <authorized-request.json> --confirm-exact-target <id>, update <backend-only|coordinated|verify|stage-overlay>".into());
        }
        let observation = adapters::observe(r, &target, command, &dir)?;
        fs::create_dir_all(&dir)?;
        let raw_path = dir.join("observation.json");
        util::save(&raw_path, &observation)?;
        let raw = fs::read(&raw_path)?;
        let observation_id = format!("vps-operations:observation:{name}");
        let record = domain::create_object(
            "Observation",
            &observation_id,
            json!({"hostId":target.host_id,"source":"live-ssh","observedAtUtc":observation["observedAtUtc"],"evidence":{"locator":raw_path,"sha256":util::hash(&raw),"byteLength":raw.len(),"mediaType":"application/json"},"observation":observation}),
            &util::now(),
        )?;
        util::save(&dir.join("observation-record.json"), &record)?;
        let evaluation = domain::evaluate(&observation, &util::now(), 60., 60.)?;
        let verification=domain::verification(&evaluation,&target.host_id,&observation_id,&util::now(),vec!["Fresh read-only evidence does not guarantee behavior after a future container restart".into(),"No model work or external business messages were submitted".into()]);
        let record = domain::create_object(
            "VerificationResult",
            &format!("vps-operations:verification:{name}"),
            verification,
            &util::now(),
        )?;
        util::save(&dir.join("verification-record.json"), &record)?;
        if command == "plan" {
            let kind = tail.first().map(String::as_str).unwrap_or("");
            if !["start", "stop", "restart"].contains(&kind) {
                return Err("Usage: plan <start|stop|restart>".into());
            }
            let runtime = domain::rows(&observation["runtimes"])
                .iter()
                .find(|r| r["service"] == target.gateway_service)
                .ok_or("Target runtime absent")?;
            let holder = domain::rows(&observation["lock"]["holders"])
                .iter()
                .find(|h| h["containerId"] == runtime["containerId"]);
            let request = json!({"hostId":target.host_id,"kind":format!("native-gateway-{kind}"),"target":{"containerId":runtime["containerId"],"service":target.gateway_service,"processIdentity":holder.map(|h|json!({"hostPid":h["pid"],"localPid":h["localPid"],"startTicks":h["startTicks"],"pidNamespace":h["namespace"],"containerId":runtime["containerId"],"uid":target.native_uid})),"lockIdentity":observation["lock"]},"preserveProcesses":[],"scope":"infrastructure","reason":format!("Proposed exact native root gateway {kind}"),"status":"proposed","authority":null,"requestedAtUtc":util::now(),"idempotencyKey":name,"constraints":{"revalidateProcessIdentity":true,"exactLockInodeRequired":kind!="start","installedNativeLifecycle":true,"broadContainerRestart":false,"destructiveDeletion":false,"exposeCredentials":false}});
            domain::validate_operation(&request)?;
            let record = domain::create_object(
                "OperationRequest",
                &format!("vps-operations:operation:{name}"),
                request.clone(),
                &util::now(),
            )?;
            util::save(&dir.join("operation-request.json"), &record)?;
            util::save(&dir.join("operation-payload.json"), &request)?;
            return output(
                &json!({"proposedOperation":dir.join("operation-payload.json"),"executed":false}),
                0,
            );
        }
        if command == "inventory" {
            return output(
                &json!({"runDirectory":dir,"inventory":observation["inventory"],"ports":observation["ports"]}),
                0,
            );
        }
        let mut v = serde_json::to_value(&evaluation)?;
        v["runDirectory"] = json!(dir);
        v["observedAtUtc"] = observation["observedAtUtc"].clone();
        v["ports"] = observation["ports"].clone();
        output(
            &v,
            if !evaluation.control_plane_passed {
                2
            } else if evaluation.all_adapters_healthy {
                0
            } else {
                3
            },
        )
    })();
    match work {
        Ok(o) => Ok(o),
        Err(e) => {
            util::save(
                &dir.join("failure.json"),
                &json!({"schemaVersion":1,"recordedAtUtc":util::now(),"command":command,"message":e.to_string()}),
            )?;
            Ok(Output {
                code: 1,
                stdout: String::new(),
                stderr: format!("{e}\n"),
            })
        }
    }
}
