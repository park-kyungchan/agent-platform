---
{
  "schema_version": 1,
  "record_id": "example-lesson",
  "kind": "lesson",
  "title": "Invented lesson",
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

## Incident references
example-incident, synthetic occurrence fixture-incident-1.

## Prevention rule
Reuse a record id when correcting the same durable explanation.

## Conditions and exclusions
Applies to same-record corrections; excludes distinct successor decisions.

## Why it worked
The simulated replacement selected the existing fixture document.

## Counterexample
A distinct new decision must not overwrite an unrelated original.

## Verification
Repeat fixture://example/readback-1 and expect one current document.
