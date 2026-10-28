# Agent platform

Use TypeScript and Bun. Read project.json and architecture/capability-source-owners.json for the current admitted implementation, semantic owner and native adapter boundaries. Edit canonical code in its listed package. Old owner imports are compatibility adapters or source-pinned generated projections; never create a second maintained implementation there. Preserve Issue/Spec/Plan/evidence/verification/evaluation/AcceptanceDecision distinctions and historical source milestones.

Native Codex/Hermes own loops, sessions, dispatch, retries, tools, profiles and credentials. Work/original/evidence/task/domain stores retain their existing owners. Source-provider proof grants no native permission. Context first-import guards must not be weakened or native caches cleared. Changes to common package.json/bun.lock require successor pins in all three active owner registrations/markers. Project metadata changes require the active Codex context registration/index successor. Root serializes exact reviewed changes and current-code CAS; preserve historical evidence and all third-state bytes.

External connectors follow `architecture/connector-policy.json`: use maintained provider MCPs through the native runtime, including replacements for redundant custom tools. Verify actual capability and consumer equivalence before removing old entrypoints. Common code owns connector meaning, not another MCP client, tool registry or credential bridge.

Ontology and Evidence schemas/generated sources each have one canonical generator. Research schema/generated/index files are pinned owner dependency copies; native research actions use scrapling-research. Evidence core requires explicit real per-instance semantic policy; native owner service/index inject their relative project.json policy. Never copy stale policy into the common core. Preserve every existing immutable 0.1.0/0.2.0 release and consumer pin.

Run bun run check:handoff and bun run check:imports for common source changes, plus the affected package/old-owner checks and exact source-provider closure. Native adapter, remote source and publication effects require their own exact reviewed owner order; no global reset, effect replay, scheduler or native trust/config mutation follows from moving source.

For Git-native common Work use the `@palantirkc/work-continuity/git-work` reader and `bun run harness`. Canonical records live under `work/<project>/<work>/` in the owning Work branch. New Work starts from native-owner-observed freshly synchronized main; continuing Work keeps its logical IDs and branch. Keep independent Tasks in separate files, group small dependent changes, and validate semantic transitions when integrating branches. Git records do not authenticate native authorship or grant permission. Preserve completed/uncertain effect fences and original-answer bindings. Actual decisions, proposals, verification, native execution, shared delivery, formal completion and Acceptance remain distinct.

For substantial quality or efficiency improvement, use docs/constrained-improvement.md: predeclare the actual consumer target, useful goal, independent validation, allowed changes, bounded limits and next decision. Compare equivalent baseline/candidate exposure except for the declared treatment; use quality-preflight/quality-compare for criterion transitions and preserve the v1 improvement-* efficiency contract. Distinguish behavior gain from independently controlled defect-detection gain, regression, preserved quality and insufficient evidence. Keep missing measurements unknown. Preserve observations, evaluator results and accept/repair/defer/stop proposals separately from AcceptanceDecision. Routine tasks need no extra ceremony; native owners retain execution and activation.

Automatic merge is prohibited. Publish independent exact-commit/tree review evidence and its authenticated status before opening a PR. The accountable Lead checks current findings, source and required checks, then explicitly merges the expected head. Review reuse requires identical reviewed tree, an explicit successor binding and current zero-unresolved-finding state. Each runtime Lead later verifies and explicitly activates the merged source; pulling main alone does not activate native configuration.

## PR delivery

1. Commit fresh native source-author metadata, push the exact head, and keep source/quality checks green.
2. From this checkout run `bun ci/reuse/review-pr.ts <pushed-head> native-claude` for a fresh read-only Opus review and authenticated artifact status.
3. Keep the unchanged raw output printed by the command; findings or transport/source drift stop publication.
4. A second native caller may run the same command with `--check`; it verifies existing proof and issues no independent judgment.
5. Open the PR after review status passes; resolve current findings/threads and require source, quality and prospective-merge checks.
6. The accountable Lead explicitly merges the reviewed expected head. Automatic merge, native activation and user Acceptance stay separate.
