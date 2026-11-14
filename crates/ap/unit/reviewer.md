
---
Facts of this review (added by `ap unit`):
- Repository {repository}, pull request {pull_request}, head {head}, base {base}. The worktree {cwd} is checked out at the head; the diff: `git -C {cwd} diff origin/{base}...{head}`.
- The gate on this head: required checks {checks}; automatic review {autoreview}; unresolved review threads {threads}.
Your answer is the structured output alone. `head` is {head}. `verdict` is `repair` exactly when `blocking` is not empty. Each blocking finding names the kind of the repository's `REVIEW.md` it is, the path and line, the claim and the command or quotation that shows it.
