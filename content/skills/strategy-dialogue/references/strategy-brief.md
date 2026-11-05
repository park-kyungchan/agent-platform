# Strategy Brief template and worked pilot

Use five lines before non-trivial execution; the user corrects wrong lines.
Preserve an existing approved decision and mark draft assumptions. Persist the
confirmed brief in task STATE/STRATEGY.md with the actual user decision reference.

```text
Objective: <user outcome and acceptance scope>
Diagnosis: <observed cause/evidence; explicit assumptions or unknowns>
KPI: <repo/docs definition + native source/time; current → adopted target>
Guardrails: <numerical floors/ceilings, protected effects and what not to do>
Domain: <relevant user knowledge; concrete missing domain facts>
```

For large work, accompany this with 2–3 strategic options and a recommendation:

| Option | Predicted KPI effect | Guardrail effect | Risk / uncertainty |
| --- | --- | --- | --- |
| <recommended approach> | <estimate with basis or unknown> | <measured/predicted bound> | <material risk> |
| <alternative> | <estimate with basis or unknown> | <measured/predicted bound> | <material risk> |

The confirmed task file owns execution and measured time series. A worker TASK
links it and names the KPI or guardrail it affects. Repository docs own KPI
formulas/units/sources; Hindsight may retain the decision/definition and rationale,
never operational KPI values/time series.

## Worked example — ws-replan pilot, 2026-11-05

Source: `/home/palantirkc/work/ws-replan/STRATEGY.md`, read on 2026-11-05.
The pilot labels the overall brief DRAFT v1 and its top-level KPIs user-confirmed.
Keep that distinction: the following sanitized example is not a new adoption or
live measurement. Private domain details and personal identities are omitted.

```text
Objective: Maximize measurable quality of Rust-first, CLI-agnostic work with Context Governance; prepare future Ontology co-engineering.
Diagnosis: Agents select outdated paths because legacy code and stale memory records remain reachable; the pilot reports 513/930 memory documents as state snapshots and recall supported for 15/33 questions.
KPI: Legacy removal 306/5,124 (6.0%) → 100%; full-gate Rust tool parity 0/44 → 44/44 (pilot baseline 2026-11-05T00:21Z; 13 bounded handlers are not full parity).
Guardrails: Linux CI ≤3,000 min/month; disk free ≥15 GB; escaped defects target 0; recall must not regress from 15/33 (target 33/33); no live domain calls, secrets, force-push or partial cutover.
Domain: User supplies real workflow semantics and tool importance; those facts determine port order and the later domain model.
```

The pilot attributes removal to its manifest/native Git measurement, parity to
the per-tool parity/test/independent-review/CI/rollback gate, CI to billing and
recall to the official regression suite. The example reports those dated values;
it does not claim to rerun the measurements. Current definitions must be in the
owning repo/docs and current values must be remeasured from their native owner.

For this content change, the expected effect is clearer shared strategy and
source/lesson routing. A numerical recall improvement is unknown until the
separately authorized regression runs; publishing skills does not prove it.
