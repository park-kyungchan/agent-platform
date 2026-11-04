use super::{remote, ssh::HermesTarget};
use crate::{Result, domain::rows, runner::Runner, update::workflow::BackendIdentity, util};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::{Duration, Instant},
};
pub const FONT_SHA: &str = "d923a5eb487a17c0a68801bba9ef0f56c81040a8";
pub const FONT_PATCH: &str = "d9d7d4bb8f446f80ffd7402d9ad685b829f454c3fd1d91c5d1589616ae8d2ee2";
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn font_policy(p: &Value) -> bool {
    p["fontReadability"] == true
        && p["botDesktop"] == true
        && p["releaseTag"] == "v0.22.2"
        && p["overlay"]["sha256"] == FONT_PATCH
}
pub fn image_tag(p: &Value, sha: &str) -> String {
    format!(
        "hermes-agent:main-{sha}{}{}",
        if p["botDesktop"] == true {
            "-desktop"
        } else {
            ""
        },
        p.get("overlay")
            .filter(|x| !x.is_null())
            .map(|o| format!("-overlay-{}", text(&o["sha256"])))
            .unwrap_or_default()
    )
}
pub fn release_stamp_matches(p: &Value, s: &Value) -> bool {
    (p["fontReadability"] != true || font_policy(p))
        && (p.get("releaseTag").is_none()
            || (s["dirty"] == json!(p["fontReadability"] == true)
                && s["displayVersion"] == text(&p["releaseTag"]).trim_start_matches('v')))
}
fn image(s: &Value) -> bool {
    util::matches("^sha256:[0-9a-f]{64}$", text(s))
}
fn overlay(p: &Value) -> Value {
    p["overlay"].get("sha256").cloned().unwrap_or(Value::Null)
}
pub fn validate_policy(t: &HermesTarget, p: &Value) -> Result<()> {
    if p["schemaVersion"] != 1 || !util::digest(text(&p["helperSha256"])) {
        return Err("Invalid backend updater policy".into());
    }
    for k in ["root", "updaterRoot", "helperPath"] {
        let s = text(&p[k]);
        if !s.starts_with('/') || s.contains('\0') || s.split('/').any(|s| s == "..") {
            return Err("Invalid backend policy path".into());
        }
    }
    let prefix = format!("{}/", text(&p["root"]));
    if !text(&p["updaterRoot"]).starts_with(&prefix) || !text(&p["helperPath"]).starts_with(&prefix)
    {
        return Err("Updater must belong to the target root".into());
    }
    let ss = rows(&p["services"]);
    if ![1, 2].contains(&ss.len())
        || !ss.contains(&json!(t.gateway_service))
        || ss
            .iter()
            .map(Value::to_string)
            .collect::<BTreeSet<_>>()
            .len()
            != ss.len()
        || ss
            .iter()
            .any(|s| !util::matches("^[a-zA-Z0-9_-]+$", text(s)))
    {
        return Err(
            "Expected only the intended Hermes service or the two bounded legacy services".into(),
        );
    }
    if ![
        "https://github.com/NousResearch/hermes-agent",
        "https://github.com/NousResearch/hermes-agent.git",
    ]
    .contains(&text(&p["upstream"]))
    {
        return Err("Backend upstream must be NousResearch/hermes-agent".into());
    }
    if let Some(o) = p.get("overlay")
        && (!util::digest(text(&o["sha256"]))
            || o["path"]
                != format!(
                    "{}/overlays/{}.patch",
                    text(&p["updaterRoot"]),
                    text(&o["sha256"])
                ))
    {
        return Err("Overlay must use its content-addressed native path".into());
    }
    for k in ["botDesktop", "fontReadability"] {
        if p.get(k).is_some_and(|x| !x.is_boolean()) {
            return Err(format!("{k} must be a boolean").into());
        }
    }
    if p["fontReadability"] == true && !font_policy(p) {
        return Err(
            "Font readability requires the exact reviewed release/Desktop/overlay tuple".into(),
        );
    }
    if p["botDesktop"] == true && p.get("overlay").is_some() && !font_policy(p) {
        return Err(
            "Official bot desktop variant requires pristine source without an overlay".into(),
        );
    }
    if let Some(tag) = p.get("releaseTag") {
        crate::update::release::validate_tag(text(tag))?;
        if p.get("overlay").is_some() && !font_policy(p) {
            return Err("Official release tag requires pristine source without an overlay".into());
        }
    }
    for (key, max) in [("pollIntervalMs", 20_000), ("jobTimeoutMs", 21_600_000)] {
        if p.get(key).is_some_and(|v| {
            v.as_f64()
                .is_none_or(|n| !n.is_finite() || n < 1. || n > max as f64)
        }) {
            return Err(format!("Invalid backend {key}").into());
        }
    }
    if let Some(a) = p.get("requiredActiveAdapters") {
        let v = rows(a);
        if !a.is_array()
            || v.is_empty()
            || v.iter()
                .map(Value::to_string)
                .collect::<BTreeSet<_>>()
                .len()
                != v.len()
            || v.iter()
                .any(|s| !util::matches("^[a-zA-Z0-9_.:-]+$", text(s)))
        {
            return Err(
                "Required active adapter names must be nonempty, unique identifiers".into(),
            );
        }
    }
    Ok(())
}
pub fn image_args(p: &Value, sha: &str) -> Result<Vec<String>> {
    if !util::sha(sha) {
        return Err("Expected a full lowercase upstream SHA".into());
    }
    if p["fontReadability"] == true && (!font_policy(p) || sha != FONT_SHA) {
        return Err("Font readability source SHA differs".into());
    }
    let mut a = vec!["--sha".into(), sha.into()];
    if p["fontReadability"] == true {
        a.push("--font-readability".into())
    }
    if p["botDesktop"] == true {
        a.push("--bot-desktop".into())
    }
    if p.get("releaseTag").is_some() {
        a.extend(["--release-tag".into(), text(&p["releaseTag"]).into()])
    }
    if let Some(o) = p.get("overlay") {
        a.extend([
            "--overlay-file".into(),
            text(&o["path"]).into(),
            "--overlay-sha256".into(),
            text(&o["sha256"]).into(),
        ])
    }
    Ok(a)
}
pub fn context(t: &HermesTarget, p: &Value) -> Result<String> {
    validate_policy(t, p)?;
    Ok(format!(
        "import json\ntarget={}\npolicy={}\n",
        util::literal(t)?,
        util::literal(p)?
    ))
}
fn snapshot_source() -> String {
    format!(
        "{}{}",
        include_str!("../../protocol/backend-update-backendServiceSelectionPython.py"),
        include_str!("../../protocol/backend-update-fragment-1.py")
    )
}
const GUARD: &str = include_str!("../../protocol/backend-update-fragment-6.py");
pub fn inspect(r: &mut dyn Runner, t: &HermesTarget, p: &Value, d: &Path) -> Result<Value> {
    remote(
        r,
        t,
        "inspect",
        &format!(
            "{}{}\nprint(json.dumps(out))\n",
            context(t, p)?,
            snapshot_source()
        ),
        d,
    )
}
pub fn resolve_main(r: &mut dyn Runner, t: &HermesTarget, p: &Value, d: &Path) -> Result<String> {
    let v = remote(
        r,
        t,
        "resolve-main",
        &format!(
            "{}{}",
            context(t, p)?,
            include_str!("../../protocol/backend-update-fragment-2.py")
        ),
        d,
    )?;
    let s = text(&v["sha"]);
    if !util::sha(s) {
        return Err("Expected a full lowercase upstream SHA".into());
    }
    Ok(s.into())
}
pub fn evaluate(t: &HermesTarget, p: &Value, sha: &str, s: &Value, time: &str) -> Value {
    let owner = &s["gatewayOwner"];
    let containers = rows(&s["containers"]);
    let null = Value::Null;
    let intended = containers
        .iter()
        .find(|c| c["service"] == t.gateway_service)
        .unwrap_or(&null);
    let n = &s["native"];
    let adapters: Vec<_> = rows(&n["adapters"])
        .iter()
        .filter(|a| a["configured"] != false)
        .collect();
    let mut required: BTreeSet<String> = p
        .get("requiredActiveAdapters")
        .map(|a| rows(a).iter().map(|x| text(x).to_owned()).collect())
        .unwrap_or_else(|| BTreeSet::from(["api_server".into()]));
    required.extend(
        rows(&n["configuredActiveAdapters"])
            .iter()
            .map(|x| text(x).to_owned()),
    );
    let age = util::seconds(time) - util::seconds(text(&s["observedAtUtc"]));
    let mut checks = BTreeMap::from([
        (
            "currentObservation",
            age.is_finite() && (-5.0..=60.0).contains(&age),
        ),
        (
            "targetScope",
            s["hostId"] == t.host_id && s["project"] == t.project,
        ),
        ("helperIdentity", s["helperSha256"] == p["helperSha256"]),
        (
            "updaterLockIdentity",
            p.get("updaterLock").is_none()
                || (s["updaterLock"]["device"] == p["updaterLock"]["device"]
                    && s["updaterLock"]["inode"] == p["updaterLock"]["inode"]),
        ),
        (
            "boundedServices",
            containers.len() == rows(&p["services"]).len()
                && rows(&p["services"])
                    .iter()
                    .all(|svc| containers.iter().filter(|c| &c["service"] == svc).count() == 1),
        ),
        (
            "sameImmutableImage",
            containers
                .iter()
                .all(|c| c["imageId"] == intended["imageId"] && image(&c["imageId"])),
        ),
        (
            "nativeCurrentImage",
            (p["fontReadability"] != true
                || (sha == FONT_SHA && s["current"]["tag"] == image_tag(p, sha)))
                && s["current"]["sha"] == sha
                && s["current"]["image_id"] == intended["imageId"]
                && s["current"]["overlay_sha256"] == overlay(p),
        ),
        (
            "healthyImagesAtPinnedCommit",
            containers.iter().all(|c| {
                c["running"] == true
                    && c["health"] == "healthy"
                    && c["revision"] == sha
                    && c["stamp"]["commit"] == sha
                    && c["provenance"]["revision"] == sha
                    && c["overlaySha256"] == c["stamp"]["overlaySha256"]
            }),
        ),
        (
            "expectedOverlay",
            p.get("overlay").is_none()
                || containers.iter().all(|c| c["overlaySha256"] == overlay(p)),
        ),
        (
            "requestedBotDesktop",
            p["botDesktop"] != true || containers.iter().all(|c| c["botDesktop"] == true),
        ),
        (
            "requestedReleaseVersion",
            p.get("releaseTag").is_none()
                || containers
                    .iter()
                    .all(|c| release_stamp_matches(p, &c["stamp"])),
        ),
        (
            "sharedDataMount",
            containers
                .iter()
                .all(|c| c["dataSource"] == intended["dataSource"]),
        ),
        (
            "exactSingletonOwner",
            s["gatewayLock"]["held"] == true
                && rows(&s["gatewayLock"]["holders"]).len() == 1
                && !owner.is_null()
                && owner["containerId"] == intended["containerId"]
                && owner["matchingFdCount"].as_u64().unwrap_or(0) > 0
                && owner["uid"] == t.native_uid,
        ),
        (
            "ownerSlotIdentity",
            !owner.is_null()
                && intended["rootSlot"]["up"] == true
                && intended["rootSlot"]["localPid"] == owner["localPid"]
                && intended["namespace"] == owner["namespace"],
        ),
        (
            "standbyParked",
            containers
                .iter()
                .filter(|c| c["service"] != t.gateway_service)
                .all(|c| c["rootSlot"]["up"] != true),
        ),
        (
            "nativeServing",
            n["nativeServing"] == true && n["nativeOwnerPid"] == owner["localPid"],
        ),
        (
            "freshIdentity",
            !owner.is_null()
                && n["identify"]["pid"] == owner["localPid"]
                && n["identify"]["start_time"] == owner["startTicks"]
                && n["identify"]["code_sha"] == sha,
        ),
        (
            "liveStatus",
            n["status"]["gateway_state"] == "running"
                && n["status"]["desired_state"] == "running"
                && n["status"]["answering_pid"] == owner["localPid"],
        ),
        (
            "expectedProfilesServing",
            t.expected_profiles.iter().all(|profile| {
                rows(&n["profiles"]).iter().any(|x| {
                    x["profile"] == *profile
                        && x["nativeOn"] == true
                        && x["liveServed"] == true
                        && x["nativeOwnerPid"] == owner["localPid"]
                })
            }),
        ),
        (
            "activeAdaptersHealthy",
            !adapters.is_empty()
                && adapters.iter().all(|a| {
                    a["state"] == "connected"
                        && a["writerPid"] == owner["localPid"]
                        && (a["errorCode"].is_null() || a["errorCode"] == "")
                })
                && required
                    .iter()
                    .all(|name| adapters.iter().any(|a| a["name"] == *name)),
        ),
        (
            "fullHealthCommit",
            n["health"]["code"] == 200 && n["health"]["commit"] == sha,
        ),
        ("ownerIncarnationPreserved", s["ownerPreserved"] == true),
    ]);
    if p["fontReadability"] == true {
        let f = &s["fontReadabilityProof"];
        checks.insert(
            "fontProbeIncarnationPreserved",
            s["fontProbeIncarnationPreserved"] == true,
        );
        checks.insert(
            "freshImageFontReadability",
            f["schema"] == 1
                && f["check"] == "font-readability"
                && f["result"] == "passed"
                && f["uid"] == 10000
                && f["files"] == 10
                && f["fresh_container"] == true
                && f["live_data_mounted"] == false
                && f["imageId"] == intended["imageId"]
                && f["overlaySha256"] == overlay(p)
                && f["overlayTreeSha"] == s["current"]["overlay_tree_sha"],
        );
    }
    json!({"passed":checks.values().all(|x|*x),"checks":checks})
}
/// Identities omit incidental time/FD count values; absent required identities fail.
pub fn font_runtime_identity(s: &Value) -> Option<Value> {
    fn process(v: &Value) -> Option<Value> {
        for k in ["hostPid", "localPid", "startTicks"] {
            if v[k].as_u64().unwrap_or(0) == 0 {
                return None;
            }
        }
        if text(&v["namespace"]).is_empty()
            || text(&v["containerId"]).is_empty()
            || v["uid"].as_u64().is_none()
        {
            return None;
        }
        Some(json!([
            v["hostPid"],
            v["localPid"],
            v["startTicks"],
            v["namespace"],
            v["containerId"],
            v["uid"]
        ]))
    }
    fn lock(v: &Value) -> Option<Value> {
        if v["device"].as_u64().is_none()
            || v["inode"].as_u64().unwrap_or(0) == 0
            || !v["held"].is_boolean()
        {
            return None;
        }
        Some(json!([v["device"], v["inode"], v["held"]]))
    }
    let o = process(&s["gatewayOwner"])?;
    let g = lock(&s["gatewayLock"])?;
    let u = lock(&s["updaterLock"])?;
    if s["gatewayOwner"]["matchingFdCount"].as_u64().unwrap_or(0) == 0
        || rows(&s["gatewayLock"]["holders"]).len() != 1
    {
        return None;
    }
    let h = process(&s["gatewayLock"]["holders"][0])?;
    if o != h {
        return None;
    }
    let c = &s["current"];
    if !util::sha(text(&c["sha"]))
        || text(&c["tag"]).is_empty()
        || !image(&c["image_id"])
        || !util::digest(text(&c["overlay_sha256"]))
        || !util::sha(text(&c["overlay_tree_sha"]))
        || text(&s["hostId"]).is_empty()
        || text(&s["project"]).is_empty()
        || !util::digest(text(&s["helperSha256"]))
        || rows(&s["containers"]).is_empty()
    {
        return None;
    }
    let mut cs = rows(&s["containers"]).to_vec();
    cs.sort_by_key(|c| text(&c["service"]).to_owned());
    let mut out = vec![];
    for c in cs {
        let slot = &c["rootSlot"];
        let si = if slot["up"] == true {
            Some(process(&slot["identity"])?)
        } else {
            None
        };
        if ["service", "containerId", "namespace", "dataSource"]
            .iter()
            .any(|k| text(&c[k]).is_empty())
            || !image(&c["imageId"])
            || !slot["up"].is_boolean()
            || (slot["up"] == true && slot["localPid"].as_u64().unwrap_or(0) == 0)
            || !util::sha(text(&c["revision"]))
            || !util::sha(text(&c["stamp"]["commit"]))
            || !util::sha(text(&c["provenance"]["revision"]))
            || !util::digest(text(&c["overlaySha256"]))
            || c["stamp"]["overlaySha256"] != c["overlaySha256"]
            || c["stamp"]["overlayTreeSha"] != s["current"]["overlay_tree_sha"]
        {
            return None;
        }
        out.push(json!([
            c["service"],
            c["containerId"],
            c["imageId"],
            c["namespace"],
            c["dataSource"],
            slot["up"],
            slot["localPid"],
            si,
            c["revision"],
            c["overlaySha256"],
            c["botDesktop"],
            c["stamp"]["commit"],
            c["stamp"]["overlaySha256"],
            c["stamp"]["overlayTreeSha"],
            c["stamp"]["dirty"],
            c["stamp"]["displayVersion"],
            c["provenance"]["revision"]
        ]));
    }
    Some(json!([
        s["hostId"],
        s["project"],
        s["helperSha256"],
        u,
        g,
        o,
        h,
        s["current"]["sha"],
        s["current"]["tag"],
        s["current"]["image_id"],
        s["current"]["overlay_sha256"],
        s["current"]["overlay_tree_sha"],
        out
    ]))
}
pub fn verify(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    sha: &str,
    d: &Path,
) -> Result<Value> {
    let args = image_args(p, sha)?;
    let native = format!(
        "import json\nrequest={}\n{}",
        util::literal(t)?,
        include_str!("../../protocol/backend-update-nativeProbePython.py")
    );
    let source = format!(
        "{}{}\nnative_code={}\n{}",
        context(t, p)?,
        snapshot_source(),
        serde_json::to_string(&native)?,
        include_str!("../../protocol/backend-update-fragment-4.py")
    );
    let mut s = remote(r, t, "verify", &source, d)?;
    if p["fontReadability"] == true {
        let c = &s["current"];
        if c["sha"] != sha
            || c["tag"] != image_tag(p, sha)
            || !image(&c["image_id"])
            || !util::sha(text(&c["overlay_tree_sha"]))
            || c["overlay_sha256"] != overlay(p)
            || rows(&s["containers"])
                .iter()
                .any(|x| x["imageId"] != c["image_id"])
        {
            return Err("Font verification current image tuple differs".into());
        }
        let mut argv = vec![
            text(&p["helperPath"]).to_owned(),
            "verify-font-readability".into(),
        ];
        argv.extend(args);
        argv.extend([
            "--probe-image-id".into(),
            text(&c["image_id"]).into(),
            "--probe-overlay-tree".into(),
            text(&c["overlay_tree_sha"]).into(),
        ]);
        let before = font_runtime_identity(&s);
        let proof = remote(
            r,
            t,
            "verify-font-readability",
            &format!(
                "{}{GUARD}\nargv={}\n{}",
                context(t, p)?,
                util::literal(&argv)?,
                include_str!("../../protocol/backend-update-fragment-5.py")
            ),
            d,
        )?;
        s = remote(r, t, "verify", &source, d)?;
        let after = font_runtime_identity(&s);
        s["fontReadabilityProof"] = proof;
        s["fontProbeIncarnationPreserved"] =
            json!(before.is_some() && after.is_some() && before == after);
    }
    s["verification"] = evaluate(t, p, sha, &s, &util::now());
    util::save(
        &d.join(format!("backend-verification-{}.json", util::id())),
        &s,
    )?;
    Ok(s)
}
pub fn correlate(job: &Value, txs: &[Value]) -> Result<Option<Value>> {
    let matches: Vec<_> = txs
        .iter()
        .filter(|tx| {
            tx["pid"] == job["mainPid"]
                && tx["candidate"]["sha"] == job["requested_sha"]
                && text(&tx["candidate"]["overlay_sha256"]) == text(&job["overlay_sha256"])
                && (job["bot_desktop"] != true
                    || tx["candidate"]["tag"]
                        == format!(
                            "hermes-agent:main-{}-desktop{}",
                            text(&job["requested_sha"]),
                            if job["overlay_sha256"].is_null() || job["overlay_sha256"] == "" {
                                String::new()
                            } else {
                                format!("-overlay-{}", text(&job["overlay_sha256"]))
                            }
                        ))
                && util::seconds(text(&tx["history"][0]["at"]))
                    >= util::seconds(text(&job["launched_at"])) - 2.
        })
        .collect();
    if matches.len() > 1 {
        return Err("Ambiguous native updater transaction correlation".into());
    }
    Ok(matches.first().map(|x| (*x).clone()))
}
pub fn completed(job: &Value, tx: Option<&Value>) -> Result<bool> {
    Ok(if let Some(tx) = tx {
        correlate(job, std::slice::from_ref(tx))?.is_some()
            && tx["last_phase"] == "complete"
            && tx["result"] == "passed"
    } else {
        false
    })
}
pub fn prepare_may_retry(job: &Value, tx: Option<&Value>, active: bool) -> Result<bool> {
    Ok(job["action"] == "prepare"
        && !active
        && if let Some(tx) = tx {
            correlate(job, std::slice::from_ref(tx))?.is_some() && tx["result"] == "failed"
        } else {
            false
        })
}
fn progress(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    job: &Value,
    d: &Path,
) -> Result<Value> {
    remote(
        r,
        t,
        &format!("poll-{}", text(&job["action"])),
        &format!(
            "{}\njob={}\n{}",
            context(t, p)?,
            util::literal(job)?,
            include_str!("../../protocol/backend-update-fragment-9.py")
        ),
        d,
    )
}
fn start_or_resume(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    action: &str,
    sha: &str,
    d: &Path,
    pre: Option<&Value>,
) -> Result<Value> {
    fs::create_dir_all(d)?;
    let cp = d.join(format!("backend-{action}-checkpoint.json"));
    let mut checkpoint: Option<Value> = if cp.try_exists()? {
        Some(util::json(&cp)?)
    } else {
        None
    };
    if let Some(c) = &checkpoint {
        if c["action"] != action
            || c["sha"] != sha
            || c["overlaySha256"] != overlay(p)
            || c["botDesktop"].as_bool().unwrap_or(false)
                != p["botDesktop"].as_bool().unwrap_or(false)
            || c["fontReadability"].as_bool().unwrap_or(false)
                != p["fontReadability"].as_bool().unwrap_or(false)
            || c["releaseTag"] != p["releaseTag"]
            || c["helperSha256"] != p["helperSha256"]
        {
            return Err("Run directory is already pinned to a different backend candidate".into());
        }
        if let Some(j) = c.get("job") {
            let progress = progress(r, t, p, j, d)?;
            let tx = correlate(j, rows(&progress["transactions"]))?;
            if prepare_may_retry(j, tx.as_ref(), progress["unitActive"] == true)? {
                fs::rename(
                    &cp,
                    d.join(format!(
                        "backend-prepare-failed-checkpoint-{}.json",
                        util::id()
                    )),
                )?;
                checkpoint = None
            } else {
                return Ok(j.clone());
            }
        }
    }
    if checkpoint.is_none() {
        let baseline = remote(
            r,
            t,
            "job-baseline",
            &format!(
                "{}\nimport pathlib\nprint(json.dumps([p.name for p in pathlib.Path(policy['updaterRoot']).joinpath('jobs').glob('*.json')]))\n",
                context(t, p)?
            ),
            d,
        )?;
        let c = json!({"schemaVersion":1,"action":action,"sha":sha,"overlaySha256":overlay(p),"botDesktop":p["botDesktop"].as_bool().unwrap_or(false),"fontReadability":p["fontReadability"].as_bool().unwrap_or(false),"releaseTag":p["releaseTag"],"helperSha256":p["helperSha256"],"requestedAtUtc":util::now(),"baselineJobs":baseline});
        util::save(&cp, &c)?;
        checkpoint = Some(c)
    } else {
        let mut c = checkpoint.ok_or("Missing checkpoint")?;
        let recovered = remote(
            r,
            t,
            "recover-job",
            &format!(
                "{}\ncheckpoint={}\n{}",
                context(t, p)?,
                util::literal(&c)?,
                include_str!("../../protocol/backend-update-fragment-7.py")
            ),
            d,
        )?;
        if !recovered.is_null() {
            c["job"] = recovered.clone();
            util::save(&cp, &c)?;
            return Ok(recovered);
        }
        return Err("Interrupted launch has no recoverable native job; inspect receipts before starting a new run directory".into());
    }
    let mut argv = vec![
        text(&p["helperPath"]).to_owned(),
        "start".into(),
        "--action".into(),
        action.into(),
    ];
    argv.extend(image_args(p, sha)?);
    if action == "deploy" {
        argv.push("--approve-deploy".into())
    }
    let job = remote(
        r,
        t,
        &format!("start-{action}"),
        &format!(
            "{}{GUARD}\nargv={}\nexpected={}\n{}",
            context(t, p)?,
            util::literal(&argv)?,
            util::literal(&pre)?,
            include_str!("../../protocol/backend-update-fragment-8.py")
        ),
        d,
    )?;
    if job["action"] != action
        || job["requested_sha"] != sha
        || job["overlay_sha256"] != overlay(p)
        || job["bot_desktop"].as_bool().unwrap_or(false)
            != p["botDesktop"].as_bool().unwrap_or(false)
        || job["font_readability"].as_bool().unwrap_or(false)
            != p["fontReadability"].as_bool().unwrap_or(false)
        || job["release_tag"] != p["releaseTag"]
    {
        return Err("Native launch receipt differs from the requested backend pin".into());
    }
    let mut c = checkpoint.ok_or("Missing checkpoint")?;
    c["job"] = job.clone();
    util::save(&cp, &c)?;
    Ok(job)
}
fn await_job(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    job: &Value,
    d: &Path,
) -> Result<Value> {
    let deadline =
        Instant::now() + Duration::from_millis(p["jobTimeoutMs"].as_u64().unwrap_or(3_600_000));
    let mut previous = String::new();
    while Instant::now() < deadline {
        let progress = progress(r, t, p, job, d)?;
        let tx = correlate(job, rows(&progress["transactions"]))?;
        if let Some(tx) = tx.as_ref() {
            util::save(
                &d.join(format!("backend-{}-transaction.json", text(&job["action"]))),
                tx,
            )?;
            if text(&tx["last_phase"]) != previous {
                eprintln!(
                    "Backend {}: {} ({})",
                    text(&job["action"]),
                    text(&tx["last_phase"]),
                    text(&tx["result"])
                );
                previous = text(&tx["last_phase"]).into()
            }
            if tx["result"] == "failed" {
                return Err(format!("Native backend {} failed at {}; durable candidate and receipts remain available",text(&job["action"]),text(&tx["last_phase"])).into());
            }
            if completed(job, Some(tx))? {
                return Ok(tx.clone());
            }
        }
        if progress["unitActive"] != true {
            return Err("Native unit stopped without a correlated successful terminal transaction; inspect receipts".into());
        }
        r.pause(p["pollIntervalMs"].as_u64().unwrap_or(5000))
    }
    Err("Backend job wait timed out; resume the same run directory to observe the existing native job".into())
}
pub fn prepared_candidate(p: &Value, sha: &str, s: &Value) -> Result<Value> {
    image_args(p, sha)?;
    let c = &s["candidate"];
    if c["status"] != "prepared"
        || c["sha"] != sha
        || c["tag"] != image_tag(p, sha)
        || c["source_verified"] != true
        || !image(&c["image_id"])
        || c["actual_image_id"] != c["image_id"]
        || c["validation_key"] != p["helperSha256"]
        || s["helperSha256"] != p["helperSha256"]
        || c["overlay_sha256"] != overlay(p)
        || (p.get("overlay").is_some() && !util::sha(text(&c["overlay_tree_sha"])))
    {
        return Err("Native candidate lacks matching immutable prepared evidence".into());
    }
    if p["fontReadability"] == true
        && (sha != FONT_SHA
            || !font_policy(p)
            || c["actual_stamp"]["overlaySha256"] != overlay(p)
            || c["actual_stamp"]["overlayTreeSha"] != c["overlay_tree_sha"])
    {
        return Err("Font-derived candidate source/stamp differs".into());
    }
    if p["botDesktop"] == true && c["bot_desktop_label"] != "1" {
        return Err("Native candidate lacks the requested official bot desktop variant".into());
    }
    if p.get("releaseTag").is_some()
        && (c["actual_stamp"]["commit"] != sha || !release_stamp_matches(p, &c["actual_stamp"]))
    {
        return Err("Native candidate release version is not independently verified".into());
    }
    let mut names = vec!["doctor", "plugin"];
    if p["fontReadability"] == true {
        names.push("fonts")
    }
    for name in names {
        let e = &c["validation_evidence"][name];
        if !text(&e["path"]).starts_with(&format!("{}/reports/", text(&p["updaterRoot"])))
            || text(&e["path"]).split('/').any(|s| s == "..")
            || !util::digest(text(&e["sha256"]))
            || e["actualSha256"] != e["sha256"]
        {
            return Err("Native candidate diagnostic evidence changed or is missing".into());
        }
    }
    let mut c = c.clone();
    c["helperSha256"] = s["helperSha256"].clone();
    Ok(c)
}
pub fn prepare(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    sha: &str,
    d: &Path,
) -> Result<Value> {
    validate_policy(t, p)?;
    image_args(p, sha)?;
    let s = inspect(r, t, p, d)?;
    if p["fontReadability"] != true
        && rows(&s["containers"]).iter().all(|c| {
            c["running"] == true
                && c["health"] == "healthy"
                && c["revision"] == sha
                && c["overlaySha256"] == overlay(p)
                && (p["botDesktop"] != true || c["botDesktop"] == true)
                && release_stamp_matches(p, &c["stamp"])
        })
    {
        let mut current = s["current"].clone();
        if current["sha"] != sha
            || rows(&s["containers"])
                .iter()
                .any(|c| c["imageId"] != current["image_id"])
        {
            return Err("Running backend differs from native recorded state".into());
        }
        current["status"] = json!("already-current");
        current["source_verified"] = json!(true);
        current["helperSha256"] = s["helperSha256"].clone();
        return Ok(current);
    }
    let job = start_or_resume(r, t, p, "prepare", sha, d, None)?;
    await_job(r, t, p, &job, d)?;
    let mut c = prepared_candidate(p, sha, &inspect(r, t, p, d)?)?;
    c["job"] = job;
    Ok(c)
}
pub fn deploy(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    c: &Value,
    d: &Path,
    authorization: &str,
) -> Result<Value> {
    validate_policy(t, p)?;
    let sha = text(&c["sha"]);
    image_args(p, sha)?;
    if authorization.trim().is_empty() {
        return Err("Backend deployment requires explicit authorization".into());
    }
    if c["helperSha256"] != p["helperSha256"] {
        return Err("Candidate updater identity differs from policy".into());
    }
    let before = inspect(r, t, p, d)?;
    let already = rows(&before["containers"]).iter().all(|x| {
        x["running"] == true
            && x["health"] == "healthy"
            && x["imageId"] == c["image_id"]
            && x["revision"] == sha
    });
    if c["status"] != "already-current" && !already {
        let recorded = prepared_candidate(p, sha, &before)?;
        if recorded["image_id"] != c["image_id"] || recorded["tag"] != c["tag"] {
            return Err("Reviewed backend candidate changed".into());
        }
        let job = start_or_resume(r, t, p, "deploy", sha, d, Some(&before))?;
        await_job(r, t, p, &job, d)?;
    }
    let mut deployed = inspect(r, t, p, d)?;
    if rows(&deployed["containers"])
        .iter()
        .any(|x| x["imageId"] != c["image_id"] || x["revision"] != sha || x["health"] != "healthy")
    {
        return Err("Backend readback differs from the completed native deployment".into());
    }
    if let Some(plan) = super::handoff::plan(t, p, c, &deployed)? {
        util::save(
            &d.join(format!("gateway-owner-handoff-{}.json", util::id())),
            &json!({"schemaVersion":1,"authority":authorization,"observedAtUtc":deployed["observedAtUtc"],"plan":plan,"scope":"Exact quiescent native service handoff; native desired state stop/start requests with final running readback; no image redeployment"}),
        )?;
        remote(
            r,
            t,
            "gateway-owner-handoff",
            &format!(
                "{}\nplan={}\n{}{}{}",
                context(t, p)?,
                util::literal(&plan)?,
                include_str!("../../protocol/gateway-owner-handoff-gatewayHandoffDriverPython.py"),
                include_str!(
                    "../../protocol/gateway-owner-handoff-gatewayHandoffProtocolPython.py"
                ),
                include_str!("../../protocol/backend-update-fragment-11.py")
            ),
            d,
        )?;
        deployed = inspect(r, t, p, d)?;
    }
    if p["parkStandby"] != false {
        if deployed["gatewayOwner"].is_null()
            || !rows(&deployed["containers"]).iter().any(|x| {
                x["service"] == t.gateway_service
                    && x["containerId"] == deployed["gatewayOwner"]["containerId"]
            })
        {
            return Err("Cannot park standby without the intended exact gateway owner".into());
        }
        remote(
            r,
            t,
            "park-standby",
            &format!(
                "{}\nexpected={}\n{}",
                context(t, p)?,
                util::literal(&deployed)?,
                include_str!("../../protocol/backend-update-fragment-10.py")
            ),
            d,
        )?;
    }
    let verified = verify(r, t, p, sha, d)?;
    if verified["verification"]["passed"] != true {
        return Err(
            "Backend native serving verification failed; inspect saved verification checks".into(),
        );
    }
    Ok(verified)
}
pub fn stage_overlay(
    r: &mut dyn Runner,
    t: &HermesTarget,
    p: &Value,
    sha: &str,
    patch: &Path,
    d: &Path,
) -> Result<Value> {
    use base64::Engine;
    validate_policy(t, p)?;
    if !util::sha(sha) {
        return Err("Expected a full lowercase upstream SHA".into());
    }
    let bytes = util::bounded(patch, 1048576)?;
    if bytes.is_empty() {
        return Err("Overlay must be nonempty and at most 1 MiB".into());
    }
    let digest = util::hash(&bytes);
    let argv = vec![
        text(&p["helperPath"]),
        "stage-overlay",
        "--sha",
        sha,
        "--overlay-sha256",
        &digest,
    ];
    let result = remote(
        r,
        t,
        "stage-overlay",
        &format!(
            "{}{GUARD}\npatch={}\nargv={}\n{}",
            context(t, p)?,
            serde_json::to_string(&base64::engine::general_purpose::STANDARD.encode(bytes))?,
            util::literal(&argv)?,
            include_str!("../../protocol/backend-update-fragment-12.py")
        ),
        d,
    )?;
    let path = format!("{}/overlays/{digest}.patch", text(&p["updaterRoot"]));
    if result["overlay_sha256"] != digest || result["source_sha"] != sha || result["path"] != path {
        return Err("Native overlay staging readback differs from the supplied patch".into());
    }
    Ok(json!({"path":path,"sha256":digest}))
}
pub fn identity(s: &Value, p: &Value) -> Result<BackendIdentity> {
    let c = &s["current"];
    if c.is_null() {
        return Err("Backend native current state is missing".into());
    }
    let cs = rows(&s["containers"]);
    Ok(BackendIdentity {
        sha: text(&c["sha"]).into(),
        image_id: text(&c["image_id"]).into(),
        healthy: s["helperSha256"] == p["helperSha256"]
            && (p["fontReadability"] != true
                || (c["sha"] == FONT_SHA
                    && c["tag"]
                        == format!("hermes-agent:main-{FONT_SHA}-desktop-overlay-{FONT_PATCH}")))
            && cs.len() == rows(&p["services"]).len()
            && rows(&p["services"])
                .iter()
                .all(|svc| cs.iter().filter(|x| &x["service"] == svc).count() == 1)
            && cs.iter().all(|x| {
                x["running"] == true
                    && x["health"] == "healthy"
                    && x["imageId"] == c["image_id"]
                    && x["revision"] == c["sha"]
                    && x["stamp"]["commit"] == c["sha"]
                    && x["provenance"]["revision"] == c["sha"]
                    && x["overlaySha256"] == overlay(p)
                    && (p["botDesktop"] != true || x["botDesktop"] == true)
                    && release_stamp_matches(p, &x["stamp"])
            }),
        verified: s["verification"]["passed"] == true,
    })
}
