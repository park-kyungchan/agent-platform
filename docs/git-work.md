# Common Git Work

GitHub is the canonical publication source for common Work, decisions and relationships. A Git commit identifies source content. Domain, Project, Work, Issue, Spec, Plan and Task IDs retain their logical meaning across branches and hosts.

Start a new Work after fetching and synchronizing current main through native Git. Create its branch/worktree from that revision. Continue an existing Work on its published branch with the same IDs. Independent, separately reviewable Tasks may use separate branches/worktrees/PRs; group small dependent changes. The owning Lead explicitly merges each reviewed PR. Automatic merge is prohibited.

## Files and source identity

Each Work lives under `work/<project-key>/<work-key>/work.json`, with separate `specs`, `plans`, `tasks`, `decisions`, `relations`, `effects`, historical `fences`, `evidence` and `inputs` files. Filenames encode stable IDs; the Work manifest does not enumerate every Task. Independent Task creation therefore avoids a shared mutable Task index.

Spec and Plan contracts use `specs/<encoded-id>@<revision>.json` and `plans/<encoded-id>@<revision>.json`. References carry both the stable ID and semantic revision. Add a new revision file and update the current Work pointer when requirements or the Plan changes; retain older contract versions and the original revision pins of historical Tasks. This preserves the meaning of an old completed Task while new Tasks use the new contract. Git continues to own ordinary file history; no separate checkpoint/CAS version store is introduced.

Same-snapshot references store a relative path and optional exact JSON fragment/hash. The reader resolves these under the caller-selected repository and commit, returning commit, path, blob and content receipts. External Git references pin an existing commit. Native/original-owner references retain their locator and receipt. A source file never embeds the SHA of the commit that will contain it.

The reader uses local committed Git objects, bounded regular files and explicit repository mappings. It does not fetch, reset, rebase or merge. Dirty worktree bytes are distinct from committed source. Repository IDs and owner receipts are caller-supplied declarations; native owners still authenticate authorship, custody and permissions.

## Reader commands

Install the pinned workspace dependencies with `bun install --frozen-lockfile --ignore-scripts`. A snapshot request contains:

```json
{
  "repositoryRoot": ".",
  "repositoryId": "park-kyungchan/agent-platform",
  "commit": "HEAD",
  "projectId": "codex-harness-engineering",
  "workId": "ontology-driven-harness-operation"
}
```

Save it outside the canonical Work as `snapshot.json`, then run `bun run harness validate snapshot.json`. The CLI resolves the selected local ref once; the public reader requires its full immutable commit SHA. `bun run check:common-work` validates and reads the delivered real Work from exact HEAD. This works from another checkout path and does not register or activate a native runtime.

For context, use a request with `snapshot` containing the selection above and `context` containing `taskId`, `latestRelevant`, `maxChars` and `maxRecords`. Run `bun run harness context context-request.json`. The native caller observes relevant source currentness; `latestRelevant: "unknown"` keeps dependent qualification held. Missing unrelated historical inputs do not hold an independent selected Task. Bounds produce explicit omissions/holds.

`bun run harness transition before.json after.json` checks two snapshots before integration. It rejects lost actual decisions/effect fences, invalid supersession, dependency cycles and invalid completion even when a textual merge succeeds. `bun run harness source snapshot.json <relative-path>` returns exact source receipts. `bun run harness start start-request.json` qualifies a native-owner-provided fresh-main observation or same-Work continuation; it performs no Git mutation.

## Decisions, effects and completion

An actual decision binds its answer and selection context to original source fragments. A proposal or changed answer text is rejected. Source integrity does not establish native user authentication. Original captures retain their source owner and their content-capture limitation.

Task execution, verification, local durability, shared delivery and formal completion are separate axes. Source-only preparation/review Tasks use source-bound passing Verification artifacts and preserve their logical IDs; they do not invent native occurrence receipts. Native-effect prerequisites retain exact producer/occurrence/payload joins, and a different runtime needs its actual receiver receipt. A verified local prerequisite can support downstream work while shared delivery is pending. Neither a source commit nor a merge establishes native execution or Acceptance.

Use `taskVerificationTarget(task)` for a Verification's `targetRefs`. It retains an existing native-owner Task artifact ID. Otherwise it derives a Work-qualified target from Domain/Project/Work/Task identity, so a repeated short Task name in another Work cannot reuse the verification. Logical Task IDs remain unchanged.

Completed effects cannot be replayed; uncertain effects require observation before retry. Historical fences retain old IDs without inventing native attempts or occurrence receipts. The original whole Work still requires both-runtime behavior and separate Acceptance. The common-source milestone delivers portable source and leaves runtime adoption to each Lead.

## Review and runtime handoff

Run `bun run check:handoff`, `bun run check:imports`, `bun run check:git-work`, `bun run check:source` and `bun run check:quality`. Commit/freeze the candidate, obtain independent exact-commit/tree review, then push and publish immutable review evidence/status before opening the PR. A changed tree requires renewed review. The Lead checks current head/base/findings and required checks, then explicitly merges the expected head.

A merge SHA may reuse a review only with proven tree equality, an explicit independent successor binding and current zero unresolved findings. Publish the new exact-main evidence/status; explicitly rerun a review job if status arrival raced. No review-arrival observer is needed.

Each WSL2/VPS Lead subsequently pulls completed main and follows the shared Runtime Index for relevant native documentation. Native verification and explicit activation are separate. Existing legacy handoff/checkpoint APIs and native owners remain available during caller migration.
