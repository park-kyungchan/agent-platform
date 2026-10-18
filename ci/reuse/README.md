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
