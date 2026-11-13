---
name: hindsight-control
description: Read when memory returns stale or irrelevant results, before any Hindsight write, replace, delete, retraction, page or refresh operation, or before claiming that such an operation took effect.
---

# Hindsight content control

Memory returned a stale or irrelevant result and you must find out why: read [how storage, retrieval and pages work](references/runtime.md) first; it names where to look, cheapest first. To change content, continue here.

Labels classify server claims; steps are caller obligations. Read [evidence and earlier source notes](references/measured-20261112.md). B=`/v1/default/banks/{bank_id}`; encode path segments.

## Version and authority

1. **measured** — API/MCP reported 1.2.0 on 2026-11-12. Read version with `curl --fail --silent --show-error http://127.0.0.1:8888/version` (`api_version`). If different, findings are unverified until re-measured.
2. **unknown** — Obtain task authority; visible tools grant none. Save recovery text; history is no established archive.

## Retain and replace (U3/U4/U14)

1. **measured** — POST B/memories accepts `items`, `async:true`, `operation_id`; known UUIDs ignore changed bytes. UUIDv8 worked; new ids create new operations even for identical text (U3). **Caller obligation:** bind id to payload hash and reject mismatches locally; the server does not enforce this.
2. **measured** — Poll parent/children; GET B/documents/{id}, compare original_text/hash; exhaust B/memories/list?document_id={id} for facts/tags. LF/CRLF/spacing survived.
3. **measured** — Save prior GET; retain same document_id with `update_mode:"replace"`/new bound id; read document/all facts: old facts/tags removed here. PATCH B/documents/{id} tags replaces the set; read document/fact tags, duplicates included. Old text can remain in derived pages/models/search after replace until refresh or model deletion (U10); read each before claiming the change.
4. **Caller obligation:** before ready, check authored retrieval terms against extracted facts/entities using the [MEASURE-H readback requirements](references/measured-20261112.md#ancestor-tags-extraction-shape-and-readable-profile-measured-on-api-120).

## Delete and retract (U5/U7/U15/F1)

1. **measured** — DELETE B/documents/{id}; read document=404, paginated facts total=0 and chunks absent. Absent DELETE=404; bad auth=401. Inspect before retry. Old model/page text, snippets and based_on can remain.
2. **measured** — For single-source/page retraction, settle earlier work; read tree for page/model ids and model GET for trigger. Created with trigger.refresh_after_consolidation:false, refresh_cron:null, full_refresh_cron:null (039,201). **unknown** — Changing existing triggers was not measured: require those GET values and no active work; otherwise stop for owner coordination. Use bank preflight below; keep markers out of metadata; delete source/prove step 1.
3. **measured** — DELETE B/mental-models/{id} for affected models. Require model, B/knowledge-base/pages/{page_id} and model /history GET=404. Read tree without node, REST/MCP searches without marker, old MCP page id with missing-page body, subject recall without source marker. Report “absent on the listed reads”, never index completeness.
4. **measured** — After deletion readback, same-name/parent recreation gets NEW ids; read body="", snippets="No content yet." without sources.

## Pages and refresh (U6/U7/F1/F2)

1. **measured** — POST B/knowledge-base/pages with name/source_query/parent_id/tags/max_tokens/trigger; save page_id/mental_model_id/operation_id; reconcile by tree/page/model GET. Duplicate same-parent page=409; duplicate folders get new ids. Nest fact_types/exclude_mental_models/tags_match and refresh fields inside trigger.
2. **measured** — PATCH B/mental-models/{id} is metadata-only: content:"" got 200/name changed/text unchanged on model/page/search/known-id reads. Read metadata; never PATCH for removal.
3. **measured** — POST B/mental-models/{id}/clear emptied page/search text, no observed new operation/LLM call; full model GET kept reflect_response.text/based_on for eight polls. Read full GET; clear is no retraction.
4. **measured** — Save model GET; POST B/mental-models/{id}/refresh with mode:"full", poll details.outcome; read full model/page/search. Nonempty refresh rewrote sources; empty refresh preserved deleted text with content_preserved_no_new_facts/new timestamp/is_stale=false/based_on={}. Use model DELETE for last-source retraction.
5. **measured** — Read dry-run-refresh/history as diagnostics: deleted-source preview kept stale text with based_on={}, retraction=null, would_persist=false. No completeness proof; dry-run submits generation work.

## Consolidation and operations (U11/U14/F3/F4/U1)

**measured (1.2.0; 465,470–475)** — Model lists use items/total/limit/offset without a completeness flag; empty items alone is insufficient: use [model-list preflight](references/measured-20261112.md#mental-model-list).
**measured (1.2.0; 467–469)** — Bank creation exposes only enable_observations; PATCH config and read back both flags before work.

1. **measured** — Bank-wide flags were set before notes: PATCH B/config updates enable_observations:false, enable_auto_consolidation:false; GET config proves values (005/006,U11). Never change a bank other writers use without its owner’s word. [] scopes still consolidated under enabled defaults (F3); "none" was rejected. **unknown** — Effect on already pending work is unmeasured; pending_consolidation can remain nonzero. Do not treat OFF as cancellation; inspect operations/stats.
2. **measured** — Exhaust B/operations limit/offset/total; GET B/operations/{id} for parent AND child; include_payload helps attribution. Refresh carries mental_model_id, retain document_id. error_message coexisted with pending parent/retrying child; never overlap a new identity while pending. Cancelled is not failed.
3. **measured** — Initialize MCP at /mcp/{bank_id}/, parse SSE, carry returned mcp-session-id and inspect bodies. Missing-page read returned HTTP 200, isError=false and structuredContent.error. Inspect body despite success flags.

## Never conclude from

**measured** — HTTP 200, completed, is_stale=false, empty based_on, a timestamp, top-k absence or a 200 PATCH failed to prove effects. Use exact readbacks; recall/search supplied no completeness barrier (U10).

## Unknown: do not assume

Read the [unmeasured boundaries](references/measured-20261112.md#unmeasured-boundaries); do not generalize beyond the tested reads, sizes or lifecycle.

## New measurement and upkeep

1. **unknown** — Obtain task authority/bounds for new measurement; tools grant none.
2. **measured** — Use one disposable zz-measure-<date>-… bank; record schema/version and other-bank counts before.
3. **measured** — Consume only HINDSIGHT_TENANT_API_KEY from /docker/hindsight/.env in process; pass via stdin/FD, never print it or put it in argv/a file. Check redacted recordings with the prefix passed via stdin/FD, never argv/file; output only a match count, never key/prefix or matching lines.
4. **measured** — Use invented marked notes, one request at a time, ≥1s between writes, bounded polling/budgets; stop on three consecutive write errors. Resolve active work, delete bank, prove absence by bank list/document GET/marker search; compare other-bank counts.
5. **unknown** — Update each measured row with its recording in the same pull request as the work needing it. Mark unrecorded claims “not measured”; replace contradictions, never keep both.
