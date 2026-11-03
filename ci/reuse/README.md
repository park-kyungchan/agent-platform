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

## Source review and historical compatibility

Ordinary caller source checks require no native source-author or logical Work
record. The historical `source-author-metadata` action input is optional,
deprecated and **IGNORED**. It has no default, path validation, file read,
readback, output or runtime dispatch, and confers no authority. An inherited
`COMMON_CI_SOURCE_AUTHOR_METADATA` environment value is also ignored by this
runner. Missing, stale or unsafe historical values do not select a gate or
bypass the caller identity and source checks above.

Review the exact candidate independently using native Git and the caller's
explicit commands. For example, from the caller checkout:

```sh
git rev-parse --show-toplevel
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=no
git diff --check <reviewed-base> <candidate>
git diff <reviewed-base> <candidate> -- <selected-source-paths>
```

Bind the actual checkout SHA to `source-sha`, select its existing package scripts
in their required order, and assess the actual command results with the source
diff. The runner executes `bun --no-env-file install --frozen-lockfile
--ignore-scripts` when installation is selected, then `bun --no-env-file run
<script>` for each selected script, rechecking tracked source after each
successful command. Independent review and Lead assessment remain separate
from execution success; no author capture, replacement record, receipt or
status publication is required by this action.

The immutable predecessor is
`475e8a9171c9bafeaca99e589f0a50af4bb21ca5`. Its action, runner, tests and former
metadata workflow remain recoverable with native Git, for example:

```sh
git show 475e8a9171c9bafeaca99e589f0a50af4bb21ca5:.github/actions/bun-source/action.yml
git show 475e8a9171c9bafeaca99e589f0a50af4bb21ca5:ci/reuse/run.ts
git show 475e8a9171c9bafeaca99e589f0a50af4bb21ca5:ci/reuse/README.md
```

Historical full-SHA action references retain their historical behavior; adopting
a reviewed successor is an explicit caller change. The separately retained
`source-author.ts` library and other helper consumers are outside this bounded
action retirement. The uncalled `source-author-cli.ts` entrypoint is retired in
a later, separately documented source closure; its historical bytes remain in Git. Their presence does not impose an
author gate here. This change proves neither foreign-consumer adoption nor full
S04/P6 completion, and grants no native/domain activation or human Acceptance.

For a source job, use `contents: read`, `persist-credentials: false` on checkout,
and a complete reviewed common commit in the action's `uses:` reference. Pass
that checkout's exact SHA explicitly: authored PR head and prospective merge
remain separate choices. Do not pass secrets to this source-only action.

This is a shared execution layer. It does not generalize `ci/source-check.ts`
or `ci/quality/run.ts`: their current registry, fixture and policy paths belong
to the common checkout. Invoke owner-owned scripts here rather than copying
those implementations or running them against an unrelated project.

Use ordinary independent review of the exact source revision. This action does
not verify reviews, publish statuses, schedule retries or authorize native
activation. Separately retained owner workflows and their effects require their
own bounded disposition.

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

Active source CI selects `.bun-version` as its single exact version source. A
caller may pass `bun-version-file: .bun-version` instead of the historical
`bun-version` literal. The shared action refuses simultaneous or missing values,
symlinks, other filenames and non-release strings before runtime setup. The
pinned trusted-ci-v1 controller retains its independently reviewed historical
bootstrap; changing that release requires a successor policy, not a moving tag.

## Retired native review publisher

The native review publisher/controller CLI is retired. Its launcher, controller
invocation, artifact dispatch and status-publication instructions are historical.
For the preserved implementation and protocol, read the Git bytes at
`87b5cd62b89b8e8d550104f5bf037793f6b89e86:ci/reuse/review-pr.sh`,
`ci/reuse/review-pr.ts` and `ci/reuse/publish-review-artifact.ts` at that same
revision, and [historical artifact interpretation](../quality/review-artifacts.md).

Use ordinary independent native review of the exact candidate revision and Lead
assessment of its source and actual check results. This retirement adds no
replacement receipt, controller or status gate. Remaining shared-action, helper
and manual-workflow consumers require their own bounded disposition.
