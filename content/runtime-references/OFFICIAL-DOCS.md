# Official documentation: when to look it up and where

Read this when a CLI, Herdr, Docker, Tailscale, Hostinger, Hindsight or another tool behaves differently from what you expect and local evidence does not settle it.

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
| Memory recall, retain, page refresh, record update | `CONTEXT-GOVERNANCE.md`; skill `hindsight-control` and its `references/runtime.md`; `GET http://127.0.0.1:8888/openapi.json` | Hindsight. Hooks of a CLI belong to that CLI |
| Git, pull requests, builds, toolchain | `BUILD-AND-DELIVERY.md`; installed `git`, `gh`, `cargo` help | Git, GitHub CLI, Rust and Cargo |
| MCP protocol semantics | The server's own documentation | Model Context Protocol |

## 3. Indexes

Coding CLIs:

| Owner | Index |
| --- | --- |
| Claude Code | https://code.claude.com/docs/llms.txt |
| Anthropic API (wire contracts the Claude Code pages leave open) | https://docs.claude.com/llms.txt |
| Codex | https://learn.chatgpt.com/docs/llms.txt |
| Muse | https://dev.meta.ai/llms.txt |
| Antigravity | https://antigravity.google/llms.txt |
| Grok Build | https://docs.x.ai/llms.txt |

Infrastructure:

| Owner | Index |
| --- | --- |
| Herdr | https://herdr.dev/llms.txt and https://herdr.dev/agent-guide.md |
| Hindsight | https://hindsight.vectorize.io/llms.txt ; API reference https://hindsight.vectorize.io/api-reference |
| Docker | https://docs.docker.com/llms.txt |
| Tailscale | https://tailscale.com/docs/llms.txt |
| Hostinger | https://www.hostinger.com/llms.txt ; API https://www.hostinger.com/api/llms.txt ; VPS API schema https://developers.hostinger.com/openapi/openapi.json |
| Model Context Protocol | https://modelcontextprotocol.io/llms.txt |
| OpenSSH | https://www.openssh.com/manual.html |
| systemd | https://www.freedesktop.org/software/systemd/man/latest/ |
| Git and GitHub CLI | https://git-scm.com/docs ; https://cli.github.com/manual/ |
| Rust and Cargo | https://doc.rust-lang.org/cargo/ ; https://doc.rust-lang.org/rustc/ |
| Node.js and Bun | https://nodejs.org/api/ ; https://bun.sh/docs |

Known dead routes, so nobody retries them: `https://developers.hostinger.com/llms.txt` returns 404; `https://tailscale.com/kb/llms.txt` redirects to an HTML page; the Hindsight `/sdks/mcp` page returns 404. Do not guess an `llms-full.txt` address.

When you verify a correction to this file, change it in the `agent-platform` source through review.
