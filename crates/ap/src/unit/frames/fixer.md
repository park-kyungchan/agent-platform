## A repair round

The verifier of your pull request returned `Verdict: repair`. Its report is the file named under "The round" below; the page above is the page you built from. Read the report completely, then the page again.

- Work only on the report's BLOCKING findings. Its notes are not your task in this round: take none of them.
- A blocking finding that can only be met by changing what the page fixes (its interface, an invariant, an acceptance check, "Not built"): change nothing for it, and say so in your final message with the rule of the page and the reason. The page is changed only by the Lead.
- A blocking finding you can show to be wrong by a command or a test: change nothing for it, and give the command and its output.
- Every other blocking finding: fix it in the same worktree and branch, as new commits (no rebase, never a forced push), with a test that fails before the fix where the finding is a behaviour.
- Then, in this order: `git fetch origin`, and when `origin/main` is not an ancestor of your head, `git merge origin/main`; the repository's check through `ap check <worktree>` until it passes; each acceptance check of the page as before; `git push origin HEAD` as one command of its own; ONE comment on the pull request that says per blocking finding what changed, or why nothing changed (no account name, session id, host path outside the repository, task-internal word, real name or private text).
- Two commands leave your view through the stage's door: `ap check <worktree>`, which so still reaches the local PC although `tailscale` does not work here, and `echo '{"parameters":{"words":"<what you look for>","limit":5}}' | ap fn route`, which answers with the files, lines and headings where a fact lives, so ask it before you scan a repository.
- The rules of "How you work" above hold, but for three things this section replaces: this is your own earlier session, not a fresh one; you open no second pull request; and your final message is the one below, not the page's report.

Your final message, per blocking finding, one of: fixed (how, and the test that pins it); not changed because the page would have to change (the rule, the reason); shown wrong (the command). Then the new head, or the words "no commit".
