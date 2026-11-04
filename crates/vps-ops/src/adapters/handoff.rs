use super::ssh::HermesTarget;
use crate::{Result, domain::rows};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub fn plan(t: &HermesTarget, p: &Value, c: &Value, s: &Value) -> Result<Option<Value>> {
    let cs = rows(&s["containers"]);
    let services = rows(&p["services"]);
    let count = services.len();
    if ![1, 2].contains(&count)
        || services
            .iter()
            .map(Value::to_string)
            .collect::<BTreeSet<_>>()
            .len()
            != count
        || !services.contains(&json!(t.gateway_service))
    {
        return Err("Invalid declared Hermes service topology".into());
    }
    let overlay = p["overlay"].get("sha256").cloned().unwrap_or(Value::Null);
    if s["hostId"] != t.host_id
        || s["project"] != t.project
        || s["helperSha256"] != p["helperSha256"]
        || cs.len() != count
        || cs
            .iter()
            .map(|x| x["service"].to_string())
            .collect::<BTreeSet<_>>()
            .len()
            != count
        || cs.iter().any(|x| !services.contains(&x["service"]))
        || cs
            .iter()
            .map(|x| x["dataSource"].to_string())
            .collect::<BTreeSet<_>>()
            .len()
            != 1
        || cs
            .iter()
            .any(|x| x["namespace"].as_str().unwrap_or("").is_empty())
        || cs
            .iter()
            .map(|x| x["namespace"].to_string())
            .collect::<BTreeSet<_>>()
            .len()
            != count
        || cs.iter().any(|x| {
            x["running"] != true
                || x["health"] != "healthy"
                || x["imageId"] != c["image_id"]
                || x["revision"] != c["sha"]
                || x["overlaySha256"] != overlay
                || x["stamp"]["commit"] != c["sha"]
                || x["provenance"]["revision"] != c["sha"]
                || x["stamp"]["overlaySha256"] != overlay
        })
    {
        return Err(
            "Gateway handoff requires exactly the declared reviewed deployed services".into(),
        );
    }
    let o = &s["gatewayOwner"];
    let hs = rows(&s["gatewayLock"]["holders"]);
    if o.is_null()
        || s["gatewayLock"]["held"] != true
        || hs.len() != 1
        || hs[0] != *o
        || o["uid"] != t.native_uid
        || o["matchingFdCount"].as_u64().unwrap_or(0) < 1
    {
        return Err("Gateway handoff requires one exact native lock owner".into());
    }
    let intended = cs
        .iter()
        .find(|x| x["service"] == t.gateway_service)
        .ok_or("Intended service missing")?;
    let from = cs
        .iter()
        .find(|x| x["containerId"] == o["containerId"])
        .ok_or("Owner service missing")?;
    if from["namespace"] != o["namespace"]
        || from["rootSlot"]["localPid"] != o["localPid"]
        || from["rootSlot"]["up"] != true
        || from["rootSlot"]["identity"] != *o
    {
        return Err("Gateway handoff owner incarnation is not the native service slot".into());
    }
    if from["containerId"] == intended["containerId"] {
        return Ok(None);
    }
    let identity = &intended["rootSlot"]["identity"];
    if intended["rootSlot"]["up"] == true
        && (identity.is_null()
            || identity["matchingFdCount"] != 0
            || identity["namespace"] != intended["namespace"])
    {
        return Err("Intended service non-owner slot identity is unverified".into());
    }
    Ok(Some(
        json!({"from":from,"intended":intended,"owner":o,"lock":{"device":s["gatewayLock"]["device"],"inode":s["gatewayLock"]["inode"]},"imageId":c["image_id"],"sourceSha":c["sha"],"overlaySha256":overlay}),
    ))
}
