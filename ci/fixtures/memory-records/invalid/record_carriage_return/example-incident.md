---
{
  "schema_version": 1,
  "record_id": "example-incident",
  "kind": "incident",
  "title": "Invented incident",
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

## Occurrence
Syntheticoccurrence fixture-incident-1, independently invented on 2026-01-02.

## Expected and actual behavior
Expected one fixture document; actual fixture output contained two.

## Boundary
The invented writer selected a new document id during a correction.

## Impact
Two conflicting fixture answers; no real service or user was affected.

## Cause
Verified in fixture://example/writer-1: the fixture writer derived identity from the title.

## Mitigation
The simulated correction reused the original stable id.

## Verification
fixture://example/readback-1 reports one corrected fixture document.
