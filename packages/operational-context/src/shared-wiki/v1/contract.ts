import { z } from "zod";
const Id = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/);
const Text = z.string().min(1).max(3000);
const Texts = z.array(Text).max(30);
export const ClaimClass = z.enum([
  "PRIMARY_OFFICIAL",
  "VERIFIED_FDE_PRACTICE",
  "USER_APPROVED_LOCAL",
  "SOURCE_OBSERVATION",
  "EXECUTABLE_LOCAL_EVIDENCE",
  "MODEL_PROPOSAL",
  "DERIVED_PROJECTION",
  "OPEN_OR_CONTESTED",
]);
export const SourceSchema = z
  .object({
    id: Id,
    url: z.string().url(),
    publisher: Text,
    sourceClass: z.enum([
      "primary-official",
      "official-conference",
      "user-decision",
      "synthetic-fixture",
    ]),
    asOf: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
    gaps: Texts,
  })
  .strict();
export const ClaimSchema = z
  .object({
    id: Id,
    sourceId: Id,
    claimClass: ClaimClass,
    quote: z.string().min(1).max(2000),
    scope: Text,
  })
  .strict();
const base = { id: Id, title: Text, claimRefs: z.array(Id).min(1).max(40), claimClass: ClaimClass };
const Principle = z
  .object({ ...base, kind: z.literal("principle"), definition: Text, decisionConsequences: Texts })
  .strict();
const Mechanism = z
  .object({
    ...base,
    kind: z.literal("mechanism"),
    product: Text,
    surface: Text,
    versionScope: Text,
    behavior: Texts,
    unknowns: Texts,
  })
  .strict();
const Pattern = z
  .object({
    ...base,
    kind: z.literal("pattern"),
    actor: Text,
    purpose: Text,
    subjectGrain: Text,
    requiredEvidence: Texts,
    operation: Text,
    effectScope: Text,
    nonEffects: Texts,
    verification: Texts,
    recovery: Texts,
    limits: Texts,
  })
  .strict();
const Case = z
  .object({
    ...base,
    kind: z.literal("case"),
    businessDomain: Text,
    sourceReportedOnly: z.literal(true),
    observedProcess: Text,
    artifactAvailability: z.enum([
      "narrative-only",
      "public-assets-unqualified",
      "public-assets-qualified",
    ]),
    localExecution: z.literal("not-executed"),
    unknowns: Texts,
  })
  .strict();
const Binding = z
  .object({
    ...base,
    kind: z.literal("binding"),
    ownerRef: Text,
    entrypoints: Texts,
    effectAuthority: z.literal("resolve-at-project-native-owner"),
    limits: Texts,
  })
  .strict();
export const NodeSchema = z.discriminatedUnion("kind", [
  Principle,
  Mechanism,
  Pattern,
  Case,
  Binding,
]);
export const RelationSchema = z
  .object({
    from: Id,
    to: Id,
    kind: z.enum(["refines", "applies", "illustrates", "depends-on", "related"]),
  })
  .strict();
export const KnowledgeSchema = z
  .object({
    schema: z.literal("shared.ontology.knowledge.v1"),
    revision: Id,
    authority: z.literal("knowledge-not-permission"),
    sources: z.array(SourceSchema).min(1).max(100),
    claims: z.array(ClaimSchema).min(1).max(1000),
    nodes: z.array(NodeSchema).min(1).max(500),
    relations: z.array(RelationSchema).max(2000),
    coverage: z
      .array(
        z
          .object({
            area: Text,
            state: z.enum(["open", "selected-scope-qualified", "deferred"]),
            gap: Text,
          })
          .strict(),
      )
      .min(1),
  })
  .strict();
export type Knowledge = z.infer<typeof KnowledgeSchema>;
export type Node = z.infer<typeof NodeSchema>;
const ArtifactRef = z
  .object({
    path: z
      .string()
      .regex(/^(?:[A-Za-z0-9_.-]+\/)*[A-Za-z0-9_.-]+$/)
      .refine((p) => !p.split("/").some((s) => s === "." || s === "..")),
    sha256: z.string().regex(/^[a-f0-9]{64}$/),
    bytes: z.number().int().nonnegative().optional(),
    articleStatus: z.string().optional(),
  })
  .strict();
export const ManifestSchema = z
  .object({
    schema: z.literal("shared.ontology.package-manifest.v1"),
    id: Id,
    revision: Id,
    status: z.enum(["candidate-unaccepted", "reviewed-source-and-contract"]),
    authority: z.literal("knowledge-not-permission"),
    createdAt: z.string().datetime(),
    files: z.array(ArtifactRef).min(1),
    sourceArtifacts: z.array(ArtifactRef).min(1),
    counts: z
      .object({
        sources: z.number().int().positive(),
        claims: z.number().int().positive(),
        nodes: z.number().int().positive(),
        relations: z.number().int().nonnegative(),
      })
      .strict(),
    consumer: z
      .object({
        argvPrefix: z.array(z.string()).min(1),
        requestSchema: Text,
        examples: z.array(z.unknown()),
      })
      .strict(),
    review: z
      .object({
        state: z.string(),
        scope: Text,
        independentEvidence: z.string().nullable(),
        freshAgentEvidence: z.string().nullable(),
      })
      .strict(),
    freshness: z
      .object({
        sourceSnapshots: z.literal("immutable-versioned"),
        revalidateWhen: Texts,
        runtimeAvailability: Text,
      })
      .strict(),
    limits: Texts,
  })
  .strict();
export const QuerySchema = z
  .object({
    kind: z.literal("context"),
    id: Id,
    purpose: z.enum(["research", "design", "implementation", "review"]),
  })
  .strict();
export class KnowledgeError extends Error {
  constructor(readonly code: string) {
    super(code);
  }
}
function unique(values: string[]) {
  if (new Set(values).size !== values.length) throw new KnowledgeError("DUPLICATE_ID");
}
export function parseKnowledge(input: unknown, sourceText: ReadonlyMap<string, string>): Knowledge {
  const parsed = KnowledgeSchema.safeParse(input);
  if (!parsed.success) throw new KnowledgeError("INVALID_KNOWLEDGE");
  const x = parsed.data;
  unique([
    ...x.sources.map((s) => s.id),
    ...x.claims.map((c) => c.id),
    ...x.nodes.map((n) => n.id),
  ]);
  const sources = new Map(x.sources.map((s) => [s.id, s]));
  const claims = new Map(x.claims.map((c) => [c.id, c]));
  const nodes = new Map(x.nodes.map((n) => [n.id, n]));
  // Missing acquisition precedes evaluating supplied quotations.
  for (const s of x.sources)
    if (!sourceText.has(s.id) || !sourceText.get(s.id))
      throw new KnowledgeError("SOURCE_TEXT_UNAVAILABLE");
  for (const c of x.claims) {
    const s = sources.get(c.sourceId);
    if (!s) throw new KnowledgeError("UNKNOWN_SOURCE");
    if (!sourceText.get(s.id)!.includes(c.quote)) throw new KnowledgeError("QUOTE_NOT_IN_SOURCE");
    const allowed: Record<
      z.infer<typeof SourceSchema>["sourceClass"],
      readonly z.infer<typeof ClaimClass>[]
    > = {
      "primary-official": ["PRIMARY_OFFICIAL", "SOURCE_OBSERVATION"],
      "official-conference": ["SOURCE_OBSERVATION"],
      "user-decision": ["USER_APPROVED_LOCAL", "SOURCE_OBSERVATION"],
      "synthetic-fixture": ["SOURCE_OBSERVATION"],
    };
    // Quotation sources cannot substantiate execution, verified practice or other evidence kinds.
    if (!allowed[s.sourceClass].includes(c.claimClass))
      throw new KnowledgeError("CLAIM_CLASS_SOURCE_MISMATCH");
    if (s.sourceClass === "synthetic-fixture" && c.claimClass !== "SOURCE_OBSERVATION")
      throw new KnowledgeError("SYNTHETIC_NOT_PRODUCT_FACT");
    if (s.sourceClass === "official-conference" && c.claimClass !== "SOURCE_OBSERVATION")
      throw new KnowledgeError("DEMO_NOT_PRODUCT_GUARANTEE");
  }
  for (const n of x.nodes) {
    for (const id of n.claimRefs) if (!claims.has(id)) throw new KnowledgeError("UNKNOWN_CLAIM");
    if (
      n.claimClass === "PRIMARY_OFFICIAL" &&
      n.claimRefs.some((id) => claims.get(id)!.claimClass !== "PRIMARY_OFFICIAL")
    )
      throw new KnowledgeError("NODE_CLAIM_CLASS_PROMOTION");
    for (const restricted of [
      "USER_APPROVED_LOCAL",
      "EXECUTABLE_LOCAL_EVIDENCE",
      "VERIFIED_FDE_PRACTICE",
    ] as const)
      if (
        n.claimClass === restricted &&
        !n.claimRefs.some((id) => claims.get(id)!.claimClass === restricted)
      )
        throw new KnowledgeError("NODE_CLAIM_CLASS_PROMOTION");
  }
  for (const r of x.relations)
    if (!nodes.has(r.from) || !nodes.has(r.to))
      throw new KnowledgeError("UNKNOWN_RELATION_ENDPOINT");
  const complete = new Set<string>(),
    visiting = new Set<string>();
  function visit(id: string): void {
    if (visiting.has(id)) throw new KnowledgeError("HIERARCHY_CYCLE");
    if (complete.has(id)) return;
    visiting.add(id);
    for (const r of x.relations) if (r.kind === "refines" && r.from === id) visit(r.to);
    visiting.delete(id);
    complete.add(id);
  }
  for (const n of x.nodes) visit(n.id);
  return x;
}
export function selectContext(corpus: Knowledge, request: unknown) {
  const parsed = QuerySchema.safeParse(request);
  if (!parsed.success) throw new KnowledgeError("INVALID_QUERY");
  const q = parsed.data;
  if (!corpus.nodes.some((n) => n.id === q.id)) throw new KnowledgeError("UNKNOWN_NODE");
  // Descendants of the requested subject only; adding an ancestor does not load siblings.
  const selected = new Set([q.id]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const r of corpus.relations)
      if (r.kind === "refines" && selected.has(r.to) && !selected.has(r.from)) {
        selected.add(r.from);
        changed = true;
      }
  }
  const focus = new Set(selected);
  for (const r of corpus.relations)
    if (r.kind !== "refines" && (focus.has(r.from) || focus.has(r.to))) {
      selected.add(r.from);
      selected.add(r.to);
    }
  changed = true;
  while (changed) {
    changed = false;
    for (const r of corpus.relations)
      if (r.kind === "refines" && selected.has(r.from) && !selected.has(r.to)) {
        selected.add(r.to);
        changed = true;
      }
  }
  const nodes = corpus.nodes.filter((n) => selected.has(n.id));
  const claimIds = new Set(nodes.flatMap((n) => n.claimRefs));
  const claims = corpus.claims.filter((c) => claimIds.has(c.id));
  const sourceIds = new Set(claims.map((c) => c.sourceId));
  return structuredClone({
    schema: "shared.ontology.context.v1",
    revision: corpus.revision,
    authority: corpus.authority,
    requestedSubject: q.id,
    purpose: q.purpose,
    selectionComplete: true,
    selectionScope: "declared-subject-closure-not-complete-world-or-permission",
    nodes,
    claims,
    sources: corpus.sources.filter((s) => sourceIds.has(s.id)),
    relations: corpus.relations.filter((r) => selected.has(r.from) && selected.has(r.to)),
    coverage: corpus.coverage,
    limits: [
      "Validated source agreement is literal support, not authenticity or semantic acceptance.",
      "Source snapshots and product claims do not certify native/browser/Foundry compatibility.",
      "Bindings reference native/Project owners; current effect authority must be resolved there.",
      "MODEL_PROPOSAL remains a proposal even when its shape and source references pass.",
    ],
  });
}
export const jsonSchemas = () => ({
  knowledge: z.toJSONSchema(KnowledgeSchema, { target: "draft-2020-12" }),
  query: z.toJSONSchema(QuerySchema, { target: "draft-2020-12" }),
});
