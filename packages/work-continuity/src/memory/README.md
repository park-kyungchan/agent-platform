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
governing originals before budgets. Conflicting governing decisions remain
unresolved. Full rows, source gaps and omissions stay visible; the query does not
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
