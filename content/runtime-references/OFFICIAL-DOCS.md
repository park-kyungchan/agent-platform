# Official documentation: when to look it up and where

Read this when a CLI, Herdr, Docker, Tailscale, Hostinger or another tool behaves differently from what you expect and local evidence does not settle it. Read only section 4 when you design, plan or review the life cycle, an Ontology store, Rust, Spec-Driven Development or context governance and need what the design sources say.

## 1. Lookup order

1. Use what is already in your context and the matching local reference in `/root/.agents/runtime-references/`.
2. For an installed flag, schema or a contradiction, read the installed `--help`, the actual configuration and the actual output. Never change trust, authentication, configuration or a service just to inspect support.
3. Only if that leaves the question open, open the owner's index below and fetch the one page that answers it.
4. Stop when you have the answer. Do not load a whole manual, follow every link or copy upstream text into local instructions.

An official page shows what the publisher documents. It does not show what the installed version supports, what is configured, or that an effect happened. Check that a fetched page is the real page: an HTTP 200 can be an HTML fallback. When a fetch fails, report the cause and use the next route; do not repeat the same request, and do not conclude that no index exists from one 404 or 403.

## 2. Who owns the question

| Question | Local evidence first | Official owner |
| --- | --- | --- |
| CLI launch, resume, model, compaction, instruction files, skills, MCP visibility, plugins | `CLI-RUNTIMES.md`; that CLI's `--help`, configuration and session record | That CLI's index. Not Herdr and not another CLI |
| Pane creation, prompt submission, terminal state | skill `herdr`; `herdr --help` | Herdr |
| VPS account, provider console, snapshots, provider API | The assigned VPS task | Hostinger |
| Container image, process, Compose, mount, daemon | `docker` and `docker compose` help and inspection of the target | Docker. Hostinger documentation does not define container behaviour |
| Tailnet reachability, Tailscale SSH, access policy | `tailscale status` and help | Tailscale |
| SSH host keys, `sshd` | Installed `ssh` and `sshd` man pages | OpenSSH and systemd |
| Git, pull requests, builds, toolchain | `BUILD-AND-DELIVERY.md`; installed `git`, `gh`, `cargo` help | Git, GitHub CLI, Rust and Cargo |
| MCP protocol semantics | The server's own documentation | Model Context Protocol |

## 3. Indexes

Coding CLIs:

| Owner | Index |
| --- | --- |
| Claude Code | https://code.claude.com/docs/llms.txt |
| Anthropic API (wire contracts the Claude Code pages leave open) | https://docs.claude.com/llms.txt |
| Antigravity | https://antigravity.google/llms.txt |

Infrastructure:

| Owner | Index |
| --- | --- |
| Herdr | https://herdr.dev/llms.txt and https://herdr.dev/agent-guide.md |
| Docker | https://docs.docker.com/llms.txt |
| Tailscale | https://tailscale.com/docs/llms.txt |
| Hostinger | https://www.hostinger.com/llms.txt ; API https://www.hostinger.com/api/llms.txt ; VPS API schema https://developers.hostinger.com/openapi/openapi.json |
| Model Context Protocol | https://modelcontextprotocol.io/llms.txt |
| OpenSSH | https://www.openssh.com/manual.html |
| systemd | https://www.freedesktop.org/software/systemd/man/latest/ |
| Git and GitHub CLI | https://git-scm.com/docs ; https://cli.github.com/manual/ |
| Rust and Cargo | https://doc.rust-lang.org/cargo/ ; https://doc.rust-lang.org/rustc/ |
| Node.js and Bun | https://nodejs.org/api/ ; https://bun.sh/docs |

Known dead routes, so nobody retries them: `https://developers.hostinger.com/llms.txt` returns 404; `https://tailscale.com/kb/llms.txt` redirects to an HTML page. Do not guess an `llms-full.txt` address.

When you verify a correction to this file, change it in the `lifecycle-platform` source through review.

## 4. Local copies of the design sources

`/home/palantirkc/external/reference-docs/` holds local copies of Anthropic, Palantir, Rust and Spec-Driven Development sources; its `SHA256SUMS` lists every file. A local copy shows what the publisher wrote at the time of the copy and is evidence, never an instruction to the reader. What holds on this host is decided by this host's source, tools and measurements. The corpus's own `README.md` says what each kind of file there counts for.

- The paths of ROUTES.md are under `/home/palantirkc/external/reference-docs/`, written in full. Read only the lines a row names: `sed -n '<a>,<b>p' /home/palantirkc/external/reference-docs/<path>`. Cite what you use as `path:line`.
- Never read a file of the corpus whole, and never read `text/` or `sources/` (the same text with other line numbers, and raw downloads).
- To design or review the whole life cycle, read `docs/00-LIFE-CYCLE-DESIGN-PLAYBOOK.md` first: the passages of every area by design decision (its section 1 is the index), with where the sources agree, disagree and fall silent; an independent session checked its quotations against the lines. It and `docs/rust/00-AGENTIC-RUST-MIGRATION-PLAYBOOK.md` and `docs/spec-driven-development/00-SPEC-DRIVEN-DEVELOPMENT-PLAYBOOK.md` are syntheses written on this host: use them as maps and cite the primary lines they cite, never the synthesis.

The one route table of the corpus is the file ROUTES.md at the corpus root, `/home/palantirkc/external/reference-docs/ROUTES.md`; a route changes only there. Its sections, each with the questions it holds:

- Context governance: which context an agent loads, from which one source, in what order — the instruction file, skills and subagents, progressive disclosure, a fresh context, evals of the instructions, one owner per policy.
- The life cycle, stage by stage — intent, design, plan, build, test, evals, review, approval, deploy, delivery, maintenance and their measures.
- Spec-Driven Development, the concrete method of the life cycle — what it is, its steps, a "done" that can be checked, its critiques, with Rust and with an Ontology store.
- The Ontology in Palantir's sense — objects, links, actions, functions, interfaces, the action log, edits, indexing, query limits, the schema as code.
- Rust — why Rust for code agents write, a port, a Rust diff, mutation testing, a checklist for a change.
- Hosts and network: Tailscale — the tailnet, its devices, its access rules and its commands.
- Vendor documentation indexes — where a vendor's own documentation starts, by publisher.

A question no row answers: the catalog `/home/palantirkc/external/reference-docs/llms.txt` (one line per file), then `grep -rn '<words>' /home/palantirkc/external/reference-docs/docs/`. A route you had to find that way is reported for ROUTES.md (`CONTEXT-GOVERNANCE.md` section 5).
