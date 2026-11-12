---
name: hindsight-control
description: Read before any Hindsight write, replace, delete, retraction, page or refresh operation, or before claiming that such an operation took effect.
---

# Hindsight content control

Labels classify server claims; steps are caller obligations. Read [the evidence table](references/measured-20261112.md) for limits. B=`/v1/default/banks/{bank_id}`; encode path segments.

## Version and authority

1. **measured** — API/MCP reported 1.2.0 on 2026-11-12. Read version with `curl --fail --silent --show-error http://127.0.0.1:8888/version` (`api_version`). If different, these findings are unverified until re-measured. Revision 34a1386 was not independently re-inspected.
2. **unknown** — Obtain task authority for bank/content/effects; visible tools grant none. Save recovery text before replacement; history is not an established text archive.

## Retain and replace (U3/U4/U14)

1. **measured** — Bind UUID operation_id to the payload hash locally before POST B/memories with `items`, `async:true`, `operation_id`. Known ids ignore changed bytes; UUIDv8 worked. New ids create new operations even for identical text. Reject mismatches locally.
2. **measured** — Poll parent/children below; then GET B/documents/{id}, compare original_text bytes/hash, and exhaust B/memories/list?document_id={id} pagination; check facts/tags. LF/CRLF/spacing survived.
3. **measured** — Save prior GET; replace same document_id with `update_mode:"replace"` and a new bound operation id. Read document/all facts: old facts/tags were removed here. PATCH B/documents/{id} tags replaces the full set; read document and fact tags, including duplicates. Read affected pages separately.

## Delete and retract (U5/U7/U15/F1)

1. **measured** — DELETE B/documents/{id}; read document=404, paginated facts total=0 and chunks absent. Absent DELETE=404; bad authorization=401. Inspect before retrying uncertain deletion. Old model/page text, search snippets and based_on can remain.
2. **measured** — For measured single-source/page retraction: settle earlier work, disable automatic page triggers/use false bank flags below; read tree for page/model ids. Keep markers out of metadata. Delete source/prove step 1.
3. **measured** — DELETE B/mental-models/{id} for affected models. Require model GET, linked B/knowledge-base/pages/{page_id} GET and model /history GET=404. Read tree without the node, REST/MCP searches without the removed marker, old MCP page id with a missing-page body, and subject recall without the source marker. Report “absent on the listed reads”, never index completeness.
4. **measured** — After deletion readback, optional same-name/parent recreation returns NEW ids. Read body="" and snippets="No content yet." in the no-source case.

## Pages and refresh (U6/U7/F1/F2)

1. **measured** — POST B/knowledge-base/pages with name/source_query/parent_id/tags/max_tokens/trigger; save page_id/mental_model_id/operation_id; read tree/page/model to reconcile. Duplicate same-parent page=409; duplicate folders get new ids. Put fact_types, exclude_mental_models, tags_match, refresh_after_consolidation, refresh_cron and full_refresh_cron inside trigger; both cron fields were null here.
2. **measured** — PATCH B/mental-models/{id} is metadata-only: content:"" got 200/name changed/text unchanged on model/page/search/known-id reads. Read metadata; never PATCH for removal.
3. **measured** — POST B/mental-models/{id}/clear emptied served page/search text, no observed new operation/LLM call; full model GET kept reflect_response.text/based_on for eight polls. Read full GET; clear is no retraction.
4. **measured** — Save model GET; POST B/mental-models/{id}/refresh with mode:"full", poll details.outcome; read full model/page/search. Nonempty refresh rewrote sources; empty refresh completed with content_preserved_no_new_facts, advanced timestamp, is_stale=false and based_on={} while preserving deleted text. Use model DELETE for last-source retraction.
5. **measured** — Read dry-run-refresh/history as diagnostics only: deleted-source preview kept stale text with based_on={}, retraction=null and would_persist=false. Neither proves provenance completeness; dry-run submits generation work.

## Consolidation and operations (U11/U14/F3/F4/U1)

1. **measured** — For consolidation OFF, PATCH B/config with updates: enable_observations:false and enable_auto_consolidation:false; GET config to prove flags. [] scopes consolidated like omitted scopes under enabled defaults; "none" was rejected. Read stats: pending_consolidation can remain nonzero.
2. **measured** — Exhaust B/operations limit/offset/total; GET B/operations/{id} for parent AND child, include_payload helps attribution. Refresh carries mental_model_id, retain document_id. error_message coexisted with pending parent/retrying child; never overlap a new identity while pending. Cancelled is not failed.
3. **measured** — Initialize MCP at /mcp/{bank_id}/, parse SSE, carry returned mcp-session-id and inspect bodies. Missing-page read returned HTTP 200, isError=false and structuredContent.error. Inspect the body despite success flags.

## Never conclude from

**measured** — HTTP 200, operation completed, is_stale=false, empty based_on, an advancing timestamp, top-k absence or a 200 PATCH failed to prove effects. Use exact readbacks; recall/page search supplied no completeness barrier (U10).

## Unknown: do not assume

- **unknown** — Terminal retain failure and same/new-id retry: do not assume retry semantics; F4 stayed active within its bound.
- **unknown** — Operation retention horizon: do not assume old ids/results remain discoverable.
- **unknown** — Limits: do not assume tested sizes are maxima.
- **unknown** — Attachment lifecycle: do not assume text-only deletion findings apply.
- **unknown** — Real redirects: do not assume credential safety across them; clients did not follow redirects.
- **unknown** — Arbitrary-query index completeness: do not assume absence beyond the listed reads or tested banks.

## New measurement and upkeep

1. **unknown** — For unrecorded behavior, obtain task authority and bounds first; a visible tool is not permission.
2. **measured** — Reuse the safe method: one disposable bank named zz-measure-<date>-…; record schema/version and other-bank counts before.
3. **measured** — Consume only HINDSIGHT_TENANT_API_KEY from /docker/hindsight/.env in process; pass via stdin/FD, never print it or put it in argv/a file. Record redacted requests/answers; check its prefix, print only match count.
4. **measured** — Use invented marked notes, one request at a time, at least one second between writes and bounded polling/budgets; stop on three consecutive write errors. Resolve active work, delete the bank, prove absence by bank list/document GET/marker search and compare other-bank counts afterward.
5. **unknown** — Correct/add each newly measured row and its recording in the same pull request as the work needing it. Mark an unrecorded statement “not measured”; replace contradicted statements rather than keeping both. Never guarantee unmeasured behavior.
