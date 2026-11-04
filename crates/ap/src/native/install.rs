use super::filesystem::{self as io, Dir, Stage};
use super::types::*;
use crate::{AgyProfile, ClaudeProfile, CodexProfile, Entry, RenderRequest, Selector};

pub(super) fn validate_spec(spec: &Spec) -> Result<()> {
    if spec.schema_version != 1 {
        return Err(err("unsupported_schema", "schema_version"));
    }
    if spec.uid != rustix::process::geteuid().as_raw() {
        return Err(err("uid_mismatch", "uid"));
    }
    if (spec.scope == Scope::Agy) != spec.agy_dir.is_some() {
        return Err(err("invalid_profile", "agy_dir"));
    }
    let mut profile_dirs = vec![&spec.codex_dir, &spec.claude_dir];
    profile_dirs.extend(spec.agy_dir.iter());
    for path in std::iter::once(&spec.canonical_path).chain(profile_dirs.iter().copied()) {
        if !io::valid_path(path) {
            return Err(err("invalid_path", "profiles"));
        }
    }
    if profile_dirs.iter().enumerate().any(|(i, a)| {
        profile_dirs
            .iter()
            .skip(i + 1)
            .any(|b| a == b || io::beneath(a, b) || io::beneath(b, a))
    }) || profile_dirs
        .iter()
        .any(|p| spec.canonical_path == **p || io::beneath(&spec.canonical_path, p))
    {
        return Err(err("path_collision", "profiles"));
    }
    match &spec.environment {
        Environment::Disposable { root } => {
            if !io::valid_path(root)
                || !std::iter::once(&spec.canonical_path)
                    .chain(profile_dirs.iter().copied())
                    .all(|p| io::beneath(p, root))
            {
                return Err(err("invalid_path", "environment"));
            }
            Dir::private(root, spec.uid)?;
        }
        Environment::CoordinatedNative { evidence } => {
            if evidence.trim().is_empty()
                || evidence.len() > 4096
                || evidence.chars().any(char::is_control)
            {
                return Err(err("ownership_unestablished", "environment"));
            }
        }
    }
    Ok(())
}

pub(super) fn profiles(spec: &Spec) -> Vec<(&str, Selector)> {
    let mut paths = Vec::new();
    if spec.scope.codex() {
        paths.push((spec.codex_dir.as_str(), Selector::CodexInstructionLink));
    }
    if spec.scope.claude() {
        paths.push((spec.claude_dir.as_str(), Selector::ClaudeCanonicalImport));
    }
    if spec.scope == Scope::Agy
        && let Some(path) = &spec.agy_dir
    {
        paths.push((path.as_str(), Selector::AgyInstructionLink));
        paths.push((path.as_str(), Selector::AgyGeminiAlias));
    }
    // Preserve legacy profile ordering; only the new same-directory pair needs
    // a filename tie-breaker. Sorting full paths changes prefix-named profiles.
    paths.sort_by(|a, b| {
        a.0.as_bytes()
            .cmp(b.0.as_bytes())
            .then_with(|| filename(&a.1).as_bytes().cmp(filename(&b.1).as_bytes()))
    });
    paths
}
fn filename(selector: &Selector) -> &'static str {
    match selector {
        Selector::CodexInstructionLink => "AGENTS.md",
        Selector::ClaudeCanonicalImport => "CLAUDE.md",
        Selector::AgyInstructionLink => "AGENTS.md",
        Selector::AgyGeminiAlias => "GEMINI.md",
    }
}
pub(super) fn topology(spec: &Spec) -> Result<Vec<DirectoryIdentity>> {
    let mut paths = profiles(spec)
        .into_iter()
        .map(|(p, _)| p)
        .collect::<Vec<_>>();
    paths.push(io::split(&spec.canonical_path)?.0);
    paths.dedup();
    let mut all = Vec::new();
    for path in paths {
        all.extend(Dir::open(path, spec.uid)?.topology);
    }
    all.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    for pair in all.windows(2) {
        if pair[0].path == pair[1].path && pair[0] != pair[1] {
            return Err(err("expected_before_conflict", "topology"));
        }
    }
    all.dedup();
    let mut roots = profiles(spec)
        .into_iter()
        .map(|(p, _)| p)
        .collect::<Vec<_>>();
    roots.push(io::split(&spec.canonical_path)?.0);
    roots.dedup();
    let mut identities = std::collections::HashSet::new();
    for path in roots {
        let id = all
            .iter()
            .find(|d| d.path == path)
            .ok_or_else(|| err("unsafe_topology", "profiles"))?;
        if !identities.insert((id.dev, id.ino)) {
            return Err(err("path_collision", "profiles"));
        }
    }
    Ok(all)
}
pub(super) fn override_absent(spec: &Spec) -> Result<()> {
    if spec.scope.codex() && !Dir::open(&spec.codex_dir, spec.uid)?.absent("AGENTS.override.md")? {
        return Err(err("instruction_conflict", "codex.instruction_override"));
    }
    Ok(())
}
fn render_entries(spec: &Spec, snapshots: &[Snapshot]) -> Result<Vec<PlannedEntry>> {
    let mut codex = Entry::Missing;
    let mut claude = Entry::Missing;
    let mut agy = spec.agy_dir.as_ref().map(|dir| AgyProfile {
        config_dir: dir.clone(),
        instructions: Entry::Missing,
        gemini_alias: Entry::Missing,
    });
    let paths = profiles(spec);
    if snapshots.len() != paths.len() {
        return Err(err("invalid_plan", "entries"));
    }
    for ((_, selector), snap) in paths.iter().zip(snapshots) {
        match selector {
            Selector::CodexInstructionLink => codex = snap.entry.clone(),
            Selector::ClaudeCanonicalImport => claude = snap.entry.clone(),
            Selector::AgyInstructionLink => {
                agy.as_mut()
                    .ok_or_else(|| err("invalid_plan", "entries"))?
                    .instructions = snap.entry.clone();
            }
            Selector::AgyGeminiAlias => {
                agy.as_mut()
                    .ok_or_else(|| err("invalid_plan", "entries"))?
                    .gemini_alias = snap.entry.clone();
            }
        }
    }
    let request = RenderRequest {
        schema_version: 1,
        canonical_path: spec.canonical_path.clone(),
        codex: CodexProfile {
            config_dir: spec.codex_dir.clone(),
            instructions: codex,
            instruction_override: Entry::Missing,
        },
        claude: ClaudeProfile {
            config_dir: spec.claude_dir.clone(),
            instructions: claude,
        },
        preserved: vec![],
        agy,
    };
    let rendered = crate::render(&request).map_err(|e| {
        let code = serde_json::to_value(e.code)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "invalid_plan".into());
        err(&code, &e.field)
    })?;
    let mut entries = Vec::new();
    for ((dir, selector), before) in paths.into_iter().zip(snapshots) {
        let path = format!("{dir}/{}", filename(&selector));
        let artifact = rendered
            .artifacts
            .iter()
            .find(|a| a.path == path)
            .ok_or_else(|| err("invalid_plan", "entries"))?;
        entries.push(PlannedEntry {
            path,
            selector,
            before: before.clone(),
            after: artifact.after.clone(),
            changed: artifact.changed,
        });
    }
    Ok(entries)
}

/// Read-only, selected public instruction snapshots. Does not inspect settings.
pub fn plan(spec: &Spec) -> Result<InstallPlan> {
    validate_spec(spec)?;
    let topology = topology(spec)?;
    let source = io::source(spec)?;
    override_absent(spec)?;
    let mut snapshots = Vec::new();
    for (path, selector) in profiles(spec) {
        let dir = Dir::open(path, spec.uid)?;
        let name = filename(&selector);
        let snap = dir.snapshot(name, spec.uid)?;
        match selector {
            Selector::CodexInstructionLink => {
                io::direct_link(&dir, name, spec, &snap, "codex.instructions")?
            }
            Selector::AgyInstructionLink => {
                io::direct_link(&dir, name, spec, &snap, "agy.instructions")?
            }
            Selector::AgyGeminiAlias => {
                io::direct_link(&dir, name, spec, &snap, "agy.gemini_alias")?
            }
            Selector::ClaudeCanonicalImport => {
                if matches!(snap.entry, Entry::Symlink { .. }) {
                    return Err(err("unsupported_target", "claude.instructions"));
                }
            }
        }
        snapshots.push(snap);
    }
    if snapshots.iter().any(|s| {
        s.stamp.as_ref().is_some_and(|m| {
            matches!(s.entry, Entry::File { .. })
                && (m.dev, m.ino) == (source.stamp.dev, source.stamp.ino)
        })
    }) {
        return Err(err("path_collision", "canonical_path"));
    }
    let entries = render_entries(spec, &snapshots)?;
    let result = InstallPlan {
        schema_version: 1,
        spec: spec.clone(),
        source,
        topology,
        entries,
    };
    dependencies(&result)?;
    Ok(result)
}

pub(super) fn validate_plan(plan: &InstallPlan) -> Result<()> {
    validate_spec(&plan.spec)?;
    if plan.schema_version != 1 {
        return Err(err("invalid_plan", "schema_version"));
    }
    let snapshots: Vec<_> = plan.entries.iter().map(|e| e.before.clone()).collect();
    for snap in &snapshots {
        if matches!(snap.entry, Entry::File { .. })
            && snap
                .stamp
                .as_ref()
                .is_some_and(|m| (m.dev, m.ino) == (plan.source.stamp.dev, plan.source.stamp.ino))
        {
            return Err(err("invalid_plan", "snapshot"));
        }
        let expected = match &snap.entry {
            Entry::Missing => {
                if snap.stamp.is_some() || snap.sha256.is_some() {
                    return Err(err("invalid_plan", "snapshot"));
                }
                continue;
            }
            Entry::File { text } => io::hash(text.as_bytes()),
            Entry::Symlink { target } => io::hash(target.as_bytes()),
            _ => return Err(err("invalid_plan", "snapshot")),
        };
        if snap.sha256.as_ref() != Some(&expected) || snap.stamp.is_none() {
            return Err(err("invalid_plan", "snapshot"));
        }
    }
    let entries =
        render_entries(&plan.spec, &snapshots).map_err(|_| err("invalid_plan", "entries"))?;
    if entries != plan.entries {
        return Err(err("invalid_plan", "entries"));
    }
    Ok(())
}

pub(super) fn dependencies(plan: &InstallPlan) -> Result<()> {
    if topology(&plan.spec)? != plan.topology || io::source(&plan.spec)? != plan.source {
        return Err(err("expected_before_conflict", "dependencies"));
    }
    override_absent(&plan.spec)
}

pub(super) fn command_dir(path: &str, spec: &Spec) -> Result<Dir> {
    if !io::valid_path(path) {
        return Err(err("invalid_path", "command_dir"));
    }
    for protected in [&spec.codex_dir, &spec.claude_dir, &spec.canonical_path]
        .into_iter()
        .chain(spec.agy_dir.iter())
    {
        if path == protected || io::beneath(path, protected) || io::beneath(protected, path) {
            return Err(err("path_collision", "command_dir"));
        }
    }
    if let Environment::Disposable { root } = &spec.environment
        && !io::beneath(path, root)
    {
        return Err(err("invalid_path", "command_dir"));
    }
    let command = Dir::private(path, spec.uid)?;
    let current = command
        .topology
        .last()
        .ok_or_else(|| err("unsafe_topology", "command_dir"))?;
    if topology(spec)?
        .iter()
        .any(|d| (d.dev, d.ino) == (current.dev, current.ino))
    {
        return Err(err("path_collision", "command_dir"));
    }
    Ok(command)
}
pub(super) fn locks(spec: &Spec) -> Result<Vec<io::Lock>> {
    let mut held = Vec::new();
    let mut paths: Vec<_> = profiles(spec).into_iter().map(|(path, _)| path).collect();
    paths.sort_unstable();
    paths.dedup();
    for path in paths {
        let dir = Dir::open(path, spec.uid)?;
        held.push(dir.lock(spec.uid)?);
        dir.recheck(spec.uid)?;
    }
    Ok(held)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Point {
    BeforeRecheck,
    AfterReplace,
    AfterPersist,
    RecoverReplace,
}
pub(super) trait Observer {
    fn event(&mut self, _point: Point, _index: usize) -> Result<()> {
        Ok(())
    }
}
pub(super) struct Quiet;
impl Observer for Quiet {}

pub fn apply(request: &ApplyRequest) -> Result<InstallReport> {
    apply_observed(request, &mut Quiet)
}

pub(super) fn apply_observed(
    request: &ApplyRequest,
    observer: &mut impl Observer,
) -> Result<InstallReport> {
    if !request.writers_quiesced {
        return Err(err("ownership_unestablished", "writers_quiesced"));
    }
    let p = &request.plan;
    validate_plan(p)?;
    let dir = command_dir(&request.command_dir, &p.spec)?;
    let _command_lock = dir.command_lock(p.spec.uid)?;
    if !dir.absent("state.json")? {
        return Err(err("state_exists", "command_dir"));
    }
    let _locks = locks(&p.spec)?;
    if !dir.absent("state.json")? {
        return Err(err("state_exists", "command_dir"));
    }
    if !dir.empty()? {
        return Err(err("command_dir_not_empty", "command_dir"));
    }
    dependencies(p)?;
    let count = p.entries.len();
    let mut state = State {
        schema_version: 1,
        plan: p.clone(),
        command_topology: dir.topology.clone(),
        phases: vec![Phase::Pending; count],
        written: vec![None; count],
        recovery_before: vec![None; count],
        recovery_after: vec![None; count],
    };
    dir.save_state(&state)?;
    for index in 0..count {
        let result = apply_entry(&mut state, index, &dir, observer);
        if let Err(error) = result {
            // Prepared means rename MAY have happened. Preserve it for reconciliation.
            if state.phases[index] == Phase::Pending {
                state.phases[index] = Phase::Conflicted;
            }
            let persistence = dir.save_state(&state);
            let outcome = if state.phases[index] == Phase::Prepared || persistence.is_err() {
                "recovery_required"
            } else {
                "partial"
            };
            return Ok(state.report(outcome, Some(error)));
        }
    }
    Ok(state.report("installed", None))
}
fn apply_entry(
    state: &mut State,
    index: usize,
    command: &Dir,
    observer: &mut impl Observer,
) -> Result<()> {
    let p = &state.plan;
    let entry = &p.entries[index];
    let (parent, name) = io::split(&entry.path)?;
    let dir = Dir::open(parent, p.spec.uid)?;
    observer.event(Point::BeforeRecheck, index)?;
    dependencies(p)?;
    if dir.snapshot(name, p.spec.uid)? != entry.before {
        return Err(err("expected_before_conflict", "target"));
    }
    if !entry.changed {
        state.phases[index] = Phase::Unchanged;
        command.save_state(state)?;
        observer.event(Point::AfterPersist, index)?;
        return Ok(());
    }
    let stage = Stage::create(&dir, index, &entry.after, &entry.before, p.spec.uid)?;
    state.written[index] = Some(stage.snapshot.clone());
    state.phases[index] = Phase::Prepared;
    command.save_state(state)?;
    dependencies(p)?;
    dir.recheck(p.spec.uid)?;
    command.recheck(p.spec.uid)?;
    if dir.snapshot(name, p.spec.uid)? != entry.before {
        return Err(err("expected_before_conflict", "target"));
    }
    stage.replace(name, p.spec.uid)?;
    observer.event(Point::AfterReplace, index)?;
    let actual = dir.snapshot(name, p.spec.uid)?;
    if actual.entry != entry.after {
        return Err(err("readback_conflict", "target"));
    }
    state.phases[index] = Phase::Applied;
    command.save_state(state)?;
    observer.event(Point::AfterPersist, index)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
