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

The reader accepts only enumerated official HTTPS hosts with surface-specific restrictions. Preferred/fallback URLs must represent the same canonical document, including the GitHub body API's exact single `pathname` query. Unicode aliases normalize to NFC. Registry changes require a new reader instance and source/version readback.

`architecture/runtime-documentation-routing/host-pointer-recipe.json` is a path-neutral recipe. A later WSL2/VPS Lead binds its verified checkout, native reader and installed version. This common change does not publish a live host pointer or modify an active runtime. Documented capability, installed version, effective configuration, callable tools, authentication and verified behavior remain distinct evidence.
