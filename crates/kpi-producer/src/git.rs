//! Exact metadata-only Git membership. No checkout, fetch or mutable ref write.
use crate::{
    Error, Result, digest, hex,
    model::*,
    process::{Call, Runner},
    strict,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryRequest {
    pub manifest_key: String,
    pub github_id: u64,
    pub full_name: String,
    pub local_root: K<String>,
    pub backend: String,
}
impl RepositoryRequest {
    pub fn identity(&self) -> Identity {
        Identity {
            github_id: self.github_id,
            full_name: self.full_name.clone(),
        }
    }
    pub fn valid(&self) -> bool {
        crate::text(&self.manifest_key)
            && self.identity().valid()
            && matches!(self.backend.as_str(), "api_only" | "local_then_api")
            && self
                .local_root
                .get()
                .is_none_or(|p| p.starts_with('/') && !p.split('/').any(|s| s == ".."))
    }
}
pub fn population(manifest: &[u8]) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let v = strict::value(manifest, 8 * 1024 * 1024)?;
    let repos = v
        .get("repos")
        .and_then(Value::as_object)
        .ok_or(Error("invalid_manifest"))?;
    let mut out = BTreeMap::new();
    for (key, repo) in repos {
        if !crate::text(key) {
            return Err(Error("invalid_manifest"));
        }
        let groups = repo
            .get("groups")
            .and_then(Value::as_array)
            .ok_or(Error("invalid_manifest"))?;
        let mut paths = BTreeSet::new();
        for group in groups {
            let disposition = group
                .get("disposition")
                .and_then(Value::as_str)
                .ok_or(Error("invalid_manifest"))?;
            if disposition == "KEEP" {
                continue;
            }
            if !matches!(
                disposition,
                "DELETE_NOW" | "PORT_TO_RUST_THEN_DELETE" | "DELETE_AFTER_CUTOVER"
            ) {
                return Err(Error("invalid_disposition"));
            }
            match group.get("files") {
                Some(Value::Array(files)) => {
                    for path in files {
                        let p = path.as_str().ok_or(Error("invalid_manifest"))?;
                        if !valid_path(p) {
                            return Err(Error("invalid_manifest_path"));
                        }
                        paths.insert(p.into());
                    }
                }
                Some(Value::Null) | None => {}
                _ => return Err(Error("invalid_manifest")),
            }
        }
        out.insert(key.clone(), paths);
    }
    if out.is_empty() {
        return Err(Error("empty_population"));
    }
    Ok(out)
}
fn valid_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\0')
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}
pub fn path_set_hash(paths: &BTreeSet<String>) -> String {
    let mut h = Sha256::new();
    h.update(b"kpi-v2-path-set-v1\0");
    for p in paths {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p.as_bytes());
    }
    format!("{:x}", h.finalize())
}
pub fn nul_paths(bytes: &[u8]) -> Result<BTreeSet<String>> {
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err(Error("truncated_git_paths"));
    }
    let mut paths = BTreeSet::new();
    if bytes.is_empty() {
        return Ok(paths);
    }
    for p in bytes[..bytes.len() - 1].split(|b| *b == 0) {
        let p = std::str::from_utf8(p).map_err(|_| Error("invalid_utf8_path"))?;
        if !valid_path(p) || !paths.insert(p.to_owned()) {
            return Err(Error("invalid_git_paths"));
        }
    }
    Ok(paths)
}
pub fn api_paths(v: &Value, tree: &str) -> Result<BTreeSet<String>> {
    if v.get("sha").and_then(Value::as_str) != Some(tree)
        || v.get("truncated").and_then(Value::as_bool) != Some(false)
    {
        return Err(Error("incomplete_tree"));
    }
    let mut seen = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for entry in v
        .get("tree")
        .and_then(Value::as_array)
        .ok_or(Error("invalid_tree"))?
    {
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or(Error("invalid_tree"))?;
        if !valid_path(path) || !seen.insert(path.to_string()) {
            return Err(Error("invalid_tree"));
        }
        let kind = entry
            .get("type")
            .and_then(Value::as_str)
            .ok_or(Error("invalid_tree"))?;
        let mode = entry
            .get("mode")
            .and_then(Value::as_str)
            .ok_or(Error("invalid_tree"))?;
        let oid = entry
            .get("sha")
            .and_then(Value::as_str)
            .ok_or(Error("invalid_tree"))?;
        if !hex(oid, 40)
            || !matches!(
                (kind, mode),
                ("blob", "100644" | "100755" | "120000")
                    | ("commit", "160000")
                    | ("tree", "040000")
            )
        {
            return Err(Error("invalid_tree"));
        }
        if kind != "tree" {
            paths.insert(path.into());
        }
    }
    Ok(paths)
}
pub fn api_call(route: &str) -> Call {
    Call::new(
        "gh",
        vec!["api".into(), "--method".into(), "GET".into(), route.into()],
    )
}
fn fetch(r: &mut impl Runner, route: &str, s: &mut Source) -> Result<(Value, String)> {
    let o = r.run(&api_call(route))?;
    s.exit_code = o.exit_code.map_or(K::Unknown, K::Value);
    s.signal = o.signal.map_or(K::Unknown, K::Value);
    if !o.success() {
        return Err(Error("github_exit_nonzero"));
    }
    Ok((
        strict::value(&o.stdout, 16 * 1024 * 1024)?,
        digest(&o.stdout),
    ))
}
pub struct Collected {
    pub repository: Repository,
    pub artifacts: Vec<(String, Vec<u8>)>,
}
pub fn collect(
    runner: &mut impl Runner,
    request: &RepositoryRequest,
    paths: &BTreeSet<String>,
    index: usize,
    clock: &mut impl FnMut() -> Result<u64>,
) -> Result<Collected> {
    let start = clock()?;
    let mut repo = Repository {
        manifest_key: request.manifest_key.clone(),
        identity: request.identity(),
        ref_name: "refs/heads/main".into(),
        ref_read: Source::new(Status::NotPerformed, "not_performed", start, start),
        commit_sha: K::Unknown,
        tree_sha: K::Unknown,
        tree_read: Source::new(Status::NotPerformed, "not_performed", start, start),
        backend: "not_performed".into(),
        population: Population {
            total: paths.len() as u64,
            path_set_sha256: path_set_hash(paths),
        },
        removed: K::Unknown,
        leaf_path_set_sha256: K::Unknown,
        absent_path_set_sha256: K::Unknown,
        membership: K::Unknown,
        source_qualified: false,
        qualification: Qualification::FailedSource,
    };
    let mut artifacts = Vec::new();
    let result = (|| -> Result<()> {
        let (identity, identity_digest) = fetch(
            runner,
            &format!("repos/{}", request.full_name),
            &mut repo.ref_read,
        )?;
        if identity.get("id").and_then(Value::as_u64) != Some(request.github_id)
            || identity.get("full_name").and_then(Value::as_str) != Some(request.full_name.as_str())
        {
            return Err(Error("repository_identity_mismatch"));
        }
        // The actual ref request interval, distinct from the preceding identity request.
        repo.ref_read.capture.start = clock()?;
        let (reference, reference_digest) = fetch(
            runner,
            &format!("repos/{}/git/ref/heads/main", request.full_name),
            &mut repo.ref_read,
        )?;
        repo.ref_read.capture.end = clock()?;
        let object = reference.get("object").ok_or(Error("invalid_ref"))?;
        let commit = object
            .get("sha")
            .and_then(Value::as_str)
            .filter(|s| hex(s, 40))
            .ok_or(Error("invalid_ref"))?
            .to_owned();
        if reference.get("ref").and_then(Value::as_str) != Some("refs/heads/main")
            || object.get("type").and_then(Value::as_str) != Some("commit")
        {
            return Err(Error("invalid_ref"));
        }
        repo.ref_read.status = Status::Captured;
        repo.ref_read.reason = "none".into();
        repo.ref_read.observed_at = K::Value(repo.ref_read.capture.end);
        let bytes = strict::bytes(
            &json!({"identity":request.identity(),"ref":"refs/heads/main","commit_sha":commit,"identity_response_sha256":identity_digest,"ref_response_sha256":reference_digest,"capture":repo.ref_read.capture}),
        )?;
        let name = format!("repo-{index}-ref.json");
        repo.ref_read.artifact = K::Value(ArtifactRef::new(&name, &bytes));
        artifacts.push((name, bytes));
        repo.tree_read.capture.start = clock()?;
        let (meta, meta_digest) = fetch(
            runner,
            &format!("repos/{}/git/commits/{commit}", request.full_name),
            &mut repo.tree_read,
        )?;
        if meta.get("sha").and_then(Value::as_str) != Some(&commit) {
            return Err(Error("commit_identity_mismatch"));
        }
        let tree = meta
            .pointer("/tree/sha")
            .and_then(Value::as_str)
            .filter(|s| hex(s, 40))
            .ok_or(Error("invalid_commit_tree"))?
            .to_owned();
        repo.commit_sha = K::Value(commit.clone());
        repo.tree_sha = K::Value(tree.clone());
        let local = if request.backend == "local_then_api" {
            if let Some(root) = request.local_root.get() {
                let args = vec![
                    "--no-replace-objects".into(),
                    "-C".into(),
                    root.clone(),
                    "rev-parse".into(),
                    "--verify".into(),
                    format!("{commit}^{{tree}}"),
                ];
                match runner.run(&Call::new("git", args)) {
                    Ok(o) if o.success() => {
                        let actual = std::str::from_utf8(&o.stdout)
                            .map_err(|_| Error("invalid_local_tree"))?
                            .trim_end_matches('\n');
                        if actual != tree {
                            return Err(Error("local_tree_mismatch"));
                        }
                        let o = runner.run(&Call::new(
                            "git",
                            vec![
                                "--no-replace-objects".into(),
                                "-C".into(),
                                root.clone(),
                                "ls-tree".into(),
                                "-r".into(),
                                "-z".into(),
                                "--full-tree".into(),
                                "--name-only".into(),
                                tree.clone(),
                            ],
                        ))?;
                        repo.tree_read.exit_code = o.exit_code.map_or(K::Unknown, K::Value);
                        repo.tree_read.signal = o.signal.map_or(K::Unknown, K::Value);
                        if !o.success() {
                            return Err(Error("git_exit_nonzero"));
                        }
                        Some(nul_paths(&o.stdout)?)
                    }
                    Ok(_) => None,
                    Err(e) => return Err(e),
                }
            } else {
                None
            }
        } else {
            None
        };
        let mut tree_response_digest = None;
        let leaf = if let Some(p) = local {
            repo.backend = "local_git".into();
            p
        } else {
            repo.backend = "github_api".into();
            let (v, raw_digest) = fetch(
                runner,
                &format!("repos/{}/git/trees/{tree}?recursive=1", request.full_name),
                &mut repo.tree_read,
            )?;
            tree_response_digest = Some(raw_digest);
            api_paths(&v, &tree)?
        };
        repo.tree_read.capture.end = clock()?;
        if repo.ref_read.capture.end > repo.tree_read.capture.end
            || !repo.ref_read.capture.valid()
            || !repo.tree_read.capture.valid()
        {
            return Err(Error("invalid_clock"));
        }
        let absent = paths.difference(&leaf).cloned().collect::<BTreeSet<_>>();
        let members: Vec<_> = paths
            .iter()
            .map(|p| json!({"path_sha256":digest(p.as_bytes()),"present":leaf.contains(p)}))
            .collect();
        let mut members = members;
        members.sort_by(|a, b| a["path_sha256"].as_str().cmp(&b["path_sha256"].as_str()));
        let bytes = strict::bytes(
            &json!({"identity":repo.identity,"commit_sha":commit,"tree_sha":tree,"members":members}),
        )?;
        let name = format!("repo-{index}-membership.json");
        repo.membership = K::Value(ArtifactRef::new(&name, &bytes));
        artifacts.push((name, bytes));
        let bytes = strict::bytes(
            &json!({"identity":repo.identity,"commit_sha":commit,"tree_sha":tree,"backend":repo.backend,"capture":repo.tree_read.capture,"commit_response_sha256":meta_digest,"tree_response_sha256":tree_response_digest,"leaf_path_set_sha256":path_set_hash(&leaf),"absent_path_set_sha256":path_set_hash(&absent),"complete":true}),
        )?;
        let name = format!("repo-{index}-tree.json");
        repo.tree_read.artifact = K::Value(ArtifactRef::new(&name, &bytes));
        artifacts.push((name, bytes));
        repo.removed = K::Value(absent.len() as u64);
        repo.leaf_path_set_sha256 = K::Value(path_set_hash(&leaf));
        repo.absent_path_set_sha256 = K::Value(path_set_hash(&absent));
        repo.tree_read.status = Status::Captured;
        repo.tree_read.reason = "none".into();
        repo.tree_read.observed_at = repo.ref_read.observed_at.clone();
        repo.source_qualified = true;
        repo.qualification = Qualification::Qualified;
        Ok(())
    })();
    if let Err(e) = result {
        let source = if repo.ref_read.status == Status::Captured {
            &mut repo.tree_read
        } else {
            &mut repo.ref_read
        };
        source.status = Status::Failed;
        source.reason = e.0.into();
        source.capture.end = clock()?;
        repo.qualification = if e.0 == "incomplete_tree" {
            Qualification::IncompleteSource
        } else {
            Qualification::FailedSource
        };
    }
    Ok(Collected {
        repository: repo,
        artifacts,
    })
}
pub fn aggregate(repositories: &[Repository], start: u64, end: u64) -> Result<Sample<LegacyValue>> {
    let mut source = Source::new(Status::Captured, "none", start, end);
    if repositories.iter().any(|r| !r.source_qualified) {
        source.status = Status::Incomplete;
        source.reason = "incomplete_repository_population".into();
        return Ok(Sample {
            value: K::Unknown,
            source,
            source_qualified: false,
            qualification: Qualification::IncompleteSource,
        });
    }
    let mut removed = 0u64;
    let mut total = 0u64;
    for r in repositories {
        removed = removed
            .checked_add(*r.removed.get().ok_or(Error("invalid_repository"))?)
            .ok_or(Error("count_overflow"))?;
        total = total
            .checked_add(r.population.total)
            .ok_or(Error("count_overflow"))?;
    }
    if total == 0 {
        return Err(Error("empty_population"));
    }
    source.observed_at = repositories
        .iter()
        .filter_map(|r| r.ref_read.observed_at.get().copied())
        .min()
        .map_or(K::Unknown, K::Value);
    let fresh = repositories
        .iter()
        .all(|r| r.ref_read.capture.end <= end && end - r.ref_read.capture.end <= 900);
    Ok(Sample {
        value: if fresh {
            K::Value(LegacyValue {
                removed,
                total,
                unit: "fraction".into(),
            })
        } else {
            K::Unknown
        },
        source,
        source_qualified: fresh,
        qualification: if fresh {
            Qualification::Qualified
        } else {
            Qualification::StaleSource
        },
    })
}
