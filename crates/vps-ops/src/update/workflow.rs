use crate::{Result, util};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    BackendOnly,
    Coordinated,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BackendOnly => "backend-only",
            Self::Coordinated => "coordinated",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIdentity {
    pub installed_sha: String,
    pub running_sha: Option<String>,
    pub running: bool,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_update_eligible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendIdentity {
    pub sha: String,
    pub image_id: String,
    pub healthy: bool,
    pub verified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub schema_version: u8,
    pub id: String,
    pub revision: u64,
    pub mode: Mode,
    pub phase: String,
    pub status: String,
    pub created_at_utc: String,
    pub updated_at_utc: String,
    pub target_sha: Option<String>,
    pub target_source: Option<String>,
    pub desktop: Option<DesktopIdentity>,
    pub candidate: Option<Value>,
    pub backend: Option<BackendIdentity>,
    pub deployment_attempted: bool,
    pub failure: Option<Value>,
    pub acceptance: String,
}
pub struct Request {
    pub id: String,
    pub mode: Mode,
    pub apply: bool,
    pub backend_sha: Option<String>,
    pub expected_release_sha: Option<String>,
    pub resume: Option<Checkpoint>,
}
pub trait Operations {
    fn inspect_backend(&mut self) -> Result<BackendIdentity>;
    fn resolve_main(&mut self) -> Result<String>;
    fn inspect_desktop(&mut self) -> Result<DesktopIdentity>;
    fn update_desktop(&mut self) -> Result<DesktopIdentity>;
    fn verify_existing_desktop(&mut self) -> Result<DesktopIdentity>;
    fn recover_desktop(&mut self) -> Result<DesktopIdentity>;
    fn prepare_backend(&mut self, sha: &str) -> Result<Value>;
    fn deploy_backend(&mut self, candidate: &Value) -> Result<()>;
    fn verify_backend(&mut self, sha: &str) -> Result<BackendIdentity>;
    fn save(&mut self, state: &Checkpoint) -> Result<()>;
}
fn require_sha(s: &str) -> Result<()> {
    if !util::sha(s) {
        return Err("A full lowercase source commit SHA is required".into());
    }
    Ok(())
}
fn desktop(d: DesktopIdentity, before: bool) -> Result<DesktopIdentity> {
    require_sha(&d.installed_sha)?;
    if (!d.verified && (!before || d.pre_update_eligible != Some(true)))
        || (d.running
            && d.running_sha
                .as_ref()
                .is_some_and(|s| s != &d.installed_sha))
    {
        return Err(if before {
            "Desktop pre-update identity is not verified or explicitly eligible"
        } else {
            "Desktop artifact and running identity are not verified"
        }
        .into());
    }
    Ok(d)
}
fn save(s: &mut Checkpoint, o: &mut dyn Operations, phase: &str) -> Result<()> {
    s.phase = phase.into();
    s.revision += 1;
    s.updated_at_utc = util::now();
    o.save(s)
}
pub fn run(request: Request, operations: &mut dyn Operations) -> Result<Checkpoint> {
    if request.mode == Mode::Coordinated && request.backend_sha.is_some() {
        return Err("Coordinated target comes from the completed native Desktop updater".into());
    }
    if let Some(s) = &request.expected_release_sha {
        require_sha(s)?
    }
    if request
        .resume
        .as_ref()
        .is_some_and(|s| s.mode != request.mode || s.id != request.id)
    {
        return Err("Resume scope does not match the original update".into());
    }
    let resumed = request.resume.is_some();
    let old_phase = request.resume.as_ref().map(|s| s.phase.clone());
    let time = util::now();
    let mut state = request.resume.clone().unwrap_or(Checkpoint {
        schema_version: 1,
        id: request.id.clone(),
        revision: 0,
        mode: request.mode,
        phase: "inspect".into(),
        status: if request.apply { "running" } else { "planned" }.into(),
        created_at_utc: time.clone(),
        updated_at_utc: time,
        target_sha: None,
        target_source: None,
        desktop: None,
        candidate: None,
        backend: None,
        deployment_attempted: false,
        failure: None,
        acceptance: "not-recorded".into(),
    });
    let work = (|| -> Result<()> {
        state.failure = None;
        state.status = if request.apply { "running" } else { "planned" }.into();
        state.backend = Some(operations.inspect_backend()?);
        if request.mode == Mode::Coordinated {
            let observed = operations.inspect_desktop()?;
            let mut current = desktop(
                observed,
                !(state.target_sha.is_some() && state.desktop.is_some()),
            )?;
            if state.target_source.as_deref() == Some("completed-desktop-artifact")
                && state.desktop.as_ref().is_some_and(|d| {
                    d.installed_sha == current.installed_sha && d.receipt_ref.is_some()
                })
            {
                current.receipt_ref = state.desktop.as_ref().and_then(|d| d.receipt_ref.clone())
            }
            if !request.apply {
                state.desktop = Some(current);
                save(
                    &mut state,
                    operations,
                    if old_phase.as_deref() == Some("desktop-update") {
                        "desktop-update"
                    } else {
                        "inspect"
                    },
                )?;
                return Ok(());
            }
            if state.target_sha.is_some() && state.desktop.is_some() {
                if state.target_sha.as_deref() != Some(&current.installed_sha) {
                    return Err("Desktop changed after target pinning; the saved coordinated update must be reviewed".into());
                }
                if request
                    .expected_release_sha
                    .as_ref()
                    .is_some_and(|s| s != &current.installed_sha)
                {
                    return Err("Desktop source differs from the requested release".into());
                }
                if state.target_source.as_deref() == Some("verified-existing-desktop-release") {
                    save(&mut state, operations, "inspect")?;
                    let verified = desktop(operations.verify_existing_desktop()?, false)?;
                    if Some(&verified.installed_sha) != state.target_sha.as_ref() {
                        return Err("Verified existing Desktop changed after target pinning".into());
                    }
                    state.desktop = Some(verified)
                } else {
                    state.desktop = Some(current)
                }
            } else {
                let mut source = "completed-desktop-artifact";
                let d = if resumed && state.phase == "desktop-update" {
                    save(&mut state, operations, "desktop-update")?;
                    desktop(operations.recover_desktop()?, false)?
                } else if request.expected_release_sha.as_deref() == Some(&current.installed_sha)
                    && current.verified
                {
                    save(&mut state, operations, "inspect")?;
                    source = "verified-existing-desktop-release";
                    desktop(operations.verify_existing_desktop()?, false)?
                } else {
                    save(&mut state, operations, "desktop-update")?;
                    desktop(operations.update_desktop()?, false)?
                };
                if request
                    .expected_release_sha
                    .as_ref()
                    .is_some_and(|s| s != &d.installed_sha)
                {
                    return Err("Desktop source differs from the requested release".into());
                }
                state.target_sha = Some(d.installed_sha.clone());
                state.desktop = Some(d);
                state.target_source = Some(source.into());
            }
        } else if state.target_sha.is_none() {
            let s = if let Some(s) = &request.backend_sha {
                s.clone()
            } else {
                operations.resolve_main()?
            };
            require_sha(&s)?;
            state.target_sha = Some(s);
            state.target_source = Some(
                if request.backend_sha.is_some() {
                    "explicit-backend-sha"
                } else {
                    "upstream-main-at-start"
                }
                .into(),
            );
        }
        let sha = state
            .target_sha
            .clone()
            .ok_or("No verified target source commit")?;
        require_sha(&sha)?;
        save(&mut state, operations, "target-pinned")?;
        if !request.apply {
            return Ok(());
        }
        let backend = state.backend.as_ref().ok_or("Missing backend identity")?;
        if state.deployment_attempted
            && state.candidate.is_some()
            && backend.sha == sha
            && backend.healthy
        {
            save(&mut state, operations, "backend-deploy")?;
            operations.deploy_backend(state.candidate.as_ref().ok_or("Missing candidate")?)?
        } else if backend.sha != sha || !backend.healthy {
            save(&mut state, operations, "backend-prepare")?;
            state.candidate = Some(operations.prepare_backend(&sha)?);
            save(&mut state, operations, "backend-deploy")?;
            state.deployment_attempted = true;
            save(&mut state, operations, "backend-deploy")?;
            operations.deploy_backend(state.candidate.as_ref().ok_or("Missing candidate")?)?
        }
        save(&mut state, operations, "verify")?;
        let b = operations.verify_backend(&sha)?;
        let valid = b.healthy && b.verified && b.sha == sha;
        state.backend = Some(b);
        if !valid {
            return Err("Backend serving identity did not pass verification".into());
        }
        if request.mode == Mode::Coordinated {
            let receipt = if state.target_source.as_deref() == Some("completed-desktop-artifact") {
                state.desktop.as_ref().and_then(|d| d.receipt_ref.clone())
            } else {
                None
            };
            let mut d = desktop(operations.inspect_desktop()?, false)?;
            if d.installed_sha != sha {
                return Err(
                    "Desktop/backend source versions diverged during final verification".into(),
                );
            }
            if request
                .expected_release_sha
                .as_ref()
                .is_some_and(|s| s != &d.installed_sha)
            {
                return Err("Desktop source differs from the requested release".into());
            }
            if receipt.is_some() {
                d.receipt_ref = receipt
            }
            state.desktop = Some(d)
        }
        state.status = "complete".into();
        save(&mut state, operations, "complete")?;
        Ok(())
    })();
    if let Err(e) = work {
        state.status = "failed".into();
        state.failure = Some(serde_json::json!({"phase":state.phase,"message":e.to_string()}));
        let phase = state.phase.clone();
        save(&mut state, operations, &phase)?;
        return Err(e);
    }
    Ok(state)
}
