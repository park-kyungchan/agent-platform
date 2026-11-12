# Hindsight runtime: how storage, retrieval and pages actually work

Read on demand when diagnosing "memory returns old/irrelevant facts". For content control,
read [hindsight-control](../skills/hindsight-control/SKILL.md), including bank flags and effect proofs.
Earlier retrieval/source reading: 2026-11-06, server reference pin 1.2.0
(image `ghcr.io/vectorize-io/hindsight@sha256:19d0c08f…`, revision `34a1386`), coding-agents 0.8.5.
**measured** — API/MCP reported 1.2.0 on 2026-11-12; those measurements did not independently
re-inspect revision `34a1386`. After upgrades, re-check source and remeasure behavioral claims.

## Engineering route and authority

The operational home is `agent-platform` `memory/README.md`: Git originals,
one native publisher and scoped native recall/exact-document reads. Old mixed-bank
pages, reflect, direct ingestion and plugin capture are not the engineering route.
The source runbook is preparation only; registration/content holds remain until
qualification and owner cutover. Historical/lesson behavior below is not proof of
an activated engineering route or of current embedding/reranker weights.

## Where to look (cheapest first)

| Need | Route |
| --- | --- |
| Endpoint and field contract of THIS install | `GET http://127.0.0.1:8888/openapi.json` (authoritative over the website) |
| Exact behaviour (ranking, scopes, refresh) | container source `/app/api/hindsight_api/` via `docker exec hindsight-hindsight-1 …`: `engine/memory_engine.py` (`search_knowledge_pages`, mental-model writes), `engine/memories/pg/recall.py`, `engine/consolidation/consolidator.py` (observation scopes), `api/http.py` (endpoint descriptions), `engine/mental_model_refresh.py`, `config.py` (every tunable with comments) |
| Official docs, short index | https://hindsight.vectorize.io/llms.txt (about 4 KB) |
| Official docs, full dump | https://hindsight.vectorize.io/llms-full.txt (about 1 MB; never load whole, fetch and grep a topic). Topic pages: `/developer/retrieval`, `/developer/api/recall`, `/developer/retain`, `/developer/observations`, `/developer/mental-models`, `/developer/knowledge-pages`, `/developer/api/documents`, `/sdks/integrations/coding-agents` |
| Host operation and pins | `/docker/hindsight/HINDSIGHT_OFFICIAL_PER_CALL.md`, `/docker/hindsight/AGENTS.md` |
| Structured record write procedure | `/root/.agents/skills/lessons-loop/references/hindsight-api.md` |
| Plugin settings meaning | `/root/.hindsight/coding-agents/skill/SKILL.md` (configuration table); host values in `/root/.hindsight/coding-agent.json` (contains a credential: read fields in process, never print) |

## Data model

Endpoint paths below are relative to `/v1/default/banks/{bank_id}`.

- A **document** (`document_id`) is retained text. Extraction turns it into **memory units**: `world`
  and `experience` facts, each with `mentioned_at`, optional `occurred_start`, tags and entities.
- **Observations** are consolidated beliefs built from several facts. Consolidation only merges facts
  inside the same resolved **observation scope** (`observation_scopes`: `combined` = the full tag set, default unless the bank sets
  `default_observation_scopes`; `per_tag`, `all_combinations`, `shared` = one global untagged scope; or
  explicit tag lists). A correcting fact in a
  different scope cannot update a stale observation. For scope changes and verification, use the content-control skill; a scope change alone is not a stale-page repair.
- **Mental models** hold synthesized content. A **knowledge page** (`kp-…`) wraps a mental model
  (`mm-…`); create pages with `POST /knowledge-base/pages` (`name`, `source_query`, `tags`,
  `max_tokens`, `trigger`). A page's `tags` SCOPE which memories it reads (`tags_match` defaults to `all_strict` when tags
  are present; `trigger.tags_match` can override it); `[]` reads the whole bank, so its `source_query` must carry the exclusion rules.

## Retrieval

**Recall** (`POST /memories/recall`): four arms (semantic, BM25, graph, temporal) fused by RRF (k=60).
A cross-encoder reranks the top candidates per budget (host: low 40, mid 80, high 300), then
recency (linear, 365 days), temporal and proof-count boosts apply. Earlier source-reported models (not current runtime proof): embeddings
`BAAI/bge-small-en-v1.5` (English-only, 384 dimensions, 512-token input); reranker
`bge-reranker-v2-m3` (multilingual); BM25 uses an `english` text-search configuration.
Consequences:
- Wording and entity form decide matching: a correction that says "PR98" does not compete with
  facts that say "PR #98". Write records in the terms agents actually ask with.
- Korean text is weak on the semantic and BM25 arms; only the reranker reads it well.
- Meta sentences in a record (revision notes, review procedure) become facts and take top-5 slots.

**Knowledge page search** (`GET /knowledge-base/search`, used by `hindsight_search_knowledge_pages`
and by `autoInject: "pages"`): BM25 over page name + content, plus ONE embedding of
`"name content"` truncated to the first 512 tokens, fused by RRF (k=60). **No reranker.** The returned
snippet is the first 280 characters of the content. Consequences:
- The page's opening (about the first 2,000 characters) decides the vector rank and the snippet.
  Identical boilerplate openings ("Scope and authority …") make pages indistinguishable.
- A broad multi-topic page has one vector and loses to focused pages on specific questions.

## Session path on this host

`autoInject: "pages"`: on a session's first prompt the plugin runs a page search with that prompt and
injects the hits; `pageRefreshEveryTurns: 5` refetches the page roster and tool guide every five user
turns. `paths.<prefix>` overrides apply per directory (`{"disabled": true}` makes memory inert there).
`retainSessions` controls transcript write-back; retained transcripts carry dated status snapshots
that later compete with corrections.
Which CLIs have hooks and which have only the MCP server: `CLI-RUNTIMES.md` section 6.

## Write semantics and costs

Content-control procedures and their measured limits have one home:
[hindsight-control](../skills/hindsight-control/SKILL.md). Read it before any write or effect claim.

- LLM: provider `chatgpt` (subscription); the model is set in the server configuration. It shares the
  ChatGPT/Codex weekly limit. When that limit is reached, extraction, consolidation, reflect and page
  refresh fail; page search and page read still work. In that state do not retry a failed write or
  refresh and do not make a task's acceptance depend on one. Observed refresh cost: 90 to 210 seconds
  per page. A dry-run refresh also costs generation work; the earlier source reference describes
  equal LLM-token cost, not independently measured by the content-control experiments.
- Output language is server-level (`HINDSIGHT_API_LLM_OUTPUT_LANGUAGE`); the earlier source reference
  reports rejection of a bank `llm_output_language` PATCH. These experiments did not remeasure it.

## Checks before claiming an effect

Use the operation-specific readbacks and status limits in
[hindsight-control](../skills/hindsight-control/SKILL.md); this runtime reference supplies no separate
content-effect predicate. Session injection and retention paths are described above.
