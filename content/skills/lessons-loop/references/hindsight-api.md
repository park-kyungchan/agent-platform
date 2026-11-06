# Official Hindsight structured-record route

Read only for authorized structured capture/update. No-write tasks leave a local
curator handoff instead. These are official API operations, not a custom client,
server extension or permission grant. Verify the installed OpenAPI/client before
version-dependent use; this text was reviewed against server 1.1.0, coding-agents 0.8.2,
upstream `a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5`. The host runs server 1.2.0
(revision `34a138684a899b6ec34175f7a2b633860466d289`) and coding-agents 0.8.5 as observed on
2026-11-06; read [HINDSIGHT-RUNTIME.md](../../../runtime-references/HINDSIGHT-RUNTIME.md) for 1.2.0 routes.

## Connection and credentials

- Local official endpoint: `http://127.0.0.1:8888`.
- Shared bank: `coding-agent::shared`; URL-encode bank and document identifiers.
- Token source: `/root/.hindsight/coding-agent.json`, field `apiToken`.
  Read it only for the authorized call, in process. Never print it, put it in
  command arguments, logs, source, shell history or a committed example. Use the
  official client's authentication or an in-memory HTTP Authorization header.
  Do not disable SSH host-key checking or bypass the Tailscale boundary remotely.

## Structured records: retain → PATCH → GET

L0–L3, decisions, Corrections, glossary/profile, strategy and KPI-definition records
use the official HTTP API/client, never title-only MCP ingestion:

1. Read current TASK/STATE and exact target identity. GET the existing document
   if present and preserve approved current tags/content pins and a recoverable
   preimage before mutable replacement. Preserve pinned originals; create a
   linked Correction/new version instead. Resolve uncertain prior effects first.
2. Retain the complete frontmatter/body through
   `POST /v1/default/banks/{bank_id}/memories` (or its official client method),
   with explicit `document_id: mem-v1-<kind>-<scope>-<stable-topic>`,
   `update_mode:"replace"` and incremented authored-record revision. The retain
   request's `items` contain the body/ID/update mode. Direct callers supply the
   actual expanded project/platform provenance; client templates are not defaults.
3. Reconcile official completion and the actual document before proceeding.
   PATCH `/v1/default/banks/{bank_id}/documents/{document_id}` with `tags` equal
   to the complete preserved-current plus approved taxonomy/provenance union.
   PATCH replaces, not appends. Do not erase genuine historical/plugin tags or
   forge `source:*`/`harness:*`. Unchanged tag sets need no redundant PATCH.
4. GET that same document. Check actual ID, complete tags, body/frontmatter,
   revision and content pin. Verify extraction, consolidation and relevant recall
   separately. A successful HTTP response, job ID or page staleness flag is not
   semantic verification. Preserve the source/result evidence and uncertainties.

Tag edits can invalidate observations and requeue co-sources. Same-ID replacement
can retire old facts. Reconcile effects and fix the measured boundary before
retrying; do not manipulate DBs, jobs, updater state or services. Document history
is not a recoverable prior-text archive. Refresh only affected official pages
when separately within actual task authority, then read them back.

MCP `ingest_document(title,content)` and `capture_initiative` are only for simple
unstructured notes and approved initiatives. The document tool derives its ID
from the title slug (with the integration's non-ASCII hash handling); it takes no
tags, arbitrary document ID or separate frontmatter fields. An initiative reuses
its existing page on material change. Neither tool is a route for these structured
records. No-write tasks draft locally and call neither write tool.

KPI definitions and strategy decisions may be durable memory with repository
provenance. Measured KPI values/time series remain with their owner: task/domain
files now, future Ontology later. Do not ingest them as contextual memory.

Official sources: [retain](https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-docs/docs/developer/api/retain.mdx),
[documents](https://github.com/vectorize-io/hindsight/blob/a5a619b5ca59c5c39e0f96be6f7a31fabd136bd5/hindsight-docs/docs/developer/api/documents.mdx).
