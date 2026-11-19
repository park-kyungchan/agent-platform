//! The detector `checks` of `ap watch` (`specs/ap/watch.md` R20, R23 to R25): GitHub main
//! workflow conclusions, open Dependabot alerts and settings drift from `github/apply --check`.
use super::{Read, Reading, Signal};
use crate::host::{Reply, System};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const GITHUB_APPLY: &str = "/home/palantirkc/platforms/agent-platform/github/apply";
pub const GITHUB_REPOSITORIES: &str =
    "/home/palantirkc/platforms/agent-platform/github/repositories.json";
const GH_MS: u64 = 30_000;

fn call_words(s: &mut impl System, program: &str, words: &[&str]) -> Result<Reply, String> {
    let args: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    s.call(program, &args, &Value::Null, GH_MS)
        .map_err(|_| format!("{program} {} could not run", words.join(" ")))
}

fn vuln_alert_repos(s: &impl System) -> Result<BTreeSet<String>, String> {
    let bytes = s
        .read(GITHUB_REPOSITORIES)
        .map_err(|_| format!("{GITHUB_REPOSITORIES} cannot be read"))?;
    let v: Value =
        serde_json::from_slice(&bytes).map_err(|_| format!("{GITHUB_REPOSITORIES} is not JSON"))?;
    let list = v["repositories"]
        .as_array()
        .ok_or_else(|| format!("{GITHUB_REPOSITORIES} lacks repositories"))?;
    let mut enabled = BTreeSet::new();
    for entry in list {
        let name = entry["repository"]
            .as_str()
            .ok_or_else(|| format!("{GITHUB_REPOSITORIES} has an entry without repository"))?;
        if entry["vulnerability_alerts"] == true {
            enabled.insert(name.to_owned());
        }
    }
    Ok(enabled)
}

fn collect_main_red(s: &mut impl System, repo: &str, out: &mut Vec<Signal>) -> Result<(), String> {
    let r = call_words(
        s,
        "gh",
        &[
            "run",
            "list",
            "-R",
            repo,
            "--branch",
            "main",
            "--limit",
            "20",
            "--json",
            "workflowName,status,conclusion,headSha,createdAt",
        ],
    )?;
    if !r.success {
        return Err(format!("gh run list {repo} failed"));
    }
    let runs: Value =
        serde_json::from_slice(&r.stdout).map_err(|_| format!("gh run list {repo}: not JSON"))?;
    let runs = runs
        .as_array()
        .ok_or_else(|| format!("gh run list {repo}: not an array"))?;
    let mut newest: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    for run in runs {
        if !run.is_object() {
            return Err(format!("gh run list {repo}: entry is not an object"));
        }
        let workflow = run["workflowName"]
            .as_str()
            .filter(|w| !w.is_empty())
            .ok_or_else(|| format!("gh run list {repo}: missing workflowName"))?;
        let status = run["status"]
            .as_str()
            .ok_or_else(|| format!("gh run list {repo}: missing status"))?;
        let created_at = run["createdAt"]
            .as_str()
            .ok_or_else(|| format!("gh run list {repo}: missing createdAt"))?;
        let head_sha = run["headSha"]
            .as_str()
            .ok_or_else(|| format!("gh run list {repo}: missing headSha"))?;
        if status != "completed" {
            continue;
        }
        let conclusion = match &run["conclusion"] {
            Value::Null => "-",
            Value::String(c) if c.is_empty() => "-",
            Value::String(c) => c.as_str(),
            _ => return Err(format!("gh run list {repo}: invalid conclusion")),
        };
        let replace = newest
            .get(workflow)
            .is_none_or(|(prev, _, _)| created_at > prev.as_str());
        if replace {
            newest.insert(
                workflow.to_owned(),
                (
                    created_at.to_owned(),
                    conclusion.to_owned(),
                    head_sha.to_owned(),
                ),
            );
        }
    }
    for (workflow, (_, conclusion, head_sha)) in newest {
        if conclusion != "success" && conclusion != "skipped" {
            let head12: String = head_sha.chars().take(12).collect();
            out.push(Signal::new(
                format!("main red {repo} {workflow}"),
                "act",
                &format!("workflow {workflow} on {repo} main concluded {conclusion}"),
                format!("workflow {workflow}, conclusion {conclusion}, head {head12}"),
            ));
        }
    }
    Ok(())
}

fn is_dependabot_off(reply: &Reply) -> bool {
    if let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(&reply.bytes) {
        let status = obj.get("status");
        let is_404 = status.is_some_and(|v| v == "404" || v == 404);
        let is_403 = status.is_some_and(|v| v == "403" || v == 403);
        let msg = obj.get("message").and_then(Value::as_str);
        if is_404 || (status.is_none() && msg == Some("Not Found")) {
            return true;
        }
        if (is_403 || status.is_none())
            && msg == Some("Dependabot alerts are disabled for this repository.")
        {
            return true;
        }
        return false;
    }
    let text = String::from_utf8_lossy(&reply.bytes);
    if text.contains("(HTTP 404)") {
        return true;
    }
    text.contains("(HTTP 403)")
        && text.contains("Dependabot alerts are disabled for this repository.")
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn collect_dependabot(
    s: &mut impl System,
    repo: &str,
    out: &mut Vec<Signal>,
) -> Result<(), String> {
    let endpoint = format!("repos/{repo}/dependabot/alerts?state=open&per_page=100");
    let r = call_words(s, "gh", &["api", &endpoint])?;
    if !r.success {
        if is_dependabot_off(&r) {
            return Ok(());
        }
        return Err(format!("gh api dependabot {repo} failed"));
    }
    let alerts: Value = serde_json::from_slice(&r.stdout)
        .map_err(|_| format!("gh api dependabot {repo}: not JSON"))?;
    let alerts = alerts
        .as_array()
        .ok_or_else(|| format!("gh api dependabot {repo}: not an array"))?;
    if alerts.is_empty() {
        return Ok(());
    }
    let mut highest = "unknown";
    let mut best_rank = 0;
    for alert in alerts {
        if !alert.is_object() {
            return Err(format!(
                "gh api dependabot {repo}: alert entry is not an object"
            ));
        }
        let sev = alert["security_vulnerability"]["severity"]
            .as_str()
            .or_else(|| alert["security_advisory"]["severity"].as_str())
            .or_else(|| alert["severity"].as_str())
            .unwrap_or("unknown");
        let rank = severity_rank(sev);
        if rank > best_rank {
            best_rank = rank;
            highest = sev;
        }
    }
    let count = alerts.len();
    out.push(Signal::new(
        format!("dependabot {repo}"),
        "act",
        &format!("{repo} has {count} open dependabot alerts (highest {highest})"),
        format!("open alerts: {count}, highest severity: {highest}"),
    ));
    Ok(())
}

fn collect_settings(s: &mut impl System, out: &mut Vec<Signal>) -> Result<(), String> {
    let r = call_words(
        s,
        "python3",
        &[
            "-I",
            "-B",
            GITHUB_APPLY,
            "--check",
            "--file",
            GITHUB_REPOSITORIES,
        ],
    )?;
    let v: Value = serde_json::from_slice(&r.stdout)
        .map_err(|_| "github/apply did not print JSON".to_owned())?;
    let repos = v["repositories"]
        .as_array()
        .filter(|_| v["mode"] == "check")
        .ok_or_else(|| "github/apply answer lacks mode=check or repositories".to_owned())?;
    for entry in repos {
        let repo = entry["repository"]
            .as_str()
            .ok_or_else(|| "github/apply entry lacks repository".to_owned())?;
        let result = entry["result"]
            .as_str()
            .ok_or_else(|| format!("github/apply entry for {repo} lacks result"))?;
        let diffs = entry["differences"]
            .as_array()
            .ok_or_else(|| format!("github/apply entry for {repo} lacks differences"))?;
        match result {
            "failed" => return Err(format!("github/apply failed for {repo}")),
            "same" | "differs" | "refused" => {
                if result == "refused" {
                    let code = entry["code"].as_str().unwrap_or("-");
                    out.push(Signal::new(
                        format!("settings {repo} classic protection"),
                        "act",
                        &format!("{repo} has classic branch protection"),
                        format!("repository {repo}, result refused, code {code}"),
                    ));
                }
                for diff in diffs {
                    let setting = diff["setting"]
                        .as_str()
                        .ok_or_else(|| format!("github/apply diff for {repo} lacks setting"))?;
                    let present = diff
                        .get("present")
                        .ok_or_else(|| format!("github/apply diff for {repo} lacks present"))?;
                    let wanted = diff
                        .get("wanted")
                        .ok_or_else(|| format!("github/apply diff for {repo} lacks wanted"))?;
                    let severity = if wanted.is_null() { "info" } else { "act" };
                    let what = if wanted.is_null() {
                        format!("{repo} has unlisted {setting}")
                    } else {
                        format!("{repo} differs on {setting}")
                    };
                    out.push(Signal::new(
                        format!("settings {repo} {setting}"),
                        severity,
                        &what,
                        format!("setting {setting}: present {present}, wanted {wanted}"),
                    ));
                }
            }
            other => return Err(format!("github/apply unknown result {other} for {repo}")),
        }
    }
    Ok(())
}

/// `checks` (R20, R23 to R25): GitHub main workflow runs, open Dependabot alerts and settings.
pub(crate) fn detect(s: &mut impl System, policy: &Result<Option<Value>, String>) -> Reading {
    let policy = match policy {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Ok(Read {
                signals: vec![],
                unknown: vec!["main red".into(), "dependabot".into(), "settings".into()],
                state: None,
            });
        }
        Err(cause) => return Err(cause.clone()),
    };
    let Some(repos_val) = policy.get("repositories") else {
        return Ok(Read {
            signals: vec![],
            unknown: vec!["main red".into(), "dependabot".into(), "settings".into()],
            state: None,
        });
    };
    let repos_map = repos_val
        .as_object()
        .ok_or_else(|| "policy repositories is not an object".to_owned())?;
    let repos: BTreeSet<String> = repos_map.keys().cloned().collect();
    let vuln_enabled = vuln_alert_repos(s)?;
    let mut r = Read::default();
    for repo in &repos {
        collect_main_red(s, repo, &mut r.signals)?;
        if vuln_enabled.contains(repo) {
            collect_dependabot(s, repo, &mut r.signals)?;
        }
    }
    collect_settings(s, &mut r.signals)?;
    Ok(r)
}
