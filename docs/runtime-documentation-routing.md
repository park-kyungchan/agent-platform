# Runtime documentation routing

The common Runtime Index selects official documentation for an explicit subject and topic. Its 98 routes preserve the 86 previously registered routes, 28 capture summaries and original artifact hashes. The Hermes Client/Backend additions supply direct topic routes with separate, limited evidence. Capture bodies stay with Scrapling/Evidence Foundry. Account snapshots, host paths and execution admission are excluded from the portable registry.

## Hermes Client and Backend: short operating instruction

Keep this instruction with the consumer instead of copying a topic list or the full corpus into each prompt:

```text
Use the common Runtime Index for Hermes documentation (surface: hermes.agent).
Select the topic first, read the returned official page's relevant section,
and use bounded index discovery only when the route or section is insufficient.
Observe Desktop Client and Backend versions separately before installed-version
questions. Reuse evidence only for the same canonical page, body digest and
applicable version; revalidate current/latest or conflicting evidence.
```

For example, ask: "Use installed-version documentation for Hermes remote-gateway; check Client and Backend versions separately." The existing reader returns the page, evidence gaps and retrieval budget. It does not fetch pages or inspect either machine.

| Question                                 | `topic`                                | Official pageId                       |
| ---------------------------------------- | -------------------------------------- | ------------------------------------- |
| Desktop application                      | `desktop-client` (`desktop`, `client`) | `user-guide/desktop`                  |
| Desktop remote connection                | `remote-gateway` (`remote-backend`)    | `user-guide/multi-connection-desktop` |
| Bot roster and profile operation         | `bot-mode` (`bot`, `bots`)             | `user-guide/bot-mode`                 |
| Screen viewer, control and hand-back     | `bot-screen` (`screen`)                | `user-guide/features/bot-screen`      |
| Native computer-use tools                | `computer-use`                         | `user-guide/features/computer-use`    |
| Distribution updates and versions        | `updating` (`update`, `version`)       | `getting-started/updating`            |
| Installation and distribution            | `installation` (`install`)             | `getting-started/installation`        |
| Platform support matrix                  | `platform-support`                     | `getting-started/platform-support`    |
| Windows native runtime                   | `windows-native` (`windows`)           | `user-guide/windows-native`           |
| WSL2 runtime                             | `windows-wsl` (`wsl2`)                 | `user-guide/windows-wsl-quickstart`   |
| Dashboard and connection troubleshooting | `web-dashboard` (`dashboard`)          | `user-guide/features/web-dashboard`   |
| Multiple profile gateways                | `multi-profile-gateways`               | `user-guide/multi-profile-gateways`   |

Existing `gateway`, `docker`, `tui`, `profiles` and `sessions` routes retain their meanings and aliases. `gateway` still selects developer internals. Start with `remote-gateway` for a Desktop connection question.

Use a separate request for each component. A Windows Client request can use `topic: "desktop-client"` and `executionHost: "local.windows"`; a Docker Backend question can use its relevant topic and `executionHost: "hostinger.container"`. An updater running in WSL2 does not establish that Desktop itself runs in WSL2. Bot Screen placement is another runtime observation.

Save this JSON as a request file and run the existing public CLI shown below:

```json
{
  "request": {
    "surface": "hermes.agent",
    "topic": "bot-screen",
    "freshness": "installed",
    "executionHost": "hostinger.container"
  }
}
```

After observing that component's installed version, supply it as `observations.installedVersion` in the envelope. A planned update to `0.22.2` is not an installed-version observation. The API accepts one applicable version per request; it does not authenticate or combine a Client/Backend version pair. Current public pages are not patch-version compatibility evidence, and these routes deliberately omit `documentationVersion`.

The old `bun run docs status/find/sections/read` instructions refer to a separate historical reader project. Use that reader only if its owning source, cache and entrypoint are present and verified. This checkout uses `bun --no-env-file --no-install scripts/harness.ts docs <request.json>` and the API below. It does not restore or refresh the historical corpus cache.

An unchanged index digest does not establish unchanged page or corpus bytes. The user's October 7 cache and October 13 document counts remain dated observations. Keep corpus byte offsets paired with the original corpus hash; use a selected current official page when freshness or coverage requires it. Do not fetch the entire corpus for a known topic.

The October 18 additions establish official-index membership and a limited Desktop page read. They preserve the earlier sources and capture summaries without promoting their dates or evidence status. Actual selected content, deployed source, effective configuration, callable tools, authentication and observed UI behavior require their own evidence.

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

## Hermes Bot Mode and Screen

Use `hermes.agent` topic `bot-mode` for the official [Bot Mode page](https://hermes-agent.nousresearch.com/docs/user-guide/bot-mode), and `bot-screen` for the official [Bot Screen page](https://hermes-agent.nousresearch.com/docs/user-guide/features/bot-screen). Screen aliases include `screen`, `bot-desktop`, `open-screen` and `봇화면`. Both routes carry partial-page evidence, require revalidation and leave `documentationVersion` unverified. The October18 discovery recorded four URL opens over two unique pages: two initial opens and two follow-up opens, with no discovery search. That history is not a claim that all four opens fit one max3-page operation; new retrievals retain the reader's existing per-operation limits. No new complete capture or original body hash is admitted.

The separately captured October18 pre-update CLI/runtime/lease source distinguishes viewing the native Desktop Screen pane from starting its backend. The inspected CLI supports `computer-use screen status --json`, while `computer-use screen start` has no JSON flag. The backend owns a profile-scoped Xfce/Xvnc desktop and its actual placement; the human/agent lease controls who may drive it. Native runtime, tools, Gateway/Desktop authentication and mutable profile state retain their existing owners. Documentation selection, backend status and a backend screenshot do not establish authenticated Windows client rendering, useful visual E2E or permission to act. Those require the actual selected native client/profile/lease and correlated result observations. No private Gateway URLs, viewer capabilities or account/profile identifiers belong in this registry.
