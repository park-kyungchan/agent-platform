# Engineering memory originals
A record is one durable explanation with dated sources and one subject.
Git owns its original bytes and history; records are outside installed `content/`.
Exclude transcripts, STATE/status copies, KPI values, worker runs, receipts,
credentials and student data. The private Academy archive stays outside this repo.
Checks neither authorize effects nor adopt proposed rules.

## Header grammar
`records/<record_id>.md` is UTF-8 without BOM: first line `---`, a JSON object,
a line `---`, then Markdown. Split on line feed only; no carriage returns anywhere.
Use unique JSON keys and exactly these fields:
`schema_version` (integer 1), `record_id`, `kind`, `title`, `subject`, `scope`,
`owner_ref`, `sources`. `title` and `scope` are nonempty strings.
`record_id` matches the filename: lowercase ASCII letters/digits separated by
single hyphens; keep it stable through edits and topic renames.
`subject` is exactly `{ "type": "platform", "id": "agent-platform" }`, with
nonempty id and type from platform, module, host, task, procedure, user, term,
memory-map. Profiles use user; explanations use term; navigation uses memory-map.
`owner_ref` is a store object Id (`object_type`, nonempty string-valued `namespace`,
`native_key`), or `{ "repository": "agent-platform", "path": "path/to/source" }`,
or `"none"`. Only `none` requires/allows the nonempty string `owner_ref_reason`;
a cross-task rationale without a store decision uses reason `spans tasks`.
`sources` is a nonempty array of exactly `{ "date": "YYYY-MM-DD", "locator": "..." }`
objects; each date is a real calendar date and each locator a nonempty string.
No status, manual revision counter or supersession field belongs in the file.

## Kinds and required body sections
Each required section is a unique, nonempty second-level heading (`## Name`).

| Kind | One-sentence test | Required sections (exact English names) |
| --- | --- | --- |
| rationale | Explains why an owned decision was made, referencing its store Id; only a cross-task decision without one states the decision here. | Decision reference; Question; Reasoning; Tradeoffs; Alternatives; Assumptions; Counterevidence; Revisit trigger |
| incident | Records one identified occurrence, its verified or explicitly unknown cause, and actual verification. | Occurrence; Expected and actual behavior; Boundary; Impact; Cause; Mitigation; Verification |
| lesson | Derives a bounded prevention rule from identified incidents and explains when it works. | Incident references; Prevention rule; Conditions and exclusions; Why it worked; Counterexample; Verification |
| pattern | Shows a shared mechanism across at least two independent incident/lesson chains, not repeated reports. | Independent occurrences; Common mechanism; Competing explanation; Scope limits; Codification candidate; Recurrence evidence |
| direction-proposal | Proposes a structural change with alternatives including retaining the design, a reversible trial and acceptance/stop conditions. | Supporting records; Contradicted assumption; Alternatives; Expected effects; Reversible trial; Acceptance and stop conditions; Decision owner; Proposal reference |
| user-profile | States an explicitly evidenced user preference within a named scope, without inferred ability or blanket permission. | Preference; Explicit user evidence; Applicability; Valid from; Revisit trigger; Confidence |
| glossary | Preserves the explanation and feedback evidence, links GLOSSARY.md without copying its canonical definition, and never infers mastery. | Term; Canonical glossary reference; Explanation; Concrete input and executor; Result; Explanation evidence; Scope; User feedback |

Unknown feedback stays explicitly unknown. Promotion creates a linked higher-kind
record; it never rewrites an incident into a general rule. Corrections edit the
same original; distinct successors link through the store.

## Authorship and publication
The task's Lead writes and commits its records in the task's closure pull request.
Independent review of a record is required only for `direction-proposal`.
The file is the original; the server holds a retrieval copy written only from a
commit on `main`. Use record_id as document_id and hash the exact UTF-8 file bytes
outside the file. Status lives only in the store; pending hides unfinished work.
No real work record is written yet: select the first records only after the
held-out questions are sealed under design section 10. This packet adds none.

## Check
`python3 -I -B ci/check-memory-records.py` checks every file under `memory/records/`.
An absent/empty record directory passes with zero files; keep fixtures only under
`ci/fixtures/memory-records/`. It is a syntax/shape check, not a factual judge.
`python3 -I -B -m unittest discover -s ci -p test_check_memory_records.py` runs tests.
