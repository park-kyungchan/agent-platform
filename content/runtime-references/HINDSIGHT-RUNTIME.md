# Hindsight runtime: how storage, retrieval and pages actually work

Read on demand before changing memory content, knowledge pages or bank configuration, or before
diagnosing "memory returns old/irrelevant facts". Verified 2026-11-06 against the installed server
1.2.0 (image `ghcr.io/vectorize-io/hindsight@sha256:19d0c08f…`, revision `34a1386`) and coding-agents
0.8.5. After an upgrade, re-check the cited source locations before relying on a number below.

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
  different scope cannot update a stale observation. Fix: `PATCH /documents/{id}` with
  `observation_scopes` (re-consolidates, without re-extraction, only memories whose resolved scopes change).
- **Mental models** hold synthesized content. A **knowledge page** (`kp-…`) wraps a mental model
  (`mm-…`); create pages with `POST /knowledge-base/pages` (`name`, `source_query`, `tags`,
  `max_tokens`, `trigger`). A page's `tags` SCOPE which memories it reads (`tags_match` defaults to `all_strict` when tags
  are present; `trigger.tags_match` can override it); `[]` reads the whole bank, so its `source_query` must carry the exclusion rules.

## Retrieval

**Recall** (`POST /memories/recall`): four arms (semantic, BM25, graph, temporal) fused by RRF (k=60).
A cross-encoder reranks the top candidates per budget (host: low 40, mid 80, high 300), then
recency (linear, 365 days), temporal and proof-count boosts apply. Host models: embeddings
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

## Native client ownership — 2026-11-07

All five native CLI configurations directly declare the same coding-agents MCP implementation; the client harness IDs are `muse`, `claude-code`, `codex`, `antigravity-cli` and `grok-build`. Grok foreign MCP imports are off. Backend bank/routing is shared through native Hindsight configuration, not through another CLI's user config. Existing credential-file references, path overrides and integration code are preserved. Native hooks exist only where configured/supported; a direct MCP entry does not create lifecycle injection or retention for Muse/Grok. See runtime-wiring.md for the custom/vendor boundary and native evidence.

## Session path on this host

`autoInject: "pages"`: on a session's first prompt the plugin runs a page search with that prompt and
injects the hits; `pageRefreshEveryTurns: 5` refetches the page roster and tool guide every five user
turns. `paths.<prefix>` overrides apply per directory (`{"disabled": true}` makes memory inert there).
`retainSessions` controls transcript write-back; retained transcripts carry dated status snapshots
that later compete with corrections.

## Write semantics and costs

- Same-`document_id` retain with `update_mode: "replace"` retires the old facts and re-extracts.
  Save the GET body first; document history is not a recoverable text archive.
- `PATCH /documents/{id}` `tags` replaces the complete tag set; it can invalidate observations and
  requeue co-sources. An unchanged set needs no PATCH.
- `PATCH /mental-models/{id}` changes `name`/`source_query`/`max_tokens`/`tags`/`trigger`. Refresh:
  `POST /mental-models/{id}/refresh` with `{"mode": "full"}` regenerates from scratch, or no body to run the model's own
  `trigger.mode` (default `full`; `delta` edits sections and falls back to full when there is no
  baseline or `source_query` changed). Full refresh can drop content (observed 70,131 to 13,857
  characters), so save the model GET first. `POST …/dry-run-refresh` previews diff and usage without
  writing, but costs the same LLM tokens as a real refresh.
- Bank levers (`GET/PATCH /config`, body `{"updates": {...}}`): `retain_mission`,
  `retain_extraction_mode` (`concise`), `retain_custom_instructions`, `observations_mission`,
  `reflect_mission`, `enable_*` flags. They affect future processing only. Output language is
  server-level only (`HINDSIGHT_API_LLM_OUTPUT_LANGUAGE`, forces facts, observations and reflect output
  into one language); a bank PATCH of `llm_output_language` is rejected.
- LLM: provider `chatgpt` (subscription, `gpt-6.1-terra`). It shares the ChatGPT/Codex weekly limit
  (user-confirmed 2026-11-06). Observed refresh cost: 90 to 210 seconds per page. Per-model monthly
  input ceiling: 5,000,000 tokens (user 2026-11-06).

## Checks before claiming an effect

Operation `completed` is not semantic success. Read back the document, facts
(`GET /memories/list?document_id=`), model content and consolidation queue (`GET /stats`
`pending_operations` and `pending_consolidation` = 0), then re-run the affected recall or page
search and judge the result.
