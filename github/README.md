# GitHub settings as files

The rulesets and repository settings of the platform repositories, kept as files and applied with one
script. The files are the wanted state; GitHub is compared with them and changed to match them.

## What the files hold

| File | What it is |
| --- | --- |
| `ruleset-main.json` | The ruleset of the branch `main`: no deletion, no force-push, a pull request merged by merge commit only, and the required checks `rust` and `lead-review`. |
| `ruleset-tags.json` | The ruleset of every tag: no deletion, no force-push. |
| `repositories.json` | The list of the ruleset files every listed repository gets (`rulesets`) and one entry per repository (`repositories`): the merge buttons (`settings`), the Actions policy (`actions`), Dependabot alerts (`vulnerability_alerts`), Dependabot security pull requests (`automated_security_fixes`) and, for a public repository, secret scanning and its push protection. |
| `apply` | The script that compares and applies, through `gh api` with the caller's own login. |
| `test_apply.py`, `fixtures/` | Its tests, against a fake `gh` (`fixtures/fake_gh.py`) that answers from invented repositories. No test reaches GitHub. |

A ruleset file holds exactly the members `name`, `target`, `enforcement`, `conditions`, `rules` and
`bypass_actors`. A live ruleset matches a file's by `name` and `target`; two rulesets are the same when
these six members are equal as JSON values. A member an entry of `repositories.json` does not hold is not
managed: it is never compared and never written. A live ruleset that no file names is reported with
`wanted` null and is never changed or deleted.

The private repository's entry differs: its Actions policy stays `all` (a pattern of allowed actions
applies only to public repositories, so it could not admit the cache action there) and it has no alerts
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

## What an undo does not take back

- Alerts raised and Dependabot pull requests opened while the settings were on: they stay and are closed
  by hand.
- `PUT vulnerability-alerts` also turns the dependency graph on; the alerts it raised and the dependency
  submissions made meanwhile are not taken back by the undo's `DELETE`.
- Workflow runs refused or started under the new Actions policy, and anything a ruleset refused meanwhile.
