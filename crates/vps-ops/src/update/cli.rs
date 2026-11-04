use super::{
    arguments, checkpoints, release, retry,
    workflow::{self, BackendIdentity, Checkpoint, DesktopIdentity, Mode, Operations},
};
use crate::{
    Result,
    adapters::{backend, desktop, ssh::HermesTarget},
    runner::Runner,
    util,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
struct NativeOperations<'a> {
    runner: &'a mut dyn Runner,
    target: HermesTarget,
    policy: Value,
    dir: PathBuf,
    attempt: PathBuf,
    root: PathBuf,
    options: arguments::Options,
    release: Option<Value>,
}
impl NativeOperations<'_> {
    fn desktop_target(&self) -> Result<Value> {
        let mut t: Value = util::json(self.root.join("architecture/desktop-update-target.json"))?;
        if let Some(release) = &self.release {
            t["channel"] = json!("stable");
            t["expectedReleaseSha"] = release["sourceSha"].clone();
            t["expectedReleaseTag"] = release["requestedReleaseTag"].clone();
        }
        Ok(t)
    }
    fn require_release(&self, d: DesktopIdentity) -> Result<DesktopIdentity> {
        if self
            .release
            .as_ref()
            .is_some_and(|r| r["sourceSha"] != d.installed_sha)
        {
            return Err("Completed Desktop SHA differs from the requested release; backend promotion is blocked".into());
        }
        Ok(d)
    }
    fn desktop_result(&self, v: Value) -> Result<DesktopIdentity> {
        if v["canonicalSha"].as_str().unwrap_or("").is_empty()
            || v["after"].is_null()
            || v["verification"]["eligibleForBackend"] != true
        {
            return Err(v["error"]["message"]
                .as_str()
                .unwrap_or("Desktop native update has no verified completed artifact")
                .to_owned()
                .into());
        }
        let mut d = desktop::identity(&v["after"], None);
        d.receipt_ref = v["receipt"]["archivePath"].as_str().map(str::to_owned);
        self.require_release(d)
    }
}
impl Operations for NativeOperations<'_> {
    fn inspect_backend(&mut self) -> Result<BackendIdentity> {
        backend::identity(
            &backend::inspect(self.runner, &self.target, &self.policy, &self.dir)?,
            &self.policy,
        )
    }
    fn resolve_main(&mut self) -> Result<String> {
        backend::resolve_main(self.runner, &self.target, &self.policy, &self.dir)
    }
    fn inspect_desktop(&mut self) -> Result<DesktopIdentity> {
        let t = self.desktop_target()?;
        let s = desktop::inspect(self.runner, &t, &self.dir, self.options.cdp.as_deref())?;
        Ok(desktop::identity(
            &s,
            desktop::pre_update_eligibility(&t, &s)?["eligible"].as_bool(),
        ))
    }
    fn verify_existing_desktop(&mut self) -> Result<DesktopIdentity> {
        let t = self.desktop_target()?;
        let s = desktop::verify_existing(self.runner, &t, &self.dir, self.options.cdp.as_deref())?;
        self.require_release(desktop::identity(&s, Some(false)))
    }
    fn update_desktop(&mut self) -> Result<DesktopIdentity> {
        let t = self.desktop_target()?;
        let v = desktop::update(
            self.runner,
            &t,
            &self.dir,
            self.options.authorization.as_deref().unwrap_or(""),
            self.options.cdp.as_deref(),
            None,
        )?;
        self.desktop_result(v)
    }
    fn recover_desktop(&mut self) -> Result<DesktopIdentity> {
        let t = self.desktop_target()?;
        let target = t["id"].as_str().unwrap_or("");
        let rejected = retry::rejected(&self.dir, target)?;
        util::record(
            &self.dir.join(format!(
                "desktop-rejected-launch-retry-decision-{}.json",
                util::id()
            )),
            &rejected,
        )?;
        let retry = retry::preflight(&self.dir, target)?;
        util::record(
            &self.dir.join(format!(
                "desktop-preflight-retry-decision-{}.json",
                util::id()
            )),
            &retry,
        )?;
        let v = if !rejected["candidate"].is_null() {
            desktop::update(
                self.runner,
                &t,
                &self.dir,
                self.options.authorization.as_deref().unwrap_or(""),
                self.options.cdp.as_deref(),
                Some(&rejected["candidate"]),
            )?
        } else if retry["eligible"] == true {
            desktop::update(
                self.runner,
                &t,
                &self.dir,
                self.options.authorization.as_deref().unwrap_or(""),
                self.options.cdp.as_deref(),
                None,
            )?
        } else {
            desktop::recover(self.runner, &t, &self.dir, self.options.cdp.as_deref())?
        };
        self.desktop_result(v)
    }
    fn prepare_backend(&mut self, sha: &str) -> Result<Value> {
        if self.release.as_ref().is_some_and(|r| r["sourceSha"] != sha) {
            return Err("Backend target differs from the requested release SHA".into());
        }
        backend::prepare(self.runner, &self.target, &self.policy, sha, &self.attempt)
    }
    fn deploy_backend(&mut self, c: &Value) -> Result<()> {
        backend::deploy(
            self.runner,
            &self.target,
            &self.policy,
            c,
            &self.attempt,
            self.options.authorization.as_deref().unwrap_or(""),
        )?;
        Ok(())
    }
    fn verify_backend(&mut self, sha: &str) -> Result<BackendIdentity> {
        backend::identity(
            &backend::verify(self.runner, &self.target, &self.policy, sha, &self.dir)?,
            &self.policy,
        )
    }
    fn save(&mut self, s: &Checkpoint) -> Result<()> {
        checkpoints::save(&self.dir, s)?;
        eprintln!("{}: {} ({})", s.mode.as_str(), s.phase, s.status);
        Ok(())
    }
}
pub fn run(r: &mut dyn Runner, args: &[String], root: &Path) -> Result<(Value, i32)> {
    let (command, options) = arguments::parse(args)?;
    let target: HermesTarget = util::json(root.join("architecture/targets.json"))?;
    let mut policy: Value = util::json(root.join("architecture/backend-update-policy.json"))?;
    let mut release = if let Some(dir) = &options.resume {
        release::load(Path::new(dir))?
    } else {
        None
    };
    let mut resume = None;
    if let Some(dir) = &options.resume {
        match checkpoints::load(Path::new(dir)) {
            Ok(s) => resume = Some(s),
            Err(e) => {
                let missing = e
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                    || e.to_string() == "Update checkpoint history is empty";
                if !missing || release.is_none() {
                    return Err(e);
                }
            }
        }
        release::validate_resume(
            release.as_ref(),
            &command,
            options.release.as_deref(),
            options.sha.as_deref(),
            resume.as_ref(),
        )?;
        if resume.as_ref().is_some_and(|s| {
            options
                .sha
                .as_ref()
                .is_some_and(|sha| s.target_sha.as_ref() != Some(sha))
        }) {
            return Err("Resume retains its original pinned SHA".into());
        }
    }
    let id = resume
        .as_ref()
        .map(|s| s.id.clone())
        .or_else(|| {
            release
                .as_ref()
                .and_then(|r| r["runId"].as_str().map(str::to_owned))
        })
        .unwrap_or_else(util::id);
    let directory = if let Some(d) = &options.resume {
        std::path::absolute(d)?
    } else {
        let d = root.join("runs").join(format!(
            "{}-update-{command}-{}",
            util::now().replace([':', '.'], "-"),
            &id[..8]
        ));
        fs::create_dir_all(d.parent().ok_or("Missing run parent")?)?;
        fs::create_dir(&d)?;
        d
    };
    let request = json!({"schemaVersion":1,"id":id,"command":command,"options":options,"actualAuthority":if options.apply{options.authorization.clone()}else{None},"effects":if options.apply{"Only the selected native update mode"}else{"Read-only target inspection"},"acceptance":"not-recorded"});
    util::record(
        &directory.join(format!("request-{}.json", util::id())),
        &request,
    )?;
    if command == "verify" {
        let sha = options.sha.as_deref().ok_or("verify requires --sha")?;
        let v = backend::verify(r, &target, &policy, sha, &directory)?;
        let mut out =
            json!({"runDirectory":directory,"mode":"read-only-verification","sourceSha":sha});
        if let Some(o) = v["verification"].as_object() {
            out.as_object_mut()
                .ok_or("Invalid output")?
                .extend(o.clone());
        }
        let code = if v["verification"]["passed"] == true {
            0
        } else {
            2
        };
        return Ok((out, code));
    }
    if command == "stage-overlay" {
        let overlay = backend::stage_overlay(
            r,
            &target,
            &policy,
            options
                .sha
                .as_deref()
                .ok_or("stage-overlay requires --sha")?,
            Path::new(options.patch.as_deref().ok_or("Missing patch")?),
            &directory,
        )?;
        util::record(
            &directory.join(format!("staged-overlay-{}.json", util::id())),
            &json!({"sourceSha":options.sha,"overlay":overlay,"nextStep":"Review the staged patch and update backend-update-policy.json overlay identity before resuming the same pinned update"}),
        )?;
        return Ok((
            json!({"runDirectory":directory,"overlay":overlay,"policyChanged":false}),
            0,
        ));
    }
    if options.resume.is_none()
        && let Some(tag) = &options.release
    {
        release = Some(release::resolve(r, &directory, &id, &command, tag)?);
    }
    if let Some(rel) = &release {
        policy["releaseTag"] = rel["requestedReleaseTag"].clone()
    }
    let attempt = directory.join(format!(
        "backend-attempt-{}-{}",
        policy["helperSha256"].as_str().unwrap_or(""),
        policy["overlay"]["sha256"]
            .as_str()
            .unwrap_or("source-only")
    ));
    fs::create_dir_all(&attempt)?;
    let mode = if command == "backend-only" {
        Mode::BackendOnly
    } else {
        Mode::Coordinated
    };
    let request = workflow::Request {
        id,
        mode,
        apply: options.apply,
        backend_sha: if mode == Mode::BackendOnly {
            release
                .as_ref()
                .and_then(|r| r["sourceSha"].as_str().map(str::to_owned))
                .or_else(|| options.sha.clone())
        } else {
            None
        },
        expected_release_sha: release
            .as_ref()
            .and_then(|r| r["sourceSha"].as_str().map(str::to_owned)),
        resume,
    };
    let mut ops = NativeOperations {
        runner: r,
        target,
        policy,
        dir: directory.clone(),
        attempt,
        root: root.to_owned(),
        options,
        release: release.clone(),
    };
    let result = workflow::run(request, &mut ops)?;
    let aligned = mode == Mode::Coordinated
        && result.target_sha.is_some()
        && result
            .desktop
            .as_ref()
            .is_some_and(|d| Some(&d.installed_sha) == result.target_sha.as_ref())
        && result
            .backend
            .as_ref()
            .is_some_and(|b| Some(&b.sha) == result.target_sha.as_ref());
    let running = if mode != Mode::Coordinated {
        "not-requested"
    } else if result.desktop.as_ref().is_none_or(|d| !d.running) {
        "not-running"
    } else if result
        .desktop
        .as_ref()
        .is_none_or(|d| d.running_sha.is_none())
    {
        "unverified"
    } else if result.desktop.as_ref().and_then(|d| d.running_sha.as_ref())
        == result.backend.as_ref().map(|b| &b.sha)
    {
        "verified"
    } else {
        "mismatch"
    };
    let mut out = json!({"runDirectory":directory,"mode":mode,"status":result.status,"sourceSha":result.target_sha,"targetSource":result.target_source,"installedVersionsAligned":aligned,"desktopRunningAlignment":running,"acceptance":result.acceptance});
    if let Some(rel) = release {
        out["requestedRelease"] = rel["requestedReleaseTag"].clone();
        out["requestedSourceSha"] = rel["sourceSha"].clone()
    }
    if let Some(b) = result.backend {
        out["backendVerified"] = json!(b.verified)
    }
    Ok((out, 0))
}
