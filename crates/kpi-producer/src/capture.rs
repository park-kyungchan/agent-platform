//! Trusted capture orchestration; the request is an owner-side CLI contract.
use crate::{
    Error, Result, digest,
    filesystem::{self, Evidence},
    git::{self, RepositoryRequest},
    guardrails::{self, Contract, DiskUnit, Outcome, Recording},
    hex,
    model::*,
    process::{self, Call, Runner},
    strict, text,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecordingInput {
    Path(String),
    State(Unattempted),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unattempted {
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub process_timeout_seconds: u32,
    pub capture_timeout_seconds: u32,
    pub process_output_bytes: u64,
    pub evidence_bytes: u64,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if !(1..=60).contains(&self.process_timeout_seconds)
            || !(1..=300).contains(&self.capture_timeout_seconds)
            || !(1..=16 * 1024 * 1024).contains(&self.process_output_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.evidence_bytes)
        {
            Err(Error("invalid_limits"))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: String,
    pub measurement_id: String,
    pub manifest_path: String,
    pub definition_contract_path: String,
    pub build_evidence_path: String,
    pub journal_path: String,
    pub evidence_root: String,
    pub repositories: Vec<RepositoryRequest>,
    pub billing_recording_path: RecordingInput,
    pub parity_recording_path: RecordingInput,
    pub recall_recording_path: RecordingInput,
    pub defects_recording_path: RecordingInput,
    pub disk_target: String,
    pub limits: Limits,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionContract {
    pub revision: String,
    pub algorithm_revision: String,
    pub manifest_sha256: String,
    pub repositories: Vec<Identity>,
    pub parity_tool_ids: BTreeSet<String>,
    pub billing: Contract,
    pub disk: Contract,
    pub recall: Contract,
    pub defects: Contract,
}
fn absolute(p: &str) -> bool {
    p.starts_with('/') && !p.contains('\0') && !p.split('/').any(|s| s == ".." || s == ".")
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        if self.schema_version != "ws-replan-kpi-capture-request/v2"
            || !measurement_id(&self.measurement_id)
            || self.disk_target != "/"
            || self.repositories.is_empty()
            || self.repositories.len() > 32
            || self.repositories.iter().any(|r| !r.valid())
            || self
                .repositories
                .windows(2)
                .any(|w| w[0].manifest_key >= w[1].manifest_key)
        {
            return Err(Error("invalid_request"));
        }
        for p in [
            &self.manifest_path,
            &self.definition_contract_path,
            &self.build_evidence_path,
            &self.journal_path,
            &self.evidence_root,
        ] {
            if !absolute(p) {
                return Err(Error("invalid_path"));
            }
        }
        if Path::new(&self.journal_path)
            .file_name()
            .and_then(|n| n.to_str())
            != Some("snapshots-v2.jsonl")
        {
            return Err(Error("invalid_journal_name"));
        }
        for i in [
            &self.billing_recording_path,
            &self.parity_recording_path,
            &self.recall_recording_path,
            &self.defects_recording_path,
        ] {
            match i {
                RecordingInput::Path(p) if absolute(p) => {}
                RecordingInput::State(s) if s.kind == "not_performed" => {}
                _ => return Err(Error("invalid_source_request")),
            }
        }
        Ok(())
    }
}
impl DefinitionContract {
    pub fn validate(&self, r: &Request, manifest: &[u8]) -> Result<()> {
        if self.revision != DEFINITION
            || self.algorithm_revision != ALGORITHM
            || self.manifest_sha256 != digest(manifest)
            || self.parity_tool_ids.is_empty()
            || self.parity_tool_ids.len() > 256
            || self.parity_tool_ids.iter().any(|s| !text(s))
        {
            return Err(Error("invalid_definition"));
        }
        let expected = r
            .repositories
            .iter()
            .map(RepositoryRequest::identity)
            .collect::<Vec<_>>();
        if self.repositories != expected {
            return Err(Error("definition_population_mismatch"));
        }
        for (c, kind) in [
            (&self.billing, "billing"),
            (&self.disk, "disk"),
            (&self.recall, "recall"),
            (&self.defects, "defects"),
        ] {
            c.validate()?;
            if c.kind() != kind {
                return Err(Error("contract_kind_mismatch"));
            }
        }
        Ok(())
    }
}
pub struct Prepared {
    pub request: Request,
    pub definition: DefinitionContract,
    pub build: BuildEvidence,
    pub manifest: Vec<u8>,
    pub request_bytes: Vec<u8>,
    pub definition_bytes: Vec<u8>,
    pub build_bytes: Vec<u8>,
}
pub fn prepare(bytes: &[u8], binary_sha256: &str) -> Result<Prepared> {
    let request: Request = strict::parse(bytes, LINE_LIMIT)?;
    request.validate()?;
    let manifest = filesystem::read(&request.manifest_path, 8 * 1024 * 1024)?;
    let definition_bytes = filesystem::read(&request.definition_contract_path, LINE_LIMIT)?;
    let definition: DefinitionContract = strict::parse(&definition_bytes, LINE_LIMIT)?;
    definition.validate(&request, &manifest)?;
    let build_bytes = filesystem::read(&request.build_evidence_path, LINE_LIMIT)?;
    let build: BuildEvidence = strict::parse(&build_bytes, LINE_LIMIT)?;
    build.validate()?;
    if build.binary_sha256 != binary_sha256 {
        return Err(Error("binary_pin_mismatch"));
    }
    let pop = git::population(&manifest)?;
    if pop
        .keys()
        .map(String::as_str)
        .ne(request.repositories.iter().map(|r| r.manifest_key.as_str()))
    {
        return Err(Error("manifest_population_mismatch"));
    }
    Ok(Prepared {
        request,
        definition,
        build,
        manifest,
        request_bytes: bytes.into(),
        definition_bytes,
        build_bytes,
    })
}
fn read_recording(i: &RecordingInput, start: u64, end: u64) -> Result<Option<Vec<u8>>> {
    match i {
        RecordingInput::State(_) => Ok(Some(strict::bytes(&Recording {
            source_revision: "explicit-not-performed/v2".into(),
            capture_start: start,
            capture_end: end,
            outcome: Outcome::NotPerformed,
        })?)),
        RecordingInput::Path(p) => match filesystem::read(p, guardrails::RECORDING_LIMIT) {
            Ok(b) => Ok(Some(b)),
            Err(Error("source_unavailable")) => Ok(None),
            Err(_) => Ok(Some(b"{}".to_vec())),
        },
    }
}
fn source_status(q: &Qualification) -> Status {
    match q {
        Qualification::NotPerformed => Status::NotPerformed,
        Qualification::MissingSource => Status::Missing,
        Qualification::FailedSource => Status::Failed,
        Qualification::InvalidSource => Status::Invalid,
        Qualification::IncompleteSource => Status::Incomplete,
        _ => Status::Captured,
    }
}
fn reason(q: &Qualification) -> &'static str {
    match q {
        Qualification::Qualified => "none",
        Qualification::NotPerformed => "not_performed",
        Qualification::MissingSource => "missing_source",
        Qualification::FailedSource => "failed_source",
        Qualification::InvalidSource => "invalid_source",
        Qualification::IncompleteSource => "incomplete_source",
        Qualification::SettlementLag => "settlement_lag",
        Qualification::UnverifiedBaseline => "unverified_baseline",
        Qualification::FutureSource => "future_source",
        Qualification::StaleSource => "stale_source",
        Qualification::UnresolvedDiskUnit => "unresolved_disk_unit",
    }
}
fn guardrail(
    c: &Contract,
    bytes: Option<Vec<u8>>,
    evidence: &mut Evidence,
    start: u64,
    end: u64,
) -> Result<GuardrailSample> {
    let evaluation = guardrails::evaluate(bytes.as_deref(), c, end)?;
    let contract = evidence.retain(&format!("{}-contract.json", c.kind()), &strict::bytes(c)?)?;
    // Invalid input is represented by a deliberately invalid, non-sensitive PR5
    // recording. Preserve only its digest, never arbitrary owner payload bytes.
    let bytes = if evaluation.qualification == Qualification::InvalidSource {
        Some(b"{}".to_vec())
    } else {
        bytes
    };
    let recording = match bytes {
        Some(b) => K::Value(evidence.retain(&format!("{}-recording.json", c.kind()), &b)?),
        None => K::Unknown,
    };
    let mut source = Source::new(
        source_status(&evaluation.qualification),
        reason(&evaluation.qualification),
        start,
        end,
    );
    source.artifact = recording.clone();
    source.observed_at = evaluation.observed_at;
    let free_gib = if c.kind() == "disk" {
        evaluation.value.get().map_or(K::Unknown, |n| {
            K::Value(Ratio {
                numerator: *n,
                denominator: GIB,
                unit: "GiB".into(),
            })
        })
    } else {
        K::Unknown
    };
    Ok(GuardrailSample {
        kind: c.kind().into(),
        value: evaluation.value,
        unit: c.unit().into(),
        total: if c.kind() == "recall" {
            evaluation.total
        } else {
            K::Unknown
        },
        free_gib,
        source,
        source_qualified: evaluation.source_qualified,
        qualification: evaluation.qualification,
        contract,
        recording,
    })
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolStatus {
    pub id: String,
    pub bounded_implemented: bool,
    pub production_qualified: bool,
    pub parity: K<ArtifactRef>,
    pub synthetic_tests: K<ArtifactRef>,
    pub independent_review: K<ArtifactRef>,
    pub ci: K<ArtifactRef>,
    pub rollback: K<ArtifactRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRecording {
    pub source_revision: String,
    pub observed_at: u64,
    pub population_complete: bool,
    pub tools: Vec<ToolStatus>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub gate: String,
    pub candidate_commit: String,
    pub passed: bool,
    pub owner_revision: String,
}
fn parity(
    input: &RecordingInput,
    tool_ids: &BTreeSet<String>,
    evidence: &mut Evidence,
    start: u64,
    end: u64,
) -> Result<Sample<ParityValue>> {
    let mut source = Source::new(Status::NotPerformed, "not_performed", start, end);
    let mut out = Sample {
        value: K::NotPerformed,
        source: source.clone(),
        source_qualified: false,
        qualification: Qualification::NotPerformed,
    };
    let RecordingInput::Path(path) = input else {
        return Ok(out);
    };
    let result = (|| -> Result<ToolRecording> {
        let bytes = filesystem::read(path, guardrails::RECORDING_LIMIT)?;
        let record: ToolRecording = strict::parse(&bytes, guardrails::RECORDING_LIMIT)?;
        if !text(&record.source_revision)
            || !record.population_complete
            || record.tools.len() != tool_ids.len()
        {
            return Err(Error("incomplete_source"));
        }
        let mut ids = BTreeSet::new();
        for t in &record.tools {
            if !ids.insert(t.id.clone()) || !tool_ids.contains(&t.id) {
                return Err(Error("invalid_source"));
            }
            if t.production_qualified {
                let mut candidate = None;
                for (kind, gate) in [
                    ("parity", &t.parity),
                    ("synthetic_tests", &t.synthetic_tests),
                    ("independent_review", &t.independent_review),
                    ("ci", &t.ci),
                    ("rollback", &t.rollback),
                ] {
                    let a = gate
                        .get()
                        .filter(|a| a.valid())
                        .ok_or(Error("incomplete_source"))?;
                    let dir = Path::new(path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .ok_or(Error("invalid_path"))?;
                    let b = filesystem::read(&format!("{dir}/{}", a.path), LINE_LIMIT)?;
                    if a.sha256 != digest(&b) || a.bytes != b.len() as u64 {
                        return Err(Error("invalid_source"));
                    }
                    let g: Gate = strict::parse(&b, LINE_LIMIT)?;
                    if !g.passed
                        || g.gate != kind
                        || !hex(&g.candidate_commit, 40)
                        || !text(&g.owner_revision)
                        || candidate.as_ref().is_some_and(|p| p != &g.candidate_commit)
                    {
                        return Err(Error("invalid_source"));
                    }
                    candidate = Some(g.candidate_commit);
                    if !evidence.references.iter().any(|r| r == a) {
                        evidence.retain(&a.path, &b)?;
                    }
                }
            }
        }
        source.artifact = K::Value(evidence.retain("parity-recording.json", &bytes)?);
        Ok(record)
    })();
    match result {
        Ok(r) => {
            source.status = Status::Captured;
            source.reason = "none".into();
            source.observed_at = K::Value(r.observed_at);
            let q = if r.observed_at > end {
                Qualification::FutureSource
            } else if end - r.observed_at > 3600 {
                Qualification::StaleSource
            } else {
                Qualification::Qualified
            };
            out.source_qualified = q == Qualification::Qualified;
            out.value = if out.source_qualified {
                K::Value(ParityValue {
                    qualified: r.tools.iter().filter(|t| t.production_qualified).count() as u64,
                    total: r.tools.len() as u64,
                    bounded_implemented: K::Value(
                        r.tools.iter().filter(|t| t.bounded_implemented).count() as u64,
                    ),
                    unit: "fraction".into(),
                    tool_set_sha256: git::path_set_hash(tool_ids),
                    result_revision: r.source_revision,
                })
            } else {
                K::Unknown
            };
            out.qualification = q;
        }
        Err(e) => {
            out.value = K::Unknown;
            out.qualification = match e.0 {
                "source_unavailable" => Qualification::MissingSource,
                "incomplete_source" => Qualification::IncompleteSource,
                _ => Qualification::InvalidSource,
            };
            source.status = source_status(&out.qualification);
            source.reason = reason(&out.qualification).into();
        }
    }
    out.source = source;
    Ok(out)
}
/// Returns an in-memory snapshot plus immutable evidence; append is a separate
/// checked boundary. Tests inject runner, clock and disk, with no network/live probe.
pub fn collect(
    prepared: Prepared,
    runner: &mut impl Runner,
    clock: &mut impl FnMut() -> Result<u64>,
    disk: &mut impl FnMut() -> Result<u64>,
) -> Result<(Snapshot, Evidence)> {
    let p = prepared;
    let r = &p.request;
    let start = clock()?;
    let git_version = String::from_utf8(process::checked(
        runner,
        &Call::new("git", vec!["--version".into()]),
    )?)
    .map_err(|_| Error("invalid_tool_version"))?
    .trim()
    .to_owned();
    let gh_version = String::from_utf8(process::checked(
        runner,
        &Call::new("gh", vec!["--version".into()]),
    )?)
    .map_err(|_| Error("invalid_tool_version"))?
    .lines()
    .next()
    .unwrap_or("")
    .to_owned();
    if !text(&git_version) || !text(&gh_version) {
        return Err(Error("invalid_tool_version"));
    }
    let mut evidence =
        Evidence::create(&r.evidence_root, &r.measurement_id, r.limits.evidence_bytes)?;
    let contract = evidence.retain("definition.json", &p.definition_bytes)?;
    let request = evidence.retain("request.json", &p.request_bytes)?;
    let build = evidence.retain("build.json", &p.build_bytes)?;
    let population = git::population(&p.manifest)?;
    let mut repositories = Vec::new();
    for (i, req) in r.repositories.iter().enumerate() {
        let got = git::collect(
            runner,
            req,
            population
                .get(&req.manifest_key)
                .ok_or(Error("manifest_population_mismatch"))?,
            i,
            clock,
        )?;
        for (name, bytes) in got.artifacts {
            evidence.retain(&name, &bytes)?;
        }
        repositories.push(got.repository);
    }
    let disk_start = clock()?;
    let disk_result = disk();
    let disk_end = clock()?;
    let disk_record = Recording {
        source_revision: "statvfs-root-f-bfree/v2".into(),
        capture_start: disk_start,
        capture_end: disk_end,
        outcome: match disk_result {
            Ok(n) => Outcome::Disk {
                observed_at: disk_end,
                numerator: n,
                denominator: 1,
                unit: DiskUnit::Bytes,
            },
            Err(_) => Outcome::Failed,
        },
    };
    let input_start = clock()?;
    let billing = read_recording(&r.billing_recording_path, input_start, input_start)?;
    let recall = read_recording(&r.recall_recording_path, input_start, input_start)?;
    let defects = read_recording(&r.defects_recording_path, input_start, input_start)?;
    let mut parity = parity(
        &r.parity_recording_path,
        &p.definition.parity_tool_ids,
        &mut evidence,
        input_start,
        clock()?,
    )?;
    let end = clock()?;
    if end < start {
        return Err(Error("invalid_clock"));
    }
    // Requalify all values at one source-completion clock. The journal timestamp
    // never refreshes an owner recording or an earlier repository ref.
    if parity.source_qualified
        && parity
            .source
            .observed_at
            .get()
            .is_none_or(|t| *t > end || end - t > 3600)
    {
        parity.value = K::Unknown;
        parity.source_qualified = false;
        parity.qualification = Qualification::StaleSource;
    }
    for repo in &mut repositories {
        if repo.source_qualified
            && (repo.ref_read.capture.end > end || end - repo.ref_read.capture.end > 900)
        {
            repo.source_qualified = false;
            repo.qualification = Qualification::StaleSource;
            repo.removed = K::Unknown;
        }
    }
    let legacy = git::aggregate(&repositories, start, end)?;
    let billing = guardrail(
        &p.definition.billing,
        billing,
        &mut evidence,
        input_start,
        end,
    )?;
    let disk = guardrail(
        &p.definition.disk,
        Some(strict::bytes(&disk_record)?),
        &mut evidence,
        disk_start,
        end,
    )?;
    let recall = guardrail(
        &p.definition.recall,
        recall,
        &mut evidence,
        input_start,
        end,
    )?;
    let defects = guardrail(
        &p.definition.defects,
        defects,
        &mut evidence,
        input_start,
        end,
    )?;
    evidence.references.sort_by(|a, b| a.path.cmp(&b.path));
    let mut snapshot = Snapshot {
        schema_version: SCHEMA.into(),
        measurement_id: r.measurement_id.clone(),
        capture_status: String::new(),
        capture: Interval { start, end },
        producer: Producer {
            revision: PRODUCER.into(),
            source_commit: p.build.source_commit,
            source_tree: p.build.source_tree,
            binary_sha256: p.build.binary_sha256,
            build_evidence: build,
            git_version,
            gh_version,
        },
        definition: DefinitionPin {
            revision: DEFINITION.into(),
            algorithm_revision: ALGORITHM.into(),
            contract,
            manifest: ManifestPin {
                source_id: "ws-replan-plan-a-v1-manifest".into(),
                sha256: digest(&p.manifest),
                bytes: p.manifest.len() as u64,
            },
            request,
        },
        kpi_legacy_removal: legacy,
        kpi_mcp_parity: parity,
        guard_ci_minutes_month: billing,
        guard_disk_free_gib: disk,
        guard_escaped_defects_week: defects,
        guard_recall_regression: recall,
        repositories,
        artifacts: evidence.references.clone(),
    };
    snapshot.capture_status = snapshot.status();
    snapshot.validate()?;
    evidence.verify()?;
    Ok((snapshot, evidence))
}
pub fn disk_free() -> Result<u64> {
    let stat = rustix::fs::statvfs("/").map_err(|_| Error("disk_stat_failed"))?;
    stat.f_bfree
        .checked_mul(stat.f_frsize)
        .ok_or(Error("disk_overflow"))
}
