---
name: lead-orchestration
description: When task STATE assigns Lead, coordinate authorized workers, isolated review, PR delivery and verified closure across CLIs.
---

# Lead orchestration

Use when the current task STATE assigns you the Lead role. Assignment comes
from STATE, never a CLI name. Preserve the permanent
[common mechanism boundary](https://github.com/park-kyungchan/agent-platform/blob/main/docs/common-harness-boundary.md): native
CLIs own loading, sessions, trust and tools; Herdr owns terminals; Hindsight owns
memory. This is a playbook, not a loader, scheduler or authorization system.

Use [strategy-dialogue](../strategy-dialogue/SKILL.md) for the strategy brief,
user decisions, KPI-first reports and visible work threads. Existing explicit
authorization remains valid; do not restart settled decisions.

## Dispatch and ownership

Workers implement. Independent reviewers review in a fresh, different context.
Lead reads back actual source, output and effects before accepting their work.
Use separate worktrees or genuinely disjoint write sets; context isolation alone
does not isolate files, Cargo locks, interfaces or shared build resources.
Allocate one expensive build owner at a time and record it in STATE.

Use this TASK.md template, with exact values rather than transcript references:

```markdown
# TASK: <bounded goal>
Owner: <Lead>; Worker: <assigned runtime/model/effort>
State: <absolute STATE.md>; Canonical rules: <absolute AGENTS.md>
Read first: /root/.agents/AGENTS.md, <absolute TASK.md>, <absolute STATE.md>, <needed skill SKILL.md absolute paths>
First output after reading: <one-line list of the exact files read>
Authority: <user decision and current task scope>
Exact targets: <repo, worktree, branch, base/head, files and selected contracts>
Strategy link: <confirmed STATE/STRATEGY.md and relevant KPI definition>
KPI this task moves: <named KPI, expected effect, native measurement or protected guardrail>
Allowed effects: <reads/writes, local commits, push, PR, owned build resources>
Forbidden effects: <holds, unrelated paths, host/domain/remote boundaries>
Expected output: <absolute report path, source revision, evidence and limitations>
Verification: <required commands and actual output/effect readback>
Completion: <closure rule or exact same-scope continuation condition>
DONE marker: <unique literal terminal line>
```

Put changed decisions in dated/numbered ADDENDUM files and name them in STATE;
do not silently rewrite an accepted task's authority. Update STATE at each
milestone with current owner, authority/holds, exact revisions, actual evidence
and one next action. Handoffs transfer goal, exact targets, allowed effects,
expected output and state path; never make a worker reconstruct a transcript.

Make handoff readback explicit. A canonical path in a prompt, a skill in the
native catalogue, or a literal `@import` does not prove the receiver read rules.
When the native loader cannot expand a pointer, name the exact files to read
first: `/root/.agents/AGENTS.md`, the absolute TASK and STATE, and each needed
skill body (for example `/root/.agents/skills/explain-precisely/SKILL.md`).
Known cases: Muse personal/project `@/root/.agents/AGENTS.md` stays literal;
Codex outside Git does not discover ancestor AGENTS.md. The receiver must read
these files with native tools before effects and show a one-line list of files
read in its first output after those reads. Lead verifies the native read calls
and readback; a claimed list alone is not proof. This uses existing handoff
verification, not a new gate, instruction copy or loader (PR #98 boundary).

## Herdr mechanics

Use the [Herdr skill](../herdr/SKILL.md) and native help only when pane control
and delegation are authorized. Verify the genuine caller and selected server.
Use one tab per worker, preserve user focus, keep unrelated panes intact and use
returned opaque IDs. Agent names are lowercase and match
`[a-z][a-z0-9_-]{0,31}`; keep them unique among live agents.

Workers keep native Hindsight (hooks and MCP); do not export `HINDSIGHT_DISABLE_HOOKS`
or disable the MCP to isolate them (user decision 2026-11-06: never block a native
feature). When a task needs isolation, such as an eval or an independent review,
run it in a directory covered by a native coding-agents `paths.<prefix>` override
(for example `{"disabled": true}` or a separate bank) and verify that override.
Verify native model, effort, cwd and readiness without changing global configuration.

Submit the exact task packet and verify that the prompt was submitted and the
worker began its turn. A send acknowledgement, `idle` or `done` alone is not
completion proof. After uncertain submission, read existing state before any
retry. Keep startup/trust questions for the user's native interaction.

Use task-owned background `until` waiters that require BOTH the exact DONE line
and a nonempty expected output file. For example, with a fresh task-owned
`terminal_output` file supplied by the authorized output capture:

```bash
set -euo pipefail
until [[ -s "$report" ]] && rg -qxF -- "$done_marker" "$terminal_output"; do
  sleep 10
done
```

Set those variables to the task's exact paths/marker. A TASK file containing the
marker is not terminal completion evidence. Do not infer success from file
existence alone, repeatedly block the Lead's foreground, or build a resident
watcher. Record waiter ownership/PID; end it when the task ends or changes.

## Review, delivery and closure

Freeze an exact candidate revision for independent review. Give reviewers the
request, contracts, raw artifacts and source revision, not the implementation
worker's reasoning, transcript or suggested verdict. Review and closure report the observed KPI/guardrail effect, its evidence and
any expected effect still unmeasured; estimates remain labeled. Lead checks each finding
against source, sends concrete repairs, and gets the changed candidate reviewed.

Use PR → CI → thread resolution → expected-head merge. Required checks must
pass for the exact reviewed head. Before an authorized merge, read the live head,
review threads and check results, then use the provider's expected-head guard.
Never force-push. A task that permits only a PR stops before merge. When changing
required checks, inspect BOTH classic branch protection AND repository rulesets;
back up and read back each changed surface.

For each completed worker: save its native session (or supported redacted
export), verify the saved artifact/session identity, record the roster (worker,
role, runtime/model/effort if known, pane/tab/session, task, head, output),
reconcile child processes/builds/uncertain effects, close its owned pane and
confirm zero remaining owned worker panes/processes/waiters. Preserve unrelated
sessions. Record unsupported retention or evidence gaps explicitly.

Closure is attested by an independent verifier using the exact roster,
artifacts and native readback; the Lead's own success claim is insufficient.
Keep a context only for the concrete unfinished continuation below.

Use [worker closure](../lessons-loop/references/worker-closure.md) for the accepting Lead's task-local evidence/readback and optional curator handoff; completion signals never substitute for acceptance evidence.

## Context Governance

Isolation is the default: give a fresh worker a small context packet containing
exact TASK/STATE/canonical paths, source revision and selected raw evidence,
never a transcript. Continuity is allowed only when the follow-up remains in
the same scope/files, reuses the loaded context and has enough context remaining.
Record why in STATE; otherwise save/close and start fresh. Reviews are always
isolated, including follow-up review of a repaired candidate.

Workers keep Hindsight hooks/MCP disabled unless doing authorized memory work.
The Lead supplies needed verified memory conclusions with source provenance.
Native resume is not renewed authority; reread current STATE and addenda.

## Memory contract

Apply Context Governance across all CLIs: isolate by default and continue a native session only when its context is needed. Hand off goal, exact targets, allowed effects, expected output and current STATE path; the receiver reads current rules/STATE before effects. Native sessions/STATE hold short-term execution, codebase docs own executable contracts, Hindsight owns durable contextual decisions/lessons, and domain owners/future Ontology own operational facts/actions/recovery. Search relevant pages before new work, bugs, tests/conventions, integration, why/remaining-work questions and commits; read selected pages, recall exact evidence and reflect only for unresolved reasoning. Verify current native truth and credit useful memory. Use the one shared bank with platform/repository provenance through official Hindsight features; tags are not privacy or authority. Respect each CLI's actual hooks/MCP/retention and explicit no-write scope. Do not copy STATE, operational records or another agent's transcript into Hindsight, infer isolation from an agent name, build a loader/registry/watcher, or reimplement native CLI/Hindsight mechanisms. Route verified L3 proposals to current STATE `DIRECTION PROPOSALS` and the user through Lead.

Read [CONTEXT-GOVERNANCE.md](../../runtime-references/CONTEXT-GOVERNANCE.md) on
demand for source authority, provenance, official retrieval/update boundaries
and anti-duplication. At each task/pane closure record workers, review findings
by severity, rework rounds and CI minutes in STATE; unknown stays unknown. Use
[lessons-loop](../lessons-loop/SKILL.md) for a durable candidate or local no-write
handoff, without inventing an incident for successful routine work.

## User interaction and safety

Use [explain-precisely](../explain-precisely/SKILL.md) for progress, answers and
decision questions. Ask only decisions belonging to the user, with a concrete
recommendation and consequences. Quality takes precedence over speed. End turns
with a concise status line naming completion or the remaining hold/next action.

Before an authorized host-config edit, save the exact prior bytes and provide a
rollback script; read back the resulting configuration. Before deletion, create
and verify the approved recovery copy (archive tag or Drive copy), then check all
live path consumers. A recovery copy does not authorize deletion by itself.
Keep custody boundaries and retained originals intact. Never print secrets.

Use task-owned Cargo target and fixture roots, bound disk use before builds and
delete those disposable roots after preserving required logs/evidence. Preserve
the task's free-space floor and unrelated build roots. Keep SSH host-key checking,
Tailscale and target-owned remote update boundaries. Failure reports use
`snake_case_code@path:line`, one short cause and no private payloads.

## Dated worker defaults — 2026-11-05

These are task-scoped defaults, not permanent CLI roles or model availability
claims. The actual assignment and permitted effects always come from task STATE.

| Runtime / model / effort | Default assignment |
| --- | --- |
| Codex `gpt-6.1-sol` / high | Implementation |
| `gpt-6.1-terra` / high | Mechanical work with fixed inputs and checks |
| Muse `muse-wonder-1.0` / high | Independent semantic review |
| `muse-spark-1.3` | Fixed checks |
| agy `Argon` only | Only the task assignment in STATE; do not substitute a model |
