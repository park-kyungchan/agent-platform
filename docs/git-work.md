# Common Git Work

Historical generic Work procedures and review protocol are retained below for source interpretation. Current task state and the adopted Rust migration plan govern new work; these sections do not authorize the retired controller or allocate new generic records. The separate native Memory stdin caller at the end remains under its native owner hold.

GitHub is the canonical publication source for common Work, decisions and relationships. A Git commit identifies source content. Domain, Project, Work, Issue, Spec, Plan and Task IDs retain their logical meaning across branches and hosts.

Start a new Work after fetching and synchronizing current main through native Git. Create its branch/worktree from that revision. Continue an existing Work on its published branch with the same IDs. Independent, separately reviewable Tasks may use separate branches/worktrees/PRs; group small dependent changes. The owning Lead explicitly merges each reviewed PR. Automatic merge is prohibited.

## Files and source identity

Each Work lives under `work/<project-key>/<work-key>/work.json`, with separate `specs`, `plans`, `tasks`, `decisions`, `relations`, `effects`, historical `fences`, `evidence` and `inputs` files. Filenames encode stable IDs; the Work manifest does not enumerate every Task. Independent Task creation therefore avoids a shared mutable Task index.

Spec and Plan contracts use `specs/<encoded-id>@<revision>.json` and `plans/<encoded-id>@<revision>.json`. References carry both the stable ID and semantic revision. Add a new revision file and update the current Work pointer when requirements or the Plan changes; retain older contract versions and the original revision pins of historical Tasks. This preserves the meaning of an old completed Task while new Tasks use the new contract. Git continues to own ordinary file history; no separate checkpoint/CAS version store is introduced.

Same-snapshot references store a relative path and optional exact JSON fragment/hash. The reader resolves these under the caller-selected repository and commit, returning commit, path, blob and content receipts. External Git references pin an existing commit. Native/original-owner references retain their locator and receipt. A source file never embeds the SHA of the commit that will contain it.

The reader uses local committed Git objects, bounded regular files and explicit repository mappings. It does not fetch, reset, rebase or merge. Dirty worktree bytes are distinct from committed source. Repository IDs and owner receipts are caller-supplied declarations; native owners still authenticate authorship, custody and permissions.

## Historical reader facade

The AP root `harness` command and exported `runHarness` facade are retired. The exact [predecessor source](https://github.com/park-kyungchan/agent-platform/blob/01fe80a5a40bdca30aa8ab7c954bbac33d58e2f2/scripts/harness.ts) records the former snapshot, context, transition, source and start request contracts. Its recipes are historical, not current commands. Existing Git helpers remain exported directly from `packages/work-continuity/src/git-work/index.ts`; `scripts/verify-common-work.ts` imports them there. The retained verifier reads the delivered committed Work and shared documentation pins without activating a native runtime. Other Work check aliases remain pending their own consumer closure.

## Decisions, effects and completion

An actual decision binds its answer and selection context to original source fragments. A proposal or changed answer text is rejected. Source integrity does not establish native user authentication. Original captures retain their source owner and their content-capture limitation.

Task execution, verification, local durability, shared delivery and formal completion are separate axes. Source-only preparation/review Tasks use source-bound passing Verification artifacts and preserve their logical IDs; they do not invent native occurrence receipts. Native-effect prerequisites retain exact producer/occurrence/payload joins, and a different runtime needs its actual receiver receipt. A verified local prerequisite can support downstream work while shared delivery is pending. Neither a source commit nor a merge establishes native execution or Acceptance.

Use `taskVerificationTarget(task)` for a Verification's `targetRefs`. It retains an existing native-owner Task artifact ID. Otherwise it derives a Work-qualified target from Domain/Project/Work/Task identity, so a repeated short Task name in another Work cannot reuse the verification. Logical Task IDs remain unchanged.

Completed effects cannot be replayed; uncertain effects require observation before retry. Historical fences retain old IDs without inventing native attempts or occurrence receipts. The original whole Work still requires both-runtime behavior and separate Acceptance. The common-source milestone delivers portable source and leaves runtime adoption to each Lead.

## Review and runtime handoff

Run `bun run check:handoff`, `bun run check:imports`, `bun run check:git-work`, `bun run check:source` and `bun run check:quality`. Commit/freeze and push the candidate. An owner-created draft PR may be opened solely to establish authenticated provenance before independent review. Publish genuine independent exact-commit/tree evidence and its authenticated status, and pass CI and current finding/thread gates before ready-for-review. A changed tree requires renewed review. The Lead checks current head/base/findings and required checks, then explicitly merges the expected head.

A merge SHA may reuse a review only with proven tree equality, an explicit independent successor binding and current zero unresolved findings. Publish the new exact-main evidence/status; explicitly rerun a review job if status arrival raced. No review-arrival observer is needed.

Each WSL2/VPS Lead subsequently pulls completed main and follows the shared Runtime Index for relevant native documentation. Native verification and explicit activation are separate. Existing legacy handoff/checkpoint APIs and native owners remain available during caller migration.

## Composed bounded context

`readGitWorkContext` retains its raw 24,000-character limit. A raw diagnostic projection can exceed that limit even when the required compact representation fits. `readCompactGitWorkContext(reader, request, selection)` runs the same canonical source and dependency validation pipeline, then the same compact codec, before checking the selected output budget. It does not accept an externally supplied qualified projection. The standalone `compactGitWorkContext` keeps its 24,000-character input guard.

An explicit fourth argument, `"metadata.v1"`, enables lossless metadata packing in the existing compact representation. The normative string table is unchanged. Fixed field maps and fragments use closed enums, full Git OIDs use canonical base64url, and repeated metadata prefixes are carried in a bounded dictionary. Header metadata stays literal. The packet names its canonical `decodeCompactGitWorkContext` entrypoint; decode before interpreting tuples, identifiers, originals or states. The decoder expands metadata and runs the existing strict structural and semantic validator. Older ordinary compact packets continue to decode unchanged. Packing grants no native authorship, permission or Acceptance.

Native adoption is explicit: Codex bindings use `gitWorkContextVersion: "v2"` with `gitWorkMetadataEncoding: "metadata.v1"`; Hermes v2 options/CLI requests use `metadataEncoding: "metadata.v1"`. Missing composed provider support holds instead of falling back. Existing callers that omit the option retain their prior v1/v2 representation. Both adapters still check the complete native envelope against their existing 12,000-character v2 cap; source compaction alone is not delivery. The 16,000-character continuity-checkpoint limit is a separate policy. Unfittable required context remains held rather than losing purpose, bound Plan revisions, originals, dependencies or effect fences.

Source changes require the common registry and affected owner registration/build/dependency pins, independent exact review, explicit merge and separate native-owner activation. Installing or merging the new codec does not opt a native owner into it.

## Native Memory stdin caller

The Hermes platform source compiler also emits `read-memory.js`. Send a JSON object on stdin with exactly `snapshot`, `requestRef` and `ownerSelection`. `snapshot` explicitly supplies `repositoryRoot`, `repositoryId`, an immutable `commit`, `projectId` and `workId`. `requestRef` is a full SHA-256 snapshot Input pin with `fragment: "#/content"`. The current owner witness contains `owningRef` (a literal `refs/heads/` branch), `admittedCommit`, `policyRevision`, `activeGenerationId` and `state: "current"`. The canonical `readGitWorkMemoryRequest` reads that Work's request/dataset/policy Inputs and their original closure; the CLI carries no per-instance policy.

A source registration and compiler capture include the complete canonical Memory/context transitive imports and their installed projections. The CLI verifies that fixed closure before importing the implementation and again after the query, before publishing output. Stale owner refs, generations, policies or source pins fail without printing input or original bytes. The four existing compiled entrypoints retain their wire contracts. This new entrypoint is a native-tool-invocable reader, not a new tool registry or Memory implementation.

The canonical output preserves source-attributed inference versus current authoritative records, owner-declared review limitations and native-permission refusal. Query success, compiler/source proof and Memory eligibility are separate from native adoption, authenticated authorship, runtime permission and Acceptance. Each runtime owner must explicitly activate the reviewed successor; this source does not select a real Work generation or grant a Memory query.
