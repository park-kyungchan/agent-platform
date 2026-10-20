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
`source-qualified` does not supply that evidence. The current native hook reads
current Work context; this route does not silently activate WorkMethod injection.

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
