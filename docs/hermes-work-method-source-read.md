# Hermes work-method source reads

The optional `hermes.work-method-source-request.v1` route uses the existing compiled
`read-work-method.js` entrypoint. The legacy owner request/response and its complete
6,000/12,000 character cap retain their behavior. Source delivery never establishes
native authentication, received/used, owner admission, permission or Acceptance.

The source owner supplies `workMethodSourceBinding` to the existing finite builder:
`{dataBase, runtimeMapPath, mapSha256}`. The map path must be beneath the exact fixed
data base. Those values are compiled constants, never request-selected paths or
executable registration. Absence holds the new route. Existing active registrations,
native hook bindings and trust are not changed by a build.

The generated UTF-8 map (compact or two-space JSON plus newline) has schema
`hermes.work-method-source-map.v1`, exact `logical` Task, `actorId`, `routeId`,
`native` Hermes tuple, `cwd`, and `entries: [{id,path,sha256,bytes}]`. Optional
`provenance` retains complete original Git/raw/fragment mappings. Paths are relative
to the compiled data base. Files must be regular, and no file or parent directory may be a
symlink. The map/files are capped at 1 MiB, entries at 2,048 and declared total at
32 MiB. Native session identity must be genuinely observed before the owner creates
this immutable selection; missing values cannot be filled with logical IDs.

A closed request contains `schema`, identity-only `event`, `selection` with
`{cwd,native,actorId,routeId,logical}`, `sourceMapSha256`, `target`, `cursor`,
`outputVersion: "v2"` and optional `maxChars` (1,024..12,000). The native event and
selection must exactly match the owner map. Targets are:

- `{kind:"mapped-source",id}` reads one opaque map ID; `@map` reads the pinned map.
- `{kind:"validated-owner-document",request}` takes the complete existing owner
  request. It shares its exact native/common validation and admission evaluation,
  including the packet byte cap. The source document preserves all standing routes,
  packet dependencies, receipt, decision and native unknowns. The legacy whole
  response remains held when oversized; this is a distinct source representation.

`cursor` is `{offsetBytes,expectedDocumentSha256}`. Only offset zero may use a null
digest. Continuations must carry the previous full-document digest. Byte offsets
must be UTF-8 scalar boundaries. Every reply includes full-document SHA256/byte
length, current/next offsets and `last`, and fits the complete serialized JSON cap,
including metadata and escaping. Text is contiguous and strict UTF-8; no ellipsis,
meaning clipping or split surrogate/multibyte sequence is allowed. A metadata-only
reply that cannot fit is held. Map/file/source drift rejects the result with closed
diagnostics. No paginator/session state is stored.

Receivers must cover every required source byte, verify the complete reassembled
hash and original/selected mapping, and actually read/use the Task, whole WHY,
current user scope, fences and producer/receiver prerequisite evidence. Only actual
owner observations may later form a common receipt/admission. Transport success or
`source-qualified` does not supply that evidence. The default native hook reads
current Work context; a source request does not silently activate WorkMethod injection.

Any application needs independently reviewed source/build/registration/compiler
and dependency successors, a freshly pinned pending install Task and native-owner
verification. Earlier host repair proof remains bound to its original source.

## Stable session source selection

When a future turn does not exist yet, the explicit
`hermes.work-method-session-source-map.v1` map selects source using
`scope: {kind:"session-source",runtime:"native-hermes",profileId,sessionId}`,
`logical`, `actorId`, `routeId`, `entries` and optional passive `provenance`.
Profile and session must be genuinely observed non-null identities. The closed map
has no event, cwd, future turn, packet template, runtime slots or receipt state.
The same compiled binding, no-follow reads, file limits, UTF-8 chunks and complete
output cap apply. This map and the original full-tuple map cannot substitute for
each other.

The closed `hermes.work-method-session-source-request.v1` contains `scope`,
`logical`, `actorId`, `routeId`, `sourceMapSha256`, `target`, `cursor`,
`outputVersion:"v2"` and optional `maxChars`. A mapped-source target reads only
the selected original or `@map`, without inventing a native event. Delivery keeps
all authentication, use, admission, permission and execution flags false.

For `{kind:"validated-owner-document",event,request}`, `event` is an actual
identity-only observation and must exactly equal the complete existing owner
request's event. Its profile/session must match the stable map. The unchanged
owner evaluator still joins the full event, cwd, selection, actor, packet,
receipt, current event, prerequisites and Common admission facts. Null remains an
exact value. A valid held Common decision stays held inside the source document;
source delivery does not change it.

The evidence flow is acyclic: actual event/source prerequisites produce receiver
use U0; the current packet Q references U0; actual complete Q read/use produces U1;
receipt R references U1 and Q's digest. Q does not reference U1 or R, and R does
not reference its own hash. The owner then supplies current source/admission facts
to the existing evaluator. A changed derived document rejects a continuation's
old digest. None of these later observations retroactively admits earlier session
creation, installation or source-read effects. Those actions require their own
exact native owner, Task, source and effect gates.

## Explicit real-callback identity projection

The existing native hook has a separate opt-in bindings schema,
`hermes.real-callback-source-hook-bindings.v1`, with exactly these fields:

```json
{
  "schema": "hermes.real-callback-source-hook-bindings.v1",
  "kind": "stableSessionSource",
  "shellSelection": {
    "profileName": "observed-shell-name",
    "sessionId": "observed-session",
    "cwd": "/exact/observed/cwd"
  },
  "sourceSelection": {
    "scope": {
      "kind": "session-source",
      "runtime": "native-hermes",
      "profileId": "declared-canonical-profile",
      "sessionId": "observed-session"
    },
    "logical": {
      "domainId": "selected-domain",
      "projectId": "selected-project",
      "workId": "selected-work",
      "taskId": "selected-task"
    },
    "actorId": "selected-actor",
    "routeId": "selected-route"
  }
}
```

These illustrative values are not native observations. The source owner must pin this
file through existing `currentWorkBindings`, also supply the existing compiled
`workMethodSourceBinding`, and explicitly select output v2. The v2 budget is
1,024..12,000 characters; `metadata.v1` is a different representation and is not
accepted for this projection. Both compiled provenance hashes and the closed map
binding must be valid. A map alone does not enable the branch. The original
`hermes.current-git-work-hook-bindings.v1` exact tuple/null/default behavior stays
unchanged, with no fallback from a failed legacy match.

For `pre_llm_call`, the direct API validates the closed binding and observed identity,
then matches the shell profile **name**, session and cwd before reading a map or Git.
Valid foreign selectors return `{}`. The compiled entry still performs its existing
registration/source/bindings guards before importing implementation code. The new
branch copies only `hook_event_name`, `profile`, `session_id`, `cwd`, and
`extra.{agent_id,project_id,task_id,run_id,turn_id}`. Missing optional IDs stay null;
observed Task, Run and turn remain distinct. Prompt, history, tool input, provider,
credential and native-store contents are not projected. The unchanged 1 MiB raw
request limit can safely hold an oversized callback instead of recording it.

The existing session-map API validates the declared source selection using `@map`;
its bounded chunk is discarded. No complete map/original receive-use is claimed.
Source/bindings and the exact map are checked again immediately before returning
`{context: JSON.stringify(inner)}`. The inner object has schema
`hermes.real-callback-identity-context.v1`, kind, declared logical/actor/route,
`wireEvent`, `declaredSourceScope`, and three provenance hashes. Both the inner JSON
and the complete serialized outer response must fit the selected budget, including
escaping. Overflow holds rather than clipping or raising the cap. Refusals retain
closed diagnostics without echoing private inputs.

`nativeInvocationVerified`, `ownerProfileMappingVerified`,
`nativeAuthenticationEstablished`, `receivedOrUsedEstablishedByProjection`,
`ownerAdmissionEstablished`, `nativePermissionGranted`, and
`executionAuthorityGranted` are always false. A compiled test or caller-supplied JSON
does not authenticate a native invocation. In particular, the shell profile name and
cwd are separate from the declared canonical gateway profile/policy cwd; no hidden
coercion or correspondence proof is supplied. An actual native owner must separately
verify the mapping, loaded source, genuine invocation and current identities before
E -> U0 -> Q -> U1 -> R and full owner admission. The existing full-event evaluator
continues to require literal joins and holds differing identity domains. Building
this capability does not install, configure, trust, invoke or adopt it in a runtime.

## Finite namespace delivery

`platform-current-work/delivery.ts` adds `deliverHermesNamespace(contract, payload)`
for one initial source delivery. The existing fourteen-file facade delivery API and
its consumed operations retain their contracts. This helper creates no native
session, agent loop, provider request, scheduler or native store.

The source owner compiles an exact `NamespaceDeliveryContract` into the small
`native-delivery.ts` entrypoint using `HERMES_NAMESPACE_DELIVERY_CONTRACT`. Unbound
entrypoints hold before reading stdin or touching a target. There are no runtime
target/command arguments: stdin contains only a closed finite payload. The common
algorithm has no instance-specific namespace. The compiled owner binding selects
the operation/Task label, fixed namespace, exact existing parent device/inode/mode/
UID/GID, current executable path/bytes/hash, protected legacy manifest path/bytes/
hash, reviewed payload digest, layout and artifact write ceiling. These are source
bindings, never authentication or native permission.

The owner constructs payload files from the existing guarded
`buildHermesPlatformSource` result and captured original source/dependency bytes:
the rebased `nativeRegistration`, every registered source/dependency file, exactly
six named bundles, selected immutable DATA, and explicitly listed resolver aliases.
Each regular file has canonical base64 bytes, SHA256, byte count and mode0600/0644.
Aliases can target only declared dependency roots inside the new namespace; they
cannot be parents of delivered artifacts or escape through another alias. Source
commit, host/runtime registration hashes, compiler hash and each build bundle pin
remain explicit provenance. Source-only provenance is not a native receipt.

Limits are 4,096 files, 256 aliases, 8,192 derived directories, 8MiB per file,
64MiB decoded total, 96MiB stdin and 16MiB metadata journal. Existing source-reader
and compiler limits are unchanged. The helper checks the exact runtime registration
layout, complete registered payload bytes and bundle names before its first write.
Every path must round-trip as UTF-8, each encoded component is at most255 bytes,
namespace-relative paths at most1,024 bytes, compiled absolute pins at most2,048
bytes and combined artifact paths at most4,095 bytes. Full intent/completion event
bytes and bounded terminal/failure metadata must fit the unchanged16MiB journal
before the namespace can be claimed. Invalid inputs cannot consume the once fence
by overflowing a known filesystem or journal bound during delivery.
It never downloads dependencies, invokes a package manager, executes payload code
or accepts an executable callback.

The executable pathname alone cannot prove which image is running. The helper
intentionally opens the single literal `/proc/self/exe` magic link, hashes the held
regular-file descriptor, checks stable image metadata and keeps it held through
delivery. This loaded image must match the compiler pin as well as the named path.
An owner-bit-masking umask is refused before mkdir; the helper never changes the
process umask. Every newly created directory is verified as owned mode0700 through
held no-follow descriptors without a path-following chmod.

The first mutation is an exclusive namespace mkdir under the verified held parent.
The first file is an exclusive durable source-owned claim; it is never removed or
reclaimed. Every artifact is created through held no-follow directory descriptors
and `O_EXCL`, with fsync and exact readback. The legacy manifest is hashed without
parsing its contents and rechecked during delivery. Gateway/control locks, active
manifests, profile/config/trust and existing projections are not written. Existing
namespace/claim means reconciliation, including an identical previous attempt;
there is no automatic resume, replacement, retry or rollback.

Intent/completion records are fsynced into a bounded metadata-only journal. An
immutable receipt snapshot is attempted independently after artifact work, even
when journal verification fails. It has `terminalState: pending`, a null self-pin
and, when verified, a `journal.scope: verified-prefix` pin covering only the first
`bytes` of the journal. That prefix remains valid when a terminal is later appended;
it never claims to hash the final journal or itself. After the receipt write and
readback outcome is known, the journal terminal records that outcome and pins the
receipt bytes one-way. Receipt allocation/write/readback failures therefore produce
a partial terminal even if stdout is lost. The returned verified whole-journal pin
has `scope: terminal` and `terminalState: confirmed`; these are not retroactively
written into the immutable snapshot. Missing/failed terminal persistence leaves
pending/unknown reconciliation, never a completed delivery inferred from the
snapshot alone. The terminal scope covers artifact and receipt outcome, not later
descriptor teardown, native transport or separate owner verification. A returned
`persistenceFailure` or `uncertain` path requires the existing owner to reconcile the
exact native tool outcome and on-disk evidence; storage failure cannot be turned
into a durable-success claim. The caller must retain the returned metadata even
when native transport or post-operation checks fail. Operation/Task labels and
these claim/journal files are source-owned one-shot work records; native Task/Run/
event/attempt IDs remain null until the native owner supplies genuine receipts.

Delivery returns `delivered-awaiting-owner-verification`. The required separate
owner post-step uses the maintained `observeHermesPlatformSource` with the fixed
runtime provider/registration paths and returned registration digest, then calls
`verify()`. This establishes the resolver/importer/immutable Ontology0.2.0 closure
without helper-selected execution. The owner records that actual verification
separately; byte delivery alone does not pass it.

This new helper must be independently source/build pinned. An older53-source
registration does not cover these new files. No active owner registration is edited
by adding the helper. Optional current-work/source bindings must be omitted from a
build when absent, never passed as null. Actual shell correspondence, both bindings,
approved invocation, hook trust/activation, first read/use and coverage remain under
their existing native/source owners; delivery supplies none of those observations.
