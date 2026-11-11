# Structured memory record schema

Source: the 2026-11-05 Hindsight architecture design, section 3. This is an
ordinary document/frontmatter convention; extraction may not preserve every
field. Verify original-document GET and recall/readback. It does not authorize
writes, configure the server or establish a validator/loader/runtime gate.
Official API behavior here was verified against an earlier server version than the
host runs; the installed versions are stated once, in
`/root/.agents/runtime-references/HINDSIGHT-RUNTIME.md`. Check installed source before a
version-dependent operation.

## Exact tag vocabulary

Preserve existing genuine `source:*`, `harness:*`, `project:*`, `plan:*`, official `knowledge:*` and historical tags. Do not forge reserved plugin attribution or normalize away historical repository names.

| Namespace | Allowed new values |
| --- | --- |
| `kind:` | `incident`, `lesson`, `pattern`, `direction-proposal`, `decision`, `rationale`, `correction`, `convention`, `user-profile`, `glossary`, `initiative`, `snapshot`, `strategy`, `kpi-definition` |
| `level:` | `l0`, `l1`, `l2`, `l3` (lesson loop only) |
| `platform:` | `agent`, `education`, `ontology`; use multiple applicable tags for shared scope, omit if genuinely unassigned |
| `layer:` | `native_cli`, `coordination`, `memory`, `platform`, `domain`, `infrastructure`, `cross_layer` |
| `failure_class:` | `stale_context`, `authority_confusion`, `provenance_loss`, `scope_mismatch`, `source_identity`, `runtime_loading`, `boundary_contract`, `concurrency`, `timeout_budget`, `resource_leak`, `privacy_exposure`, `verification_gap`, `schema_drift`, `user_explanation`, `unclassified` |
| `severity:` | `p0`, `p1`, `p2`, `p3`, `unassessed` |
| `recurrence:` | `first`, `repeated`, `systemic`, `unknown` |
| `status:` | `open`, `mitigated`, `codified`, `superseded` |
| `codified_in:` | `none`, `canonical-guidance`, `agent-platform`, `education-platform`, `ontology-platform`, `module`, `skills` |

Severity definitions: p0 = verified broad irreversible/external impact or credential/authorization compromise; p1 = material wrong effect, authority/privacy boundary failure or major blocked function; p2 = bounded defect/rework without established serious effect; p3 = minor friction. `unassessed` is mandatory when evidence is insufficient. Severity does not create approval authority. `recurrence` counts independent incidents sharing a verified mechanism, never copies, re-ingests, pages or multiple reports of one event. `codified_in` tags are coarse routing only; exact repository/path/revision/evidence lives in the field below.

## Common lesson-loop frontmatter and body

```yaml
---
schema_version: 1
record_id: mem-v1-incident-cross-source-0123456789abcdef
revision: 1
kind: incident
level: L0
title: A verified symptom at its source boundary
platforms: [agent, education]
layer: memory
failure_class: scope_mismatch
severity: p2
recurrence: {class: first, independent_incident_count: 1, incident_ids: []}
status: open
observed_at: "2026-11-05T00:00:00Z"
verified_at: null
owner: assigned-task-owner
scope: explicit affected implementation and permitted evidence
sources:
  - bank_id: coding-agent::shared
    document_id_sha256: exact-source-id-hash
    original_text_sha256: exact-decoded-source-text-hash
    source_revision: exact-revision-or-unknown
    locator: allowed-source-or-task-evidence-locator
supersedes: []
superseded_by: []
related: []
codified_in: []
review_due: next-relevant-incident-or-curator-review
---
Symptom: observed output, separate from expected output.
Cause: verified boundary/mechanism, or explicitly unknown.
Impact: established effect and bounds; never a hypothetical effect stated as fact.
Mitigation: actual action and readback, or proposed action clearly marked.
Verification: exact source/output reference and remaining uncertainty.
Lesson candidate: reusable prevention rule and applicability limits.
```

Use actual dates, no example literal as data. A protected locator may remain an approved opaque handle; never dereference a forbidden path. Include failed attempts and counterevidence when needed to understand the mechanism. No personal operational payload belongs in a lesson.

| Kind/level | Additional required content |
| --- | --- |
| L0 incident | Expected/actual behavior, distinct occurrence identity, boundary, impact, evidence, cause confidence, mitigation and verification. |
| L1 lesson | `derived_from` verified incident IDs, prevention rule, conditions/exclusions, why the mitigation worked, counterexample and repeatable verification. |
| L2 pattern | At least two independent L1/incident chains, common mechanism across tasks or layers, failed competing explanation, scope limits, codification candidate and recurrence evidence. |
| L3 direction-proposal | `derived_from` patterns/critical incident, structural assumption contradicted, alternatives including retaining the design, expected quality/cost effect, reversible trial, success/stop criteria, required decision owner and Lead STATE proposal reference. Always proposed until adopted through current authority. |

Promotion creates a **new linked record at the higher level**; never rewrite an incident into a general rule. L0 -> L1 needs cause/mitigation evidence and an applicable prevention rule, not necessarily repetition. L1 -> L2 needs two independent occurrences, not two documents. L2 -> L3 normally needs recurrence across two layers/platforms, or repeated failure after codification; one well-supported critical architectural contradiction may justify an explicitly exceptional proposal. Never require another harmful event merely to meet a count. Record the exception and alternatives. No inferred or automatically enforced policy is created by a promoted document.

Status transitions are `open -> mitigated -> codified`; `mitigated -> open` when a mitigation fails; `codified -> open` on a new contradicting incident; any status -> `superseded` only with an explicit successor and scoped reason. L0 may be mitigated while its L1 remains open. L3 is codified only after adoption plus actual implementation/verification, not after a user merely agrees to discuss it. Append dated transition evidence. `codified_in` entries contain `{repository,path,revision,verification_ref}` and an allowed content pin where appropriate. A status tag is a view, not an authority token.

## Other document schemas

Every schema uses the common `schema_version`, `record_id`, `revision`, `kind`, title, scope/platforms, status, dated evidence and supersession fields above. Do not apply lesson level/severity to an unrelated profile or glossary merely to fill a template.

| Kind | Required additional fields |
| --- | --- |
| decision | `decision`, `decision_owner`, `decided_at`, `alternatives`, `rationale`, `constraints`, `consequences`, `revisit_trigger`, `evidence`; distinguish explicit adoption from a proposal. |
| rationale | `decision_ref`, `question`, `reasoning`, `tradeoffs`, `rejected_alternatives`, `assumptions`, `counterevidence`, `revisit_trigger`. Link the decision rather than copying it. |
| correction | `prior_claim`, exact `prior_document_refs` with content pins, `corrected_claim`, `effective_at`, `scope`, `native_evidence`, `preserved_history`, `affected_models`, `readback`; supersede only the contradicted claim. |
| convention | `rule`, `applies_to`, `exceptions`, `rationale_refs`, `codified_in`, `verification`, `review_trigger`; source docs remain executable authority. |
| user-profile | `preference`, explicit `user_evidence`, `applies_to_runner_or_domain`, `valid_from`, `revisit_trigger`, `confidence`; no inferred demographics, ability, blanket permission or permanent CLI role. |
| glossary | `term_literal`, `language`, `definition_ko`, `concrete_input`, `executor`, `result`, `explained_at`, `explanation_evidence_ref`, `scope`, `user_feedback`, `last_confirmed`; “explained” is not “mastered.” Missing feedback stays unknown. |
| initiative | `purpose`, `approved_scope`, `rationale`, `user_decision_ref`, `canonical_task_state`, `page_id`, `material_changes`; status is dated intent, not a second execution tracker. Preserve the official initiative marker/page instead of replacing it with a parallel registry. |
| strategy | `brief_ref`, `objective`, `diagnosis`, `kpi_refs`, adopted `targets`, `guardrails`, `domain_refs`, `user_decision_ref`, `alternatives`, `rationale`, `revisit_trigger`; confirmed decisions only, no KPI measurement series. |
| kpi-definition | `kpi_id`, `definition_ref` (repo/path/revision), `formula`, `unit`, `native_source`, `direction`, `cadence`, `owner`, `rationale`, `user_decision_ref`; no observed KPI values/time series. Repository docs remain the canonical definition. |
| snapshot | Dated `as_of`, `source_state_ref`, `historical_only:true`; existing originals retained as evidence. Do not create new snapshots by copying STATE. |

## Stable IDs and official update paths

New durable IDs are ASCII `mem-v1-<kind>-<scope>-<stable-topic>`, scope `agent|education|ontology|cross`. Source-derived drafts end `source-<first16-of-source-ID-SHA256>`; require uniqueness against the **full** selector before creation. An authorized migration uses its exact approved per-document IDs. A topic rename does not change identity. Store document IDs and page IDs separately.

For mutable newly authored records, increment frontmatter `revision` and retain the **complete** new body under the same `document_id` with explicit `update_mode:"replace"` through the official retain API/client. An ID identifies mutable identity, not immutable content. For an already pinned/retained original or a materially superseding decision, preserve it and create a new `...-r2` or separately named Correction linked to its pin. Do not silently change historical IDs to reflect repository renames.

For tags only, prefer official `PATCH /v1/default/banks/{bank_id}/documents/{document_id}` with the **complete union** of current preserved tags and approved taxonomy tags. It replaces, not appends. Unchanged tag sets are no-ops. The live schema also supports `observation_scopes`; there is no general document-metadata PATCH in this revision. Re-retain can supply metadata, but a metadata need is not a reason to re-extract pinned text. Tag/scope changes can invalidate observations and requeue co-sources, so a small document count can cause substantial consolidation work. Same-ID text replacement may retire old facts; preserve a recoverable before-image before using it.

Document history carries operation/event IDs and counts, not recoverable prior text, and depends on retained operations. Model history is enabled by installed default, 50 entries **per kind per model** (versions and refresh failures separately), configurable by `HINDSIGHT_API_ENABLE_MENTAL_MODEL_HISTORY` and `HINDSIGHT_API_MENTAL_MODEL_HISTORY_MAX_ENTRIES`. Verify effective configuration before claiming those defaults apply. Neither history is an unlimited rollback archive.


Structured writes follow [hindsight-api.md](hindsight-api.md): explicit stable
ID through official HTTP/client, then full-tag-set PATCH and GET readback. The
reviewed kind extensions `strategy` and `kpi-definition` preserve the distinction
between durable definitions/decisions and operational KPI measurements.
