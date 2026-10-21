import { randomUUID, createHash } from "node:crypto";
import type {
  CreateRequest,
  Graph,
  Scope,
  Work,
  Task,
  Run,
  Contract,
  AllocationBinding,
} from "./generated/v3.ts";
import { assertV3, canonicalV3, digestV3 } from "./v3-codec.ts";
import {
  resolveEffectiveContract,
  validateGitWorkV3Graph,
  v3ContractDigest,
  v3ContractRef,
  v3TaskKey,
  v3WorkKey,
} from "./effective-contract.ts";
import { encodedId } from "./source.ts";
import { gitOwnerRead as git, readOptionalGitRecord as readOptional, checkedGitOwnerTarget as checkedTarget, qualifySourceGitTransaction, publishGitOwnerFiles, sourceBlobOid, immutableGitLedgerCreation, type NativeSourceGitHookPolicy } from "./git-owner-transaction.ts";
import type { GitAllocationTarget } from "./git-owner-transaction.ts";
export type { GitAllocationTarget, NativeSourceGitHookPolicy } from "./git-owner-transaction.ts";
import { verifyRecordSuccessorLineage } from "./record-lineage.ts";

export interface CurrentAllocationAuthority {
  ownerId: string;
  actorId: string;
  scope: Scope;
  kinds: CreateRequest["kind"][];
  revision: number;
  authorityRef: string;
  expiresAtUtc: string;
  membership: true;
  gitHookPolicy: NativeSourceGitHookPolicy;
}
/** Implemented by the existing native owner, never loaded from request JSON.
 * The owner holds its current authority/membership lease across operation and CAS.
 * readGraph projects its exact selected Git snapshot; it must not select latest.
 */
export interface NativeGitIdentityOwner {
  withCurrentAuthority<T>(
    input: GitAllocationTarget & { head: string; request: CreateRequest },
    operation: (authority: CurrentAllocationAuthority) => Promise<T>,
  ): Promise<T>;
  readGraph(input: { repositoryRoot: string; ownerRef: string; head: string }): Promise<Graph>;
}
interface PreparedAllocation {
  record: Work | Task | Run;
  contracts: Contract[];
  recordPath: string;
  contractPaths: string[];
}
type Binding = AllocationBinding;
const uuid = (id: string): boolean =>
  /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(id);
const hash = (bytes: string): string => createHash("sha256").update(bytes).digest("hex");
const fileBytes = (value: unknown): string => canonicalV3(value) + "\n";
const workPath = (r: { projectId: string; workId: string }): string =>
  `work/${encodedId(r.projectId)}/${encodedId(r.workId)}/v3`;
function contractPath(c: Contract): string {
  const owner = c.owner;
  return (
    workPath(owner) +
    (owner.kind === "task" ? `/tasks/${encodedId(owner.taskId)}` : "") +
    `/contracts/${c.kind}/${encodedId(c.id)}@${c.revision}.json`
  );
}
function requestPath(r: CreateRequest): string {
  // Owner-global request namespace catches same-key kind/project/Work scope changes.
  return `work/.identity-requests/${hash(r.ownerId)}/${hash(r.createRequestId)}.json`;
}
const identityPath = (id: string): string => `work/.identity-ids/${id}.json`;

function checkRequest(input: unknown): asserts input is CreateRequest {
  assertV3("CreateRequest", input);
  const scopeKind = { work: "project", task: "work", run: "task" }[input.kind];
  if (input.scope.kind !== scopeKind) throw Error("V3_ALLOCATION_KIND_SCOPE_MISMATCH");
}

/** Explicit template tokens avoid guessing text replacement or accepting caller-picked IDs.
 * {$allocatedId:true} is the new ID; {$contractRef:n} refers to an earlier template.
 */
export function prepareV3Allocation(input: unknown, allocatedId: string): PreparedAllocation {
  checkRequest(input);
  if (!uuid(allocatedId)) throw Error("V3_UUID_V4_REQUIRED");
  const contracts: Contract[] = [];
  let identityMarkers = 0;
  function render(value: unknown): unknown {
    if (Array.isArray(value)) return value.map(render);
    if (!value || typeof value !== "object") return value;
    const row = value as Record<string, unknown>;
    if (Object.keys(row).length === 1 && row.$allocatedId === true) {
      identityMarkers++;
      return allocatedId;
    }
    if (Object.keys(row).length === 1 && Object.hasOwn(row, "$contractRef")) {
      if (
        !Number.isSafeInteger(row.$contractRef) ||
        Number(row.$contractRef) < 0 ||
        !contracts[Number(row.$contractRef)]
      )
        throw Error("V3_ORDERED_CONTRACT_TEMPLATE_REQUIRED");
      return v3ContractRef(contracts[Number(row.$contractRef)]!);
    }
    return Object.fromEntries(Object.entries(row).map(([key, item]) => [key, render(item)]));
  }
  for (const template of input.contracts) {
    const c = render(template) as Contract;
    if (!c || typeof c !== "object" || Array.isArray(c) || Object.hasOwn(c, "digest"))
      throw Error("V3_CONTRACT_TEMPLATE_DIGEST_IS_DERIVED");
    const contract = { ...c, digest: v3ContractDigest(c) };
    assertV3("Contract", contract);
    contracts.push(contract);
  }
  const record = render(input.record);
  const scope = input.scope;
  let path: string;
  if (input.kind === "work") {
    assertV3("Work", record);
    if (record.legacySource || record.leadActorId === null) throw Error("V3_NEW_ALLOCATION_CANNOT_IMPORT_LEGACY_WORK");
    if (
      scope.kind !== "project" ||
      record.projectId !== scope.projectId ||
      record.workId !== allocatedId ||
      record.status !== "proposed" ||
      record.revision !== 1
    )
      throw Error("V3_NEW_WORK_SCOPE_OR_STATE");
    path = workPath(record) + "/work.json";
  } else if (input.kind === "task") {
    assertV3("Task", record);
    if (record.legacyBinding || record.assigneeActorId === null) throw Error("V3_NEW_ALLOCATION_CANNOT_IMPORT_LEGACY_TASK");
    if (
      scope.kind !== "work" ||
      record.projectId !== scope.projectId ||
      record.workId !== scope.workId ||
      record.taskId !== allocatedId ||
      record.status !== "planned" ||
      record.revision !== 1
    )
      throw Error("V3_NEW_TASK_SCOPE_OR_STATE");
    path = workPath(record) + `/tasks/${allocatedId}.json`;
  } else {
    assertV3("Run", record);
    if (
      scope.kind !== "task" ||
      v3TaskKey(scope) !== v3TaskKey(record.task) ||
      record.runId !== allocatedId ||
      record.status !== "reserved" ||
      record.nativeBindings.length ||
      record.effectRefs.length ||
      input.contracts.length
    )
      throw Error("V3_NEW_RUN_SCOPE_OR_STATE");
    path = workPath(record.task) + `/runs/${allocatedId}.json`;
  }
  if (!identityMarkers) throw Error("V3_ALLOCATION_ID_MARKER_REQUIRED");
  for (const c of contracts) {
    if (
      input.kind === "work"
        ? c.owner.kind !== "work" ||
          c.owner.workId !== allocatedId ||
          scope.kind !== "project" ||
          c.owner.projectId !== scope.projectId
        : input.kind === "task"
          ? c.owner.kind !== "task" ||
            c.owner.taskId !== allocatedId ||
            scope.kind !== "work" ||
            v3WorkKey(c.owner) !== v3WorkKey(scope)
          : true
    )
      throw Error("V3_COMPANION_CONTRACT_OWNER");
  }
  const paths = contracts.map(contractPath);
  if (new Set(paths).size !== paths.length) throw Error("V3_DUPLICATE_CONTRACT_PUBLICATION");
  return { record, contracts, recordPath: path, contractPaths: paths };
}

const scopeContains = (parent: Scope, child: Scope): boolean =>
  canonicalV3(parent) === canonicalV3(child) ||
  (parent.kind === "project" &&
    ["work", "task"].includes(child.kind) &&
    "projectId" in child &&
    parent.projectId === child.projectId) ||
  (parent.kind === "work" && child.kind === "task" && v3WorkKey(parent) === v3WorkKey(child));

async function existingBinding(
  root: string,
  head: string,
  request: CreateRequest,
): Promise<Binding | null> {
  const bytes = await readOptional(root, head, requestPath(request));
  if (bytes === null) return null;
  const b = JSON.parse(bytes) as Binding;
  assertV3("AllocationBinding", b);
  if (
    b.schema !== "common.git-work-allocation.v3" ||
    b.ownerId !== request.ownerId ||
    b.createRequestId !== request.createRequestId ||
    b.requestDigest !== digestV3(request) ||
    b.kind !== request.kind ||
    canonicalV3(b.scope) !== canonicalV3(request.scope)
  )
    throw Error("V3_CREATE_REQUEST_CONFLICT");
  if (!uuid(b.id)) throw Error("V3_INVALID_ALLOCATION_BINDING");
  const prepared = prepareV3Allocation(request, b.id);
  const creationCommit = await immutableGitLedgerCreation(root, head, requestPath(request), bytes);
  if (await immutableGitLedgerCreation(root, head, identityPath(b.id), bytes) !== creationCommit) throw Error("V3_IDENTITY_INDEX_CREATION_MISMATCH");
  const record = await git(root, ["cat-file", "blob", b.creation.recordBlobOid]);
  const wrapped = {
    schema: "common.git-work-record.v3",
    kind: request.kind,
    value: prepared.record,
  };
  if (
    b.recordPath !== prepared.recordPath ||
    record !== fileBytes(wrapped) ||
    hash(record) !== b.recordSha256 ||
    sourceBlobOid(record) !== b.creation.recordBlobOid ||
    await readOptional(root, creationCommit, prepared.recordPath) !== record ||
    (await readOptional(root, head, identityPath(b.id))) !== bytes ||
    b.contracts.length !== prepared.contractPaths.length ||
    b.creation.contractBlobs.length !== prepared.contractPaths.length
  )
    throw Error("V3_ALLOCATION_PUBLICATION_MISMATCH");
  for (let i = 0; i < prepared.contracts.length; i++) {
    const path = prepared.contractPaths[i]!;
    const content = await readOptional(root, head, path);
    if (
      content !== fileBytes(prepared.contracts[i]) ||
      b.contracts[i]!.path !== path ||
      b.contracts[i]!.sha256 !== hash(content) ||
      b.creation.contractBlobs[i]?.path !== path ||
      b.creation.contractBlobs[i]?.blobOid !== sourceBlobOid(content) ||
      b.creation.contractBlobs[i]?.sha256 !== hash(content) ||
      await readOptional(root, creationCommit, path) !== content
    )
      throw Error("V3_ALLOCATION_CONTRACT_MISMATCH");
  }
  await verifyRecordSuccessorLineage(root, creationCommit, head, prepared.recordPath, record);
  return b;
}

async function readVerifiedOwnerGraph(owner: NativeGitIdentityOwner, target: GitAllocationTarget, head: string): Promise<Graph> {
  const observedGraph = await owner.readGraph({
          repositoryRoot: target.repositoryRoot,
          ownerRef: target.ownerRef,
          head,
        });
  const graph = { ...observedGraph, works: [...observedGraph.works], tasks: [...observedGraph.tasks], contracts: [...observedGraph.contracts] };
  validateGitWorkV3Graph(graph);
      // Native metadata still owns Domain/Project semantics. Every projected v3
      // Work/Task/contract must be backed by exactly the selected Git bytes.
      for (const w of graph.works)
        if (
          (await readOptional(target.repositoryRoot, head, workPath(w) + "/work.json")) !==
          fileBytes({ schema: "common.git-work-record.v3", kind: "work", value: w })
        )
          throw Error("V3_OWNER_GRAPH_WORK_MISMATCH");
      for (const t of graph.tasks)
        if (
          (await readOptional(
            target.repositoryRoot,
            head,
            workPath(t) + "/tasks/" + encodedId(t.taskId) + ".json",
          )) !== fileBytes({ schema: "common.git-work-record.v3", kind: "task", value: t })
        )
          throw Error("V3_OWNER_GRAPH_TASK_MISMATCH");
      for (const c of graph.contracts)
        if ((await readOptional(target.repositoryRoot, head, contractPath(c))) !== fileBytes(c))
          throw Error("V3_OWNER_GRAPH_CONTRACT_MISMATCH");
  return graph;
}

export async function previewGitWorkV3Allocation(input: unknown, target: GitAllocationTarget) {
  checkRequest(input);
  const head = await checkedTarget(target);
  if (head !== target.expectedHead) throw Error("V3_STALE_OWNER_HEAD");
  const prepared = prepareV3Allocation(input, "00000000-0000-4000-8000-000000000000");
  return {
    schema: "common.git-work-allocation-preview.v3",
    head,
    requestDigest: digestV3(input),
    recordPathExample: prepared.recordPath,
    companionContractCount: prepared.contracts.length,
    id: null,
    publication: "none",
    requiresNativeOwnerAuthentication: true,
    nativePermissionGranted: false,
  };
}

/** The mapping, record, companion contracts and collision index land in one owner-ref CAS.
 * Failed/ambiguous CAS is reconciled once; no commit/ref mutation is retried here.
 */
export async function allocateGitWorkIdentity(
  input: unknown,
  target: GitAllocationTarget,
  owner: NativeGitIdentityOwner,
) {
  checkRequest(input);
  if (
    !owner ||
    typeof owner.withCurrentAuthority !== "function" ||
    typeof owner.readGraph !== "function"
  )
    throw Error("V3_NATIVE_OWNER_ADAPTER_REQUIRED");
  const request = structuredClone(input);
  target = structuredClone(target);
  const head = await checkedTarget(target);
  return await owner.withCurrentAuthority(
    { ...target, head, request: structuredClone(request) },
    async (authority) => {
      const expires = Date.parse(authority.expiresAtUtc);
      if (
        authority.membership !== true ||
        authority.ownerId !== request.ownerId ||
        authority.revision !== target.expectedAuthorityRevision ||
        !authority.actorId?.trim() ||
        !authority.authorityRef?.trim() ||
        !Number.isFinite(expires) ||
        expires <= Date.now() ||
        !authority.kinds.includes(request.kind) ||
        !scopeContains(authority.scope, request.scope)
      )
        throw Error("V3_CURRENT_OWNER_AUTHORITY_REQUIRED");
      const transaction = qualifySourceGitTransaction(target, head, request.ownerId, authority.gitHookPolicy);
      if (await readOptional(target.repositoryRoot, head, `work/.identity-legacy-requests/${hash(request.ownerId)}/${hash(request.createRequestId)}.json`) !== null) throw Error("V3_LEGACY_REQUEST_REQUIRES_VERSIONED_RECOVERY");
      const verifiedGraph = await readVerifiedOwnerGraph(owner, target, head);
      const prior = await existingBinding(target.repositoryRoot, head, request);
      if (prior)
        return {
          id: prior.id,
          head,
          status: "existing" as const,
          requestDigest: prior.requestDigest,
          nativePermissionGranted: false as const,
        };
      if (head !== target.expectedHead) throw Error("V3_STALE_OWNER_HEAD");
      const id = randomUUID();
      const prepared = prepareV3Allocation(request, id);
      const allPaths = (
        await git(target.repositoryRoot, ["ls-tree", "-r", "--name-only", head, "--", "work"])
      ).split("\n");
      if (
        allPaths.some(
          (path) =>
            path === identityPath(id) ||
            path.split("/").includes(id) ||
            path.endsWith("/" + id + ".json"),
        )
      )
        throw Error("V3_ID_COLLISION");
      const graph = verifiedGraph;
      // Prove selected parent membership against actual owner bytes, not graph-only claims.
      const selectedScope = request.scope;
      if (selectedScope.kind === "work" || selectedScope.kind === "task") {
        const work = graph.works.find((w) => v3WorkKey(w) === v3WorkKey(selectedScope));
        if (
          !work ||
          (await readOptional(target.repositoryRoot, head, workPath(work) + "/work.json")) !==
            fileBytes({ schema: "common.git-work-record.v3", kind: "work", value: work })
        )
          throw Error("V3_OWNER_WORK_MEMBERSHIP_MISMATCH");
        if (!["admitted", "active"].includes(work.status)) throw Error("V3_OWNER_WORK_NOT_ACTIVE");
      }
      if (request.kind === "work") graph.works.push(prepared.record as Work);
      else if (request.kind === "task") graph.tasks.push(prepared.record as Task);
      else {
        const run = prepared.record as Run;
        const task = graph.tasks.find((t) => v3TaskKey(t) === v3TaskKey(run.task));
        if (
          !task ||
          (await readOptional(
            target.repositoryRoot,
            head,
            workPath(task) + "/tasks/" + encodedId(task.taskId) + ".json",
          )) !== fileBytes({ schema: "common.git-work-record.v3", kind: "task", value: task })
        )
          throw Error("V3_OWNER_TASK_MEMBERSHIP_MISMATCH");
        if (!["planned", "ready"].includes(task.status))
          throw Error("V3_OWNER_TASK_NOT_RESERVABLE");
        if (task.assigneeActorId === null) throw Error("V3_RUN_NATIVE_ASSIGNEE_UNOBSERVED");
        if (run.effectiveContractDigest !== resolveEffectiveContract(graph, run.task).digest)
          throw Error("V3_RUN_EFFECTIVE_CONTRACT_MISMATCH");
        let lastAttempt = 0;
        for (const path of allPaths.filter(
          (p) => p.startsWith(workPath(run.task) + "/runs/") && p.endsWith(".json"),
        )) {
          const prior = JSON.parse((await readOptional(target.repositoryRoot, head, path))!) as {
            schema: string;
            kind: string;
            value: unknown;
          };
          assertV3("Run", prior.value);
          if (prior.schema !== "common.git-work-record.v3" || prior.kind !== "run")
            throw Error("V3_RUN_RECORD_REQUIRED");
          if (v3TaskKey(prior.value.task) === v3TaskKey(run.task))
            lastAttempt = Math.max(lastAttempt, prior.value.attempt);
        }
        if (run.attempt !== lastAttempt + 1) throw Error("V3_NEXT_RUN_ATTEMPT_REQUIRED");
      }
      graph.contracts.push(...prepared.contracts);
      validateGitWorkV3Graph(graph);
      const recordBytes = fileBytes({
        schema: "common.git-work-record.v3",
        kind: request.kind,
        value: prepared.record,
      });
      assertV3("StoredRecord", {
        schema: "common.git-work-record.v3",
        kind: request.kind,
        value: prepared.record,
      });
      const binding: Binding = {
        schema: "common.git-work-allocation.v3",
        ownerId: request.ownerId,
        createRequestId: request.createRequestId,
        requestDigest: digestV3(request),
        kind: request.kind,
        scope: request.scope,
        id,
        recordPath: prepared.recordPath,
        recordSha256: hash(recordBytes),
        creation: { recordBlobOid: sourceBlobOid(recordBytes), contractBlobs: prepared.contracts.map((c, i) => ({ path: prepared.contractPaths[i]!, blobOid: sourceBlobOid(fileBytes(c)), sha256: hash(fileBytes(c)) })) },
        contracts: prepared.contracts.map((c, i) => ({
          path: prepared.contractPaths[i]!,
          sha256: hash(fileBytes(c)),
        })),
        authority: {
          actorId: authority.actorId,
          revision: authority.revision,
          authorityRef: authority.authorityRef,
          gitHookPolicyRef: transaction.policy.qualificationRef,
        },
      };
      const files = new Map([
        [prepared.recordPath, recordBytes],
        [requestPath(request), fileBytes(binding)],
        [identityPath(id), fileBytes(binding)],
        ...prepared.contracts.map(
          (c, i) => [prepared.contractPaths[i]!, fileBytes(c)] as [string, string],
        ),
      ]);
      for (const path of files.keys())
        if ((await readOptional(target.repositoryRoot, head, path)) !== null)
          throw Error("V3_ALLOCATION_TARGET_EXISTS");
      const publication = await publishGitOwnerFiles(target, head, files, "Allocate source identity " + digestV3(request), transaction);
      if (publication.cas === "observe") {
        const observed = await checkedTarget(target);
        const reconciled = await existingBinding(target.repositoryRoot, observed, request);
        if (reconciled) return { id: reconciled.id, head: observed, status: "reconciled" as const, requestDigest: reconciled.requestDigest, nativePermissionGranted: false as const };
        throw Error("V3_OWNER_CAS_NOT_APPLIED_OR_UNCERTAIN_OBSERVE_BEFORE_NEW_ATTEMPT");
      }
      const observed = await checkedTarget(target);
      const receipt = await existingBinding(target.repositoryRoot, observed, request);
      if (!receipt || receipt.id !== id) throw Error("V3_POST_CAS_RECEIPT_UNCERTAIN");
      return {
        id,
        head: observed,
        status: "created" as const,
        requestDigest: receipt.requestDigest,
        nativePermissionGranted: false as const,
      };
    },
  );
}
