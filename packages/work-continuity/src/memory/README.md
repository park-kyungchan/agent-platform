# Memory v1 production boundary

Use the explicit Bun/TypeScript package entrypoint
`@palantirkc/work-continuity/memory/v1`. Historical package facades, Work readers,
immutable releases and their finite dependency projections retain their contracts.
The single Ontology generator owns `generated/`; its `--memory-v1 --check` mode
pins both the Memory schema and the canonical Git Work v3 shared definitions.
Never edit generated validators/types by hand.

`withMemoryQuery(request, nativeOwner, consume)` accepts a schema-validated JSON
request and a separate actual `NativeMemoryReader`. No reader, capability,
policy generation or owner implementation can be supplied in request JSON.
User Memory needs neither a fabricated Work nor an Episode. Discovery spans all
currently permitted owners/scopes by default. Explicit scopes restrict matching
owners/subjects; selected Work supplies a relevance tie break. Optional Task
context uses the existing S01 resolver for full/compact/dispatch/resume, with its
original source qualifications, effective digest and prerequisite holds.

The existing native owner implements `withCurrentRead`, authenticates the caller,
and holds **all** current reader/policy/source-head leases until the consumer
callback finishes. It must independently bind opaque permits to the selected
revision, current permissions, originals and all source/derived exclusions.
`readOriginal` returns exact immutable original bytes; `acquire` returns the
canonical Memory/Governing document. A URI is an address, never a grant. Common
code checks owner generations, `allowedReaders`, original hashes, document hash,
canonical transport and exact header joins before indexing or delivery. A lease
change, expiry or source contradiction rejects the whole query. Typed acquisition
gaps remain explicit and do not turn unavailable evidence into knowledge.

Current mode requires a native-current active revision valid now. History still
requires **present** permission and retains candidate/superseded/revoked/expired
labels and original identities. It does not revive old permissions. Original
source access revocation applies to every representation. `createMemoryQueryView`
retains query metadata only and re-enters the current owner boundary on every use.
No persistent body, snippet, embedding or result cache is provided. Native copies
already delivered remain under the native owner's custody/invalidation policy.

Memory `kind: decision` and `authority: owner-adopted` alone never govern.
Instruction eligibility requires separate authenticated native adoption proof from
already acquired originals. Actual decisions additionally require the actual
Governing document, matching source/scope, allowed reader, current status and native
applicability qualification. Recalled summaries close over already acquired actual
governing originals before discretionary hit budgets. Current applicable governing
originals and their conflicts are built before lexical selection: a search miss
does not remove a current obligation or select a policy winner. Advisory Memory
recall still requires relevance; this closure does not promote unrelated Memory.

`NativeMemoryUse` is a disjoint native callback contract. Standing preference or
procedure uses contain only `kind` and acquired adoption `source`; any attached
`decision`, including an undefined or historical/foreign/inapplicable one, refuses
at runtime. A `governing-decision` requires the actual Decision document and the
complete schema, original/source, exact owner/local-key (when the summary has a
claim), reader, current status/time and independent governing-qualification joins.
Only that admitted use can produce an adopted governing row or actual claim.
Native results are copied into private query snapshots. Qualification callbacks
receive detached documents and cannot mutate the exact document being admitted.
Normal standing preferences/procedures remain eligible without Decision authority.
Two fully qualified current standing uses with different values for the same
exact owner/local claim stay ineligible until an actual qualified governing
original resolves that claim. `standingConflict: true` and `standingSources`
retain their exact envelope/adoption references and values, even on a lexical
miss. Without a governing winner their `resolution` is `conflict`; with one,
only standing uses compatible with its value are eligible. Standing sources
never enter `governing` or set `governingConflict`. Single/compatible standing
uses, independently owned claims, and historical or unadopted evidence keep
their existing meanings. There is no implicit recency or summary-text winner.

`conflictingEvidence` includes values from both admitted summary claims and actual
governing documents, grouped by the same exact owner-qualified claim identity.
An optional absent summary claim stays absent; actual Decision values still count.
A summary/original mismatch remains visible while only the actual current
qualified original can govern. Authorized historical originals remain evidence
without becoming a current `governingConflict` or standing instruction. Supplying
the same actual Decisions directly or through valid adoption preserves their
conflict meaning and original-source references.

Claims use the exact canonical tuple `(owner Scope, local key)`, consistently for
recall closure, conflict output, unique resolution and standing eligibility.
`claimKey` and every legacy key retain their original bytes; conflict output adds
`claimIdentity` so callers can distinguish independently owned policies. Subject
overlap, selected Work, parent scope and equal key/value text do not establish
override or applicability. Native `qualifyGoverning` must establish applicability
in the request context. Cross-scope override/refinement is not inferred by this
API; any such relationship needs an explicit native-qualified authority mapping,
not a caller-supplied alias. Memory adoption still requires its actual Decision
to have the same exact owner and local claim.

Known permitted current authority sources that are unavailable or omitted by
source/original/envelope budgets retain their exact source, owner, record class
and claim provenance. Only their affected owner-qualified claims have
`resolution: incomplete`, null `selectedGoverningValue` and held standing
eligibility. Observed conflicts stay explicit even when closure is incomplete.
Historical, unrelated, non-authoritative and unscoped unknown gaps do not invent
a relationship to an otherwise complete claim. Unscoped discovery gaps still
make overall source coverage partial; a resolved claim is scoped to its known
current permitted inventory, not proof that every unknown source was discovered.
Metadata beyond the 1000-source acquisition limit is still validated and currently
authorized before recording a scoped omission; its bodies are never read.
Governing output omissions also hold the affected claim, and authority warnings
precede advisory hits. If the required metadata cannot fit, the whole query
rejects rather than return a positive resolution without its provenance.

Conflicting governing decisions remain unresolved. Aggregate retained envelopes
(including legacy metadata) are bounded at 2 MiB;
original acquisition is bounded at 16 MiB plus the final bounded read. Budget
omissions are explicit. Full rows, source gaps and omissions stay visible; the query does not
select a winner by recency, summary wording, or an enum.

The retained literal AND algorithm measured no recall on the frozen natural
Korean query subset. The bounded successor uses normalized words, Korean fragments
and an explicit technical alias vocabulary. Narrow absent evidence can cause
abstention. This is lexical retrieval, not general semantic inference. Feature
maps and the old baseline's SQLite index are query-local and disposed before the
consumer. There are no provider calls or new dependencies. Public corpus gains
are public-tuned; withheld generalization and native adoption remain unknown.
See the source-bound isolated observations in the delivery result for exact
numerators, denominators, unsupported historical API exposure and remaining misses.

`projectQualifiedLegacyMemory` preserves old identity, generation, claim, policy
and metadata after the existing legacy qualifier. Its native-supplied mappings
must cover every Episode source at the exact digest/revision. All originals and
the legacy record remain in the new source closure. The projection is candidate
model inference, grants no access/adoption, and does not change old stores.

The `harness memory-query` route requires its existing owner to supply the native
reader argument in process; the standalone JSON CLI cannot invent one. Academy's
separate technical admission API preserves all channel, original custody,
quarantine/fresh conversation and actual teacher/human gates. Actual native
readers, writes, activation, publication and consumer receive/use remain with their
existing owners and distinct approved Work stages.
