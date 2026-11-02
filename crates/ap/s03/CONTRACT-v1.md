# S03 scoped installation and recovery contract v1

This independently authored contract precedes implementation. Base S02 is
2d4269e87533f3ef7c26fbe2347448cbff08163d. The adopted ZIP SHA256 is
964845e260a25350823fb42d38a60e7cba973f8af607e10b505727d3bf78915c.
S03/AP-01..08 and PROPOSAL section 9 govern. No source retirement, global
installation, fresh runtime loading or P6 completion is asserted by this source.
Independent Wonder review and Lead compiled/staged/native proof remain required.

## Scope and admission

Linux only. New API/JSON schema version is 1, independent of the unchanged S02
render schema. Existing `ap render`, top-level help and all S02 transcripts stay
byte-compatible. Add `ap install plan`, `ap install apply`, `ap install recover`,
`ap doctor inspect`, and `ap install --help`. Bare `doctor` remains usage error.
One JSON document on stdin, <=1MiB, strict required fields/unknown-field rejection.
No environment/default home/profile/cwd discovery. Diagnostics contain fixed
codes and static fields, never payloads. New successes are compact JSON plus LF.
Admission errors exit 2; contract/filesystem refusals exit 3; partial/recovery
required reports exit 5 with a report on stdout. I/O reports never promise that
no effect occurred after a replacement. Outputs expose public instruction text
in a plan, never native configuration contents or canonical source contents.

Plan input `Spec` fields: schema_version, canonical_path, codex_dir, claude_dir,
scope (`codex`, `claude`, `both`), uid, environment. uid must equal effective UID.
Environment is a strict tagged object, either
`{"kind":"disposable","root":"/absolute/private/staging/root"}` or
`{"kind":"coordinated_native","evidence":"nonempty coordination reference"}`.
Native evidence is an explicit caller attestation of external-writer coordination,
not something an advisory lock or ap can prove. Plan/doctor may inspect it; apply
and recovery refuse `writers_quiesced:false` and require the attestation for native
work. There is no implicit native apply. All actual /root native paths are read-only
for the S03 worker. Lead alone owns reviewed native effects and fresh loading.

Both profile paths are explicit but only selected scopes are opened/locked.
Path spelling uses S02's strict absolute ASCII component grammar. Source and
profile trees are disjoint; profiles may not nest. The disposable root must be
an existing mode0700 directory owned by uid, and all source/profile/state paths
must be strictly beneath it. No ancestor symlinks (including chains/dangling),
non-directories, or group/other writable ancestors. Every ancestor owner must be
root or uid. Traversal is descriptor-relative with NOFOLLOW at each component;
recheck the logical path's directory identities before each replacement. A renamed
ancestor is a conflict, never a reason to write a displaced tree silently.

Canonical source must be an ordinary single-link file, owned by uid, readable,
mode with no special/group-write/other-write bits, max1MiB. Never write it. Freeze
its SHA256 and metadata for recheck. Target files require the same owner and safe
mode; preserve exact existing uid/gid/mode. Missing Claude files use mode0644 and
effective gid; new links use kernel symlink mode0777. Reject hardlinks, special
files, directories, non-UTF8 and oversized instructions. Existing Codex links
must point directly (absolute or lexical relative) to the canonical ordinary file;
no chain/dangling/foreign target. Claude symlinks are unsupported. Codex override
must be absent and stays a read dependency throughout apply/recovery. Unselected
profiles are not read; S02 rendering uses synthetic missing unselected inputs.

S02 owns only creation of a missing Codex link and addition of the exact Claude
canonical import. Existing valid Codex links are unchanged, including relative
spelling. Existing Claude text remains exact; S02 refusals still apply. No owned
settings/MCP/hook/trust selectors exist in this increment. Unknown settings,
Daniel Prayer and official Hindsight/Herdr registrations are untouched opaque
files, never parsed/reformatted or executed. No new pane hook has a justified caller.

## Planning, locks and apply

Plan returns Spec, canonical fingerprint, directory topology, override absence,
and ordered entries with selector/path/before snapshot/after Entry/changed. A
snapshot includes exact public Entry, SHA256 and metadata. Compute desired entries
by unchanged S02 render. Never accept caller-supplied replacement content without
recomputing and exactly validating the plan. Planning is read-only; before apply,
re-read everything under all selected profile `.ap-install.lock` advisory locks,
acquired in bytewise profile path order using kernel try-lock. Lock files are
single-link regular files owned by uid, mode0600; never unlink a lock inode.
Locks cover different command directories for the same profiles. Refuse busy locks.
Lock creation is the only permitted side effect before a command state exists.

Apply input: plan, command_dir, writers_quiesced. command_dir is an existing empty
private uid-owned mode0700 directory disjoint from source/profile trees. ap creates
only state.json and bounded .ap-stage-* files. No global receipt/registry/history
or background recovery process. Refuse reuse of an existing state.json. Private
state persists original selected snapshots and write intent before each rename,
fsyncs the state and directory, then rechecks expected-before and dependencies
immediately before descriptor-relative rename. Stage and validate ordinary file
bytes/owner/mode or link target, sync staged data and containing directory.
Replacement is atomic for ONE entry, never across files. Refuse observed edits,
including unrelated instruction edits during planning; replan safely from current
bytes. Unrelated native config edits do not cause a conflict because no config
file is a dependency or output. A recheck is not universal compare-and-swap:
an editor ignoring coordination can still win the final window. Such external
coordination is a precondition, not an exclusivity claim inferred from locks.

Per-entry durable phases: pending, prepared, applied, unchanged, conflicted,
recovered. All entries initially pending; unchanged entries are recorded only
after recheck. Stop on first failure; retain exact applied/unchanged/conflicted
and pending/prepared entries, never imply all-or-nothing. After rename, failed
readback/state/fsync produces recovery_required, with prepared state permitting
reconciliation. A process death after rename and before state update is deliberately
ambiguous; recovery inspects actual fields, does not replay apply. Return command
state/report explicitly; stdout failure can be reconciled using command_dir.

## Recovery

Recover input: command_dir, writers_quiesced. Reopen private state and validate
its schema, plan, paths, snapshots and phases before any effect. Lock the same
profile set; verify source/topology/override. Reconcile pending/prepared/applied:
current==before means no effect; current==after means installed. Unexpected owned
values conflict, never overwrite. Unchanged entries have no rollback authority.
Recover in reverse entry order, persist recovery intent before each replacement.
Recovery is itself restartable; exact already-restored entries are recognized.

Codex recovery removes only a newly created link still matching what this command
wrote. Claude recovery removes the exact one added import line, using S02 text
validation to reject duplicate/changed/ambiguous canonical references. If the
current whole text equals installed text, return exact original bytes (including
original missing terminal newline). Otherwise remove only that complete import
line and its EOL, preserving all later unrelated bytes, including a bridge newline
needed to keep later text on its own line. A created file becomes missing only
when nothing else remains; otherwise preserve it. Preserve the CURRENT safe
uid/gid/mode on an unrelated later edit. No stale whole-file backup restore.
If the import was already removed, preserve remaining unrelated text and record
recovered. If changed to another import or duplicate/fence, report conflict.
Recovery can finish partially; report each entry and allow a later explicit retry.
Retain the private command state; no deletion of originals/backups by this worker.

## Doctor and runtime evidence

`ap doctor inspect` takes Spec and runs only bounded path/topology/metadata/source,
override and selected instruction checks. Report each selected selector as
`ready` or `needs_install`, source fingerprint and
`native_loading:"unverified", hooks_executed:0, p6_complete:false`.
No subprocesses, shell, event-hook execution, settings parsing, native model calls,
network, vendor trust edits or schema widening. A fake executable event hook must
remain uninvoked. Muse/agy: retained native mechanisms with loading verification
pending; neither is removed or represented as supported by these two adapters.
Hermes relay/mount remains separately owned; absent Grok/Gemini executables grant
no deletion authority. Exact-ID native logs and unknown current agy hash drift
remain intact. Lead must obtain native loading evidence beyond manual file reads
or model self-report; source checks cannot satisfy AP-08 fresh-session proof.

## Verification obligation

Literal cases.json and negative comparator controls are frozen before candidate
behavior is run. Driver builds only disposable fixtures under an explicitly supplied
private root. Candidate must match exact selected effects/report fields and leave
all opaque files unchanged. Internal Rust tests inject failures at recheck, rename
and state-persistence boundaries, including process exit after rename. These tests
are source obligations until Lead grants the sole -j1 build slot and runs them.
No source-level Python experiment is compiled Rust evidence. All S02 frozen bytes
and successful pinned binary/evidence remain unchanged. Any future expectation
correction is additive/versioned and requires independent review.
