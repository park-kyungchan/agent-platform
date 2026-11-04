use super::filesystem::{self as io, Dir, Stage};
use super::install::{self, Observer, Point, Quiet};
use super::types::*;
use crate::{Entry, Selector};

/// Remove only this install's added selector. Exact-original restoration applies
/// only when CURRENT text is still exactly what this command wrote.
fn rollback(entry: &PlannedEntry, current: &Snapshot, canonical: &str) -> Result<Entry> {
    if current.entry == entry.before.entry {
        return Ok(current.entry.clone());
    }
    match entry.selector {
        Selector::CodexInstructionLink
        | Selector::AgyInstructionLink
        | Selector::AgyGeminiAlias => {
            if current.entry == Entry::Missing {
                return Ok(Entry::Missing);
            }
            if current.entry != entry.after {
                return Err(err("owned_field_conflict", "target"));
            }
            Ok(entry.before.entry.clone())
        }
        Selector::ClaudeCanonicalImport => {
            if current.entry == entry.after {
                return Ok(entry.before.entry.clone());
            }
            let Entry::File { text } = &current.entry else {
                return if current.entry == Entry::Missing {
                    Ok(Entry::Missing)
                } else {
                    Err(err("owned_field_conflict", "target"))
                };
            };
            let import = format!("@{canonical}");
            // Independently qualify CURRENT bytes through the unchanged renderer.
            // It rejects duplicate imports, fences and ambiguous canonical references.
            let mut request = crate::RenderRequest {
                schema_version: 1,
                canonical_path: canonical.into(),
                codex: crate::CodexProfile {
                    config_dir: "/ap-recovery-codex".into(),
                    instructions: Entry::Missing,
                    instruction_override: Entry::Missing,
                },
                claude: crate::ClaudeProfile {
                    config_dir: "/ap-recovery-claude".into(),
                    instructions: current.entry.clone(),
                },
                preserved: vec![],
                agy: None,
            };
            // Avoid synthetic path collision for all admitted canonical paths.
            if canonical.starts_with("/ap-recovery-") {
                request.codex.config_dir = "/ap-recovery2-codex".into();
                request.claude.config_dir = "/ap-recovery2-claude".into();
            }
            crate::render(&request).map_err(|_| err("owned_field_conflict", "target"))?;
            let mut found = false;
            let mut remaining = String::new();
            for line in text.split_inclusive('\n') {
                let body = line.strip_suffix('\n').unwrap_or(line);
                let body = body.strip_suffix('\r').unwrap_or(body);
                if body == import {
                    found = true;
                } else {
                    remaining.push_str(line);
                }
            }
            if !found {
                // A different active import is a changed owned value, not proof
                // that our selector was intentionally removed.
                if text.lines().any(|line| line.trim_start().starts_with('@')) {
                    return Err(err("owned_field_conflict", "target"));
                }
                return Ok(current.entry.clone());
            }
            if remaining.is_empty() && entry.before.entry == Entry::Missing {
                Ok(Entry::Missing)
            } else {
                Ok(Entry::File { text: remaining })
            }
        }
    }
}

fn validate_state(state: &State) -> Result<()> {
    install::validate_plan(&state.plan)?;
    let count = state.plan.entries.len();
    if state.schema_version != 1
        || state.phases.len() != count
        || state.written.len() != count
        || state.recovery_before.len() != count
        || state.recovery_after.len() != count
    {
        return Err(err("invalid_state", "state"));
    }
    for index in 0..count {
        let entry = &state.plan.entries[index];
        if let Some(written) = &state.written[index] {
            if written.entry != entry.after || written.stamp.is_none() {
                return Err(err("invalid_state", "written"));
            }
        } else if matches!(
            state.phases[index],
            Phase::Prepared | Phase::Applied | Phase::Recovering
        ) {
            return Err(err("invalid_state", "written"));
        }
        if !entry.changed
            && !matches!(
                state.phases[index],
                Phase::Pending | Phase::Unchanged | Phase::Conflicted | Phase::Recovered
            )
        {
            return Err(err("invalid_state", "phase"));
        }
        match (&state.recovery_before[index], &state.recovery_after[index]) {
            (None, None) => {
                if state.phases[index] == Phase::Recovering {
                    return Err(err("invalid_state", "recovery"));
                }
            }
            (Some(before), Some(after)) => {
                if rollback(entry, before, &state.plan.spec.canonical_path)? != *after {
                    return Err(err("invalid_state", "recovery"));
                }
            }
            _ => return Err(err("invalid_state", "recovery")),
        }
    }
    Ok(())
}

pub fn recover(request: &RecoverRequest) -> Result<InstallReport> {
    recover_observed(request, &mut Quiet)
}
pub(super) fn recover_observed(
    request: &RecoverRequest,
    observer: &mut impl Observer,
) -> Result<InstallReport> {
    if !request.writers_quiesced {
        return Err(err("ownership_unestablished", "writers_quiesced"));
    }
    let uid = rustix::process::geteuid().as_raw();
    let initial = Dir::private(&request.command_dir, uid)?;
    let _command_lock = initial.command_lock(uid)?;
    let mut state = initial.read_state(uid)?;
    validate_state(&state)?;
    let command = install::command_dir(&request.command_dir, &state.plan.spec)?;
    if command.topology != state.command_topology {
        return Err(err("expected_before_conflict", "command_dir"));
    }
    let locked_state = state.clone();
    let _locks = install::locks(&state.plan.spec)?;
    // State may have changed while acquiring profile locks.
    state = command.read_state(uid)?;
    if state != locked_state {
        return Err(err("expected_before_conflict", "state"));
    }
    validate_state(&state)?;
    if command.topology != state.command_topology {
        return Err(err("expected_before_conflict", "command_dir"));
    }
    install::dependencies(&state.plan)?;
    let mut last_error = None;
    for index in (0..state.plan.entries.len()).rev() {
        if let Err(error) = recover_entry(&mut state, index, &command, observer) {
            // Keep recovery intent if replacement could have occurred.
            if !matches!(state.phases[index], Phase::Recovering | Phase::Prepared) {
                state.phases[index] = Phase::Conflicted;
            }
            if let Err(persistence) = command.save_state(&state) {
                return Ok(state.report("recovery_required", Some(persistence)));
            }
            last_error = Some(error);
        }
    }
    Ok(state.report(
        if last_error.is_none() {
            "recovered"
        } else {
            "recovery_required"
        },
        last_error,
    ))
}
fn recover_entry(
    state: &mut State,
    index: usize,
    command: &Dir,
    observer: &mut impl Observer,
) -> Result<()> {
    let entry = &state.plan.entries[index];
    let uid = state.plan.spec.uid;
    if state.phases[index] == Phase::Recovered {
        return Ok(());
    }
    if !entry.changed {
        // No authority to alter preexisting selectors, even after another edit.
        state.phases[index] = Phase::Unchanged;
        return command.save_state(state);
    }
    if state.written[index].is_none() {
        // This entry never reached durable write intent; no rollback authority.
        state.phases[index] = Phase::Recovered;
        return command.save_state(state);
    }
    let (parent, name) = io::split(&entry.path)?;
    let dir = Dir::open(parent, uid)?;
    let current = dir.snapshot(name, uid)?;
    if state.phases[index] == Phase::Prepared && current.entry != entry.before.entry {
        let written = state.written[index]
            .as_ref()
            .ok_or_else(|| err("invalid_state", "written"))?;
        let same_inode = match (&current.stamp, &written.stamp) {
            (Some(a), Some(b)) => {
                (a.dev, a.ino, a.uid, a.gid, a.mode) == (b.dev, b.ino, b.uid, b.gid, b.mode)
            }
            _ => false,
        };
        if !same_inode || current.entry != written.entry {
            return Err(err("uncertain_owned_fields", "target"));
        }
    }
    let desired = if state.phases[index] == Phase::Recovering {
        let old = state.recovery_before[index]
            .as_ref()
            .ok_or_else(|| err("invalid_state", "recovery"))?;
        let after = state.recovery_after[index]
            .as_ref()
            .ok_or_else(|| err("invalid_state", "recovery"))?;
        if current.entry == *after {
            state.phases[index] = Phase::Recovered;
            return command.save_state(state);
        }
        if current.entry == old.entry {
            after.clone()
        } else {
            rollback(entry, &current, &state.plan.spec.canonical_path)?
        }
    } else {
        rollback(entry, &current, &state.plan.spec.canonical_path)?
    };
    if current.entry == desired {
        state.phases[index] = Phase::Recovered;
        return command.save_state(state);
    }
    let stage = Stage::create(&dir, index, &desired, &current, uid)?;
    state.recovery_before[index] = Some(current.clone());
    state.recovery_after[index] = Some(desired.clone());
    state.phases[index] = Phase::Recovering;
    command.save_state(state)?;
    install::dependencies(&state.plan)?;
    command.recheck(uid)?;
    dir.recheck(uid)?;
    if dir.snapshot(name, uid)? != current {
        return Err(err("expected_before_conflict", "target"));
    }
    stage.replace(name, uid)?;
    observer.event(Point::RecoverReplace, index)?;
    if dir.snapshot(name, uid)?.entry != desired {
        return Err(err("readback_conflict", "target"));
    }
    state.phases[index] = Phase::Recovered;
    command.save_state(state)
}
