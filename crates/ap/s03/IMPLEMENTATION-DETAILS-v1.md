# S03 implementation details for independent review

These details supplement the preimplementation CONTRACT-v1 and do not redefine
S02 rendering or edit frozen v1/v2 corpus bytes. No compiled candidate execution
has occurred during authorship.

* Lock the private command directory inode (kernel advisory try-lock) before the
  selected sorted profile locks. This closes participating commands with disjoint
  scopes reusing the same command_dir, without adding another state/registry file.
* Persist the stage's inode/content fingerprint in `written` before replacement.
  A `prepared` entry whose current inode cannot be tied to that stage remains
  ambiguous and refuses recovery, including on repeated recovery attempts.
  Never-attempted entries have no rollback authority. Completed recovery does not
  regain authority if a later command installs the same selector again.
* Missing destinations use Linux RENAME_NOREPLACE. Existing files use atomic
  rename after expected-before/dependency checks. Neither operation claims CAS
  for arbitrary in-place changes by an uncooperative same-UID writer. Caller
  coordination is still required.
* Direct symlink qualification traverses every intermediate component, including
  components later cancelled by '..'. Lexical normalization must not hide a
  dangling, non-directory or symlink ancestor. Directory and source inode alias
  checks also refuse distinct selected spellings that actually share an owned
  root or source inode, without trying to alter mount topology.
* Root/UID-controlled ancestors with no writable group/other or special mode
  bits are supported. `/tmp` and symlinked checkout ancestors are intentionally
  unsupported; supply a private task-local scratch root beneath safe ancestors.
* Files are bounded to 1MiB; private state is also bounded to 1MiB. Large aggregate
  snapshots can therefore refuse before a rename. A subsequent serialization or
  fsync failure after a prior entry changed returns recovery_required and retains
  the last durable state. No canonical source bytes appear in the state.
* `.ap-install.lock` stays in selected profile directories. It is not a receipt;
  never unlink it while participating writers may exist. Stages are sibling files
  named by process/index. Crash leftovers are retained, never blindly adopted or
  swept; recovery can use its own new stage and retained state. Private state
  stages use a process-qualified name. PID reuse encountering an old stage is a
  refusal requiring operator reconciliation, not permission to delete it.
* Recovery preserves the current safe mode/gid, including a later unrelated chmod.
  It removes only the exact added Claude import line. When CURRENT is exactly the
  installed text, exact original text can be restored; otherwise bridge newlines
  remain where needed to avoid joining later unrelated text to an original line.
* `ap doctor inspect` contains no subprocess/network implementation and never
  opens native settings. It reports native_loading=unverified and p6_complete=false.
  Format checks here concern only explicit selected public instruction files.

Dependencies are pinned official crates: rustix=1.1.2 (safe openat/statat/readlinkat,
NOFOLLOW traversal, rename/unlink/fsync/fchown and UID), sha2=0.10.9 (SHA256), plus
S02's unchanged serde=1.0.228/serde_json=1.0.150. Native Cargo resolution/fetch only;
no system/toolchain install. Existing locked package versions, including
unicode-ident=1.0.24, are preserved. Cargo.lock pins all new transitive archives.
Official API source inspected from native Cargo's rustix-1.1.2 source at
`/root/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustix-1.1.2/src/fs/`;
reference links: https://docs.rs/rustix/1.1.2/rustix/fs/index.html and
https://doc.rust-lang.org/1.95.0/std/fs/struct.File.html#method.try_lock .

Wonder should challenge the exact candidate, the explicit coordination precondition,
ambiguous prepared states, restartable recovery, command-directory reuse, source
and ancestor races, unknown settings preservation and each false-success boundary.
Any oracle correction needs an additive version and an independent contract reason.
