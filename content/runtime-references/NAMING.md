# Names on this host

Read this when you name a new repository, crate, binary, MCP server or tool, task directory, branch or file, or when you must tell which of two names is the right one.

## 1. Long and short name

Every platform has one long name and one short name, and no third form. The long name is for the GitHub repository, its main checkout and installed directories. The short name is for crates, binaries, task scopes, branch prefixes, MCP servers and tool prefixes.

| Thing | Long name | Short name |
| --- | --- | --- |
| The agent platform | `agent-platform` | `ap` |
| The Ontology platform | `ontology-platform` | `ont` |
| The education platform | `education-platform` | `edu` |
| Remote host and Hermes operations | `agent-vps-operations` | `vps-ops` |
| Work on the whole host | none | `ws` |
| Long-term memory work | none | `mem` |

- `academy` is the private-academy teaching domain inside `edu` (crates `edu-academy-*`, MCP server `academy`, tools `academy_*`). It is not another word for `edu`.
- Not used as a task scope or branch prefix: `agent-`, `ontology-`, `workspace-`, `academy/`.

## 2. Pattern per kind

| Kind | Pattern |
| --- | --- |
| Repository and main checkout | `<word>-platform`, or `<platform word>-<module>` for a module; the checkout has the repository's name under `/home/palantirkc/platforms/` |
| Crate and its directory | `<short>-<role>` or `<short>-<domain>-<role>` |
| Binary | The crate's name, or the short name alone for the platform's one command |
| MCP server | The short name or the domain, no suffix |
| MCP tool | `<server>_<object>_<verb>`, or `<server>_<verb>` when there is no object; lower case with underscores |
| Task directory | `<scope>-<topic>`, scope one of `ap`, `ont`, `edu`, `ws`, `mem`; no date in the name |
| Task worktree | `work/<scope>-<topic>/wt/<repo>/` |
| Branch | `<short>/<topic>-<YYYYMMDD>` |
| File of a task or an instruction | Upper-case words joined by hyphens, extension `.md`; a packet is `TASK-<id>.md`, its report `REPORT-<id>.md`, a review `REVIEW-<what>.md`, a design note `DESIGN-<topic>.md` |
| Shared skill | Lower case, hyphen, no date |
| Top-level directory of `/home/palantirkc` | One lower-case word from `/home/palantirkc/work/ws-layout-standard/LAYOUT.md`, plus `tools` |
| Installed first-party | `/usr/local/bin/<binary>`; state and previous binaries under `/var/lib/<long name>/` |
| Other files in a repository | That repository's own convention |

## 3. Names that break a pattern

- A closed task directory, an archived repository and a third-party tool keep their names.
- An older name is renamed only in the order and on the conditions of `/home/palantirkc/work/workspace-strategy-reassessment-20261109/naming-20261111/NAMING-STANDARD.md`. Do not rename one because it breaks a pattern.
- Known exceptions that stay until that order reaches them: the checkout of `education-problem-edit` at `/home/palantirkc/work/problem-edit`; the MCP server name `academy_operations`; task directories with a date or a long scope word.
- Do not copy an exception: a new thing follows section 2.
