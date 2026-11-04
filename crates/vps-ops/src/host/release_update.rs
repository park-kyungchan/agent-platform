//! Legacy release helper port. Main promotions still belong exclusively to hermes-main-update.
use crate::{
    Result,
    runner::{CommandSpec, Runner},
    util,
};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
pub fn replace_first_image(source: &str, image: &str) -> Result<String> {
    let r = regex::Regex::new(r"(?m)^(\s*image:\s*)\S+\s*$")?;
    if !r.is_match(source) {
        return Err("exactly one service image line was expected".into());
    }
    Ok(r.replacen(source, 1, |c: &regex::Captures<'_>| {
        format!("{}{image}", &c[1])
    })
    .into_owned())
}
fn command(r: &mut dyn Runner, args: Vec<String>) -> Result<crate::runner::CommandOutput> {
    {
        let mut spec = CommandSpec::new("docker", args);
        spec.timeout_ms = u64::MAX;
        r.run(spec)
    }
}
fn compose(project: &Path, file: &Path) -> Vec<String> {
    vec![
        "compose".into(),
        "-p".into(),
        "hermes-agent-2q3y".into(),
        "--env-file".into(),
        project.join(".env").display().to_string(),
        "-f".into(),
        file.display().to_string(),
    ]
}
fn with(mut a: Vec<String>, b: &[&str]) -> Vec<String> {
    a.extend(b.iter().map(|s| s.to_string()));
    a
}
pub struct ReleasePaths {
    pub project: PathBuf,
    pub lock: PathBuf,
}
impl Default for ReleasePaths {
    fn default() -> Self {
        Self {
            project: PathBuf::from("/docker/hermes-agent-2q3y"),
            lock: PathBuf::from("/run/lock/hermes-agent-update.lock"),
        }
    }
}
pub fn run(r: &mut dyn Runner, p: &ReleasePaths) -> Result<String> {
    let status = fs::read_to_string("/proc/self/status")?;
    let uid = status
        .lines()
        .find(|s| s.starts_with("Uid:"))
        .and_then(|s| s.split_whitespace().nth(2))
        .ok_or("Effective UID unavailable")?
        .parse()?;
    run_with_uid(r, p, uid)
}
/// The caller UID is injectable for deterministic fixtures; CLI always reads its actual effective UID.
#[derive(Debug)]
pub struct ReleaseFailure {
    pub stdout: String,
    pub message: String,
}
impl std::fmt::Display for ReleaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ReleaseFailure {}
pub fn run_with_uid(r: &mut dyn Runner, p: &ReleasePaths, uid: u32) -> Result<String> {
    let mut stdout = String::new();
    match execute_release(r, p, uid, &mut stdout) {
        Ok(()) => Ok(stdout),
        Err(e) => Err(Box::new(ReleaseFailure {
            stdout,
            message: e.to_string(),
        })),
    }
}
fn execute_release(r: &mut dyn Runner, p: &ReleasePaths, uid: u32, out: &mut String) -> Result<()> {
    if uid != 0 {
        return Err("root privileges are required; run with sudo.".into());
    }
    let file = p.project.join("docker-compose.yml");
    if !file.is_file() {
        return Err(format!("Compose file not found: {}", file.display()).into());
    }
    if !p.project.join(".env").is_file() {
        return Err(format!(
            "Environment file not found: {}",
            p.project.join(".env").display()
        )
        .into());
    }
    fs::create_dir_all(p.project.join(".update-backups"))?;
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(&p.lock)?;
    lock.try_lock()
        .map_err(|_| "another Hermes update is already running.")?;
    let mut spec = CommandSpec::new(
        "curl",
        [
            "-fsSL",
            "--retry",
            "3",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: hermes-agent-updater",
            "https://api.github.com/repos/NousResearch/hermes-agent/releases/latest",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    spec.timeout_ms = 120_000;
    let response = r.run(spec)?;
    if response.exit_code != 0 {
        return Err("could not retrieve the latest Hermes release.".into());
    }
    let release: Value = serde_json::from_str(&response.stdout)
        .map_err(|_| "GitHub returned an invalid release tag.")?;
    let tag = release["tag_name"].as_str().unwrap_or("");
    if !util::matches(r"^v[0-9][0-9A-Za-z._-]*$", tag) {
        return Err("GitHub returned an invalid release tag.".into());
    }
    let target = format!("nousresearch/hermes-agent:{tag}");
    *out = format!("Latest published release: {tag}\n");
    let pull = command(r, vec!["pull".into(), target.clone()])?;
    out.push_str(&pull.stdout);
    if pull.exit_code != 0 {
        return Err(format!("could not pull {target}").into());
    }
    let readback = command(
        r,
        vec![
            "image".into(),
            "inspect".into(),
            target.clone(),
            "--format".into(),
            "{{index .RepoDigests 0}}".into(),
        ],
    )?;
    let digest = readback.stdout.trim();
    if !digest.starts_with("nousresearch/hermes-agent@sha256:") {
        return Err(format!("could not determine the immutable image digest for {target}").into());
    }
    out.push_str(&format!("Immutable image: {digest}\n"));
    let source = fs::read_to_string(&file)?;
    let candidate = replace_first_image(&source, digest)?;
    let temporary = p
        .project
        .join(format!("docker-compose.yml.new.{}", util::id()));
    util::write_new(&temporary, candidate.as_bytes())?;
    let validate = command(
        r,
        with(compose(&p.project, &temporary), &["config", "--quiet"]),
    )?;
    if validate.exit_code != 0 {
        fs::remove_file(temporary)?;
        return Err("the generated Compose configuration is invalid.".into());
    }
    let backup = p.project.join(".update-backups").join(format!(
        "docker-compose.yml.{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(&file, &backup)?;
    fs::set_permissions(&backup, fs::metadata(&file)?.permissions())?;
    fs::rename(&temporary, &file)?;
    let deploy = command(
        r,
        with(
            compose(&p.project, &file),
            &[
                "up",
                "-d",
                "--force-recreate",
                "--remove-orphans",
                "--pull",
                "never",
                "--wait",
                "--wait-timeout",
                "180",
            ],
        ),
    )?;
    out.push_str(&deploy.stdout);
    if deploy.exit_code != 0 {
        fs::copy(&backup, &file)?;
        let rollback = command(
            r,
            with(
                compose(&p.project, &file),
                &[
                    "up",
                    "-d",
                    "--remove-orphans",
                    "--pull",
                    "never",
                    "--wait",
                    "--wait-timeout",
                    "180",
                ],
            ),
        );
        if let Ok(rollback) = rollback {
            out.push_str(&rollback.stdout);
        }
        return Err("Update failed; restoring the previous Compose configuration.\nERROR: Hermes was rolled back to the previous configuration.".into());
    }
    let _ = command(r, vec!["image".into(), "prune".into(), "-f".into()]);
    let container = command(
        r,
        with(compose(&p.project, &file), &["ps", "-q", "hermes-agent"]),
    )?;
    let cid = container.stdout.trim();
    if cid.is_empty() {
        return Err("Hermes container was not found after deployment.".into());
    }
    let status=command(r,vec!["inspect".into(),cid.into(),"--format".into(),"status={{.State.Status}} health={{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}} image={{.Config.Image}}".into()])?;
    out.push_str(status.stdout.trim());
    out.push('\n');
    if status.stdout.trim() != format!("status=running health=healthy image={digest}") {
        return Err("deployment verification failed.".into());
    }
    out.push_str(&format!(
        "Hermes latest release update completed successfully: {tag}\n"
    ));
    Ok(())
}
