use crate::{
    Result,
    domain::rows,
    runner::{CommandSpec, Runner},
    update::workflow::DesktopIdentity,
    util,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, Instant},
};
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn reasons_value(reasons: Vec<String>) -> Value {
    json!({"eligible":reasons.is_empty(),"reasons":reasons})
}
pub fn validate_target(t: &Value) -> Result<()> {
    if t["branch"] != "main" || t.get("channel").is_some_and(|c| c != "stable") {
        return Err("Invalid native Desktop update selection".into());
    }
    if t.get("expectedReleaseSha").is_some() != t.get("expectedReleaseTag").is_some()
        || (t.get("expectedReleaseSha").is_some()
            && (!util::sha(text(&t["expectedReleaseSha"]))
                || crate::update::release::validate_tag(text(&t["expectedReleaseTag"])).is_err()
                || t["channel"] != "stable"))
    {
        return Err(
            "An exact release requires its stable channel, full SHA and release tag".into(),
        );
    }
    if let Some(a) = t.get("allowedPreUpdateDiff") {
        let paths = rows(&a["paths"]);
        if !util::digest(text(&a["sha256"]))
            || paths.is_empty()
            || paths
                .iter()
                .map(Value::to_string)
                .collect::<BTreeSet<_>>()
                .len()
                != paths.len()
            || paths.iter().any(|p| {
                text(p).is_empty()
                    || text(p).starts_with('/')
                    || text(p).contains('\\')
                    || text(p).split('/').any(|x| ["..", ".", ""].contains(&x))
            })
        {
            return Err("Invalid reviewed pre-update diff identity".into());
        }
    }
    Ok(())
}
fn sorted(v: &Value) -> Vec<String> {
    let mut a: Vec<_> = rows(v).iter().map(|v| text(v).to_owned()).collect();
    a.sort();
    a
}
pub fn selection(t: &Value) -> Result<Value> {
    validate_target(t)?;
    let mut s = json!({});
    if let Some(c) = t.get("channel") {
        s["channel"] = c.clone()
    } else {
        s["branch"] = t["branch"].clone()
    }
    if let Some(sha) = t.get("expectedReleaseSha") {
        s["expectedReleaseSha"] = sha.clone();
        s["expectedReleaseTag"] = t["expectedReleaseTag"].clone()
    }
    if let Some(a) = t.get("allowedPreUpdateDiff") {
        s["allowedPreUpdateDiff"] = json!({"sha256":a["sha256"],"paths":sorted(&a["paths"])});
    }
    Ok(s)
}
pub fn pre_update_eligibility(t: &Value, s: &Value) -> Result<Value> {
    validate_target(t)?;
    let mut reasons = vec![];
    let aligned = util::sha(text(&s["checkoutSha"]))
        && s["checkoutSha"] == s["checkoutStamp"]["commit"]
        && s["checkoutSha"] == s["installedArtifact"]["commit"]
        && s["nativeHomeVerified"] == true
        && s["installedArtifact"]["source"] != "fallback"
        && s["installedArtifact"]["updateMechanism"] == "self"
        && s["installedArtifact"]["payload"] == "bootstrap";
    if !aligned {
        reasons.push("native-install-identity-mismatch".into())
    }
    if s["checkoutDirty"] == true {
        let a = &t["allowedPreUpdateDiff"];
        let diff = &s["checkoutDiff"];
        if a.is_null()
            || diff.is_null()
            || diff["sha256"] != a["sha256"]
            || sorted(&diff["paths"]) != sorted(&a["paths"])
        {
            reasons.push("unreviewed-dirty-checkout".into())
        }
        if !rows(&diff["untrackedPaths"]).is_empty() {
            reasons.push("unreviewed-untracked-source".into())
        }
        if !rows(&s["processes"]).is_empty() {
            reasons.push("reviewed-dirty-update-requires-closed-desktop".into())
        }
    } else if s["checkoutStamp"]["dirty"] != false || s["installedArtifact"]["dirty"] != false {
        reasons.push("dirty-installed-artifact-without-reviewed-diff".into())
    }
    if s["marker"]["present"] == true {
        reasons.push("native-update-marker-present".into())
    }
    Ok(reasons_value(reasons))
}
pub fn validate_fleet(v: &Value, t: &Value, sha: &str) -> Vec<String> {
    let mut reasons = vec![];
    if v["install_method"] != "git"
        || v["updatable_in_place"] != true
        || v["expected_sha"] != sha
        || !v["runtimes"].is_array()
    {
        reasons.push("native-plan-identity-unverified".into())
    }
    for r in rows(&v["runtimes"]) {
        if !rows(&t["allowedNativeRuntimes"])
            .iter()
            .any(|a| a["kind"] == r["kind"] && a["supervisor"] == r["supervisor"])
        {
            reasons.push(format!(
                "unapproved-native-runtime:{}:{}",
                text(&r["kind"]),
                text(&r["supervisor"])
            ))
        }
    }
    reasons
}
pub fn normalize_receipt(raw: &Value, path: &str, sha: &str, len: usize) -> Value {
    let mut owed: Vec<String> = rows(&raw["followups"])
        .iter()
        .map(|r| {
            format!(
                "followup:{}",
                r["step"]
                    .as_str()
                    .filter(|x| !x.is_empty())
                    .unwrap_or("unknown")
            )
        })
        .collect();
    let restart = &raw["gateway_restart"];
    let recovery = &restart["fresh_recovery"];
    for (condition, reason) in [
        (
            !raw["user_action"].is_null()
                && raw["user_action"] != false
                && raw["user_action"] != "",
            "user-action",
        ),
        (restart["incomplete"] == true, "gateway-restart-incomplete"),
        (
            !rows(&restart["failed_units"]).is_empty(),
            "gateway-failed-units",
        ),
        (
            !text(&restart["phase_error"]).is_empty(),
            "gateway-phase-error",
        ),
        (
            !rows(&recovery["failed"]).is_empty(),
            "native-recovery-failed",
        ),
        (
            !rows(&recovery["stale_runtimes"]).is_empty(),
            "stale-runtimes",
        ),
        (
            !rows(&recovery["serve_units"]["failed"]).is_empty(),
            "serve-unit-failed",
        ),
    ] {
        if condition {
            owed.push(reason.into())
        }
    }
    for r in rows(&raw["fleet"]) {
        if ["stale", "down", "restart_pending", "unknown"].contains(&text(&r["state"])) {
            owed.push(format!("fleet:{}", text(&r["state"])))
        }
    }
    for r in rows(&raw["stages"]) {
        if ["build", "restart"].contains(&text(&r["name"]))
            && ["failed", "partial", "interrupted"].contains(&text(&r["outcome"]))
        {
            owed.push(format!("stage:{}", text(&r["name"])))
        }
    }
    for r in rows(&raw["steps"]) {
        if r["ok"] == false {
            owed.push(format!(
                "step:{}",
                r["name"]
                    .as_str()
                    .filter(|x| !x.is_empty())
                    .unwrap_or("unknown")
            ))
        }
    }
    let mut seen = BTreeSet::new();
    owed.retain(|s| seen.insert(s.clone()));
    let mut v = json!({"schema":raw["schema"],"updateId":raw["update_id"],"correlationId":raw["correlation_id"],"actionId":raw["action_id"],"startedAtUtc":raw["started_at"],"finishedAtUtc":raw["finished_at"],"outcome":raw["outcome"],"preSha":raw["pre_update"]["sha"],"postSha":raw["post_update"]["sha"],"owed":owed,"archivePath":path,"sha256":sha,"byteLength":len});
    let stash: Vec<_> = rows(&raw["steps"])
        .iter()
        .filter(|r| r["name"] == "local_changes_stash" && r["ok"] == false)
        .collect();
    if stash.len() == 1 {
        let detail = text(&stash[0]["detail"]);
        if let Some(sha) = detail
            .strip_prefix("parked: ")
            .and_then(|s| s.strip_suffix(" (--keep-stash)"))
            && util::sha(sha)
        {
            v["parkedLocalChanges"] = json!({"ok":false,"detail":detail,"stashSha":sha})
        }
    }
    v
}
pub fn verify_receipt(receipt: &Value, s: &Value, expected: &Value) -> Value {
    let mut reasons = vec![];
    let started = util::seconds(text(&receipt["startedAtUtc"]));
    let finished = util::seconds(text(&receipt["finishedAtUtc"]));
    if receipt["schema"] != 1 || !util::run_id(text(&receipt["updateId"])) {
        reasons.push("invalid-receipt-identity".into())
    }
    if !text(&receipt["archivePath"]).ends_with(&format!("_{}.json", text(&receipt["updateId"])))
        || text(&receipt["archivePath"]).ends_with("/latest.json")
    {
        reasons.push("not-run-specific-archive".into())
    }
    if !util::digest(text(&receipt["sha256"])) || receipt["byteLength"].as_u64().unwrap_or(0) == 0 {
        reasons.push("missing-original-receipt-provenance".into())
    }
    if receipt["correlationId"] != expected["correlationId"] {
        reasons.push("correlation-mismatch".into())
    }
    if !started.is_finite()
        || !finished.is_finite()
        || finished < started
        || started < util::seconds(text(&expected["startedAtUtc"])) - 5.
    {
        reasons.push("nonterminal-or-stale-receipt".into())
    }
    if receipt["outcome"] != "success" {
        reasons.push(format!(
            "native-outcome:{}",
            receipt["outcome"].as_str().unwrap_or("missing")
        ))
    }
    if receipt["preSha"] != expected["preSha"] {
        reasons.push("pre-update-sha-mismatch".into())
    }
    if expected
        .get("expectedReleaseSha")
        .is_some_and(|x| !x.is_null() && x != &receipt["postSha"])
    {
        reasons.push("requested-release-sha-mismatch".into())
    }
    let a = &expected["allowedPreUpdateDiff"];
    let stash = &expected["reviewedStash"];
    let parked = &receipt["parkedLocalChanges"];
    let matches = !a.is_null()
        && !stash.is_null()
        && !parked.is_null()
        && parked["ok"] == false
        && parked["detail"] == format!("parked: {} (--keep-stash)", text(&parked["stashSha"]))
        && util::sha(text(&parked["stashSha"]))
        && stash["stashSha"] == parked["stashSha"]
        && stash["baseSha"] == expected["preSha"]
        && stash["diffSha256"] == a["sha256"]
        && stash["receiptSha256"] == receipt["sha256"]
        && sorted(&stash["paths"]) == sorted(&a["paths"]);
    let discharged =
        if matches && rows(&receipt["owed"]).contains(&json!("step:local_changes_stash")) {
            vec!["step:local_changes_stash"]
        } else {
            vec![]
        };
    for r in rows(&receipt["owed"]) {
        if !discharged.contains(&text(r)) {
            reasons.push(text(r).into())
        }
    }
    let installed = util::sha(text(&receipt["postSha"]))
        && receipt["postSha"] == s["checkoutSha"]
        && s["checkoutSha"] == s["checkoutStamp"]["commit"]
        && s["checkoutSha"] == s["installedArtifact"]["commit"]
        && s["checkoutDirty"] == false
        && s["checkoutStamp"]["dirty"] == false
        && s["installedArtifact"]["dirty"] == false
        && s["installedArtifact"]["source"] != "fallback"
        && s["nativeHomeVerified"] == true;
    if !installed {
        reasons.push("installed-artifact-identity-mismatch".into())
    }
    let running = !rows(&s["processes"]).is_empty()
        && s["runningArtifact"]["commit"] == receipt["postSha"]
        && s["runningArtifact"]["bundleSwapPending"] == false
        && s["runningArtifact"]["bundleOutOfSync"] == false;
    if !s["runningArtifact"].is_null() && !running {
        reasons.push("running-artifact-identity-mismatch".into())
    }
    let mut v = json!({"eligibleForBackend":reasons.is_empty()&&installed,"canonicalSha":if reasons.is_empty()&&installed{receipt["postSha"].clone()}else{Value::Null},"installedAligned":installed,"runningVerified":running,"runningState":if running{"verified"}else if rows(&s["processes"]).is_empty(){"not-running"}else{"unverified"},"reasons":reasons});
    if !discharged.is_empty() {
        v["dischargedOwed"] = json!(discharged)
    }
    v
}
pub fn poll_state(code: Option<i32>, wait: bool, terminal: bool, marker: bool) -> &'static str {
    if code.is_some_and(|c| c != 0) {
        "native-failed"
    } else if terminal && !marker && (!wait || code == Some(0)) {
        "settled"
    } else {
        "waiting"
    }
}
pub fn build_command(t: &Value, id: &str) -> Result<Value> {
    validate_target(t)?;
    if !util::run_id(id) {
        return Err("Invalid native update correlation id".into());
    }
    let root = text(&t["installRootWindows"]);
    let script = format!("{root}\\scripts\\desktop-update\\windows.ps1");
    let exe = format!("{root}\\apps\\desktop\\release\\win-unpacked\\Hermes.exe");
    let env = json!({"HERMES_HOME":t["nativeHomeWindows"],"HERMES_INSTALL_ROOT":root,"HERMES_UPDATE_CORRELATION_ID":id,"PYTHONDONTWRITEBYTECODE":"1"});
    let mut args = vec!["-InstallRoot".to_owned(), root.into()];
    let mut params = json!({"InstallRoot":root,"NoGateway":true,"NoUi":true,"RelaunchExe":exe});
    if let Some(c) = t.get("channel") {
        args.extend(["-Channel".into(), text(c).into()]);
        params["Channel"] = c.clone()
    } else {
        args.extend(["-Branch".into(), text(&t["branch"]).into()]);
        params["Branch"] = t["branch"].clone()
    }
    args.extend([
        "-NoGateway".into(),
        "-NoUi".into(),
        "-RelaunchExe".into(),
        exe,
    ]);
    let mut source =
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue';\n".to_owned();
    for k in [
        "HERMES_HOME",
        "HERMES_INSTALL_ROOT",
        "HERMES_UPDATE_CORRELATION_ID",
        "PYTHONDONTWRITEBYTECODE",
    ] {
        source.push_str(&format!("$env:{k}={};\n", util::ps_quote(text(&env[k]))))
    }
    let order = [
        "InstallRoot",
        if t.get("channel").is_some() {
            "Channel"
        } else {
            "Branch"
        },
        "NoGateway",
        "NoUi",
        "RelaunchExe",
    ];
    source.push_str("$env:HERMES_RUNTIME_DIR=$null; $nativeParameters=@{");
    source.push_str(
        &order
            .iter()
            .map(|k| {
                format!(
                    "{k}={}",
                    if params[k].is_boolean() {
                        "$true".into()
                    } else {
                        util::ps_quote(text(&params[k]))
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
    );
    source.push_str(&format!(
        "}};\n& {} @nativeParameters; exit $LASTEXITCODE;",
        util::ps_quote(&script)
    ));
    Ok(
        json!({"command":t["powershellExecutable"],"args":["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-EncodedCommand",util::utf16_base64(&source)],"nativeArguments":args,"nativeParameters":params,"environment":env,"powershellSource":source}),
    )
}
pub fn sanitize_output(s: &str) -> String {
    let mut out = s.to_owned();
    for (p, replacement) in [
        (r"\x1b\[[0-9;]*[A-Za-z]", ""),
        (
            r"(?s)-----BEGIN [^-]*PRIVATE KEY-----.*?-----END [^-]*PRIVATE KEY-----",
            "[redacted-private-key]",
        ),
        (
            r"(?i)\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+",
            "[redacted-authorization]",
        ),
    ] {
        out = regex::Regex::new(p)
            .expect("constant sanitization regex")
            .replace_all(&out, replacement)
            .into_owned()
    }
    let url = regex::Regex::new(r#"(?i)https?://[^\s<>"']+"#).expect("URL regex");
    out = url
        .replace_all(&out, |c: &regex::Captures<'_>| {
            if let Ok(mut u) = url::Url::parse(&c[0]) {
                if !u.username().is_empty() || u.password().is_some() {
                    let _ = u.set_username("redacted");
                    let _ = u.set_password(None);
                }
                if u.query().is_some() {
                    u.set_query(Some("redacted"))
                }
                u.to_string()
            } else {
                "[redacted-url]".into()
            }
        })
        .into_owned();
    regex::Regex::new(r#"(?i)(["']?(?:authorization|password|passwd|secret|api[_-]?key|access[_-]?token|refresh[_-]?token|auth[_-]?token|credential)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;<]+)"#).expect("credential regex").replace_all(&out,"${1}[redacted]").into_owned()
}
fn record(d: &Path, name: &str, v: &Value) -> Result<()> {
    util::record(&d.join(format!("{name}.json")), v)
}
fn render_ps(source: &str, t: &Value, extra: &[(&str, String)]) -> String {
    let mut s = source.to_owned();
    if let Some(o) = t.as_object() {
        for (k, v) in o {
            if let Some(v) = v.as_str() {
                s = s.replace(&format!("@@{k}@@"), &v.replace('\'', "''"))
            }
        }
    }
    for (k, v) in extra {
        s = s.replace(&format!("@@{k}@@"), &v.replace('\'', "''"))
    }
    s
}
fn powershell(r: &mut dyn Runner, t: &Value, source: &str) -> Result<Value> {
    let code = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false);\n{source}"
    );
    let args = vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-EncodedCommand".into(),
        util::utf16_base64(&code),
    ];
    let o = r.run(CommandSpec::new(text(&t["powershellExecutable"]), args))?;
    if o.exit_code != 0 {
        return Err(format!(
            "Native metadata probe failed (exit {}); stderr hash {}",
            o.exit_code,
            util::hash(o.stderr)
        )
        .into());
    }
    Ok(serde_json::from_str(
        o.stdout.trim().trim_start_matches('\u{feff}'),
    )?)
}
fn artifact(p: &Path) -> Result<Value> {
    let bytes = util::bounded(p, 65536)?;
    let raw: Value =
        serde_json::from_slice(bytes.strip_prefix(&[239, 187, 191]).unwrap_or(&bytes))?;
    let mut a = json!({"sha256":util::hash(&bytes),"path":p});
    for k in ["commit", "builtAt", "source", "updateMechanism", "payload"] {
        a[k] = raw[k]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| json!(s))
            .unwrap_or(Value::Null)
    }
    a["dirty"] = raw["dirty"]
        .as_bool()
        .map(|v| json!(v))
        .unwrap_or(Value::Null);
    Ok(a)
}
pub fn marker(t: &Value) -> Result<Value> {
    let p = Path::new(text(&t["nativeHomeWsl"])).join(".hermes-update-in-progress");
    let bytes = match util::bounded(p, 16384) {
        Ok(b) => b,
        Err(e) => {
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                return Ok(json!({"present":false,"runId":null}));
            }
            return Err(e);
        }
    };
    let s = String::from_utf8_lossy(&bytes);
    let re = regex::Regex::new(r"(?m)^run:([A-Za-z0-9-]{16,128})\r?$")?;
    Ok(json!({"present":true,"runId":re.captures(&s).map(|c|c[1].to_owned())}))
}
struct Cdp {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    seq: u64,
    session: Option<String>,
}
impl Cdp {
    fn send(&mut self, method: &str, params: Value) -> Result<Value> {
        self.seq += 1;
        let n = self.seq;
        let mut request = json!({"id":n,"method":method,"params":params});
        if let Some(s) = &self.session {
            request["sessionId"] = json!(s)
        }
        self.socket
            .send(tungstenite::Message::Text(request.to_string().into()))?;
        loop {
            let msg = self.socket.read()?;
            if let tungstenite::Message::Text(s) = msg {
                let v: Value = serde_json::from_str(&s)?;
                if v["id"] == n {
                    if v.get("error").is_some() {
                        return Err("Native Desktop CDP command failed".into());
                    }
                    return Ok(v["result"].clone());
                }
            }
        }
    }
    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let v = self.send(
            "Runtime.evaluate",
            json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),
        )?;
        if v.get("exceptionDetails").is_some() {
            return Err("Desktop native bridge evaluation failed".into());
        }
        Ok(v["result"]["value"].clone())
    }
}
fn connect_desktop(r: &mut dyn Runner, t: &Value, endpoint: &str) -> Result<(Cdp, Value)> {
    let url = url::Url::parse(endpoint)?;
    if !["http", "https", "ws", "wss"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err("CDP must be an explicit HTTP/WebSocket endpoint without URL credentials or query parameters".into());
    }
    let ws = if ["http", "https"].contains(&url.scheme()) {
        let base = endpoint.trim_end_matches('/');
        let o = r.run(CommandSpec::new(
            "curl",
            vec![
                "-fsS".into(),
                "--max-time".into(),
                "10".into(),
                format!("{base}/json/version"),
            ],
        ))?;
        if o.exit_code != 0 {
            return Err("Desktop CDP metadata unavailable".into());
        }
        let v: Value = serde_json::from_str(&o.stdout)?;
        text(&v["webSocketDebuggerUrl"]).to_owned()
    } else {
        endpoint.into()
    };
    let endpoint = url::Url::parse(&ws)?;
    use std::net::ToSocketAddrs;
    let address = (
        endpoint.host_str().ok_or("CDP host missing")?,
        endpoint.port_or_known_default().ok_or("CDP port missing")?,
    )
        .to_socket_addrs()?
        .next()
        .ok_or("CDP address unavailable")?;
    let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let (socket, _) = tungstenite::client_tls(ws.as_str(), stream)?;
    let mut cdp = Cdp {
        socket,
        seq: 0,
        session: None,
    };
    let timeout = Some(Duration::from_secs(10));
    match cdp.socket.get_mut() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => {
            s.set_read_timeout(timeout)?;
            s.set_write_timeout(timeout)?
        }
        tungstenite::stream::MaybeTlsStream::Rustls(s) => {
            s.sock.set_read_timeout(timeout)?;
            s.sock.set_write_timeout(timeout)?
        }
        _ => {}
    }
    let targets = cdp.send("Target.getTargets", json!({}));
    let mut matches = vec![];
    if let Ok(ts) = targets {
        for target in rows(&ts["targetInfos"]) {
            if target["type"] != "page" {
                continue;
            }
            cdp.session = None;
            let a = cdp.send(
                "Target.attachToTarget",
                json!({"targetId":target["targetId"],"flatten":true}),
            )?;
            let session = text(&a["sessionId"]).to_owned();
            cdp.session = Some(session.clone());
            if let Ok(v)=cdp.evaluate("(async()=>globalThis.hermesDesktop ? await globalThis.hermesDesktop.getVersion() : {})()")&& util::windows_path(text(&v["hermesRoot"]))==util::windows_path(text(&t["installRootWindows"])){matches.push((Some(session),v));}
        }
    } else {
        cdp.session = None;
        if let Ok(v)=cdp.evaluate("(async()=>globalThis.hermesDesktop ? await globalThis.hermesDesktop.getVersion() : {})()")&& util::windows_path(text(&v["hermesRoot"]))==util::windows_path(text(&t["installRootWindows"])){matches.push((None,v))}
    }
    if matches.len() != 1 {
        return Err("CDP must expose exactly one matching Desktop native bridge; no page is created or closed".into());
    }
    let (session, version) = matches.remove(0);
    cdp.session = session;
    Ok((cdp, version))
}
fn running_artifact(v: &Value) -> Value {
    json!({"commit":v["commit"].as_str().filter(|s|!s.is_empty()),"bundleSwapPending":v["bundleSwapPending"].as_bool(),"bundleOutOfSync":v["bundleOutOfSync"].as_bool()})
}
pub fn inspect(r: &mut dyn Runner, t: &Value, d: &Path, cdp: Option<&str>) -> Result<Value> {
    validate_target(t)?;
    if t["schemaVersion"] != 1
        || !text(&t["installRootWsl"]).starts_with("/mnt/")
        || !text(&t["nativeHomeWsl"]).starts_with("/mnt/")
    {
        return Err("Invalid Windows Desktop target".into());
    }
    if t["receiptTimeoutMs"]
        .as_u64()
        .is_none_or(|n| !(1000..=3_600_000).contains(&n))
    {
        return Err(
            "Native receipt observation duration must be bounded between 1 second and 1 hour"
                .into(),
        );
    }
    if marker(t)?["present"] == true {
        return Err(
            "Native Desktop inspection waits for the existing update marker to settle".into(),
        );
    }
    for (wsl, win) in [
        ("installRootWsl", "installRootWindows"),
        ("nativeHomeWsl", "nativeHomeWindows"),
    ] {
        let p = Path::new(text(&t[wsl]));
        if fs::canonicalize(p)? != p {
            return Err("Desktop roots must be canonical paths".into());
        }
        let o = r.run(CommandSpec::new(
            "wslpath",
            vec!["-w".into(), text(&t[wsl]).into()],
        ))?;
        if o.exit_code != 0
            || util::windows_path(o.stdout.trim()) != util::windows_path(text(&t[win]))
        {
            return Err("WSL and Windows target paths do not name the same install/home".into());
        }
    }
    let root = Path::new(text(&t["installRootWsl"]));
    let checkout = artifact(&root.join("install-stamp.json"))?;
    let installed =
        artifact(&root.join("apps/desktop/release/win-unpacked/resources/install-stamp.json"))?;
    let script = util::bounded(root.join("scripts/desktop-update/windows.ps1"), 256 * 1024)?;
    let active = marker(t)?;
    let n = powershell(
        r,
        t,
        &render_ps(include_str!("../../protocol/desktop-inspect.ps1"), t, &[]),
    )?;
    if !util::sha(text(&n["checkoutSha"])) || !n["checkoutDirty"].is_boolean() {
        return Err("Checkout identity could not be verified".into());
    }
    let mut s = json!({"schemaVersion":1,"observedAtUtc":util::now(),"targetId":t["id"],"checkoutSha":n["checkoutSha"],"checkoutDirty":n["checkoutDirty"],"checkoutStamp":checkout,"installedArtifact":installed,"processes":rows(&n["processes"]),"nativeUserHomeWindows":n["nativeUserHomeWindows"],"nativeHomeWindows":n["nativeHomeWindows"],"nativeHomeVerified":util::windows_path(text(&n["nativeUserHomeWindows"]))==util::windows_path(text(&t["nativeUserHomeWindows"]))&&util::windows_path(text(&n["nativeHomeWindows"]))==util::windows_path(text(&t["nativeHomeWindows"])),"nativeScript":{"path":root.join("scripts/desktop-update/windows.ps1"),"sha256":util::hash(&script),"automaticCorrelation":String::from_utf8_lossy(&script).contains("$env:HERMES_UPDATE_CORRELATION_ID = $script:UpdateCorrelation")&&String::from_utf8_lossy(&script).contains("run_id")},"marker":active,"runningArtifact":null});
    if n["checkoutDirty"] == true {
        s["checkoutDiff"] = json!({"sha256":n["checkoutDiff"]["sha256"],"paths":rows(&n["checkoutDiff"]["paths"]),"untrackedPaths":rows(&n["checkoutDiff"]["untrackedPaths"])});
    }
    if let Some(endpoint) = cdp
        && !rows(&s["processes"]).is_empty()
    {
        let (_, version) = connect_desktop(r, t, endpoint)?;
        s["runningArtifact"] = running_artifact(&version)
    }
    record(d, &format!("desktop-inspection-{}", util::id()), &s)?;
    Ok(s)
}
fn verify_installed(r: &mut dyn Runner, t: &Value) -> Result<()> {
    let result = powershell(
        r,
        t,
        &render_ps(
            include_str!("../../protocol/desktop-verify-installed.ps1"),
            t,
            &[],
        ),
    )?;
    if result["verified"] != true {
        return Err("Native installed artifact integrity was not verified".into());
    }
    Ok(())
}
pub fn verify_release_target(v: &Value, t: &Value) -> Result<()> {
    validate_target(t)?;
    if t["channel"] != "stable"
        || v["requestedChannel"] != "stable"
        || v["channel"] != "stable"
        || v["repository"] != "NousResearch/hermes-agent"
        || !util::sha(text(&v["commit"]))
        || t.get("expectedReleaseSha")
            .is_some_and(|x| x != &v["commit"])
        || t.get("expectedReleaseTag")
            .is_some_and(|x| x != &format!("v{}", text(&v["version"])))
    {
        return Err("native-release-target-mismatch".into());
    }
    Ok(())
}
fn release_target(r: &mut dyn Runner, t: &Value) -> Result<Value> {
    use base64::Engine;
    let projection = format!(
        "import json; from pathlib import Path; from hermes_cli.source_releases import resolve_source_target; t=resolve_source_target('stable', git_cmd=[{}], cwd=Path({})); print(json.dumps({{'requestedChannel':t.requested_channel,'channel':t.channel,'repository':t.repository,'commit':t.commit,'version':t.version,'buildId':t.build_id}}))",
        serde_json::to_string(&t["nativeGitExecutableWindows"])?,
        serde_json::to_string(&t["installRootWindows"])?
    );
    let append = format!(
        "; exec(__import__('base64').b64decode('{}'))",
        base64::engine::general_purpose::STANDARD.encode(projection)
    );
    let v = powershell(
        r,
        t,
        &render_ps(
            include_str!("../../protocol/desktop-release.ps1"),
            t,
            &[("append", append)],
        ),
    )?;
    verify_release_target(&v, t)?;
    Ok(v)
}
fn verify_stash(
    r: &mut dyn Runner,
    t: &Value,
    receipt: &Value,
    pre: &str,
    d: &Path,
) -> Result<Value> {
    let parked = &receipt["parkedLocalChanges"];
    let allowed = &t["allowedPreUpdateDiff"];
    if parked.is_null() || allowed.is_null() {
        return Ok(Value::Null);
    }
    validate_target(t)?;
    if receipt["outcome"] != "success"
        || !util::seconds(text(&receipt["finishedAtUtc"])).is_finite()
        || !util::sha(pre)
        || receipt["preSha"] != pre
        || !util::sha(text(&receipt["postSha"]))
        || !util::sha(text(&parked["stashSha"]))
        || parked["detail"] != format!("parked: {} (--keep-stash)", text(&parked["stashSha"]))
    {
        return Ok(Value::Null);
    }
    if marker(t)?["present"] == true {
        return Err(
            "Reviewed stash readback waits for the existing native marker to settle".into(),
        );
    }
    let v = powershell(
        r,
        t,
        &render_ps(
            include_str!("../../protocol/desktop-stash.ps1"),
            t,
            &[("stashSha", text(&parked["stashSha"]).into())],
        ),
    )?;
    if v["stashSha"] != parked["stashSha"]
        || v["baseSha"] != pre
        || v["headSha"] != receipt["postSha"]
        || v["diffSha256"] != allowed["sha256"]
        || sorted(&v["paths"]) != sorted(&allowed["paths"])
    {
        return Err("reviewed-stash-identity-mismatch".into());
    }
    let proof = json!({"stashSha":parked["stashSha"],"baseSha":pre,"diffSha256":allowed["sha256"],"paths":v["paths"],"receiptSha256":receipt["sha256"],"observedAtUtc":util::now()});
    record(
        d,
        &format!(
            "desktop-reviewed-stash-{}-{}",
            text(&receipt["updateId"]),
            util::id()
        ),
        &proof,
    )?;
    Ok(proof)
}
pub fn archive(t: &Value, correlation: &str) -> Result<Option<Value>> {
    let d = Path::new(text(&t["nativeHomeWsl"])).join("logs/update_receipts");
    if !d.try_exists()? {
        return Ok(None);
    }
    let mut found = vec![];
    for name in util::names(&d)? {
        if !util::matches(r"^update_[A-Za-z0-9_-]+\.json$", &name) {
            continue;
        }
        let p = d.join(name);
        let b = util::bounded(&p, 2 * 1024 * 1024)?;
        let raw: Value = serde_json::from_slice(b.strip_prefix(&[239, 187, 191]).unwrap_or(&b))?;
        if raw["correlation_id"] == correlation && !text(&raw["finished_at"]).is_empty() {
            found.push(normalize_receipt(
                &raw,
                &p.to_string_lossy(),
                &util::hash(&b),
                b.len(),
            ))
        }
    }
    if found.len() > 1 {
        return Err("More than one terminal archive matches the native correlation; explicit reconciliation is required".into());
    }
    Ok(found.pop())
}
pub fn existing_identity(t: &Value, s: &Value) -> Result<&'static str> {
    let sha = &t["expectedReleaseSha"];
    if sha.is_null()
        || s["targetId"] != t["id"]
        || s["checkoutSha"] != *sha
        || s["checkoutDirty"] != false
        || s["nativeHomeVerified"] != true
        || s["marker"]["present"] != false
        || s["checkoutStamp"]["commit"] != *sha
        || s["checkoutStamp"]["dirty"] != false
        || s["installedArtifact"]["commit"] != *sha
        || s["installedArtifact"]["dirty"] != false
        || text(&s["installedArtifact"]["source"]).is_empty()
        || s["installedArtifact"]["source"] == "fallback"
        || s["installedArtifact"]["updateMechanism"] != "self"
        || s["installedArtifact"]["payload"] != "bootstrap"
    {
        return Err(
            "Existing Desktop release source and installed artifact are not strictly aligned"
                .into(),
        );
    }
    let a = &s["runningArtifact"];
    if !a.is_null()
        && ((!a["commit"].is_null() && a["commit"] != *sha)
            || a["bundleSwapPending"] == true
            || a["bundleOutOfSync"] == true)
    {
        return Err("Existing Desktop running artifact differs from the requested release".into());
    }
    Ok(if rows(&s["processes"]).is_empty() {
        "not-running"
    } else if a["commit"] == *sha
        && a["bundleSwapPending"] == false
        && a["bundleOutOfSync"] == false
    {
        "verified"
    } else {
        "unverified"
    })
}
pub fn existing_unchanged(before: &Value, after: &Value) -> bool {
    [
        "checkoutSha",
        "nativeUserHomeWindows",
        "nativeHomeWindows",
        "processes",
    ]
    .iter()
    .all(|k| before[k] == after[k])
        && ["checkoutStamp", "installedArtifact", "nativeScript"]
            .iter()
            .all(|k| before[k]["sha256"] == after[k]["sha256"])
}
pub fn verify_existing(
    r: &mut dyn Runner,
    t: &Value,
    d: &Path,
    cdp: Option<&str>,
) -> Result<Value> {
    validate_target(t)?;
    if t.get("expectedReleaseSha").is_none() || t.get("expectedReleaseTag").is_none() {
        return Err("Existing Desktop verification requires an exact release SHA and tag".into());
    }
    let before = inspect(r, t, d, cdp)?;
    existing_identity(t, &before)?;
    verify_installed(r, t)?;
    let after = inspect(r, t, d, cdp)?;
    let alignment = existing_identity(t, &after)?;
    if !existing_unchanged(&before, &after) {
        return Err(
            "Existing Desktop identity changed during native integrity verification".into(),
        );
    }
    record(
        d,
        &format!("desktop-existing-release-verification-{}", util::id()),
        &json!({"schemaVersion":1,"kind":"verified-existing-desktop-release","readOnly":true,"expectedReleaseSha":t["expectedReleaseSha"],"expectedReleaseTag":t["expectedReleaseTag"],"nativeArtifactIntegrityVerified":true,"runningAlignment":alignment,"before":before,"after":after,"updaterInvoked":false}),
    )?;
    Ok(after)
}
pub fn identity(s: &Value, eligible: Option<bool>) -> DesktopIdentity {
    let installed = text(&s["installedArtifact"]["commit"]).to_owned();
    let running = &s["runningArtifact"];
    let running_sha = running["commit"]
        .as_str()
        .filter(|c| {
            !c.is_empty()
                && (*c != installed
                    || (running["bundleSwapPending"] == false
                        && running["bundleOutOfSync"] == false))
        })
        .map(str::to_owned);
    DesktopIdentity {
        installed_sha: installed,
        running_sha,
        running: !rows(&s["processes"]).is_empty(),
        verified: s["nativeHomeVerified"] == true
            && s["checkoutDirty"] == false
            && s["checkoutSha"] == s["checkoutStamp"]["commit"]
            && s["checkoutSha"] == s["installedArtifact"]["commit"]
            && s["installedArtifact"]["dirty"] == false
            && s["checkoutStamp"]["dirty"] == false
            && s["installedArtifact"]["source"] != "fallback"
            && running["bundleSwapPending"] != true
            && running["bundleOutOfSync"] != true,
        pre_update_eligible: eligible,
        receipt_ref: None,
    }
}
pub fn recovery_request(values: &[Value], t: &Value, checkpoints: &[Value]) -> Result<Value> {
    if values.len() != 1 {
        return Ok(
            json!({"request":null,"correlationId":null,"reasons":["recovery-requires-exactly-one-native-request"]}),
        );
    }
    let v = &values[0];
    let scope = &v["scope"];
    let selected = selection(t)?;
    let mut reasons = vec![];
    if v["schemaVersion"] != 1
        || !util::run_id(text(&v["operationId"]))
        || v["targetId"] != t["id"]
        || !["closed-script", "open-ipc"].contains(&text(&v["mode"]))
        || !util::sha(text(&v["preSha"]))
        || !util::seconds(text(&v["startedAtUtc"])).is_finite()
    {
        reasons.push("invalid-recovery-request".into())
    }
    if util::windows_path(text(&v["installRootWindows"]))
        != util::windows_path(text(&t["installRootWindows"]))
        || util::windows_path(text(&v["nativeHomeWindows"]))
            != util::windows_path(text(&t["nativeHomeWindows"]))
        || [
            "branch",
            "channel",
            "expectedReleaseSha",
            "expectedReleaseTag",
            "allowedPreUpdateDiff",
        ]
        .iter()
        .any(|k| scope[k] != selected[k])
        || scope["noGateway"] != true
        || text(&v["authorization"]).is_empty()
    {
        reasons.push("recovery-scope-mismatch".into())
    }
    reasons.extend(validate_fleet(
        &scope["nativeFleetPlan"],
        t,
        text(&v["preSha"]),
    ));
    let mut correlation = v["correlationId"].clone();
    if v["mode"] == "closed-script" && correlation != v["operationId"] {
        reasons.push("closed-native-correlation-mismatch".into())
    }
    if v["mode"] == "open-ipc" {
        let matches: Vec<_> = checkpoints
            .iter()
            .filter(|x| x["operationId"] == v["operationId"] && x["targetId"] == v["targetId"])
            .collect();
        if matches.len() != 1
            || matches[0]["nativeHandoffAcknowledged"] != true
            || !util::run_id(text(&matches[0]["correlationId"]))
        {
            reasons.push("interrupted-open-handoff-correlation-unresolved".into())
        } else {
            correlation = matches[0]["correlationId"].clone()
        }
    }
    if !util::run_id(text(&correlation)) {
        reasons.push("native-correlation-missing".into())
    }
    Ok(
        json!({"request":if reasons.is_empty(){v.clone()}else{Value::Null},"correlationId":if reasons.is_empty(){correlation}else{Value::Null},"reasons":reasons}),
    )
}
pub fn verify_rejected_proof(
    p: &Value,
    request: &Value,
    prior: &Value,
    current: &Value,
) -> Vec<String> {
    let binding = &p["binding"];
    let array = &binding["arraySplat"];
    let named = &binding["namedSplat"];
    let state = &p["currentNativeState"];
    let native = &state["nativeResult"];
    let log = &state["handoffLog"];
    let mut reasons = vec![];
    if p["schema"] != "vps-operations.hermes-native-script-binding-check.v1"
        || p["readOnly"] != true
        || p["correlationId"] != request["correlationId"]
        || p["nativeRequestStartedAt"] != request["startedAtUtc"]
        || request["mode"] != "closed-script"
        || request["correlationId"] != request["operationId"]
        || request["scope"]["channel"] != "stable"
    {
        reasons.push("rejected-launch-proof-scope-mismatch".into())
    }
    if p["originalNativeScriptSha256"] != current["nativeScript"]["sha256"]
        || prior["before"]["nativeScript"]["sha256"] != current["nativeScript"]["sha256"]
        || current["checkoutSha"] != request["preSha"]
        || prior["before"]["checkoutSha"] != request["preSha"]
        || prior["before"]["installedArtifact"]["sha256"] != current["installedArtifact"]["sha256"]
        || prior["before"]["checkoutStamp"]["sha256"] != current["checkoutStamp"]["sha256"]
    {
        reasons.push("rejected-launch-install-identity-changed".into())
    }
    if array["bodyReached"] != false
        || array["error"]["type"]
            != "System.Management.Automation.ParameterBindingValidationException"
        || array["error"]["id"] != "ParameterArgumentValidationError"
        || named["bodyReached"] != true
        || !named["error"].is_null()
        || binding["originalNativeScriptInvoked"] != false
        || p["verification"]["passed"] != true
        || util::windows_path(text(&named["value"]["installRoot"]))
            != util::windows_path(text(&request["installRootWindows"]))
        || named["value"]["channel"] != "stable"
        || named["value"]["noUi"] != true
        || named["value"]["noGateway"] != true
    {
        reasons.push("native-before-body-rejection-unproven".into())
    }
    if prior["operationId"] != request["operationId"]
        || prior["status"] != "failed"
        || prior["error"]["code"] != "native-coordinator-failed"
        || !prior["receipt"].is_null()
        || !prior["after"].is_null()
        || prior["executionBoundary"]
            != "Native updater invoked; verification is separately recorded"
    {
        reasons.push("original-native-launch-outcome-unproven".into())
    }
    let started = util::seconds(text(&request["startedAtUtc"]));
    let result_time = util::seconds(text(&native["modifiedAt"]));
    let log_time = util::seconds(text(&log["modifiedAt"]));
    if state["marker"]["present"] != false
        || state["correlatedReceiptCount"] != 0
        || native["matchesCorrelation"] != false
        || !result_time.is_finite()
        || !log_time.is_finite()
        || result_time >= started
        || log_time >= started
        || current["marker"]["present"] == true
    {
        reasons.push("native-no-effect-state-unproven".into())
    }
    reasons
}
#[derive(Debug)]
struct DesktopFailure {
    code: String,
    message: String,
    recovery: String,
}
impl std::fmt::Display for DesktopFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for DesktopFailure {}
fn failure(
    code: &str,
    message: impl Into<String>,
    recovery: &str,
) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(DesktopFailure {
        code: code.into(),
        message: message.into(),
        recovery: recovery.into(),
    })
}
fn result_base() -> Value {
    json!({"schemaVersion":1,"operationId":util::id(),"startedAtUtc":util::now(),"completedAtUtc":"","status":"blocked","canonicalSha":null,"before":null,"after":null,"receipt":null,"verification":null,"nativeFleetPlan":null,"error":null,"executionBoundary":"Native mutation not executed"})
}
fn finish_error(result: &mut Value, e: &(dyn std::error::Error + Send + Sync + 'static)) {
    let details = e.downcast_ref::<DesktopFailure>();
    result["status"] = json!(
        if result["executionBoundary"] == "Native mutation not executed" {
            "blocked"
        } else {
            "failed"
        }
    );
    result["error"] = json!({"code":details.map(|d|d.code.as_str()).unwrap_or("desktop-adapter-failed"),"message":e.to_string(),"recovery":details.map(|d|d.recovery.as_str()).unwrap_or("Inspect the saved bounded observation and native outcome; do not force-kill, overwrite locks, or run concurrent updaters.")});
}
pub fn update(
    r: &mut dyn Runner,
    t: &Value,
    d: &Path,
    authorization: &str,
    cdp: Option<&str>,
    retry: Option<&Value>,
) -> Result<Value> {
    let mut result = result_base();
    let attempt = util::id();
    let work = (|| -> Result<()> {
        if authorization.trim().is_empty() {
            return Err(failure(
                "authorization-missing",
                "A current explicit authorization reference is required",
                "Supply the authorized coordinated Desktop/backend operation reference.",
            ));
        }
        fs::create_dir_all(d)?;
        let requests: Vec<_> = util::names(d)?
            .into_iter()
            .filter(|n| util::matches(r"^desktop-native-request-[A-Za-z0-9-]+\.json$", n))
            .collect();
        let mut original = Value::Null;
        let mut proof = Value::Null;
        let mut prior = Value::Null;
        if let Some(retry) = retry {
            if !util::digest(text(&retry["proofSha256"]))
                || !util::run_id(text(&retry["operationId"]))
                || requests.len() != 1
            {
                return Err(failure(
                    "desktop-rejected-launch-retry-unproven",
                    "An exact existing native request and hashed rejection proof are required",
                    "Reconcile the original native request and its before-body rejection evidence.",
                ));
            }
            let selected = recovery_request(&[util::json(d.join(&requests[0]))?], t, &[])?;
            if selected["request"].is_null()
                || selected["request"]["operationId"] != retry["operationId"]
            {
                return Err(failure(
                    "desktop-rejected-launch-retry-unproven",
                    selected["reasons"].to_string(),
                    "Retain the exact original native scope and correlation.",
                ));
            }
            original = selected["request"].clone();
            result["operationId"] = original["operationId"].clone();
            result["startedAtUtc"] = original["startedAtUtc"].clone();
            let b = util::bounded(Path::new(text(&retry["proofRef"])), 2 * 1024 * 1024)?;
            if util::hash(&b) != retry["proofSha256"] {
                return Err(failure(
                    "desktop-rejected-launch-proof-changed",
                    "The pinned native rejection proof changed",
                    "Review the original proof bytes before any retry.",
                ));
            }
            proof = serde_json::from_slice(&b)?;
            let prior_b = util::bounded(
                d.join(format!(
                    "desktop-update-result-{}.json",
                    text(&original["operationId"])
                )),
                2 * 1024 * 1024,
            )?;
            if proof["failedNativeResultSha256"] != util::hash(&prior_b)
                || proof["nativeArguments"]
                    != build_command(t, text(&original["correlationId"]))?["nativeArguments"]
            {
                return Err(failure(
                    "desktop-rejected-launch-evidence-mismatch",
                    "The rejection proof is not bound to the original failed result and exact native arguments",
                    "Pin the original result bytes and argument values in the reviewed proof.",
                ));
            }
            if let Some(expected) = proof.get("originalParameterBlockSha256") {
                let script = util::bounded(
                    Path::new(text(&t["installRootWsl"]))
                        .join("scripts/desktop-update/windows.ps1"),
                    256 * 1024,
                )?;
                let script = String::from_utf8_lossy(&script);
                let re = regex::Regex::new(r"(?ms)^param\(.*?^\)")?;
                if re
                    .find(&script)
                    .is_none_or(|m| json!(util::hash(m.as_str())) != *expected)
                {
                    return Err(failure(
                        "desktop-rejected-launch-parameter-block-changed",
                        "The original native parameter block identity differs",
                        "Reconcile the exact native script parameter binder before retrying.",
                    ));
                }
            }
            prior = serde_json::from_slice(&prior_b)?;
            if d.join(format!(
                "desktop-native-rejected-launch-retry-{}.json",
                text(&original["operationId"])
            ))
            .try_exists()?
            {
                return Err(failure(
                    "desktop-rejected-launch-retry-already-requested",
                    "The original rejected launch already has a durable retry intent",
                    "Recover that existing launch; another updater will not be invoked.",
                ));
            }
        } else if !requests.is_empty() {
            return Err(failure(
                "desktop-native-recovery-required",
                "This run already has a durable native request; a fresh updater will not be invoked",
                "Call recoverDesktopUpdate for the existing operation and reconcile its current native proof.",
            ));
        }
        let mut before = inspect(r, t, d, None)?;
        result["before"] = before.clone();
        if !original.is_null() {
            let reasons = verify_rejected_proof(&proof, &original, &prior, &before);
            if !reasons.is_empty() || archive(t, text(&original["correlationId"]))?.is_some() {
                return Err(failure(
                    "desktop-rejected-launch-retry-unproven",
                    reasons.join(", "),
                    "Recover the original native outcome; no retry is admitted.",
                ));
            }
        }
        let eligibility = pre_update_eligibility(t, &before)?;
        if eligibility["eligible"] != true {
            return Err(failure(
                "native-install-preflight-failed",
                eligibility["reasons"].to_string(),
                "Reconcile the exact installation identity and any reviewed local edits before updating.",
            ));
        }
        let running = !rows(&before["processes"]).is_empty();
        if running && t.get("channel").is_some() {
            return Err(failure(
                "desktop-release-ipc-unsupported",
                "The running native Update bridge cannot bind an explicit stable release selection",
                "Close Desktop normally, then use the correlated native coordinator with -Channel stable.",
            ));
        }
        if running && cdp.is_none() {
            return Err(failure(
                "desktop-cdp-required",
                "Desktop is running; its native Update bridge requires the explicitly selected CDP endpoint",
                "Provide that app CDP endpoint, or close Desktop normally and rerun the correlated closed-app path.",
            ));
        }
        if running && before["nativeScript"]["automaticCorrelation"] != true {
            return Err(failure(
                "native-correlation-unavailable",
                "The running legacy Desktop handoff cannot bind its native receipt to this operation",
                "Close Desktop normally and use the correlated native script path, or use its existing Update button manually.",
            ));
        }
        let mut connection = if running {
            let (c, version) = connect_desktop(r, t, cdp.ok_or("CDP missing")?)?;
            before["runningArtifact"] = running_artifact(&version);
            if before["runningArtifact"]["commit"] != before["installedArtifact"]["commit"]
                || before["runningArtifact"]["bundleSwapPending"] != false
            {
                return Err(failure(
                    "running-desktop-skew",
                    "Running and installed Desktop artifacts differ or cannot be compared",
                    "Use the native relaunch path and re-inspect the running artifact.",
                ));
            }
            Some(c)
        } else {
            None
        };
        result["before"] = before.clone();
        if before["checkoutDirty"] == true {
            let v = powershell(
                r,
                t,
                &render_ps(
                    include_str!("../../protocol/desktop-stash-support.ps1"),
                    t,
                    &[],
                ),
            )?;
            if v["nativeExitCode"] != 0 {
                return Err(failure(
                    "native-update-help-failed",
                    format!(
                        "Native update --help exited {}; no native update was invoked",
                        v["nativeExitCode"]
                    ),
                    "Reconcile native help before retrying.",
                ));
            }
            if v["keepStash"] != true {
                return Err(failure(
                    "native-keep-stash-unavailable",
                    "Reviewed local edits require verified native --keep-stash support",
                    "Use an installation with native stash preservation.",
                ));
            }
        }
        if t.get("channel").is_some() {
            record(
                d,
                &format!(
                    "desktop-release-target-{}{}",
                    text(&result["operationId"]),
                    if original.is_null() {
                        String::new()
                    } else {
                        format!("-attempt-{attempt}")
                    }
                ),
                &release_target(r, t)?,
            )?
        }
        let fleet = powershell(
            r,
            t,
            &render_ps(include_str!("../../protocol/desktop-fleet.ps1"), t, &[]),
        )?;
        result["nativeFleetPlan"] = fleet.clone();
        let footprint = validate_fleet(&fleet, t, text(&before["checkoutSha"]));
        if !footprint.is_empty() {
            return Err(failure(
                "native-fleet-footprint-unapproved",
                footprint.join(", "),
                "Review the native plan and explicitly reconcile any gateway/manual service footprint. NoGateway does not eliminate all native lifecycle effects.",
            ));
        }
        if !original.is_null() && original["scope"]["nativeFleetPlan"] != fleet {
            return Err(failure(
                "desktop-rejected-launch-scope-changed",
                "The native fleet footprint differs from the rejected request",
                "Review the changed native scope before any lifecycle effect.",
            ));
        }
        let current = inspect(r, t, d, None)?;
        if !existing_unchanged(&before, &current)
            || pre_update_eligibility(t, &current)?["eligible"] != true
            || current["checkoutDirty"] != before["checkoutDirty"]
            || current["checkoutDiff"] != before["checkoutDiff"]
            || current["marker"]["present"] == true
        {
            return Err(failure(
                "desktop-preflight-changed",
                "Desktop source/artifact/process/marker identity changed during preflight",
                "Re-inspect and retry the same authorized scope.",
            ));
        }
        let mut correlation = text(&result["operationId"]).to_owned();
        let request = if original.is_null() {
            let mut scope = selection(t)?;
            scope["noGateway"] = json!(true);
            scope["nativeFleetPlan"] = fleet;
            json!({"schemaVersion":1,"operationId":result["operationId"],"targetId":t["id"],"mode":if running{"open-ipc"}else{"closed-script"},"correlationId":if running{Value::Null}else{json!(correlation)},"preSha":before["checkoutSha"],"startedAtUtc":result["startedAtUtc"],"authorization":authorization,"installRootWindows":t["installRootWindows"],"nativeHomeWindows":t["nativeHomeWindows"],"scope":scope})
        } else {
            original.clone()
        };
        if original.is_null() {
            if util::names(d)?
                .iter()
                .any(|n| util::matches(r"^desktop-native-request-[A-Za-z0-9-]+\.json$", n))
            {
                return Err(failure(
                    "desktop-native-recovery-required",
                    "A native request appeared during preflight",
                    "Recover the existing run; do not invoke concurrent updaters.",
                ));
            }
            record(
                d,
                &format!("desktop-native-request-{}", text(&result["operationId"])),
                &request,
            )?
        } else {
            record(
                d,
                &format!(
                    "desktop-native-rejected-launch-retry-{}",
                    text(&result["operationId"])
                ),
                &json!({"schemaVersion":1,"operationId":result["operationId"],"correlationId":original["correlationId"],"attemptId":attempt,"observedAtUtc":util::now(),"proofRef":retry.map(|x|x["proofRef"].clone()),"proofSha256":retry.map(|x|x["proofSha256"].clone()),"nativeScriptSha256":current["nativeScript"]["sha256"],"originalRequestPreserved":true}),
            )?
        }
        result["executionBoundary"] =
            json!("Native updater invoked; verification is separately recorded");
        let mut coordinator = None;
        let mut coordinator_code = None;
        if let Some(connection) = connection.as_mut() {
            let reply = connection
                .evaluate("(async()=>await globalThis.hermesDesktop.updates.apply())()")?;
            if reply["ok"] != true || reply["handedOff"] != true {
                return Err(failure(
                    "native-handoff-unverified",
                    "Native IPC did not acknowledge a handoff with a correlated run id",
                    "Observe the native outcome; loss of the app connection is not proof of success. Do not start a second updater or force a kill.",
                ));
            }
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut native = None;
            while Instant::now() < deadline {
                let m = marker(t)?;
                if util::run_id(text(&m["runId"])) {
                    native = Some(text(&m["runId"]).to_owned());
                    break;
                }
                let p = Path::new(text(&t["nativeHomeWsl"])).join(".hermes-update-result.json");
                if p.try_exists()? {
                    let v: Value = serde_json::from_slice(&util::bounded(p, 65536)?)?;
                    if v["started_at"].as_f64().unwrap_or(0.)
                        >= util::seconds(text(&result["startedAtUtc"])).floor()
                        && util::run_id(text(&v["run_id"]))
                    {
                        native = Some(text(&v["run_id"]).into());
                        break;
                    }
                }
                r.pause(500)
            }
            correlation = native.ok_or_else(|| {
                failure(
                    "native-handoff-unverified",
                    "Native IPC did not acknowledge a handoff with a correlated run id",
                    "Observe the same native outcome; no second updater is invoked.",
                )
            })?;
            record(
                d,
                &format!("desktop-native-handoff-{}", text(&result["operationId"])),
                &json!({"schemaVersion":1,"operationId":result["operationId"],"targetId":t["id"],"correlationId":correlation,"nativeHandoffAcknowledged":true,"observedAtUtc":util::now()}),
            )?;
        } else {
            let native = build_command(t, &correlation)?;
            let mut spec = CommandSpec::new(
                text(&native["command"]),
                rows(&native["args"])
                    .iter()
                    .map(|s| text(s).into())
                    .collect(),
            );
            spec.environment = native["environment"]
                .as_object()
                .ok_or("Invalid native environment")?
                .iter()
                .map(|(k, v)| (k.clone(), text(v).into()))
                .collect();
            coordinator = Some(r.start_native(spec)?);
        }
        let deadline = Instant::now()
            + Duration::from_millis(t["receiptTimeoutMs"].as_u64().ok_or("Invalid timeout")?);
        let mut poller = ReceiptPoller::new(FilesystemReceipts {
            target: t.clone(),
            correlation: correlation.clone(),
        });
        while Instant::now() < deadline {
            if let Some(job) = coordinator.as_mut()
                && coordinator_code.is_none()
                && let Some(output) = job.poll()?
            {
                coordinator_code = Some(output.exit_code);
                let stdout: Value = serde_json::from_str(&output.stdout)?;
                let stderr: Value = serde_json::from_str(&output.stderr)?;
                record(
                    d,
                    &format!(
                        "desktop-native-coordinator-{}-{attempt}",
                        text(&result["operationId"])
                    ),
                    &json!({"schemaVersion":1,"operationId":result["operationId"],"correlationId":correlation,"attemptId":attempt,"exitCode":output.exit_code,"completedAtUtc":util::now(),"stdout":stdout,"stderr":stderr}),
                )?
            }
            let active = marker(t)?;
            result["observedMarker"] = active.clone();
            result["receipt"] = poller
                .poll(
                    &correlation,
                    active["present"] != true
                        && (coordinator.is_none() || coordinator_code.is_some()),
                )?
                .unwrap_or(Value::Null);
            if poll_state(
                coordinator_code,
                coordinator.is_some(),
                !result["receipt"].is_null(),
                active["present"] == true,
            ) != "waiting"
            {
                break;
            }
            r.pause(1000)
        }
        if coordinator_code.is_some_and(|n| n != 0) {
            return Err(failure(
                "native-coordinator-failed",
                format!(
                    "Native Desktop coordinator exited {}",
                    coordinator_code.unwrap_or(1)
                ),
                "Observe its persisted request, current marker and any correlated partial receipt. No second updater is invoked.",
            ));
        }
        if result["receipt"].is_null()
            || marker(t)?["present"] == true
            || (coordinator.is_some() && coordinator_code.is_none())
        {
            return Err(failure(
                "native-update-observation-timeout",
                "The native updater has no settled correlated terminal proof yet",
                "Observe the same native run; this adapter never kills it or launches an automatic retry.",
            ));
        }
        result["receipt"]=archive(t,&correlation)?.ok_or_else(||failure("native-terminal-receipt-missing","The authoritative terminal archive is missing after the native coordinator settled","Recover the same native run; no cached observation can establish completion."))?;
        result["after"] = inspect(r, t, d, cdp)?;
        verify_installed(r, t)?;
        result["reviewedStash"] =
            verify_stash(r, t, &result["receipt"], text(&before["checkoutSha"]), d)?;
        result["verification"] = verify_receipt(
            &result["receipt"],
            &result["after"],
            &json!({"correlationId":correlation,"startedAtUtc":result["startedAtUtc"],"preSha":before["checkoutSha"],"expectedReleaseSha":t["expectedReleaseSha"],"allowedPreUpdateDiff":t["allowedPreUpdateDiff"],"reviewedStash":result["reviewedStash"]}),
        );
        if result["verification"]["eligibleForBackend"] != true {
            return Err(failure(
                "desktop-terminal-proof-failed",
                result["verification"]["reasons"].to_string(),
                "Resolve the native owed/identity state; no canonical backend SHA is admitted.",
            ));
        }
        result["canonicalSha"] = result["verification"]["canonicalSha"].clone();
        result["status"] = json!(if result["verification"]["runningVerified"] == true {
            "verified-running"
        } else {
            "installed-only"
        });
        Ok(())
    })();
    if let Err(e) = work {
        finish_error(&mut result, e.as_ref())
    }
    result["completedAtUtc"] = json!(util::now());
    record(
        d,
        &format!(
            "desktop-update-result-{}{}",
            text(&result["operationId"]),
            if retry.is_some() {
                format!("-attempt-{attempt}")
            } else {
                String::new()
            }
        ),
        &result,
    )?;
    Ok(result)
}
pub fn recover(r: &mut dyn Runner, t: &Value, d: &Path, cdp: Option<&str>) -> Result<Value> {
    let mut result = result_base();
    let recovery = text(&result["operationId"]).to_owned();
    let work = (|| -> Result<()> {
        let names = util::names(d)?;
        let read = |prefix: &str| -> Result<Vec<Value>> {
            names
                .iter()
                .filter(|n| util::matches(&format!(r"^{prefix}[A-Za-z0-9-]+\.json$"), n))
                .map(|n| {
                    Ok(serde_json::from_slice(&util::bounded(
                        d.join(n),
                        2 * 1024 * 1024,
                    )?)?)
                })
                .collect()
        };
        let selected = recovery_request(
            &read("desktop-native-request-")?,
            t,
            &read("desktop-native-handoff-")?,
        )?;
        let request = &selected["request"];
        if request.is_null() || selected["correlationId"].is_null() {
            return Err(failure(
                "desktop-recovery-unresolved",
                selected["reasons"].to_string(),
                "Observe/reconcile the same persisted native run.",
            ));
        }
        result["operationId"] = request["operationId"].clone();
        result["startedAtUtc"] = request["startedAtUtc"].clone();
        result["nativeFleetPlan"] = request["scope"]["nativeFleetPlan"].clone();
        let prior = d.join(format!(
            "desktop-update-result-{}.json",
            text(&request["operationId"])
        ));
        let prior: Value = if prior.try_exists()? {
            util::json(prior)?
        } else {
            Value::Null
        };
        result["after"] = inspect(r, t, d, cdp)?;
        if result["after"]["marker"]["present"] == true {
            return Err(failure(
                "native-update-still-active-or-unresolved",
                "The existing native run has not released its marker; no new updater was invoked",
                "Observe the existing native run.",
            ));
        }
        result["receipt"] = archive(t, text(&selected["correlationId"]))?.ok_or_else(|| {
            failure(
                "native-terminal-receipt-missing",
                "No terminal archive belongs to the persisted native correlation",
                "Observe the existing native run.",
            )
        })?;
        if !text(&prior["receipt"]["sha256"]).is_empty()
            && prior["receipt"]["sha256"] != result["receipt"]["sha256"]
        {
            return Err(failure(
                "native-terminal-archive-changed",
                "The previously observed terminal archive changed; reconcile its provenance explicitly",
                "Retain the original archive evidence.",
            ));
        }
        verify_installed(r, t)?;
        result["reviewedStash"] =
            verify_stash(r, t, &result["receipt"], text(&request["preSha"]), d)?;
        result["verification"] = verify_receipt(
            &result["receipt"],
            &result["after"],
            &json!({"correlationId":selected["correlationId"],"startedAtUtc":request["startedAtUtc"],"preSha":request["preSha"],"expectedReleaseSha":request["scope"]["expectedReleaseSha"],"allowedPreUpdateDiff":request["scope"]["allowedPreUpdateDiff"],"reviewedStash":result["reviewedStash"]}),
        );
        if result["verification"]["eligibleForBackend"] != true {
            return Err(failure(
                "desktop-recovery-proof-failed",
                result["verification"]["reasons"].to_string(),
                "Observe/reconcile the same persisted native run.",
            ));
        }
        result["canonicalSha"] = result["verification"]["canonicalSha"].clone();
        result["status"] = json!(if result["verification"]["runningVerified"] == true {
            "verified-running"
        } else {
            "installed-only"
        });
        Ok(())
    })();
    if let Err(e) = work {
        let details = e.downcast_ref::<DesktopFailure>();
        result["error"] = json!({"code":details.map(|d|d.code.as_str()).unwrap_or("desktop-recovery-unresolved"),"message":e.to_string(),"recovery":"Observe/reconcile the same persisted native run. Recovery never invokes another updater, guesses an interrupted open handoff, kills processes, or replaces markers."});
    }
    result["completedAtUtc"] = json!(util::now());
    record(d, &format!("desktop-update-recovery-{recovery}"), &result)?;
    Ok(result)
}

pub trait ReceiptReaders {
    fn list(&mut self) -> Result<Vec<String>>;
    fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>>;
    fn audit(&mut self) -> Result<Option<Value>>;
}
pub struct ReceiptPoller<R> {
    pub readers: R,
    cached_path: Option<String>,
}
impl<R: ReceiptReaders> ReceiptPoller<R> {
    pub fn new(readers: R) -> Self {
        Self {
            readers,
            cached_path: None,
        }
    }
    pub fn poll(&mut self, correlation: &str, full_audit: bool) -> Result<Option<Value>> {
        if full_audit {
            let terminal = self.readers.audit()?;
            self.cached_path = terminal
                .as_ref()
                .and_then(|v| v["archivePath"].as_str().map(str::to_owned));
            return Ok(terminal);
        }
        let read_matching = |readers: &mut R, path: &str| -> Result<(bool, Option<Value>)> {
            let Some(bytes) = readers.read(path)? else {
                return Ok((false, None));
            };
            let raw: Value =
                serde_json::from_slice(bytes.strip_prefix(&[239, 187, 191]).unwrap_or(&bytes))?;
            if raw["correlation_id"] != correlation {
                return Ok((false, None));
            }
            let terminal = if !text(&raw["finished_at"]).is_empty() {
                Some(normalize_receipt(
                    &raw,
                    path,
                    &util::hash(&bytes),
                    bytes.len(),
                ))
            } else {
                None
            };
            Ok((true, terminal))
        };
        if let Some(path) = &self.cached_path {
            let (matched, result) = read_matching(&mut self.readers, path)?;
            if matched {
                return Ok(result);
            }
            self.cached_path = None
        }
        for path in self.readers.list()? {
            let (matched, result) = read_matching(&mut self.readers, &path)?;
            if matched {
                self.cached_path = Some(path);
                return Ok(result);
            }
        }
        Ok(None)
    }
}
struct FilesystemReceipts {
    target: Value,
    correlation: String,
}
impl ReceiptReaders for FilesystemReceipts {
    fn list(&mut self) -> Result<Vec<String>> {
        let d = Path::new(text(&self.target["nativeHomeWsl"])).join("logs/update_receipts");
        if !d.try_exists()? {
            return Ok(vec![]);
        }
        Ok(util::names(&d)?
            .into_iter()
            .filter(|n| util::matches(r"^update_[A-Za-z0-9_-]+\.json$", n))
            .map(|n| d.join(n).to_string_lossy().into_owned())
            .collect())
    }
    fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>> {
        match util::bounded(path, 2 * 1024 * 1024) {
            Ok(b) => Ok(Some(b)),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    fn audit(&mut self) -> Result<Option<Value>> {
        archive(&self.target, &self.correlation)
    }
}
