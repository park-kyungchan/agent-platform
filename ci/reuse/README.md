# Caller-owned Bun source checks

`.github/actions/bun-source` is a reusable composite action for an explicitly
checked-out caller repository. It installs the caller's exact Bun version and
runs a nonempty JSON list of existing package script names, in order. Frozen
dependency installation disables lifecycle scripts and can be disabled when the
caller has a separate reviewed setup step. The caller retains its checkout,
timeout, token permissions, Node version and domain commands.

The runner uses `GITHUB_WORKSPACE`, checks the caller repository/project/commit,
and refuses a different Git root, dirty tracked source, wrong Bun version or
unknown script. It rechecks the source after each successful command, stops on
failure and preserves the failing command's exit code. It never resets source.
Package scripts are trusted caller code, executed without interpreting action
inputs as shell source. Their effects remain the caller's responsibility.

## Required native source-author metadata

New versions of this action require a committed `.github/source-author.json`
before dependency installation or any owner script. The optional path input
`source-author-metadata` selects another exact repository-relative JSON path;
it never disables the requirement. Historical full-SHA action references retain
their historical behavior. Update consumers explicitly to a reviewed successor.

The single canonical contract and reader are `ci/reuse/source-author.ts`.
Source ownership (`repository`, code-owner Project and domain) stays distinct
from the author's logical Domain/Project/Work/Task. Native Codex author `threadId`
and native Hermes author `sessionId` are separate fields; unavailable native
Task/Run IDs are explicit `null`, never copied from logical IDs. Published
metadata contains no cwd, home, profile, account, credential or raw event/session.
Only the actual native author can supply these original observations. A GitHub
actor, Git commit author name or publisher account is not a native author ID.

In the native Codex process, prepare an exact scope JSON with two-space
indentation and one final newline, for example:

```json
{
  "sourceOwner": {
    "repository": "fixture/owner",
    "projectId": "fixture-owner",
    "domainId": "fixture-domain"
  },
  "logicalWork": {
    "domainId": "native-harness",
    "projectId": "fixture-integration",
    "workId": "fictional-work",
    "taskId": "fictional-task"
  }
}
```

Run the reviewed common CLI from the authoring checkout:

```sh
bun --no-env-file --no-install /reviewed/common/ci/reuse/source-author-cli.ts capture /exact/scope.json
```

The CLI reads the actual `CODEX_THREAD_ID` and current Git HEAD as `sourceBaseSha`.
It prints the record only after validation; the native owner records that
successful output in `.github/source-author.json` with the source change. It
does not write files or publish anything. Capture refuses missing native identity
and CI/GitHub Actions environments. Do not fill an unknown author ID manually
to pass CI. For Hermes, its native owner can call `captureHermesHookSourceAuthor`
with only the documented observed `pre_llm_call` `session_id` and exact
`extra.task_id`/`extra.run_id` fields, filling unexposed extras with `null`.
The API refuses CI capture and unexpected raw-event fields; it creates no hook,
profile, provider, store or native permission.

Before publication, call the same verifier against the newly committed exact
source. Set `COMMON_CI_REPOSITORY`, `COMMON_CI_PROJECT_ID`, and
`COMMON_CI_SOURCE_SHA` to the actual owner and full source commit, then run:

```sh
bun --no-env-file --no-install /reviewed/common/ci/reuse/source-author-cli.ts verify
```

Use a complete Git history (`fetch-depth: 0` in CI). The reader verifies that the
declared source base is an available ancestor, the metadata is a regular committed
Git blob, working bytes match that blob, and source owner Project/domain match
the committed `project.json`. A normal authored head must have exactly one
parent: `sourceBaseSha` must equal that immediate parent, and the metadata blob
must be new or different from the parent's blob. Capture again before each new
authoring commit, from its actual base checkout; retaining an earlier record or
changing its timestamp while keeping an older ancestor as base refuses.

The only inherited-attribution topology is a clean two-parent prospective merge:
the first parent is the target base and the second is the authored head. That
authored head must satisfy the single-parent freshness rule, its own committed
Project must match the metadata owner, and the merge must retain exactly its
metadata blob. Git `merge-tree --write-tree --no-messages` must produce exactly
the merge's committed tree from those ordered parents. This requires that Git
plumbing capability, full history, a shared merge base and a conflict-free
built-in merge; missing capabilities, shallow history, external configured merge
drivers, manual merge edits, nested authored-parent merges, root and octopus
heads refuse. Renormalization is disabled for this object computation. It may
add immutable Git objects; it does not update worktree, index, refs or config.
After manual conflict resolution, make a normal metadata-bearing authoring
commit with a fresh capture against the resolved commit before publication.

Duplicate keys, extra/private fields, unsafe paths,
missing native identity and substituted logical IDs refuse. The generated JSON
representation is closed: two-space indentation and exactly one final newline.

The execution/publication readback binds the authoritative current `sourceHead`
and tree to the exact metadata path, Git blob and SHA-256. A record committed in
that tree cannot also contain its own future HEAD/tree/hash; these are observed
by the verifier afterwards. The runner repeats this readback after every owner
command and refuses drift. Keep the resulting readback with exact source review
and publication evidence. The common repository's own source workflow requires
the same record before its source checks.

The protected-main review gate waits up to300seconds for genuinely absent
exact-head review status, retaining its5second poll cadence. A late response,
present hold, invalid publisher or malformed proof cannot turn that wait into
success. Root prepares actual-M receipt/status publication before merge and
publishes promptly after the exact merge joins; the longer wait does not replace
that ordering. Caller/evaluation budgets and token/lease/admission validity are
unchanged. See [`docs/operational-timeouts.json`](../../docs/operational-timeouts.json).

This is required, source-attributed provenance, not cryptographic authentication
of a native author or proof that one author wrote every line of a merged tree.
The native owner must refresh the captured author/Work for each new authoring
attempt; the base and readback preserve its scope. CI cannot manufacture missing
originals. Publisher authentication and independent exact-head review remain the
existing separate owner gates. No user exception for an older PR is silently
carried into future action versions, and source success grants no native/domain
activation or human Acceptance.

For a source job, use `contents: read`, `persist-credentials: false` on checkout,
and a complete reviewed common commit in the action's `uses:` reference. Pass
that checkout's exact SHA explicitly: authored PR head and prospective merge
remain separate choices. Do not pass secrets to this source-only action.

This is a shared execution layer. It does not generalize `ci/source-check.ts`
or `ci/quality/run.ts`: their current registry, fixture and policy paths belong
to the common checkout. Invoke owner-owned scripts here rather than copying
those implementations or running them against an unrelated project.

Keep native independent review in its existing separate job and retain its
owner-specific receipt, publisher and exact-head status policy. This action
does not verify reviews, publish statuses, schedule retries or authorize native
activation. A passing source action does not resolve the observed authored-head
versus actual-merge review publication race.

The first proposed consumer is Academy's existing source job, with Bun `1.4.2`,
Node `24.14.1`, its finite dependency restore and all existing source/synthetic
commands. The native review job remains owner-owned. On October 18 the private
common repository's Actions access setting was `none`; cross-private download
therefore remains an explicit GitHub owner adoption gate. Local validation does
not prove a hosted invocation. The caller also needs permission to use the
action under its own Actions policy.

GitHub permits sharing private actions with other private repositories owned by
the same account after enabling access. Public repositories cannot consume
this private action. Preserve their visibility: a future reviewed source-pinned
public projection, or a proportionate owner-specific exception, needs its own
distribution decision. No repository visibility or access setting is changed
by this source proposal.

References: [private sharing](https://docs.github.com/en/actions/how-tos/reuse-automations/share-across-private-repositories),
[reuse and access boundaries](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations).
