use crate::{Result, util};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Options {
    pub apply: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}
pub const USAGE: &str = "update <backend-only|coordinated> [--plan|--apply] [--sha FULL_BACKEND_SHA|--release vX.Y.Z] [--resume RUN_DIRECTORY] [--desktop-cdp URL] [--authorization TEXT]; update verify --sha FULL_SHA; update stage-overlay --sha FULL_SHA --patch FILE --apply --authorization TEXT";
pub fn parse(args: &[String]) -> Result<(String, Options)> {
    let command = args.first().map(String::as_str).unwrap_or("");
    if !["backend-only", "coordinated", "verify", "stage-overlay"].contains(&command) {
        return Err(USAGE.into());
    }
    let mut o = Options::default();
    let mut action = false;
    let mut i = 1;
    while i < args.len() {
        let t = &args[i];
        if t == "--apply" || t == "--plan" {
            if action {
                return Err("Choose one of --plan or --apply".into());
            }
            o.apply = t == "--apply";
            action = true;
            i += 1;
            continue;
        }
        let field = match t.as_str() {
            "--sha" => &mut o.sha,
            "--release" => &mut o.release,
            "--resume" => &mut o.resume,
            "--desktop-cdp" => &mut o.cdp,
            "--authorization" => &mut o.authorization,
            "--patch" => &mut o.patch,
            _ => return Err(format!("Unknown, duplicate or incomplete option: {t}").into()),
        };
        i += 1;
        let v = args
            .get(i)
            .ok_or_else(|| format!("Unknown, duplicate or incomplete option: {t}"))?;
        if field.is_some() || v.starts_with("--") {
            return Err(format!("Unknown, duplicate or incomplete option: {t}").into());
        }
        *field = Some(v.clone());
        i += 1;
    }
    if o.sha.as_deref().is_some_and(|s| !util::sha(s)) {
        return Err("Use a full lowercase commit SHA".into());
    }
    if let Some(tag) = &o.release {
        super::release::validate_tag(tag)?
    }
    let error = if o.release.is_some() && ["verify", "stage-overlay"].contains(&command) {
        Some("--release belongs only to an update mode")
    } else if o.release.is_some() && o.sha.is_some() {
        Some("Choose one target: --release or --sha")
    } else if command == "coordinated" && o.sha.is_some() {
        Some("Desktop native completion determines the coordinated SHA")
    } else if command != "coordinated" && o.cdp.is_some() {
        Some("Desktop CDP belongs only to coordinated mode")
    } else if o.apply && o.authorization.as_deref().unwrap_or("").trim().is_empty() {
        Some("Record the current explicit update instruction with --authorization")
    } else if command == "stage-overlay" && (o.patch.is_none() || !o.apply) {
        Some("stage-overlay requires --patch and --apply")
    } else if command != "stage-overlay" && o.patch.is_some() {
        Some("--patch belongs only to stage-overlay")
    } else if ["verify", "stage-overlay"].contains(&command) && o.resume.is_some() {
        Some("--resume belongs only to an update mode")
    } else if command == "verify" && o.apply {
        Some("verify is read-only")
    } else {
        None
    };
    if let Some(e) = error {
        return Err(e.into());
    }
    if ["verify", "stage-overlay"].contains(&command) && o.sha.is_none() {
        return Err(format!("{command} requires --sha").into());
    }
    Ok((command.into(), o))
}
