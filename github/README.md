# GitHub settings as files

The rulesets and repository settings of the platform repositories, kept as files and applied with one
script. The files are the wanted state; GitHub is compared with them and changed to match them.

## What the files hold

| File | What it is |
| --- | --- |
| `ruleset-main.json` | The ruleset of the branch `main`: no deletion, no force-push, a pull request merged by merge commit only, and the required checks `rust` and `lead-review`, each from one creator by its `integration_id`: `rust` from GitHub Actions (15368) and `lead-review` from the Lead's App (5863852). The same name from another creator (a stage's or a workflow's token, the owner's login) leaves a pull request blocked. |
| `ruleset-tags.json` | The ruleset of every tag: no deletion, no force-push. |
| `repositories.json` | The list of the ruleset files every listed repository gets (`rulesets`) and one entry per repository (`repositories`): the merge buttons (`settings`), the Actions policy (`actions`), Dependabot alerts (`vulnerability_alerts`), Dependabot security pull requests (`automated_security_fixes`) and secret scanning and its push protection (`secret_scanning`, `secret_scanning_push_protection`). An entry holds `secret_scanning` and `secret_scanning_push_protection` only for a repository whose `repos/<r>` answer holds `security_and_analysis`. An entry may hold `repository` alone, which gets the rulesets and nothing else. |
| `apply` | The script that compares and applies, through `gh api` with the caller's own login. |
| `test_apply.py`, `fixtures/` | Its tests, against a fake `gh` (`fixtures/fake_gh.py`) that answers from invented repositories. No test reaches GitHub. |

A ruleset file holds exactly the members `name`, `target`, `enforcement`, `conditions`, `rules` and
`bypass_actors`. A live ruleset matches a file's by `name` and `target`; two rulesets are the same when
these six members are equal as JSON values. A member an entry of `repositories.json` does not hold is not
managed: it is never compared and never written. A live ruleset that no file names is reported with
`wanted` null and is never changed or deleted.

`park-kyungchan/agent-platform` and `park-kyungchan/ontology-platform` are private: GitHub answers
`security_and_analysis` null for them, so their entries hold neither `secret_scanning` nor
`secret_scanning_push_protection`; they hold Actions policy `selected` with a pattern.

The entry `park-kyungchan/agent-vps-operations` holds `repository` alone: it gets both rulesets and nothing
else of it is compared or written. While the classic protection of its `main` is present, `--check`
answers `refused` and an apply writes nothing.

The entry `park-kyungchan/education-platform` differs: its Actions policy stays `all`, and it has no alerts
and no Dependabot member, because those are decided by the owner of that repository.

## The two commands

```
github/apply --check [--repo OWNER/NAME]...
github/apply --undo-dir DIR [--repo OWNER/NAME]...
```

`--check` only reads (every call is a `GET`) and prints, per repository, each setting whose live value
differs from the files: exit 0 when every repository is `same`, 1 otherwise. It is the read that a drift
finding uses.

`--undo-dir DIR` applies. Per repository, in the file's order: it reads; a repository with classic
branch protection on `main` is `refused` and gets no write; a repository equal to the files is `same` and
gets no write and no undo file; otherwise it writes the undo first, then only the differing settings,
then reads again and reports `applied`, or `failed` with `read_back_differs`. A failed write or read, or
a read-back that differs, stops every later write: the repositories after it are `not_run`.

`--file PATH` names another settings file (the ruleset files are read beside it). `--repo` limits the run
to the named entries. Exit 2 means nothing was called: `invalid_usage`, `invalid_file`,
`repository_not_listed` or `undo_dir_exists`.

The output is one JSON object on standard output:
`{"mode", "repositories": [{"repository", "result", "code", "differences": [{"setting", "present", "wanted"}], "undo"}]}`.

## Where the undo is

`DIR` must not exist; the script creates it with mode 0700. Before its first write to a repository it
writes there `OWNER__NAME.present.json` (the values it read and compares, nothing else of GitHub's
answers) and `OWNER__NAME.undo.sh`, with the bodies of its calls beside it. `bash OWNER__NAME.undo.sh`
puts back each setting the run was about to change, in the reverse order of the writes, deletes a ruleset
the run created by its new id, runs every call whatever the earlier ones answered, prints each result and
exits 1 when one failed. Read `--check` again after an undo.

## Who applies

Only the Lead applies, by hand, in a worktree at a merged commit of `main`, one repository at a time
with `--repo`, after `--check` showed the expected differences. Workers and verifiers run `--check` only.
No timer, workflow or `ap` verb applies these files.

The classic protection of a refused repository is removed by the Lead by hand, immediately before the
Lead's apply of that repository, and its `main` has no protection between those two calls.

## The Lead's App and the stage tokens

What follows holds for these files and for `specs/ap/unit-run.md` (R3, R38, R41 to R43). The paths in
the example are invented.

(i) The secrets. The user writes each secret by hand to a root-owned file of mode `0600` under
`/etc/agent-platform/credentials/`, which `specs/ap/run-limits.md` R10 hides from every stage. No agent
makes, copies or prints one.

(ii) The App. `kc-ap-lead`, id 5863852, holds the repository permission `Commit statuses: read and write`
and no other. It is installed on agent-platform, ontology-platform and agent-vps-operations, on each
before the Lead applies `ruleset-main.json` there. Its one private key, from the App's page, is in the
file `units.github.lead_app.key` names. `ap` names the key by its path to `openssl` and never opens it;
each status gets a fresh installation token, which reaches the statuses of that one repository.

(iii) The stage tokens. Per repository one fine-grained personal access token: `Repository access` that
repository; `Contents: read and write`, `Pull requests: read and write`, `Workflows: read and write`,
`Actions: read-only` and `Commit statuses: read-only`, nothing else (GitHub adds `Metadata: read-only`
itself); with an expiry. `Workflows` is there for the push of a workflow change, `Actions` for reading a
run's logs and jobs, and `Commit statuses` for reading the statuses of a head. It is in the file `units.github.stage_tokens` names for that repository, and it is replaced by writing
that file again: the run reads it anew before each author stage. An implementer or a fixer of that
repository pushes and writes to its pull request with it (`GH_TOKEN`) and does not see the host's login;
no verifier holds a login or a token: `ap unit run` gives it the pull request's description as a file and
posts its report. A repository the policy maps and `stage_tokens` does not name keeps the host's login
for its authors and its `lead-review` until the user writes its token (after the App is installed there)
and the policy names it; `<n>.credential` in the unit's directory and `credentials` in the run's answer
show each author's credential, `token` or `host`.

(iv) The form, in the host policy's member `units`:

```
"github": {
  "lead_app": {"id": 5863852, "key": "/etc/agent-platform/credentials/invented-app.pem"},
  "stage_tokens": {
    "park-kyungchan/agent-platform": "/etc/agent-platform/credentials/invented-agent-platform.token"
  }
}
```

`ap unit run` refuses a policy without `lead_app` (`github_not_mapped`), before it makes or applies
anything.

(v) The order, each step by the one named:

1. The user writes the secrets.
2. A unit of agent-vps-operations adds `units.github` to its `etc/agent-platform/policy.json` (the host
   policy's source) and merges under the older `ap`, which installs nothing: its
   `scripts/install-etc.sh` runs `ap policy check` first, and an older `ap` refuses the member
   (`policy_invalid`).
3. Before this change merges, the Lead stops `ap-work.timer` (then no pass starts a unit,
   `specs/ap/work.md`); from this change's installation until step 4 every `ap unit run` refuses
   `github_not_mapped`.
4. After that installation the Lead runs `scripts/install-etc.sh install <the merge of step 2>` in that
   repository's clone (or `ap deliver` does: `specs/ap/deliver.md` R54 to R56), then `ap policy check`,
   and starts the timer again. A unit refused in between is started by no later pass, so the Lead runs it
   with `ap unit run <key>`. The host's policy file is never edited by hand (the installer refuses
   `target_changed`).
5. One unit runs to its merge with a `lead-review` whose creator is the App.
6. Only then, per repository with the App installed and its token named in `units.github.stage_tokens`,
   `github/apply --check` and the apply. Applied earlier, or without both (`ap` posts as the App only
   there), no pull request there merges until that apply's undo script runs.

Until step 6 the watch's `settings <repository> ruleset:...` (`specs/ap/watch.md` R25) is expected for
each listed repository, and is not acted on before step 5. `park-kyungchan/education-platform`, which
`repositories.json` also lists, is not applied in step 6: only once the App is installed on it under the
Academy's own task. Until then its finding stays, expected, and an apply there would block every pull
request of it.

(vi) What a stage token still reaches: every branch and pull request of its repository, another unit's
too (a push, a close, a branch deleted, a tag created). None of that reaches `main`, which takes only a
pull request: `ap unit run` posts `lead-review` on the judged head alone, merges with
`--match-head-commit <that head>` and judges a moved head again (`specs/ap/unit-run.md` R30, R31). With `Workflows`, a stage can change a
workflow on its own branch, and that workflow runs on its push or pull request with the repository's
Actions secrets, before `ap unit run`'s protected-path stop reads the diff; `main` still takes only a pull
request that passes `lead-review` and the required checks.

(vii) What it does not reach: another repository and writing a commit status (it only reads them). A push
that changes `.github/workflows/` is reached with `Workflows`. For a token that lacks `Workflows`, e.g. a
repository whose token is replaced without it, such a push is refused, and the Lead pushes that branch
once from its worktree with the host's login, never forced, and runs the unit again; the unit stops
`no_pull_request` (an implementer) or `head_not_seen` (a fixer). Expected, not measured, under that same
condition: an author's push of its merge of `origin/main` after `main` gained a workflow change (the
frames order that merge first) is refused too. The run's own update (R26) pushes with the host's login,
which holds the OAuth scope `workflow`, so it is expected, not measured, not to be refused.

## What an undo does not take back

- Alerts raised and Dependabot pull requests opened while the settings were on: they stay and are closed
  by hand.
- `PUT vulnerability-alerts` also turns the dependency graph on; the alerts it raised and the dependency
  submissions made meanwhile are not taken back by the undo's `DELETE`.
- Workflow runs refused or started under the new Actions policy, and anything a ruleset refused meanwhile.
