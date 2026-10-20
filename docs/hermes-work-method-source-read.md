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
