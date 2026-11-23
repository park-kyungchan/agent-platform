//! The roles file beside a scratch store of the real engine (`specs/ap/roles.md` R5; engine action.md R113): the
//! uid that owns the store's directory may act as every role of the schema's grants, in the form the engine reads,
//! mode 0644. Each test that installs the schema into a real engine puts it beside its store.
#![allow(
    dead_code,
    reason = "each test that includes this module uses part of it"
)]
use serde_json::{Value, json};
use std::{os::unix::fs::MetadataExt, os::unix::fs::PermissionsExt, path::Path};

/// Writes `<dir>/roles.json`, the file the engine reads for the store `<dir>/<name>.sqlite`.
pub fn put(dir: &Path, schema: &Value) {
    let uid = std::fs::metadata(dir).unwrap().uid();
    let mut roles: Vec<&str> = schema["grants"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|g| g["role"].as_str())
        .collect();
    roles.sort_unstable();
    let file = dir.join("roles.json");
    let body = json!({"principals": {format!("uid:{uid}"): roles}});
    std::fs::write(&file, format!("{body}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
}

/// The role a call of `ont` claims by `ONT_ROLE` (a function call takes its role from there alone, engine fn.md
/// R21): `route` needs none; any other function is called as `lead`. Applies carry `role` in their bodies.
pub fn fn_role(args: &[&str]) -> Option<(&'static str, &'static str)> {
    match args {
        ["fn", "route"] => None,
        ["fn", _] => Some(("ONT_ROLE", "lead")),
        _ => None,
    }
}
