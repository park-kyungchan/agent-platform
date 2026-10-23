# Agent platform

This Bun workspace owns admitted reusable implementations. Domain semantics and native execution remain explicit in [the ownership registry](architecture/capability-source-owners.json).

| Package                   | Canonical code                                                     | Native and data owner                                             |
| ------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| work-continuity           | Handoff, context, Memory, originals and recovery                   | codex-harness-engineering                                         |
| ontology                  | Ontology schemas, contracts and generators                         | ontology-foundation release adapter; immutable releases preserved |
| evidence-foundry          | Evidence schemas, storage/evaluation logic and explicit-policy API | evidence-foundry CLI, relative policy and stores                  |
| research-capture          | Capture, bundle and upstream implementation                        | scrapling-research runtime, Python, CLI and output roots          |
| infrastructure-operations | Six infrastructure domain modules                                  | vps-operations native adapters and CLI                            |
| native-codex-adapter      | Hooks, Home context and evaluation-ledger implementation           | Codex fixed-owner loader, trust, CLI, sessions and ledgers        |
| native-hermes-adapter     | Nine context and budget modules                                    | Hermes source CLI, packager, compiler, profiles and data roots    |
| operational-context       | Eight backend and SharedWiki contracts/projections                 | Existing remote native readers, callback, policy and data         |

Install with the supported installed Bun using `bun --no-env-file install --offline --frozen-lockfile --ignore-scripts` when the admitted dependency cache is available. Run `bun run check:handoff` and `bun run check:imports`, then the affected package and owner checks. Existing immutable Ontology consumers retain their 0.1.0 or0.2.0 asset; this workspace does not repack them.

Reusable modules require their declared dependencies and explicit semantic policy where applicable. Native adapters deliberately use source-owned owner registrations, sibling code-owner packages and existing owner launch paths; a source import or machine-global fallback is not standalone clone/runtime proof. Fresh publication/cold-start custody is measured separately. Source/build provenance grants no native trust, profile, lifecycle, model or remote permission.

The backend semantic-context old path is a source-pinned generated projection. Generate it from the canonical common implementation with `bun scripts/generate-hermes-semantic-context.ts`; native publication remains a separately reviewed owner effect. Generated code is never an independent writer.

Run `bun run check:source` for the finite owner test tree and `bun run check:quality` for the complete source quality gate. Local `bun run format` applies the one shared Prettier configuration; hosted CI cannot rewrite files or increase the explicit-any allowance. Individual checks are `check:format`, `check:lint`, `check:architecture`, `check:workflows` and `check:dependencies`.

The quality gate checks the ownership hierarchy and dependency topology in `architecture/quality-policy.json`, canonical source pins, public exports, package boundaries, typed async correctness, formatting, Workflow syntax/security, dependency advisories and maintained-source secrets. Tool archives and cached binaries are verified. Negative fixtures prove that representative violations fail. Source snapshots and final architecture validation reject input changes during verification. Immutable releases and unadmitted Work source retain their explicit boundaries.

Pull requests independently verify the authored head and prospective merge, require an exact-head independent native review, and retain a bounded quality JSON artifact. The Lead publishes the immutable review result, receipt and authenticated status before opening the PR. If a new main review status arrives after its job ran, the Lead explicitly reruns that job. There is no review-arrival observer or automatic merge. A successful source gate establishes source verification. Native provider consumption, VPS serving identity, remote application and acceptance require separate evidence.

Common Work records now use committed Git source through `@palantirkc/work-continuity/git-work`. Purpose, actual decisions, dependencies and effect fences live in `work/<project>/<work>/`; independent Tasks have separate files. A new Work starts from freshly synchronized main. Continue the same Work on its existing branch with stable logical IDs. See [the Git Work guide](docs/git-work.md) and run `bun run harness` for the bounded reader commands.

For substantial quality or efficiency improvement, [constrained improvement](docs/constrained-improvement.md) binds a useful consumer goal, independent validation and bounded changes to comparable baseline/candidate evidence. Read-only harness preflight/comparison commands propose accept, repair, defer or stop; review, Acceptance and native activation remain separate.

The shared [Runtime Index](architecture/runtime-documentation-routing/registry.json) selects official documentation by runtime subject, topic, version and execution host. It grants no execution authority. Each srv1873899 VPS Lead pulls completed main, verifies native consumption and explicitly activates its own revision. Existing native adapters and public legacy APIs remain available during that migration.
