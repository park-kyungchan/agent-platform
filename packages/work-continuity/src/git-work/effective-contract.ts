import type {
  Graph,
  Work,
  Task,
  TaskRef,
  Scope,
  Contract,
  ContractRef,
  ContractSet,
  EffectiveContract,
  Dependency,
  AcceptanceCriterion,
  ContextPacket,
  CompactContextPacket,
} from "./generated/v3.ts";
import { assertV3, canonicalV3, digestV3, encodeV3, decodeV3 } from "./v3-codec.ts";
import { effectiveTaskRefs, selectedArtifactForDependency, qualifySelectedExternalArtifacts, type ExactV3SourceReader } from "./external-artifacts.ts";
import { assertQualifiedLegacyTask, qualifyLegacyGraphBindings } from "./legacy-qualification.ts";
import { validateGitWorkAliases, qualifyGitWorkAliases } from "./aliases.ts";
import { deflateRawSync, inflateRawSync } from "node:zlib";

export const v3WorkKey = (row: { projectId: string; workId: string }): string =>
  canonicalV3([row.projectId, row.workId]);
export const v3TaskKey = (row: TaskRef): string =>
  canonicalV3([row.projectId, row.workId, row.taskId]);
const referenceKey = (row: ContractRef): string => canonicalV3([row.owner, row.id, row.revision]);
const kinds = ["issue", "spec", "plan"] as const;
const equal = (a: unknown, b: unknown): boolean => canonicalV3(a) === canonicalV3(b);
const sorted = <T>(rows: T[]): T[] =>
  [...rows].sort((a, b) =>
    canonicalV3(a) < canonicalV3(b) ? -1 : canonicalV3(a) > canonicalV3(b) ? 1 : 0,
  );
const taskRef = (t: TaskRef): TaskRef => ({
  projectId: t.projectId,
  workId: t.workId,
  taskId: t.taskId,
});

function unique<T>(rows: T[], key: (row: T) => string, label: string): Map<string, T> {
  const result = new Map<string, T>();
  for (const row of rows) {
    const id = key(row);
    if (result.has(id)) throw Error(`V3_DUPLICATE_${label}`);
    result.set(id, row);
  }
  return result;
}

/** Iterative cycle checks stay bounded even at the schema's maximum graph depth. */
function acyclic(nodes: string[], edges: [string, string][], label: string): void {
  const indegrees = new Map(nodes.map((key) => [key, 0]));
  const children = new Map(nodes.map((key) => [key, [] as string[]]));
  for (const [a, b] of edges) {
    if (!indegrees.has(a) || !indegrees.has(b)) throw Error(`V3_MISSING_${label}_ENDPOINT`);
    children.get(a)!.push(b);
    indegrees.set(b, indegrees.get(b)! + 1);
  }
  const queue = nodes.filter((key) => indegrees.get(key) === 0);
  for (let index = 0; index < queue.length; index++)
    for (const next of children.get(queue[index]!)!) {
      indegrees.set(next, indegrees.get(next)! - 1);
      if (indegrees.get(next) === 0) queue.push(next);
    }
  if (queue.length !== nodes.length) throw Error(`V3_${label}_CYCLE`);
}

export function v3ContractRef(c: Contract): ContractRef {
  return {
    id: c.id,
    owner: structuredClone(c.owner),
    revision: c.revision,
    kind: c.kind,
    digest: c.digest,
  };
}

/** All identity fields participate; moving/revising identical text is a new contract. */
export function v3ContractDigest(c: Omit<Contract, "digest">): string {
  return digestV3(c);
}

export interface ValidatedV3Graph {
  works: Map<string, Work>;
  tasks: Map<string, Task>;
  contracts: Map<string, Contract>;
}

export function validateGitWorkV3Graph(input: unknown): ValidatedV3Graph {
  assertV3("Graph", input);
  const g = input;
  validateGitWorkAliases(g.aliases ?? []);
  const domains = unique(g.domains, (x) => x.id, "DOMAIN");
  const projects = unique(g.projects, (x) => x.id, "PROJECT");
  const works = unique(g.works, v3WorkKey, "WORK");
  const tasks = unique(g.tasks, v3TaskKey, "TASK");
  const contracts = unique(g.contracts, referenceKey, "CONTRACT_REVISION");
  for (const p of g.projects)
    if (!domains.has(p.primaryDomainId)) throw Error("V3_MISSING_PRIMARY_DOMAIN");
  for (const w of g.works) if (!projects.has(w.projectId)) throw Error("V3_MISSING_WORK_PROJECT");
  acyclic(
    [...domains.keys()],
    g.domains.filter((d) => d.parentDomainId !== undefined).map((d) => [d.parentDomainId!, d.id]),
    "DOMAIN_PARENT",
  );
  const scopeExists = (s: Scope): boolean =>
    s.kind === "user" ||
    (s.kind === "domain"
      ? domains.has(s.domainId)
      : s.kind === "project"
        ? projects.has(s.projectId)
        : s.kind === "work"
          ? works.has(v3WorkKey(s))
          : tasks.has(v3TaskKey(s)));
  for (const c of g.contracts) {
    if (!scopeExists(c.owner)) throw Error("V3_MISSING_CONTRACT_OWNER");
    const { digest, ...body } = c;
    if (digest !== v3ContractDigest(body)) throw Error("V3_CONTRACT_DIGEST_MISMATCH");
  }
  const resolve = (r: ContractRef, kind: ContractRef["kind"], owner: Scope): Contract => {
    if (r.kind !== kind) throw Error("V3_WRONG_CONTRACT_KIND");
    if (!equal(r.owner, owner)) throw Error("V3_WRONG_CONTRACT_OWNER");
    const c = contracts.get(referenceKey(r));
    if (!c || c.kind !== kind || c.digest !== r.digest)
      throw Error("V3_UNRESOLVED_CONTRACT_REVISION_OR_DIGEST");
    return c;
  };
  const criteria = (c: Contract): Map<string, AcceptanceCriterion> => {
    if (c.kind !== "spec") throw Error("V3_SPEC_REQUIRED");
    return unique(c.body.acceptanceCriteria, (row) => row.id, "CRITERION");
  };
  for (const w of g.works) {
    if (w.leadActorId === null && !w.legacySource) throw Error("V3_WORK_NATIVE_ACTOR_OR_LEGACY_SOURCE_REQUIRED");
    const owner: Scope = { kind: "work", projectId: w.projectId, workId: w.workId };
    for (const k of kinds) resolve(w.contracts[k], k, owner);
    const plan = contracts.get(referenceKey(w.contracts.plan))!;
    if (plan.kind !== "plan" || !equal(plan.body.appliesToSpec, w.contracts.spec))
      throw Error("V3_WORK_PLAN_SPEC_BINDING");
    criteria(contracts.get(referenceKey(w.contracts.spec))!);
  }
  for (const t of g.tasks) {
    const w = works.get(v3WorkKey(t));
    if (!w) throw Error("V3_MISSING_TASK_WORK");
    assertQualifiedLegacyTask(t);
    if (t.assigneeActorId === null && !t.legacyBinding) throw Error("V3_TASK_NATIVE_ACTOR_OR_LEGACY_SOURCE_REQUIRED");
    const owner: Scope = { kind: "task", ...taskRef(t) };
    if (t.legacyBinding) {
      if (Object.keys(t.contractOverrides).length) throw Error("V3_LEGACY_TASK_CANNOT_FORGE_OVERRIDE");
      for (const kind of kinds) resolve(t.legacyBinding.contracts[kind], kind, { kind: "work", projectId: t.projectId, workId: t.workId });
    }
    for (const k of kinds)
      if (t.contractOverrides[k]) {
        const c = resolve(t.contractOverrides[k]!, k, owner);
        const parent =
          c.kind === "issue"
            ? c.body.parentIssue
            : c.kind === "spec"
              ? c.body.refinesSpec
              : c.body.methodRef;
        if (!parent || !equal(parent, w.contracts[k])) throw Error("V3_TYPED_CONTRACT_REFINEMENT");
      }
    const refs = effectiveTaskRefs(g, t);
    const plan = contracts.get(referenceKey(refs.plan))!;
    if (plan.kind !== "plan" || !equal(plan.body.appliesToSpec, refs.spec))
      throw Error("V3_EFFECTIVE_PLAN_SPEC_BINDING");
    const workCriteria = criteria(contracts.get(referenceKey(w.contracts.spec))!);
    const spec = contracts.get(referenceKey(refs.spec))!;
    const effectiveCriteria = criteria(spec);
    if (spec.owner.kind === "task")
      for (const row of effectiveCriteria.values()) {
        if (!workCriteria.has(row.id) || row.covers !== undefined) {
          if (
            !row.covers?.length ||
            row.covers.some((id) => !workCriteria.has(id)) ||
            new Set(row.covers).size !== row.covers.length
          )
            throw Error("V3_TASK_CRITERION_COVERAGE");
        }
      }
    unique(t.acceptanceCriteria, (id) => id, "CRITERION_SELECTION");
    if (
      !t.acceptanceCriteria.length ||
      t.acceptanceCriteria.some((id) => !effectiveCriteria.has(id))
    )
      throw Error("V3_EFFECTIVE_CRITERION_SELECTION");
    unique(t.resources, (r) => r.resource, "RESOURCE_CLAIM");
    for (const r of t.resources)
      if (
        r.resource.split("/").some((part) => ["", ".", ".."].includes(part)) ||
        r.resource.includes("\\")
      )
        throw Error("V3_NONCANONICAL_RESOURCE");
  }
  acyclic(
    [...tasks.keys()],
    g.tasks
      .filter((t) => t.parentTaskId !== undefined)
      .map((t) => [v3TaskKey({ ...t, taskId: t.parentTaskId! }), v3TaskKey(t)]),
    "TASK_PARENT",
  );
  unique(g.dependencies, (d) => canonicalV3([d.producer, d.consumer]), "DEPENDENCY");
  for (const d of g.dependencies)
    if (v3WorkKey(d.producer) !== v3WorkKey(d.consumer) && !d.artifact)
      throw Error("V3_CROSS_WORK_ARTIFACT_REQUIRED");
  acyclic(
    [...tasks.keys()],
    g.dependencies.map((d) => [v3TaskKey(d.producer), v3TaskKey(d.consumer)]),
    "DEPENDENCY",
  );
  for (const d of g.dependencies) selectedArtifactForDependency(g, d);
  for (const r of g.relations)
    if (!scopeExists(r.from) || !scopeExists(r.to)) throw Error("V3_RELATED_SCOPE_MISSING");
  for (const d of g.delegations)
    if (!scopeExists(d.from) || !scopeExists(d.to)) throw Error("V3_DELEGATION_SCOPE_MISSING");
  acyclic(
    [...new Set(g.delegations.flatMap((d) => [canonicalV3(d.from), canonicalV3(d.to)]))],
    g.delegations.map((d) => [canonicalV3(d.from), canonicalV3(d.to)]),
    "DELEGATION",
  );
  for (const p of g.provenance)
    if (!scopeExists(p.subject)) throw Error("V3_PROVENANCE_SCOPE_MISSING");
  for (const n of g.nativeCorrelations)
    if (!tasks.has(v3TaskKey(n.task))) throw Error("V3_NATIVE_CORRELATION_TASK_MISSING");
  return { works, tasks, contracts };
}

/** A single resolver for full, compact, dispatch and immutable recovery source paths. */
export function resolveEffectiveContract(input: unknown, target: TaskRef): EffectiveContract {
  const { works, tasks } = validateGitWorkV3Graph(input);
  const g = input as Graph;
  const t = tasks.get(v3TaskKey(target));
  if (!t) throw Error("V3_UNKNOWN_TASK");
  const w = works.get(v3WorkKey(t))!;
  const parentTaskChain: EffectiveContract["parentTaskChain"] = [];
  let child = t;
  while (child.parentTaskId) {
    const p = tasks.get(v3TaskKey({ ...child, taskId: child.parentTaskId }))!;
    parentTaskChain.unshift({
      task: taskRef(p),
      revision: p.revision,
      contracts: effectiveTaskRefs(g, p),
    });
    child = p;
  }
  const selected = new Set([v3TaskKey(t)]);
  const pending = [v3TaskKey(t)];
  const dependencies: Dependency[] = [];
  for (let index = 0; index < pending.length; index++)
    for (const d of g.dependencies)
      if (v3TaskKey(d.consumer) === pending[index]) {
        dependencies.push(d);
        const key = v3TaskKey(d.producer);
        if (!selected.has(key)) {
          selected.add(key);
          pending.push(key);
        }
      }
  const dependencyTasks = sorted(
    [...selected]
      .filter((key) => key !== v3TaskKey(t))
      .map((key) => {
        const producer = tasks.get(key)!;
        return {
          task: taskRef(producer),
          revision: producer.revision,
          contracts: effectiveTaskRefs(g, producer),
          resources: sorted(producer.resources),
          acceptanceCriteria: [...producer.acceptanceCriteria].sort(),
        };
      }),
  );
  const body = {
    schema: "common.git-work-effective.v3" as const,
    task: taskRef(t),
    workRevision: w.revision,
    taskRevision: t.revision,
    workContracts: w.contracts,
    taskOverrides: t.contractOverrides,
    contracts: effectiveTaskRefs(g, t) as ContractSet,
    ...(t.legacyBinding ? { legacyBinding: t.legacyBinding } : {}),
    holdReasons: [...new Set([...selected].flatMap((key) => tasks.get(key)!.legacyBinding?.holdReasons ?? []))].sort(),
    externalArtifacts: sorted(dependencies.flatMap((d) => { const artifact = selectedArtifactForDependency(g, d); return artifact ? [artifact] : []; })),
    parentTaskChain,
    taskResources: sorted(t.resources),
    acceptanceCriteria: [...t.acceptanceCriteria].sort(),
    dependencies: sorted(dependencies),
    dependencyTasks,
  };
  const result = { ...body, digest: digestV3(body) };
  assertV3("EffectiveContract", result);
  return structuredClone(result);
}

export function readGitWorkV3Context(input: {
  graph: unknown;
  task: TaskRef;
  mode: "full" | "compact" | "dispatch" | "resume";
  expectedDigest?: string;
}) {
  if (!["full", "compact", "dispatch", "resume"].includes(input.mode))
    throw Error("V3_EXPLICIT_CONTEXT_MODE_REQUIRED");
  if (["dispatch", "resume"].includes(input.mode) && !input.expectedDigest)
    throw Error("V3_PINNED_EFFECTIVE_DIGEST_REQUIRED");
  const effective = resolveEffectiveContract(input.graph, input.task);
  if (input.expectedDigest !== undefined && effective.digest !== input.expectedDigest)
    throw Error("V3_STALE_EFFECTIVE_CONTRACT");
  const g = input.graph as Graph;
  const refs = new Set(
    [
      effective.workContracts,
      effective.contracts,
      ...effective.parentTaskChain.map((p) => p.contracts),
      ...effective.dependencyTasks.map((p) => p.contracts),
    ].flatMap((set) => kinds.map((kind) => referenceKey(set[kind]))),
  );
  const packet = {
    schema: "common.git-work-context.v3" as const,
    mode: input.mode,
    effective,
    status: effective.holdReasons.length ? "held" as const : "source-qualified" as const,
    holdReasons: effective.holdReasons,
    contractSources: sorted(g.contracts.filter((c) => refs.has(referenceKey(c)))),
    authority: "none" as const,
    nativePermissionGranted: false as const,
    semanticAcceptanceEstablished: false as const,
  };
  assertV3("ContextPacket", packet);
  if (input.mode === "compact") {
    const encoded = encodeV3("ContextPacket", packet);
    const compact = {
      schema: "common.git-work-context.compact.v3" as const,
      encoding: "deflate-raw+base64.v1" as const,
      effectiveDigest: effective.digest,
      decodedBytes: Buffer.byteLength(encoded),
      payload: deflateRawSync(encoded).toString("base64"),
      authority: "none" as const,
      nativePermissionGranted: false as const,
      semanticAcceptanceEstablished: false as const,
    };
    assertV3("CompactContextPacket", compact);
    return compact;
  }
  return packet;
}

/** Lossless transport only; decoded source is never native authority. */
export function expandGitWorkV3Context(input: ContextPacket | CompactContextPacket): ContextPacket {
  let packet: ContextPacket;
  if (input.schema === "common.git-work-context.compact.v3") {
    assertV3("CompactContextPacket", input);
    if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(input.payload))
      throw Error("V3_CANONICAL_BASE64_REQUIRED");
    const decoded = inflateRawSync(Buffer.from(input.payload, "base64"), {
      maxOutputLength: 4_194_304,
    });
    if (decoded.length !== input.decodedBytes) throw Error("V3_COMPACT_BYTE_LENGTH_MISMATCH");
    packet = decodeV3("ContextPacket", new TextDecoder("utf-8", { fatal: true }).decode(decoded));
    if (packet.effective.digest !== input.effectiveDigest || packet.mode !== "compact")
      throw Error("V3_COMPACT_DIGEST_MISMATCH");
  } else {
    assertV3("ContextPacket", input);
    packet = structuredClone(input);
  }
  if (!equal(packet.holdReasons, packet.effective.holdReasons) || packet.status !== (packet.holdReasons.length ? "held" : "source-qualified")) throw Error("V3_CONTEXT_HOLD_STATUS_MISMATCH");
  const { digest, ...effective } = packet.effective;
  if (digestV3(effective) !== digest) throw Error("V3_EFFECTIVE_PACKET_DIGEST_MISMATCH");
  const sources = unique(packet.contractSources, referenceKey, "CONTRACT_SOURCE");
  for (const c of sources.values()) {
    const { digest, ...body } = c;
    if (v3ContractDigest(body) !== digest) throw Error("V3_CONTRACT_DIGEST_MISMATCH");
  }
  const required = new Set(
    [
      packet.effective.workContracts,
      packet.effective.contracts,
      ...packet.effective.parentTaskChain.map((p) => p.contracts),
      ...packet.effective.dependencyTasks.map((p) => p.contracts),
    ].flatMap((set) => kinds.map((k) => canonicalV3(set[k]))),
  );
  const provided = new Set(packet.contractSources.map((c) => canonicalV3(v3ContractRef(c))));
  if (required.size !== provided.size || [...required].some((key) => !provided.has(key)))
    throw Error("V3_CONTEXT_CONTRACT_SOURCE_CLOSURE");
  return packet;
}

/** Source qualifications are minted from actual immutable Git bytes on every reopen. */
export async function readSourceQualifiedGitWorkV3Context(input: Parameters<typeof readGitWorkV3Context>[0] & { graph: Graph; reader: ExactV3SourceReader }) {
  await qualifyLegacyGraphBindings(input.graph, input.reader);
  await qualifyGitWorkAliases(input.graph.aliases ?? [], input.reader);
  await qualifySelectedExternalArtifacts(input.graph, input.reader);
  return readGitWorkV3Context(input);
}
