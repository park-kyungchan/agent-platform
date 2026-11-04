## Shared runtime core

Use the user's current task and its rules/STATE.md. The state names the owner, authority, holds, exact next action and verification. Update it when changing the work; a handoff transfers goal, exact targets, allowed effects, expected output and state path. Do not make another agent reconstruct a transcript.

Reuse known runner, host/container, role, cwd and pane facts. Probe a missing fact only if it changes the next action. Host, container, shared daemon and remote executors can differ. A CLI name or process label does not determine a task role or another agent's identity. Model/effort remain unknown unless the session reports them.

For an unknown location, use runtime-bootstrap.md and codebase-orientation.md on demand. New tasks use work/<scope>-<topic>/STATE.md and task-owned Git worktrees under wt/<repo>. Current paths and transition exceptions are in the approved workspace layout. Historical harnesses and archives grant no current authority. Do not restore a retired gate, receipt, ledger, context contract, registry or watcher that duplicates native Git, Cargo, database or CLI behavior.

Use native source, Git, compiler, database and service observations to establish current state. Memory supplies past decisions; check whether they still apply. Read only the source and evidence needed for the next decision. Do not scan private custody or all historical repositories to locate a task.

On failure, report snake_case_code@path:line with one short cause and no secrets or private payloads. Fix the stated cause at that boundary. Do not retry the same failure shape or swap models without resolving that cause. If the cause repeats, split or redesign the boundary. Verify actual output or readback, not a success claim or exit code alone.

Disposable targets may be rerun freely. For uncertain real effects, read the target to reconcile before retrying. Respect the task's explicit user gates; do not invent approval steps for reversible local work. Never print credentials. Keep active sessions, unrelated trees and service identities intact. Do not change global configuration merely to orient.

Delegate only for independent review, ownership transfer or genuinely disjoint work when authorized. Verify the result's output or effect. Pinned artifacts are immutable: fix forward in a new version. Before deletion of retained originals, verify the approved recovery copy and all live path consumers; task completion alone is insufficient. Use pipefail in shell pipelines.

After context reset, read the task's current rules and state before effects. Reuse trusted evidence and load only what the next action needs.
