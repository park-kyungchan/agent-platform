# Additive public content coverage — 2026-11-04

Authority: user-approved repo-reorg-v2/ap-strip-v1/TASK.md section C. This is a
bounded successor to S03's owned-entry rules, not a change to frozen S02/S03
requests, transcripts, pins or installation selectors. No new framework.

New explicit commands:

- `ap render content < request.json`: pure proposal for the fixed 19 public
  content files. Strict request fields: schema_version (1), agents_dir (explicit
  absolute S03 path grammar), entries (each relative_path and before Entry).
  Every inventory path must occur exactly once; unknown/duplicate/missing paths
  are refused. Only missing or ordinary UTF-8 file snapshots (<=1 MiB; no NUL)
  are admitted. Artifacts sorted by absolute bytewise path preserve before,
  propose the embedded file text as after, and mark changed. Existing files'
  proposed replacements confer no installation authority.
- `ap doctor content < spec.json`: strict schema_version (1), agents_dir, uid and
  S03 environment (disposable root or coordinated_native evidence). UID must
  equal effective UID; disposable root is private mode0700, agents_dir strictly
  beneath it. All ancestors use S03 descriptor-relative no-follow owner/mode
  checks. Each target must be missing or a safe single-link uid-owned ordinary
  UTF-8 file, <=1 MiB. Unsafe metadata, special files and links are refused.
  Missing parent directories yield missing without creating them.

The fixed owned set is content/AGENTS.md, content/runtime-references/*.md and
all files in exactly three generic skill bundles. No caller can select a
settings/MCP/hooks/trust file or add arbitrary content. Product input/output
schemas remain strict. Top-level help and existing S02/S03 commands keep their
frozen bytes; these additive commands are documented here and in the README.

Doctor reports ready, missing or drift, expected SHA256 and observed SHA256
(null for missing). It exposes no observed text. It reads only the fixed public
entries, creates no locks/directories/state, spawns no process, executes no hook,
and returns native_loading:unverified, hooks_executed:0, p6_complete:false.
Each observation is individually checked; the whole inventory is not an atomic
snapshot. No filesystem diagnosis proves actual native loading.

There is deliberately no content apply/recover API. Existing S03 installation
owns its original link/import selectors only, and never writes canonical source
content. Any future content install requires a separately reviewed ownership,
conflict/recovery and loading contract. Real-host content is not applied by this
task. Missing/drift can be reviewed through the pure render proposal first.
