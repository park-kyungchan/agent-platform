---
{
  "schema_version": 1,
  "record_id": "example-rationale",
  "kind": "rationale",
  "title": "Invented rationale",
  "subject": {
    "type": "platform",
    "id": "synthetic-example"
  },
  "scope": "Invented offline fixture only",
  "owner_ref": "none",
  "owner_ref_reason": "Invented offline example",
  "sources": [
    {
      "date": "2026-01-02",
      "locator": "fixture://example/evidence"
    }
  ]
}
---

## Decision reference
fixture://store/decision-1 (invented store decision Id).

## Question
Why did the invented example choose a stable document id?

## Reasoning
A correction can replace the same retrieval document.

## Tradeoffs
The writer must preserve identity through topic renames.

## Alternatives
Rejected a new document id on every edit because it leaves duplicate answers.

## Assumptions
The example store retains the stable id.

## Counterevidence
A genuinely distinct successor needs its own id and a predecessor link.

## Revisit trigger
Revisit if the retrieval service loses same-id replacement.
