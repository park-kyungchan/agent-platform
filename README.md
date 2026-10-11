# Agent platform

This Bun workspace owns admitted reusable implementations. Domain semantics and native execution remain explicit in [the ownership registry](architecture/capability-source-owners.json).

| Package | Canonical code | Native and data owner |
| --- | --- | --- |
| work-continuity | Handoff, context, Memory, originals and recovery | codex-harness-engineering |
| ontology | Ontology schemas, contracts and generators | ontology-foundation release adapter; immutable releases preserved |
| evidence-foundry | Evidence schemas, storage/evaluation logic and explicit-policy API | evidence-foundry CLI, relative policy and stores |
| research-capture | Capture, bundle and upstream implementation | scrapling-research runtime, Python, CLI and output roots |
| infrastructure-operations | Six infrastructure domain modules | vps-operations native adapters and CLI |
| native-codex-adapter | Hooks, Home context and evaluation-ledger implementation | Codex fixed-owner loader, trust, CLI, sessions and ledgers |
| native-hermes-adapter | Nine context and budget modules | Hermes source CLI, packager, compiler, profiles and data roots |
| operational-context | Eight backend and SharedWiki contracts/projections | Existing remote native readers, callback, policy and data |

Install with the supported installed Bun using `bun --no-env-file install --offline --frozen-lockfile --ignore-scripts` when the admitted dependency cache is available. Run `bun run check:handoff` and `bun run check:imports`, then the affected package and owner checks. Existing immutable Ontology consumers retain their 0.1.0 or0.2.0 asset; this workspace does not repack them.

Reusable modules require their declared dependencies and explicit semantic policy where applicable. Native adapters deliberately use source-owned owner registrations, sibling code-owner packages and existing owner launch paths; a source import or machine-global fallback is not standalone clone/runtime proof. Fresh publication/cold-start custody is measured separately. Source/build provenance grants no native trust, profile, lifecycle, model or remote permission.

The backend semantic-context old path is a source-pinned generated projection. Generate it from the canonical common implementation with `bun scripts/generate-hermes-semantic-context.ts`; native publication remains a separately reviewed owner effect. Generated code is never an independent writer.
