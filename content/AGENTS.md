# Shared agent instructions

Every coding CLI on this host loads this one file through its own pointer file. Its source is `content/` in the `lifecycle-platform` repository, its rules `specs/content/agents-file.md` there. Edit the source; never edit a pointer and never make a per-CLI copy.

## 1. Who decides

- The user's words bind. A correction or a stop of the user overrides every earlier decision, yours included.
- The agent the user speaks to completes the user's short words into what they mean, says in its first sentence what it took them to mean, so that one word of the user corrects it, and proceeds on work that can be undone, within the task's holds. Where the task names it Lead, it follows the skill `lead`.
- A worker does its page and nothing more: it adds no goal, widens no allowed effect, and reports a rule of the page that cannot hold instead of weakening it.
- An available tool, a registered server, a CLI's name or a remembered decision is no permission. After a context reset or a resumed session, read the task's limits again before any effect.

## 2. The order of work

1. Request: a message of the user, recorded with its words.
2. Intent: the objective, with criteria that can each be checked true or false, confirmed by the user.
3. Spec: the contract, a specification in the owning repository, merged through a unit.
4. Plan: the units of an intent with their dependencies, reviewed before they start.
5. Unit: one page, built, checked, reviewed and merged as one pull request.
6. Deliver: installed from the merged revision, read back, with a rollback.
7. Operate: the result observed in use; a finding becomes a request, a unit or an intent.
8. Retire: the old part removed in the pull request that replaces it.

Lanes: a question is answered from source with no record; `small` is one unit of class `plain` in one repository that a revert undoes (Request, Unit, Deliver); `objective` is anything else and takes every stage; an incident, a finding to act on, is triaged into one of them or dismissed with its reason.

When a request asks for the output of a later stage while the record of an earlier stage is missing, name the missing stage in one line, ask that stage's questions with defaults, and say what you will do without an answer. One message, at most five questions. Not for a question, a stop, an incident, or work that is already in order.

## 3. Four commands

- `ap status --brief` in the task's directory before anything else: what is next, stopped, waiting on you, owed or held.
- `ap request` for every message of the user, as `specs/ap/request.md` says: recorded in the turn it arrives, in the user's own words; list the topic's current requests before you ask the user anything.
- `ap unit page <key>` when you are given a unit: its page is your whole assignment.
- `ap status --term <word>`: what one word of the life cycle means, from the store's schema.

## 4. Evidence

- A fact comes from source, Git, a compiler, the store or a service's output, read now; a remembered fact and a path are checked before you build on them. A time comes from the clock, never typed.
- Verify by the output or a readback, not by a success message or an exit code alone. Use `set -o pipefail` in shell pipelines.
- Merged (the change is on the main branch), installed (the host's files equal the merged ones), loaded (a session's own record shows the text) and used (the unit's named run happened) are four facts, each shown by its own output.
- Read only what the next decision needs, a long file by line range; do not scan every repository to find a task. The model and the effort are unknown unless the session reports them.

## 5. Effects

- Never print a credential, nor write it into a file, a commit, a store or a message.
- Never touch a session, pane, process, worktree or service identity that is not yours.
- Do not change global configuration merely to orient yourself.
- A CLI's private memory holds nothing another agent or a later session needs: that goes to the store, the owning repository or the task's directory.
- Before deleting, verify the recovery copy and every consumer of the path; a task's completion is not enough. A pinned artifact is immutable: fix forward in a new version.
- When an effect on a real target has an uncertain outcome, read the target before a retry; only a disposable target is simply rerun.
- A failure is reported as `code@path:line` with one cause and no secret or private payload. The same failing step is not retried unchanged, nor passed by switching models: split or redesign it.
- Remote work keeps SSH host-key checking and the Tailscale boundary.

## 6. The user's standing limits

- The Academy: no `edu-*` directory and no `education-platform` or `academy-platform` path is opened without the Academy's own task.
- Private data: metadata and hashes only, unless the user authorizes more; the Academy's records are real data inside the Academy's own task, never in Git.
- Models: only those the policy file `/etc/agent-platform/policy.json` allows.
- Long-term memory is off until the user deploys it anew: call no memory tool; a fact with a key, a state or an owner is an object of the store. A memory hook's output is a past record, checked against current source.

## 7. Language

Everything except a message to the user is in English, by every agent, down to a prompt typed into another agent and a note. Korean appears only in a message to the user, in a stored field whose text is for the user, and in a quotation of the user's words, followed by its English reading. A message to the user follows the skill `report`.

## 8. Where things are

- `/home/palantirkc/platforms/<repo>/`: the platform and module repositories; a file there is read at `origin/main` (`git -C /home/palantirkc/platforms/<repo> show origin/main:<path>`) or in a task's worktree, never from the checkout's files.
- `/home/palantirkc/work/<scope>-<topic>/`: a task's directory, its worktrees in `wt/<repo>/`; read its `STATE.md`, where one exists, before any effect.
- `/home/palantirkc/external/`: pinned external source.
- `/root/.agents/`: these instructions as installed; the skills in `skills/` and the references in `runtime-references/`, each of which says in its first lines when to read it.
- The store: `ap status` and `ont`.
- `/home/palantirkc/external/reference-docs/`: the corpus of design sources; read only the lines a row of `/root/.agents/runtime-references/OFFICIAL-DOCS.md` section 4 names.
