use super::install;
use super::types::*;

/// Static inspection of explicit public instruction paths. Never execute hooks.
pub fn doctor(spec: &Spec) -> Result<DoctorReport> {
    let plan = install::plan(spec)?;
    Ok(DoctorReport {
        schema_version: 1,
        outcome: "diagnosed".into(),
        source: plan.source,
        entries: plan
            .entries
            .into_iter()
            .map(|entry| DoctorEntry {
                path: entry.path,
                selector: entry.selector,
                status: if entry.changed {
                    "needs_install"
                } else {
                    "ready"
                }
                .into(),
            })
            .collect(),
        native_loading: "unverified".into(),
        hooks_executed: 0,
        p6_complete: false,
    })
}
