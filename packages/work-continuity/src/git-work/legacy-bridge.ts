import type {
  Graph,
  Contract,
  ContractSet,
  GitSourcePin,
  LegacyMigrationRequest,
  LegacyMigrationReceipt,
  LegacyWriterFence,
  LegacyCreateRequestBinding,
  GitWorkAlias,
  WorkRef,
  TypedLegacyAddress,
} from "./generated/v3.ts";
import type {
  GitWorkSelection,
  GitWorkReader,
  GitWorkRecord,
  TaskRecord,
  ResolvedSource,
  SpecRecord,
  PlanRecord,
  WorkRecord,
} from "./types.ts";
import {
  openGitWork,
  openGitSourceSnapshot,
  gitWorkPath,
  gitRecordPath,
  encodedId,
  verifiedOriginalBytes,
} from "./source.ts";
import { assertRecord } from "./records.ts";
import { assertV3, canonicalV3, digestV3 } from "./v3-codec.ts";
import {
  legacyIssueContract,
  legacySpecContract,
  legacyPlanContract,
  legacyTaskBinding,
  legacyTaskProjection,
} from "./legacy-model.ts";
import { qualifyLegacyGraphBindings } from "./legacy-qualification.ts";
import { openExactV3SourceReader, readExactV3Source } from "./external-artifacts.ts";
import { validateGitWorkAliases } from "./aliases.ts";
import {
  validateGitWorkV3Graph,
  readGitWorkV3Context,
  v3ContractRef,
} from "./effective-contract.ts";
import {
  checkedGitOwnerTarget,
  readOptionalGitRecord,
  sourceBlobOid,
  sourceSha256,
  qualifySourceGitTransaction,
  publishGitOwnerFiles,
  immutableGitLedgerCreation,
  type GitAllocationTarget,
  type NativeSourceGitHookPolicy,
} from "./git-owner-transaction.ts";
import { verifyRecordSuccessorLineage } from "./record-lineage.ts";

const bytes = (value: unknown): string => canonicalV3(value) + "\n";
const same = (a: unknown, b: unknown): boolean => canonicalV3(a) === canonicalV3(b);
const pinOf = (s: ResolvedSource): GitSourcePin => ({
  repositoryId: s.repositoryId,
  commit: s.commit,
  path: s.path,
  sha256: s.sha256,
});
export const legacyWriterFencePath = (scope: WorkRef): string =>
  gitWorkPath(scope.projectId, scope.workId) + "/v3/writer.json";
const legacyRequestPath = (ownerId: string, requestId: string): string =>
  `work/.identity-legacy-requests/${sourceSha256(ownerId)}/${sourceSha256(requestId)}.json`;
const migrationReceiptPath = (request: LegacyMigrationRequest): string =>
  gitWorkPath(request.snapshot.projectId, request.snapshot.workId) +
  `/v3/migrations/${sourceSha256(request.ownerId + ":" + request.createRequestId)}.json`;
const contractPath = (c: Contract): string =>
  gitWorkPath(c.owner.projectId, c.owner.workId) +
  "/v3" +
  (c.owner.kind === "task" ? `/tasks/${encodedId(c.owner.taskId)}` : "") +
  `/contracts/${c.kind}/${encodedId(c.id)}@${c.revision}.json`;

export interface LegacySourceProjection {
  graph: Graph;
  reader: GitWorkReader;
  sources: GitSourcePin[];
  inventoryDigest: string;
  aliases: GitWorkAlias[];
  records: GitWorkRecord[];
}
/** Read only, exact historical scope. Originals remain the historical owner store. */
export async function readLegacyGitWorkProjection(
  selection: GitWorkSelection,
): Promise<LegacySourceProjection> {
  const reader = await openGitWork(selection);
  const records: GitWorkRecord[] = [];
  const sources: GitSourcePin[] = [];
  const pins = new Map<string, GitSourcePin>();
  for (const ref of await reader.listRecords()) {
    const path = gitRecordPath(selection.projectId, selection.workId, ref);
    const source = await reader.readSource({ kind: "snapshot", path });
    verifiedOriginalBytes(source);
    const row = assertRecord(source.document);
    if (
      row.projectId !== selection.projectId ||
      row.workId !== selection.workId ||
      row.domainId !== reader.work.domainId
    )
      throw Error("V3_LEGACY_RECORD_SCOPE_MISMATCH");
    records.push(row);
    const pin = pinOf(source);
    sources.push(pin);
    pins.set(path, pin);
  }
  const original = reader.work;
  const contracts: Contract[] = [];
  const workPin = pins.get(reader.workPath + "/work.json")!;
  const issue = legacyIssueContract(original, workPin);
  contracts.push(issue);
  const specs = new Map<string, Contract>();
  for (const spec of records.filter((r): r is SpecRecord => r.kind === "spec")) {
    const c = legacySpecContract(
      spec,
      pins.get(
        gitRecordPath(spec.projectId, spec.workId, {
          kind: "spec",
          id: spec.id,
          revision: spec.revision,
        }),
      )!,
    );
    specs.set(c.id + "@" + c.revision, c);
    contracts.push(c);
  }
  const plans = new Map<string, Contract>();
  for (const plan of records.filter((r): r is PlanRecord => r.kind === "plan")) {
    const spec = specs.get(plan.specId + "@" + plan.specRevision);
    if (!spec) throw Error("V3_LEGACY_PLAN_SPEC_SOURCE_MISSING");
    const c = legacyPlanContract(
      plan,
      pins.get(
        gitRecordPath(plan.projectId, plan.workId, {
          kind: "plan",
          id: plan.id,
          revision: plan.revision,
        }),
      )!,
      v3ContractRef(spec),
    );
    plans.set(c.id + "@" + c.revision, c);
    contracts.push(c);
  }
  const refs = (
    specId: string,
    specRevision: number,
    planId: string,
    planRevision: number,
  ): ContractSet => {
    const spec = specs.get(specId + "@" + specRevision),
      plan = plans.get(planId + "@" + planRevision);
    if (
      !spec ||
      !plan ||
      plan.kind !== "plan" ||
      !same(plan.body.appliesToSpec, v3ContractRef(spec))
    )
      throw Error("V3_LEGACY_EXACT_PLAN_SPEC_BINDING_MISSING");
    return { issue: v3ContractRef(issue), spec: v3ContractRef(spec), plan: v3ContractRef(plan) };
  };
  const work = {
    projectId: original.projectId,
    workId: original.workId,
    title: original.workId,
    leadActorId: null,
    revision: 1,
    contracts: refs(
      original.spec.id,
      original.spec.revision,
      original.plan.id,
      original.plan.revision,
    ),
    status: "proposed" as const,
    legacySource: workPin,
  };
  const tasks = records
    .filter((r): r is TaskRecord => r.kind === "task")
    .map((task) => {
      if (task.logical.issueId !== original.issueId) throw Error("V3_LEGACY_ISSUE_SCOPE_MISMATCH");
      const bindings = refs(
        task.logical.specId,
        task.logical.specRevision,
        task.logical.planId,
        task.logical.planRevision,
      );
      const spec = specs.get(bindings.spec.id + "@" + bindings.spec.revision)!;
      if (spec.kind !== "spec") throw Error("V3_LEGACY_SPEC_REQUIRED");
      return legacyTaskProjection(
        task,
        legacyTaskBinding(
          task,
          pins.get(gitRecordPath(task.projectId, task.workId, { kind: "task", id: task.id }))!,
          bindings,
        ),
        spec.body.acceptanceCriteria.map((c) => c.id),
      );
    });
  const graph: Graph = {
    schema: "common.git-work.v3",
    domains: [{ id: original.domainId, title: original.domainId }],
    projects: [
      { id: original.projectId, title: original.projectId, primaryDomainId: original.domainId },
    ],
    works: [work],
    tasks,
    contracts,
    dependencies: [],
    relations: [],
    delegations: [],
    provenance: [],
    nativeCorrelations: [],
    aliases: [],
  };
  for (const task of records.filter((r): r is TaskRecord => r.kind === "task"))
    for (const dependency of task.dependsOn)
      if (["execution-and-verification", "acceptance"].includes(dependency.milestone))
        graph.dependencies.push({
          producer: { projectId: task.projectId, workId: task.workId, taskId: dependency.taskId },
          consumer: { projectId: task.projectId, workId: task.workId, taskId: task.id },
          required: dependency.milestone === "acceptance" ? "accepted" : "verified",
        });
  const aliases: GitWorkAlias[] = [];
  function alias(
    kind: TypedLegacyAddress["kind"],
    scope: TypedLegacyAddress["scope"],
    oldKey: string,
    newKey: string,
    provenance: GitSourcePin,
  ) {
    if (oldKey !== newKey)
      aliases.push({
        namespace: "git-work-versioned-locator",
        from: { kind, scope, key: oldKey },
        to: { kind, scope, key: newKey },
        revision: 1,
        provenance,
        reason:
          "Explicit owner source writer version changes the canonical locator; logical identity and original Git address remain unchanged.",
      });
  }
  alias(
    "work",
    { kind: "work", projectId: work.projectId, workId: work.workId },
    workPin.path,
    reader.workPath + "/v3/work.json",
    workPin,
  );
  for (const t of tasks)
    alias(
      "task",
      { kind: "task", projectId: t.projectId, workId: t.workId, taskId: t.taskId },
      t.legacyBinding!.source.path,
      reader.workPath + `/v3/tasks/${encodedId(t.taskId)}.json`,
      t.legacyBinding!.source,
    );
  for (const c of contracts) {
    const pin = (c.body as Record<string, unknown>).legacySource as GitSourcePin;
    alias(
      c.kind,
      c.owner,
      pin.path + (c.kind === "issue" ? "#/issueId" : ""),
      contractPath(c),
      pin,
    );
  }
  graph.aliases = aliases;
  const exact = await openExactV3SourceReader(selection);
  await qualifyLegacyGraphBindings(graph, exact);
  validateGitWorkV3Graph(graph);
  const sorted = sources.sort((a, b) => a.path.localeCompare(b.path));
  return { graph, reader, sources: sorted, inventoryDigest: digestV3(sorted), aliases, records };
}

/** Once adopted, legacy reads explicitly select the retained immutable snapshot. */
export async function readVersionedLegacyGitWork(
  selection: GitWorkSelection,
): Promise<LegacySourceProjection> {
  const snapshot = await openGitSourceSnapshot(selection);
  const path = legacyWriterFencePath(selection);
  if (!(await snapshot.listPaths(path)).length) return readLegacyGitWorkProjection(selection);
  const source = await snapshot.readSource({ kind: "snapshot", path });
  assertV3("LegacyWriterFence", source.document);
  const fence = source.document;
  if (
    fence.scope.projectId !== selection.projectId ||
    fence.scope.workId !== selection.workId ||
    fence.legacyReadSnapshot.repositoryId !== selection.repositoryId
  )
    throw Error("V3_WRITER_FENCE_SCOPE_MISMATCH");
  for (const pin of fence.sources) {
    const current = await snapshot.readSource({ kind: "snapshot", path: pin.path });
    if (current.sha256 !== pin.sha256) throw Error("V3_RETIRED_LEGACY_WRITER_CHANGED_ORIGINAL");
  }
  const old = await readLegacyGitWorkProjection({
    ...selection,
    commit: fence.legacyReadSnapshot.commit,
  });
  if (old.inventoryDigest !== fence.inventoryDigest || !same(old.sources, fence.sources))
    throw Error("V3_WRITER_FENCE_INVENTORY_MISMATCH");
  return old;
}
export async function assertLegacyGitWriterAllowed(selection: GitWorkSelection): Promise<void> {
  const snapshot = await openGitSourceSnapshot(selection);
  if ((await snapshot.listPaths(legacyWriterFencePath(selection))).length)
    throw Error("V3_LEGACY_AUTHORITATIVE_WRITER_RETIRED");
}

export interface CurrentLegacyMigrationAuthority {
  ownerId: string;
  actorId: string;
  scope: WorkRef;
  operation: "admit-existing-legacy-work";
  revision: number;
  authorityRef: string;
  expiresAtUtc: string;
  membership: true;
  gitHookPolicy: NativeSourceGitHookPolicy;
}
/** Separate current native operation; this is never an allocation/request-JSON permission. */
export interface NativeLegacyMigrationOwner {
  withCurrentMigrationAuthority<T>(
    input: GitAllocationTarget & { head: string; request: LegacyMigrationRequest },
    operation: (authority: CurrentLegacyMigrationAuthority) => Promise<T>,
  ): Promise<T>;
}
function selectionFor(request: LegacyMigrationRequest, repositoryRoot: string): GitWorkSelection {
  return { repositoryRoot, ...request.snapshot };
}
async function bootstrapBinding(
  projection: LegacySourceProjection,
  pin: GitSourcePin,
  ownerId: string,
): Promise<LegacyCreateRequestBinding> {
  if (!projection.sources.some((source) => same(source, pin)))
    throw Error("V3_BOOTSTRAP_SOURCE_NOT_IN_EXACT_SCOPE_INVENTORY");
  const source = await readExactV3Source(
    await openExactV3SourceReader(projection.reader.selection),
    pin,
  );
  const record = assertRecord(source.document);
  if (
    record.kind !== "input" ||
    record.inputKind !== "source-owner-record" ||
    !record.content ||
    typeof record.content !== "object" ||
    Array.isArray(record.content)
  )
    throw Error("V3_TYPED_BOOTSTRAP_INPUT_REQUIRED");
  const content = record.content as Record<string, unknown>;
  const expectedOwner = {
    repositoryId: projection.reader.selection.repositoryId,
    projectId: record.projectId,
    workId: record.workId,
  };
  if (
    content.kind !== "Task" ||
    !same(content.owner, expectedOwner) ||
    typeof content.createRequestId !== "string" ||
    typeof content.proposedTaskId !== "string" ||
    !content.meaning ||
    typeof content.meaning !== "object" ||
    Array.isArray(content.meaning) ||
    content.meaningSha256 !== digestV3(content.meaning)
  )
    throw Error("V3_LEGACY_BOOTSTRAP_OWNER_KIND_OR_MEANING_CONFLICT");
  const meaning = content.meaning as Record<string, unknown>;
  const task = projection.records.find(
    (r): r is TaskRecord => r.kind === "task" && r.id === content.proposedTaskId,
  );
  if (
    !task ||
    !same(meaning.spec, { id: task.logical.specId, revision: task.logical.specRevision }) ||
    !same(meaning.plan, { id: task.logical.planId, revision: task.logical.planRevision }) ||
    meaning.executionClass !== task.executionClass ||
    typeof meaning.purpose !== "string" ||
    !meaning.purpose.trim() ||
    !/^[a-f0-9]{64}$/.test(String(meaning.approvedManifestSha256))
  )
    throw Error("V3_LEGACY_BOOTSTRAP_TARGET_OR_CONTRACT_CONFLICT");
  const binding = {
    schema: "common.git-legacy-create-binding.v3" as const,
    ownerId,
    legacyOwner: expectedOwner,
    createRequestId: content.createRequestId,
    kind: "task" as const,
    id: task.id,
    meaningSha256: content.meaningSha256 as string,
    source: pin,
  };
  assertV3("LegacyCreateRequestBinding", binding);
  return binding;
}
async function prepareMigration(request: LegacyMigrationRequest, root: string) {
  assertV3("LegacyMigrationRequest", request);
  const projection = await readLegacyGitWorkProjection(selectionFor(request, root));
  if (projection.inventoryDigest !== request.expectedInventoryDigest)
    throw Error("V3_LEGACY_SOURCE_INVENTORY_CHANGED");
  validateGitWorkAliases(request.aliases);
  if (!same(request.aliases, projection.aliases))
    throw Error("V3_MIGRATION_ALIAS_EXACT_SOURCE_TARGET_REQUIRED");
  const expectedBootstraps = projection.records
    .filter(
      (r) =>
        r.kind === "input" &&
        r.inputKind === "source-owner-record" &&
        r.content &&
        typeof r.content === "object" &&
        !Array.isArray(r.content) &&
        r.content.kind === "Task" &&
        r.content.createRequestId !== undefined,
    )
    .map((r) =>
      projection.sources.find(
        (p) => p.path === gitRecordPath(r.projectId, r.workId, { kind: "input", id: r.id }),
      )!,
    );
  if (
    !same(
      [...request.bootstrapRequests].sort((a, b) => a.path.localeCompare(b.path)),
      expectedBootstraps.sort((a, b) => a.path.localeCompare(b.path)),
    )
  )
    throw Error("V3_ALL_LEGACY_BOOTSTRAP_REQUESTS_MUST_BE_IMPORTED");
  const bindings: LegacyCreateRequestBinding[] = [];
  for (const source of request.bootstrapRequests)
    bindings.push(await bootstrapBinding(projection, source, request.ownerId));
  if (new Set(bindings.map((b) => b.createRequestId)).size !== bindings.length)
    throw Error("V3_AMBIGUOUS_LEGACY_BOOTSTRAP_REQUEST");
  return { projection, bindings };
}
export async function previewLegacyGitWorkMigration(input: unknown, repositoryRoot: string) {
  assertV3("LegacyMigrationRequest", input);
  const { projection, bindings } = await prepareMigration(input, repositoryRoot);
  return {
    schema: "common.git-legacy-migration-preview.v3",
    legacySnapshot: input.snapshot,
    inventoryDigest: projection.inventoryDigest,
    taskIds: projection.graph.tasks.map((t) => t.taskId),
    aliases: projection.aliases,
    existingRequestIds: bindings.map((b) => b.createRequestId),
    allocation: "none",
    publication: "none",
    nativePermissionGranted: false,
  };
}
async function observedMigration(
  request: LegacyMigrationRequest,
  root: string,
  head: string,
): Promise<LegacyMigrationReceipt | null> {
  const path = migrationReceiptPath(request);
  const content = await readOptionalGitRecord(root, head, path);
  if (content === null) return null;
  const receipt: unknown = JSON.parse(content);
  assertV3("LegacyMigrationReceipt", receipt);
  if (
    receipt.ownerId !== request.ownerId ||
    receipt.createRequestId !== request.createRequestId ||
    receipt.requestDigest !== digestV3(request) ||
    !same(receipt.legacySnapshot, request.snapshot) ||
    receipt.inventoryDigest !== request.expectedInventoryDigest
  )
    throw Error("V3_LEGACY_MIGRATION_REQUEST_CONFLICT");
  const creation = await immutableGitLedgerCreation(root, head, path, content);
  const { projection, bindings } = await prepareMigration(request, root);
  if (
    !same(receipt.preservedSources, projection.sources) ||
    !same(receipt.aliases, projection.aliases)
  )
    throw Error("V3_LEGACY_MIGRATION_RECEIPT_SOURCE_MISMATCH");
  for (const pin of projection.sources)
    if (sourceSha256((await readOptionalGitRecord(root, head, pin.path)) ?? "") !== pin.sha256)
      throw Error("V3_RETIRED_LEGACY_WRITER_CHANGED_ORIGINAL");
  const expectedPaths = new Set([
    gitWorkPath(request.snapshot.projectId, request.snapshot.workId) + "/v3/work.json",
    ...projection.graph.tasks.map(
      (t) => gitWorkPath(t.projectId, t.workId) + `/v3/tasks/${encodedId(t.taskId)}.json`,
    ),
    ...projection.graph.contracts.map(contractPath),
    legacyWriterFencePath(request.snapshot),
    gitWorkPath(request.snapshot.projectId, request.snapshot.workId) + "/v3/aliases.json",
    ...bindings.map((b) => legacyRequestPath(b.ownerId, b.createRequestId)),
  ]);
  if (
    receipt.projectedFiles.length !== expectedPaths.size ||
    receipt.projectedFiles.some((r) => !expectedPaths.delete(r.path)) ||
    expectedPaths.size ||
    !same(
      receipt.legacyRequestPaths,
      bindings.map((b) => legacyRequestPath(b.ownerId, b.createRequestId)),
    ) ||
    receipt.writerFencePath !== legacyWriterFencePath(request.snapshot)
  )
    throw Error("V3_MIGRATION_PUBLICATION_CLOSURE_MISMATCH");
  for (const file of receipt.projectedFiles) {
    const initial = await readOptionalGitRecord(root, creation, file.path);
    if (
      initial === null ||
      sourceSha256(initial) !== file.sha256 ||
      sourceBlobOid(initial) !== file.blobOid
    )
      throw Error("V3_MIGRATION_CREATION_BLOB_MISMATCH");
    const row = JSON.parse(initial) as Record<string, unknown>;
    if (row.schema === "common.git-work-record.v3")
      await verifyRecordSuccessorLineage(root, creation, head, file.path, initial);
    else if (
      (await readOptionalGitRecord(root, head, file.path)) !== initial ||
      (await immutableGitLedgerCreation(root, head, file.path, initial)) !== creation
    )
      throw Error("V3_MIGRATION_IMMUTABLE_PROJECTION_CHANGED");
  }
  await readMigratedGitWorkV3({ ...selectionFor(request, root), commit: head });
  return receipt;
}
/** Existing identities only. One exact native-qualified CAS retires the old writer. */
export async function admitLegacyGitWork(
  input: unknown,
  target: GitAllocationTarget,
  owner: NativeLegacyMigrationOwner,
) {
  assertV3("LegacyMigrationRequest", input);
  const request = structuredClone(input);
  target = structuredClone(target);
  if (!owner || typeof owner.withCurrentMigrationAuthority !== "function")
    throw Error("V3_CURRENT_NATIVE_MIGRATION_OWNER_REQUIRED");
  const head = await checkedGitOwnerTarget(target);
  return owner.withCurrentMigrationAuthority(
    { ...target, head, request: structuredClone(request) },
    async (authority) => {
      if (
        authority.operation !== "admit-existing-legacy-work" ||
        authority.ownerId !== request.ownerId ||
        !same(authority.scope, {
          projectId: request.snapshot.projectId,
          workId: request.snapshot.workId,
        }) ||
        authority.membership !== true ||
        authority.revision !== target.expectedAuthorityRevision ||
        !authority.actorId?.trim() ||
        !authority.authorityRef?.trim() ||
        !Number.isFinite(Date.parse(authority.expiresAtUtc)) ||
        Date.parse(authority.expiresAtUtc) <= Date.now()
      )
        throw Error("V3_CURRENT_NATIVE_MIGRATION_AUTHORITY_REQUIRED");
      const transaction = qualifySourceGitTransaction(
        target,
        head,
        request.ownerId,
        authority.gitHookPolicy,
      );
      if (await observedMigration(request, target.repositoryRoot, head))
        return {
          status: "existing" as const,
          head,
          allocation: "none" as const,
          nativePermissionGranted: false as const,
        };
      if (head !== target.expectedHead || head !== request.snapshot.commit)
        throw Error("V3_MIGRATION_EXACT_CURRENT_LEGACY_HEAD_REQUIRED");
      const { projection, bindings } = await prepareMigration(request, target.repositoryRoot);
      const base = gitWorkPath(request.snapshot.projectId, request.snapshot.workId) + "/v3";
      const files = new Map<string, string>();
      for (const work of projection.graph.works) {
        work.leadActorId = authority.actorId;
        work.status = "admitted";
        files.set(
          base + "/work.json",
          bytes({ schema: "common.git-work-record.v3", kind: "work", value: work }),
        );
      }
      for (const task of projection.graph.tasks)
        files.set(
          base + `/tasks/${encodedId(task.taskId)}.json`,
          bytes({ schema: "common.git-work-record.v3", kind: "task", value: task }),
        );
      for (const contract of projection.graph.contracts)
        files.set(contractPath(contract), bytes(contract));
      // The owner-qualified actor/status transition changes the projection.
      // Refresh its complete source closure before consuming the new graph.
      await qualifyLegacyGraphBindings(
        projection.graph,
        await openExactV3SourceReader(selectionFor(request, target.repositoryRoot)),
      );
      validateGitWorkV3Graph(projection.graph);
      const fence: LegacyWriterFence = {
        schema: "common.git-work-writer-fence.v3",
        ownerId: request.ownerId,
        scope: { projectId: request.snapshot.projectId, workId: request.snapshot.workId },
        authoritativeProtocol: "v3",
        legacyReadSnapshot: request.snapshot,
        inventoryDigest: projection.inventoryDigest,
        migrationRequestDigest: digestV3(request),
        sources: projection.sources,
      };
      assertV3("LegacyWriterFence", fence);
      files.set(legacyWriterFencePath(request.snapshot), bytes(fence));
      files.set(base + "/aliases.json", bytes(projection.aliases));
      for (const binding of bindings) {
        const generic = `work/.identity-requests/${sourceSha256(binding.ownerId)}/${sourceSha256(binding.createRequestId)}.json`;
        if ((await readOptionalGitRecord(target.repositoryRoot, head, generic)) !== null)
          throw Error("V3_BOOTSTRAP_ALREADY_ALLOCATED_IN_NEW_NAMESPACE");
        files.set(legacyRequestPath(binding.ownerId, binding.createRequestId), bytes(binding));
      }
      const receipt: LegacyMigrationReceipt = {
        schema: "common.git-legacy-migration-receipt.v3",
        ownerId: request.ownerId,
        createRequestId: request.createRequestId,
        requestDigest: digestV3(request),
        scope: fence.scope,
        legacySnapshot: request.snapshot,
        inventoryDigest: projection.inventoryDigest,
        preservedSources: projection.sources,
        projectedFiles: [...files].map(([path, content]) => ({
          path,
          blobOid: sourceBlobOid(content),
          sha256: sourceSha256(content),
        })),
        writerFencePath: legacyWriterFencePath(request.snapshot),
        legacyRequestPaths: bindings.map((b) => legacyRequestPath(b.ownerId, b.createRequestId)),
        aliases: projection.aliases,
        authority: {
          actorId: authority.actorId,
          revision: authority.revision,
          authorityRef: authority.authorityRef,
          gitHookPolicyRef: transaction.policy.qualificationRef,
        },
      };
      assertV3("LegacyMigrationReceipt", receipt);
      files.set(migrationReceiptPath(request), bytes(receipt));
      const publication = await publishGitOwnerFiles(
        target,
        head,
        files,
        "Admit existing source identities " + digestV3(request),
        transaction,
      );
      const observed = await checkedGitOwnerTarget(target);
      if (!(await observedMigration(request, target.repositoryRoot, observed)))
        throw Error("V3_MIGRATION_CAS_UNCERTAIN_OBSERVE_BEFORE_NEW_ATTEMPT");
      return {
        status: publication.cas === "applied" ? ("admitted" as const) : ("reconciled" as const),
        head: observed,
        allocation: "none" as const,
        nativePermissionGranted: false as const,
      };
    },
  );
}
/** Typed recovery never creates or translates an opaque legacy request into a new UUID. */
export async function recoverLegacyCreateRequest(input: {
  selection: GitWorkSelection;
  ownerId: string;
  legacyOwner: LegacyCreateRequestBinding["legacyOwner"];
  createRequestId: string;
  kind: "task";
  meaningSha256: string;
}) {
  const snapshot = await openGitSourceSnapshot(input.selection);
  const source = await snapshot.readSource({
    kind: "snapshot",
    path: legacyRequestPath(input.ownerId, input.createRequestId),
  });
  const b = source.document;
  assertV3("LegacyCreateRequestBinding", b);
  if (
    b.ownerId !== input.ownerId ||
    b.createRequestId !== input.createRequestId ||
    b.kind !== input.kind ||
    !same(b.legacyOwner, input.legacyOwner) ||
    b.meaningSha256 !== input.meaningSha256
  )
    throw Error("V3_LEGACY_CREATE_REQUEST_CONFLICT");
  const historical = await readVersionedLegacyGitWork(input.selection);
  const expected = await bootstrapBinding(historical, b.source, input.ownerId);
  if (!same(b, expected)) throw Error("V3_LEGACY_REQUEST_SOURCE_MISMATCH");
  await immutableGitLedgerCreation(
    input.selection.repositoryRoot,
    input.selection.commit,
    source.path,
    Buffer.from(verifiedOriginalBytes(source)).toString("utf8"),
  );
  return {
    id: b.id,
    kind: b.kind,
    source: b.source,
    allocation: "none" as const,
    nativePermissionGranted: false as const,
  };
}
/** Reopen migrated authoritative records, requalifying historical contracts from originals. */
export async function readMigratedGitWorkV3(selection: GitWorkSelection): Promise<Graph> {
  const historical = await readVersionedLegacyGitWork(selection);
  const snapshot = await openGitSourceSnapshot(selection);
  if (!(await snapshot.listPaths(legacyWriterFencePath(selection))).length)
    throw Error("V3_EXPLICIT_NATIVE_MIGRATION_REQUIRED");
  const aliases = await snapshot.readSource({
    kind: "snapshot",
    path: gitWorkPath(selection.projectId, selection.workId) + "/v3/aliases.json",
  });
  if (!same(aliases.document, historical.aliases)) throw Error("V3_MIGRATED_ALIAS_SOURCE_CHANGED");
  const graph: Graph = { ...historical.graph, works: [], tasks: [], contracts: [] };
  const base = gitWorkPath(selection.projectId, selection.workId) + "/v3/";
  for (const path of await snapshot.listPaths(base)) {
    if (!path.endsWith(".json") || path.includes("/migrations/")) continue;
    const source = await snapshot.readSource({ kind: "snapshot", path });
    const document = source.document;
    if (path.includes("/contracts/")) {
      assertV3("Contract", document);
      graph.contracts.push(document);
    } else if (
      document &&
      typeof document === "object" &&
      !Array.isArray(document) &&
      document.schema === "common.git-work-record.v3"
    ) {
      assertV3("StoredRecord", document);
      if (document.kind === "work") graph.works.push(document.value);
      else if (document.kind === "task") graph.tasks.push(document.value);
    }
  }
  const exact = await openExactV3SourceReader(selection);
  await qualifyLegacyGraphBindings(graph, exact);
  validateGitWorkV3Graph(graph);
  return graph;
}
