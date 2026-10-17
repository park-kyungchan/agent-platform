# Runtime documentation routing

The common Runtime Index selects official documentation for an explicit subject and topic. Its 86 routes preserve the 84 historical routes, 28 capture summaries and original artifact hashes. Hermes TUI and slash-command routes add separate bounded review evidence without claiming complete body review or native permission. Capture bodies stay with Scrapling/Evidence Foundry. Account snapshots, host paths and execution admission are excluded from the portable registry.

```ts
import { readDocumentationRegistry } from "@palantirkc/work-continuity/runtime-docs";

const reader = await readDocumentationRegistry();
const plan = reader.select({
  surface: "codex.cli",
  topic: "hooks",
  freshness: "installed",
  executionHost: "local.wsl",
});
```

`readDocumentationRegistry(path?)` reads one bounded JSON data file and returns a reader. `createDocumentationReader(registry)` accepts explicit data instead. Both clone, validate and deeply freeze the registry once per instance. Subsequent `select(request, observations?)` calls use indexed aliases without validating the registry again. `registrySha256` identifies the validated JSON snapshot serialized by the reader; the source manifest separately pins original file bytes.

Requests allow only `surface`, `topic`, `freshness`, `executionHost` and `targetVersion`. Runtime authority, approval, authentication, billing, effect and intent fields are rejected. The subject surface remains independent of its execution host. A host can add at most two documentation companions. Missing subjects/topics return bounded discovery; selection never fetches a page, installs software or holds native execution. Every plan declares `executionAuthority: "none"` and `nativePermissionGranted: false`.

Optional observations supply an installed version and cached-page evidence for documentation planning. These are caller assertions, not authenticated host observations. Cache reuse requires an exact canonical page, applicable version and matching independently confirmed body digest. Sensitive/latest reads additionally require current-task revalidation. A missing/conflicting installed version or documentation-version mismatch produces an explicit evidence gap and retrieval direction. An SDK documentation version does not prove compatibility with an installed runtime patch release.

## Public CLI transport

From the repository root, `bun --no-env-file --no-install scripts/harness.ts docs <request.json>` accepts every existing flat `DocumentationRequest` unchanged. It also accepts an explicit `{ "request": { ... }, "observations": { ... } }` envelope; `observations` may be omitted or `{}`. For example, this executable shell snippet creates and removes a disposable request file:

```sh
request=$(mktemp)
trap 'rm -f "$request"' EXIT
printf '%s\n' '{"request":{"surface":"hermes.agent","topic":"tui","freshness":"installed"},"observations":{"installedVersion":"0.22.0"}}' > "$request"
bun --no-env-file --no-install scripts/harness.ts docs "$request"
```

The envelope allows only `request` and optional `observations`. The request retains the canonical reader's existing validation. Observations allow only optional `installedVersion` and `cache`. A supplied cache requires string `canonicalUrl`, `bodySha256`, `independentlyConfirmedBodySha256` and boolean `revalidatedInCurrentTask`, with optional string `targetVersion`. Observation strings must be nonblank and contain no C0/DEL control characters; their maximum lengths are 120 UTF-16 code units for versions, 2000 for the URL and 64 for each digest field. The whole JSON file retains its 1,048,576-byte limit. Nulls, arrays, wrong scalar types, unknown fields and oversized values are rejected with exit code 1 and no plan on stdout.

The CLI passes validated observations unchanged to the sole canonical `reader.select(request, observations)`. It does not probe an installed runtime, authenticate a host or cache, fetch documentation, store cache data or grant permission. Canonical page/version/digest matching and current-task revalidation still decide reuse; an asserted revalidation flag or matching strings alone are not independent proof. The example's `0.22.0` is a caller claim, not installation/authentication/compatibility evidence. Partial page evidence and all page/discovery/excerpt limits remain intact, as do `executionAuthority: "none"` and `nativePermissionGranted: false`.

The reader accepts only enumerated official HTTPS hosts with surface-specific restrictions. Preferred/fallback URLs must represent the same canonical document, including the GitHub body API's exact single `pathname` query. Unicode aliases normalize to NFC. Registry changes require a new reader instance and source/version readback.

`architecture/runtime-documentation-routing/host-pointer-recipe.json` is a path-neutral recipe. A later WSL2/VPS Lead binds its verified checkout, native reader and installed version. This common change does not publish a live host pointer or modify an active runtime. Documented capability, installed version, effective configuration, callable tools, authentication and verified behavior remain distinct evidence.
