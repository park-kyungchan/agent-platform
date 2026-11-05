# KPI producer v2

`ap-kpi` is an owner-side Rust collector in agent-platform. It captures Git
metadata, exact UTF-8 path membership, disk bytes and explicitly supplied
sanitized owner evidence. It appends a new versioned journal. It does not fetch,
checkout, install, schedule, call Academy, qualify a deployment or attribute work
to a PR. Ontology owns its separate v2 intake adapter and report authorization.

This PR implements the approved ws-replan design at SHA-256
`5785ddd21ef0e2844042cef5242b7f8d8e79be8e847810d44e83a8a191caa50a`,
with Lead DECISION-1's adopted disk minimum of **15 GiB = 16106127360 bytes**.
The pinned original design is retained in the task directory; it is not rewritten.

## Invocation and release boundary

`ap-kpi capture --request /absolute/owner-request.json` is the only acquisition
command. The operator must separately authorize its authenticated GitHub GETs,
source reads, evidence directory creation and journal append. A source PR/test
pass grants no live-capture authority. Run a pinned build by its explicit path;
there is no host installation in this change. `--help` performs no acquisition.

The CLI trusts the authorized owner, filesystem/mount namespace, source ownership,
native `git`/`gh` executables and their existing authentication context. Requests
are not an untrusted service protocol: their absolute paths are the operator's
explicit source/output selection. Do not expose this command to a caller who
lacks those read/write authorities. No credential value or raw subprocess error
is printed; stdout contains only a verified append receipt. Input owners must
sanitize identity/revision strings; a field allowlist is not content redaction.

The evidence root and journal parent must already exist. Production deployment
uses `/home/palantirkc/work/ws-replan/preparation/kpi/snapshots-v2.jsonl` and
`preparation/kpi/v2-captures/<measurement_id>/`. Tests use only disposable paths.
The journal basename must be `snapshots-v2.jsonl`; the v1 filename is refused.
Manifest paths are read as metadata in memory, never opened as file payloads or
copied into capture artifacts. Existing v1 producer/journal bytes are unchanged.

## Closed request and definition schemas

JSON rejects duplicate keys recursively, unknown typed fields, decimal/negative
integer fields, overflow and bounded-input violations. Each field below is
required. `K<T>` is exactly `{"kind":"value","value":T}`, `{"kind":"unknown"}`
or `{"kind":"not_performed"}`. Bare null is only supported for the PR5 recording
fields explicitly represented by Option: baseline total, settlement lag and
incident fixed_at. Zero never supplies a missing state.

The request schema is `capture::Request`:

| Field | Type / constraint |
| --- | --- |
| schema_version | `ws-replan-kpi-capture-request/v2` |
| measurement_id | unique lowercase ASCII letters/digits/hyphens, 1..96; first alphanumeric |
| manifest_path, definition_contract_path, build_evidence_path | absolute owner-approved input paths |
| journal_path, evidence_root | absolute owner-approved output paths |
| repositories | sorted unique manifest-key array of `{manifest_key,github_id,full_name,local_root,backend}`; local_root is K<absolute path>; backend is `local_then_api` or `api_only` |
| billing_recording_path, parity_recording_path, recall_recording_path, defects_recording_path | absolute sanitized owner recording path, or exactly `{"kind":"not_performed"}` |
| disk_target | `/` |
| limits | `{process_timeout_seconds,capture_timeout_seconds,process_output_bytes,evidence_bytes}`; positive maxima 60 seconds, 300 seconds, 16 MiB and 32 MiB |

The definition schema is `capture::DefinitionContract`:

| Field | Type / constraint |
| --- | --- |
| revision | `ws-replan-kpi-definition/v2` |
| algorithm_revision | `exact-utf8-path-membership/v2` |
| manifest_sha256 | exact original manifest SHA-256 |
| repositories | adopted `{github_id,full_name}` identities in request order |
| parity_tool_ids | complete adopted unique tool ID array; never a compiled 44 |
| billing, disk, recall, defects | PR5-compatible tagged Contract objects described below |

Build evidence is exactly `{source_commit,source_tree,binary_sha256,clean}`.
Commit/tree are full 40-character lowercase OIDs, clean must be true, and the
binary SHA-256 must match the running executable. The owner supplies an attested
build record linking the commit/tree and accepted build; hashes alone do not
authenticate that linkage. The runtime cwd never determines source revision.

Guardrail contracts use the existing PR5 wire shapes:

- Billing: `{kind:"billing",account,month:{year,month},linux_skus:[...],limit}`.
- Disk: `{kind:"disk",minimum:{kind:"exact",quantity:15,unit:"gib"}}` is the
  adopted definition. `{kind:"unresolved_gb_or_gib",quantity}` is also parsed for
  compatibility; that preserves Partial coverage even with qualified source bytes.
- Recall: `{kind:"recall",baseline:{supported,total,evaluation_set_sha256,
  result_revision,artifact_sha256}}` from the explicit baseline owner.
- Defects: `{kind:"defects",week_start,limit}`; UTC Monday-to-Monday week.

Contracts must exist before capture even when their owner recordings are
not_performed; limits/baseline provenance are trusted inputs, not invented
measurement values. A configured but unavailable recording yields Unknown.

## Evidence and measurement semantics

Git acquisition uses `gh api --method GET` for adopted repository identity,
`git/ref/heads/main` and `git/commits/<full-commit>`. It reads the resolved actual
tree OID through local Git, or `git/trees/<actual-tree>?recursive=1`. A truncated
API tree cannot qualify. Local Git disables replacement objects, lazy fetching,
interactive prompts and optional locks; full commit/tree identity must agree.
A missing local object may select the declared read-only API fallback. A failed
actual local tree read stays failed; it does not turn into an empty tree or an
unrecorded retry. Capture never updates shared refs. Commit/tree OIDs are distinct.

Native Git uses `ls-tree -r -z --full-tree --name-only <tree>`. Decode NUL records
with strict UTF-8; do not normalize, case-fold or unquote display paths. API
leaf entries include blobs/symlinks and gitlinks, excluding directory entries.
The population is unique non-KEEP manifest paths; removal is their exact set
difference from the pinned leaf population. Aggregate removal is Unknown unless
every adopted repository qualifies. Successful components remain available.

Hash path sets by sorted UTF-8 bytes: SHA-256 of `kpi-v2-path-set-v1` plus NUL,
then each `u64 big-endian byte_length || path_bytes`. Membership artifacts retain
only path SHA-256 and presence, not private names. Response digests hash exact
received bytes; retained metadata is an explicitly bounded projection. Digests
do not prove source authenticity. Each ref has its own capture interval; the
global interval never claims a simultaneous cross-repository snapshot.

Guardrail recordings are PR5-compatible `{source_revision,capture_start,
capture_end,outcome}`; see `guardrails::Recording/Outcome` for their closed types.
Billing carries account, UTC month bounds, cutoff, settlement lag, complete page
coverage and explicit SKU/unit/quantity. Empty successful complete one-page
coverage with observed zero lag may be zero. Missing/failed response, partial
pages, wrong account/month/SKU/unit, duplicate usage IDs, fractional minutes or
unknown/nonzero lag never qualify zero. Unknown settlement is preserved, not
assumed away. Fractional minutes require a later explicit consumer revision.

Recall requires executed matching evaluation-set/results, and defects require
a complete incident window. Fixing an escaped incident does not erase it.
Baseline assertions remain unverified. Parity uses `capture::ToolRecording` and
the full adopted tool population. Each production-qualified tool requires five
hashed owner gate artifacts, with `{gate,candidate_commit,passed,owner_revision}`,
all passed and bound to the same candidate. Planning booleans alone do not count.
Retained gates use the producer's `owner-gate-<sha256>.json` namespace, regardless
of owner basenames. Preserve exact `parity-owner-recording.json`, a rewritten
`parity-recording.json` projection and `parity-gate-bindings.json` linking both
recording hashes and each original/retained gate reference. Unchecked gates of
unqualified tools remain in the original; their projected Value claims become
Unknown. Owner filenames never reserve or overwrite producer artifacts.

Disk uses statvfs `f_bfree * f_frsize`, matching v1's unallocated-space basis,
including reserved blocks. Exact bytes are primary; `free_gib` is the exact
ratio with explicit `unit:"GiB"` and denominator 1073741824. No rounded threshold
comparison or silent conversion to `f_bavail` occurs. Missing statvfs is failed.

Freshness is evaluated at source completion: 900 seconds for remote endpoint
reads/disk, 3600 parity/recall/defects, 86400 billing; equality is fresh. Import
timestamps do not refresh sources. Ontology must qualify again for delivery.
Subprocesses have monotonic deadlines and bounded pipe readers; timeout/output
overflow terminates and reaps their private process group. Regular-file byte
bounds do not promise a deadline for a blocked kernel/filesystem I/O operation.

## Output and consumer integration

`model::Snapshot` is the exact closed `ws-replan-kpi-snapshot/v2` schema. Its root
fields are schema_version, measurement_id, capture_status, capture, producer,
definition, kpi_legacy_removal, kpi_mcp_parity, guard_ci_minutes_month,
guard_disk_free_gib, guard_escaped_defects_week, guard_recall_regression,
repositories and artifacts. Source failures have explicit status/reason and
exit/signal states. Primary values use K wrappers; no rounded pct or old gb field.

The root definition pins the retained external manifest by source_id/hash/bytes;
all other references are hashed relative artifact basenames in the new private
capture directory. Immutable artifacts precede the row append.
The producer syncs artifact files, the capture directory and its evidence-root
parent before a journal append; a parent-sync failure prevents verification.
Sources use UTC Unix seconds with start/end intervals, immutable repository
IDs and full OIDs.
JSON object keys are sorted, one bounded row plus LF, <=64 KiB. Journal limits
are 4 MiB/4096 rows. No automatic rotation/deletion/repair rewrites history.

`capture_status=complete` means every metric has complete qualified evidence,
not that every guardrail passes. Partial evidence is useful; Unknown and
NotPerformed remain distinct. A disk source may qualify under an unresolved
bound while coverage remains Partial. Ontology must honor both states.

PR3's v1 fixed-path/closed-key parser deliberately cannot consume this schema.
The separate Ontology `program-snapshot-intake/v2` adapter must verify exact
row+LF/line/envelope hashes, external manifest binding, all artifact hashes and
per-repository endpoint binding, without changing v1 observations. Cross-check
producer qualification at capture.end, then qualify at the current trusted time.
Pass the retained four guardrail recording bytes and separately trusted adopted
Contracts through PR5's existing qualify_recording. Do not trust row-supplied
bounds or a producer flag alone. Retain source-only observations behind their
own Program scope/type grants; never copy hidden qualification onto a public root.

Historical v1 rows stay producer-v1-rendered-paths assertions. A new v2 baseline
does not supply old endpoint pins. Compatible future attribution additionally
requires complete first-parent/PR association/reconciliation evidence from the
Ontology owner. Preserve **qualified merge contributions + unresolved
reconciliation** whenever those conditions are unresolved; never infer unique
PR attribution from this capture or subtract values across algorithms.

## Journal protocol and exit contract

The journal uses descriptor-relative no-follow regular-file opening and a native
nonblocking exclusive advisory lock. Another active writer returns journal_busy.
The producer validates bounded prior rows and LF before appending/syncing and
reading back bytes. Same measurement ID + same bytes preserves the exact file
bytes, but still syncs the journal and its parent before returning a receipt;
cached bytes after an earlier append_unknown are not sufficient. A failed sync
continues to return append_unknown. Changed bytes are a conflict. Truncated
tails stop further writes. Native locking coordinates
cooperating producers; trusted owners must not replace the journal concurrently.

A receipt binds measurement ID, line number, byte offset, row length/hash and
the journal prefix length/hash through that row. A later append preserves that
pin. Verify output/readback, not exit alone. After append_unknown, reconcile
the exact ID/bytes before another capture attempt; do not rerun blindly.

| Exit | Meaning |
| --- | --- |
| 0 | Complete evidence append/readback verified; bounds can still fail |
| 3 | Partial/Failed evidence append/readback verified |
| 2 | Invalid request/usage/trusted source contract before a valid capture |
| 4 | Journal/evidence I/O or uncertain append; readback/reconciliation required |

## Verification and resources

Run workspace fmt, strict Clippy and tests in the Lead-assigned target/build slot.
The local CI script now includes the new crate; existing frozen ap judges remain
unchanged. Tests use synthetic owner evidence, disposable Git repositories and
injected responses, with no Git/GitHub network calls or live Academy calls.
The dependency graph adds one local package using workspace serde/serde_json/
sha2/rustix; zero registry package/version/feature changes.

The design estimates <=1 GiB local build target and <=32 MiB evidence/capture.
No new workflow/matrix/scheduled capture or installed service is added. Actual
build disk and remote CI results belong in the task report, not these estimates.
