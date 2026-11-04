use crate::{
    Result,
    runner::{CommandSpec, Runner},
    util,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
pub const UPSTREAM: &str = "https://github.com/NousResearch/hermes-agent.git";
pub fn validate_tag(tag: &str) -> Result<()> {
    if !util::matches(
        r"^v(?:0|[1-9]\d{0,2})\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$",
        tag,
    ) {
        return Err("Use a canonical stable release tag such as v0.22.0".into());
    }
    Ok(())
}
pub fn parse_refs(stdout: &str, tag: &str) -> Result<String> {
    validate_tag(tag)?;
    let direct = format!("refs/tags/{tag}");
    let peeled = format!("{direct}^{{}}");
    let mut refs = BTreeMap::new();
    for l in stdout.lines().filter(|s| !s.trim().is_empty()) {
        let l = l.trim_end_matches('\r');
        let (sha, name) = l
            .split_once('\t')
            .ok_or("Invalid or ambiguous official release tag response")?;
        if !util::sha(sha)
            || ![direct.as_str(), peeled.as_str()].contains(&name)
            || refs.insert(name, sha).is_some()
        {
            return Err("Invalid or ambiguous official release tag response".into());
        }
    }
    if !refs.contains_key(direct.as_str()) {
        return Err("Official release tag was not found".into());
    }
    Ok(refs
        .get(peeled.as_str())
        .or_else(|| refs.get(direct.as_str()))
        .ok_or("Official release tag was not found")?
        .to_string())
}
fn args(tag: &str) -> Vec<String> {
    vec![
        "ls-remote".into(),
        "--exit-code".into(),
        UPSTREAM.into(),
        format!("refs/tags/{tag}"),
        format!("refs/tags/{tag}^{{}}"),
    ]
}
pub fn load(dir: &Path) -> Result<Option<Value>> {
    let p = dir.join("release-target.json");
    if !p.try_exists()? {
        return Ok(None);
    }
    let v: Value = util::json(p)?;
    let s = v["resolution"]["stdout"]
        .as_str()
        .ok_or("Invalid persisted release target evidence")?;
    let tag = v["requestedReleaseTag"]
        .as_str()
        .ok_or("Invalid persisted release target evidence")?;
    if v["schemaVersion"] != 1
        || v["runId"].as_str().unwrap_or("").is_empty()
        || !["backend-only", "coordinated"].contains(&v["mode"].as_str().unwrap_or(""))
        || v["upstream"] != UPSTREAM
        || !util::sha(v["sourceSha"].as_str().unwrap_or(""))
        || !util::seconds(v["resolvedAtUtc"].as_str().unwrap_or("")).is_finite()
        || v["acceptance"] != "not-recorded"
        || v["resolution"]["program"] != "git"
        || v["resolution"]["stdoutSha256"] != util::hash(s)
    {
        return Err("Invalid persisted release target evidence".into());
    }
    if v["resolution"]["args"] != json!(args(tag)) || parse_refs(s, tag)? != v["sourceSha"] {
        return Err("Persisted release target does not match its original Git evidence".into());
    }
    Ok(Some(v))
}
pub fn validate_resume(
    target: Option<&Value>,
    mode: &str,
    release: Option<&str>,
    sha: Option<&str>,
    checkpoint: Option<&super::workflow::Checkpoint>,
) -> Result<()> {
    let Some(t) = target else {
        if release.is_some() {
            return Err(
                "Cannot add a release target to an existing run without persisted release evidence"
                    .into(),
            );
        }
        return Ok(());
    };
    if t["mode"] != mode || checkpoint.is_some_and(|c| t["runId"] != c.id) {
        return Err("Persisted release target belongs to a different update run".into());
    }
    if sha.is_some() || release.is_some_and(|r| t["requestedReleaseTag"] != r) {
        return Err("Resume retains its original requested release and source SHA".into());
    }
    if checkpoint
        .and_then(|c| c.target_sha.as_ref())
        .is_some_and(|s| t["sourceSha"] != *s)
    {
        return Err("Saved update target differs from the requested release SHA".into());
    }
    Ok(())
}
pub fn resolve(r: &mut dyn Runner, dir: &Path, id: &str, mode: &str, tag: &str) -> Result<Value> {
    validate_tag(tag)?;
    if let Some(t) = load(dir)? {
        if t["runId"] != id {
            return Err("Release target belongs to a different run".into());
        }
        validate_resume(Some(&t), mode, Some(tag), None, None)?;
        return Ok(t);
    }
    let a = args(tag);
    let mut spec = CommandSpec::new("git", a.clone());
    spec.timeout_ms = 45_000;
    spec.environment
        .insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    spec.environment
        .insert("GIT_NO_LAZY_FETCH".into(), "1".into());
    let o = r.run(spec)?;
    if o.exit_code != 0 {
        return Err("Official release tag lookup failed; no update target was pinned".into());
    }
    let v = json!({"schemaVersion":1,"runId":id,"mode":mode,"requestedReleaseTag":tag,"sourceSha":parse_refs(&o.stdout,tag)?,"upstream":UPSTREAM,"resolvedAtUtc":util::now(),"resolution":{"program":"git","args":a,"stdout":o.stdout,"stdoutSha256":util::hash(&o.stdout)},"acceptance":"not-recorded"});
    util::immutable(&dir.join("release-target.json"), &v)?;
    Ok(v)
}
