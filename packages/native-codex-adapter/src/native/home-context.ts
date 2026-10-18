import { createHash } from "node:crypto";
import {
  closeSync,
  constants,
  fstatSync,
  lstatSync,
  openSync,
  readSync,
  realpathSync,
} from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { HookBinding, NativeContextLoader, NativeWorkCheckoutObservation } from "./hooks.ts";
import type {
  CompactGitWorkContext,
  GitWorkCompactOutput,
  CompactSourcePointer,
  GitWorkContext,
} from "@palantirkc/work-continuity/git-work";

type ObjectValue = Record<string, unknown>;
interface Pointer {
  scope: string;
  owner: string;
  reference: string;
}
export interface HomeContextContract {
  schema: "codex.home-context.v1";
  projectId: string;
  entrypointCwd: string;
  sourceProjectRoot: string;
  globalProfilePath: string;
  northStar: string;
  priorities: string[];
  ownership: Pointer[];
  canonicalSources: { role: string; path: string; readPolicy: string }[];
  projectRoute: {
    projectId: string;
    root: string;
    workId: string;
    effectfulWorkCwd: string;
    contextCommand: string;
    checkpointPath: string;
    purposeRef: string;
    nextSessionRef: string;
    boundaryRef: string;
    kickoffPath: string;
  };
  recordPolicy: {
    distinctions: string[];
    materialBoundary: string;
    tinyWork: string;
    sources: string;
    ontologyPromotion: string;
  };
  boundaries: ObjectValue;
}
/**
 * Explicit HOST successor. The code owner is fixed by the native wrapper, while
 * projectId/workId identify the independently selected immutable Git Work.
 * A checkout receipt is owner-local source evidence, never native permission.
 */
export interface HomeContextContractV2 extends Omit<
  HomeContextContract,
  "schema" | "sourceProjectRoot" | "projectRoute"
> {
  schema: "codex.home-context.v2";
  nativeCodexHome: string;
  codeOwner: { projectId: "codex-harness-engineering"; root: string };
  selectedWork: {
    repositoryRoot: string;
    repositoryId: string;
    commit: string;
    projectId: string;
    workId: string;
    checkoutObservation?: { path: string; sha256: string };
  };
}
export type NativeHomeContextContract = HomeContextContract | HomeContextContractV2;
export interface HomeContextProjection {
  status: "current" | "held" | "repair-required";
  text: string;
  chars: number;
  authority: "none";
  nativePermissionGranted: false;
  omitted: string[];
}
const object = (value: unknown): value is ObjectValue =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const within = (root: string, path: string) => {
  const rel = relative(root, path);
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
};
const hash = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
function text(value: unknown, max = 600): string {
  if (
    typeof value !== "string" ||
    !value ||
    value.length > max ||
    /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/u.test(value)
  )
    throw new Error("Invalid bounded context text");
  return value;
}
function pointer(value: unknown): string {
  const path = text(value, 1024);
  if (!isAbsolute(path) || resolve(path) !== path) throw new Error("Invalid canonical pointer");
  return path;
}
function list(value: unknown, max = 30, itemMax = 600): string[] {
  if (!Array.isArray(value) || value.length > max) throw new Error("Invalid bounded context list");
  return value.map((item) => text(item, itemMax));
}
function row(value: unknown): ObjectValue {
  if (!object(value)) throw new Error("Invalid context record");
  return value;
}
function exactKeys(value: ObjectValue, required: string[], optional: string[] = []): void {
  if (
    required.some((key) => !Object.hasOwn(value, key)) ||
    Object.keys(value).some((key) => !required.includes(key) && !optional.includes(key))
  )
    throw new Error("Invalid exact Home v2 record");
}
function canonicalDirectory(value: unknown): string {
  const path = pointer(value);
  if (realpathSync(path) !== path || !lstatSync(path).isDirectory())
    throw new Error("Unsafe Home directory");
  return path;
}
function sourceFile(path: string, maxBytes: number): { value: unknown; sha256: string } {
  const before = lstatSync(path);
  if (
    resolve(path) !== path ||
    realpathSync(path) !== path ||
    !before.isFile() ||
    before.isSymbolicLink() ||
    before.size > maxBytes
  )
    throw new Error("Unsafe or oversized context source");
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const opened = fstatSync(fd);
    if (
      !opened.isFile() ||
      opened.dev !== before.dev ||
      opened.ino !== before.ino ||
      opened.size > maxBytes
    )
      throw new Error("Context source changed before open");
    const buffer = Buffer.alloc(opened.size + 1);
    let count = 0;
    while (count < buffer.length) {
      const next = readSync(fd, buffer, count, buffer.length - count, null);
      if (!next) break;
      count += next;
    }
    const after = fstatSync(fd),
      named = lstatSync(path);
    if (
      count !== opened.size ||
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      named.dev !== opened.dev ||
      named.ino !== opened.ino ||
      named.isSymbolicLink() ||
      realpathSync(path) !== path
    )
      throw new Error("Context source changed during read");
    const bytes = buffer.subarray(0, count);
    return { value: JSON.parse(bytes.toString("utf8")), sha256: hash(bytes) };
  } finally {
    closeSync(fd);
  }
}
function ownerPointers(value: unknown): Pointer[] {
  if (!Array.isArray(value) || value.length > 20)
    throw new Error("Invalid semantic owner pointers");
  return value.map((item) => {
    const data = row(item);
    return { scope: text(data.scope), owner: text(data.owner), reference: pointer(data.reference) };
  });
}
function safeProfile(value: unknown): ObjectValue {
  const data = row(value),
    implementation = row(data.implementation),
    artifacts = row(data.canonicalArtifacts),
    conversation = row(data.conversation),
    ontology = row(data.ontology);
  if (typeof artifacts.nativeRequiredFormatsAllowed !== "boolean")
    throw new Error("Invalid artifact format preference");
  const safe: ObjectValue = {
    implementation: {
      language: text(implementation.language, 80),
      runtime: text(implementation.runtime, 80),
    },
    canonicalArtifacts: {
      extensions: list(artifacts.extensions, 10, 30),
      language: text(artifacts.language, 80),
      nativeRequiredFormatsAllowed: artifacts.nativeRequiredFormatsAllowed,
    },
    conversation: { language: text(conversation.language, 100) },
    ontology: {
      ownership: text(ontology.ownership, 100),
      organization: text(ontology.organization, 100),
    },
    excludedDomains: list(data.excludedDomains, 40, 120),
  };
  const capabilities = row(data.selectedCapabilities),
    entries = Object.entries(capabilities);
  if (entries.length > 30) throw new Error("Too many capability preferences");
  safe.selectedCapabilities = Object.fromEntries(
    entries.map(([id, value]) => {
      const capability = row(value);
      if (!/^S[1-9][0-9]{0,2}$/.test(id) || typeof capability.enabled !== "boolean")
        throw new Error("Invalid selected capability");
      return [id, { enabled: capability.enabled, meaning: text(capability.meaning, 350) }];
    }),
  );
  const native = row(data.nativeBinding);
  safe.semanticOwners = ownerPointers(native.semanticOwners);
  if (!Array.isArray(native.projectEntrypoints) || native.projectEntrypoints.length > 20)
    throw new Error("Invalid project entrypoints");
  safe.projectEntrypoints = native.projectEntrypoints.map((value) => {
    const entry = row(value),
      root = pointer(entry.root),
      entrypoints = list(entry.entrypoints, 15, 300);
    if (entrypoints.some((path) => isAbsolute(path) || !within(root, resolve(root, path))))
      throw new Error("Unsafe registered source pointer");
    return { projectId: text(entry.projectId, 120), root, entrypoints };
  });
  return safe;
}
/** Validate a prospective Home value without reading profile bytes or publishing native files. */
export function readHomeContextContractValue(
  value: HomeContextContract,
  binding: HookBinding,
  cwd: string,
): HomeContextContract;
export function readHomeContextContractValue(
  value: HomeContextContractV2,
  binding: HookBinding,
  cwd: string,
): HomeContextContractV2;
export function readHomeContextContractValue(
  value: unknown,
  binding: HookBinding,
  cwd: string,
): NativeHomeContextContract;
export function readHomeContextContractValue(
  value: unknown,
  binding: HookBinding,
  cwd: string,
): NativeHomeContextContract {
  if (
    !binding.entrypointContextPath ||
    binding.qualificationLedgerPath ||
    pointer(cwd) !== cwd ||
    !binding.entrypointCwds?.includes(cwd) ||
    pointer(binding.root) !== binding.root ||
    within(binding.root, cwd)
  )
    throw new Error("Invalid exact Home owner binding");
  const path = binding.entrypointContextPath;
  if (pointer(path) !== path || !within(binding.root, path))
    throw new Error("Home contract escapes its project owner");
  const data = row(structuredClone(value)),
    boundaries = row(data.boundaries),
    policy = row(data.recordPolicy);
  if (data.schema === "codex.home-context.v2") return readHomeContextV2Value(data, binding, cwd);
  const route = row(data.projectRoute);
  if (
    data.schema !== "codex.home-context.v1" ||
    data.projectId !== binding.projectId ||
    data.entrypointCwd !== cwd ||
    data.sourceProjectRoot !== binding.root ||
    data.globalProfilePath !== resolve(cwd, ".codex/harness/profile.json") ||
    route.projectId !== binding.projectId ||
    route.root !== binding.root ||
    route.effectfulWorkCwd !== binding.root ||
    route.checkpointPath !== binding.checkpointPath ||
    (binding.workId !== undefined && route.workId !== binding.workId)
  )
    throw new Error("Home context differs from the canonical owner and actual cwd");
  for (const key of [
    "nativePermissionGranted",
    "captureHomeRawPrompts",
    "qualificationLedgerBoundToHome",
    "newModelBudgetGranted",
    "directoryMoveAuthorized",
    "hermesRuntimeEffectsAuthorized",
    "configuredInjectionIsFreshSessionProof",
  ])
    if (boundaries[key] !== false)
      throw new Error("Home context boundary grants unsupported effects");
  if (
    boundaries.authority !== "none" ||
    boundaries.maxContextChars !== 6000 ||
    boundaries.nativeContextTokenLimit !== 1500
  )
    throw new Error("Home context boundary or budget differs");
  const projectRoute = {
    projectId: text(route.projectId, 120),
    root: pointer(route.root),
    workId: text(route.workId, 120),
    effectfulWorkCwd: pointer(route.effectfulWorkCwd),
    contextCommand: text(route.contextCommand, 600),
    checkpointPath: pointer(route.checkpointPath),
    purposeRef: text(route.purposeRef, 300),
    nextSessionRef: text(route.nextSessionRef, 300),
    boundaryRef: text(route.boundaryRef, 300),
    kickoffPath: pointer(route.kickoffPath),
  };
  if (
    !within(binding.root, projectRoute.kickoffPath) ||
    [projectRoute.purposeRef, projectRoute.nextSessionRef, projectRoute.boundaryRef].some(
      (path) => isAbsolute(path) || !within(binding.root, resolve(binding.root, path)),
    )
  )
    throw new Error("Project route escapes its owner");
  if (!Array.isArray(data.canonicalSources) || data.canonicalSources.length > 20)
    throw new Error("Invalid canonical context pointers");
  const canonicalSources = data.canonicalSources.map((value) => {
    const entry = row(value);
    return {
      role: text(entry.role),
      path: pointer(entry.path),
      readPolicy: text(entry.readPolicy),
    };
  });
  const contract: HomeContextContract = {
    schema: "codex.home-context.v1",
    projectId: binding.projectId,
    entrypointCwd: cwd,
    sourceProjectRoot: binding.root,
    globalProfilePath: pointer(data.globalProfilePath),
    northStar: text(data.northStar, 1600),
    priorities: list(data.priorities, 10, 300),
    ownership: ownerPointers(data.ownership),
    canonicalSources,
    projectRoute,
    recordPolicy: {
      distinctions: list(policy.distinctions, 15, 100),
      materialBoundary: text(policy.materialBoundary),
      tinyWork: text(policy.tinyWork),
      sources: text(policy.sources),
      ontologyPromotion: text(policy.ontologyPromotion),
    },
    boundaries,
  };
  return contract;
}

function readHomeContextV2Value(
  data: ObjectValue,
  binding: HookBinding,
  cwd: string,
): HomeContextContractV2 {
  exactKeys(data, [
    "schema",
    "projectId",
    "entrypointCwd",
    "nativeCodexHome",
    "globalProfilePath",
    "codeOwner",
    "selectedWork",
    "northStar",
    "priorities",
    "ownership",
    "canonicalSources",
    "recordPolicy",
    "boundaries",
  ]);
  const codeOwner = row(data.codeOwner),
    selected = row(data.selectedWork),
    boundaries = row(data.boundaries),
    policy = row(data.recordPolicy);
  exactKeys(codeOwner, ["projectId", "root"]);
  exactKeys(
    selected,
    ["repositoryRoot", "repositoryId", "commit", "projectId", "workId"],
    ["checkoutObservation"],
  );
  if (
    binding.gitWorkContextVersion !== "v2" ||
    binding.gitWorkMetadataEncoding !== "metadata.v1" ||
    !binding.nativeCodexHome ||
    canonicalDirectory(binding.nativeCodexHome) !== data.nativeCodexHome ||
    canonicalDirectory(binding.root) !== codeOwner.root ||
    canonicalDirectory(cwd) !== data.entrypointCwd ||
    !within(binding.root, pointer(binding.checkpointPath)) ||
    codeOwner.projectId !== "codex-harness-engineering" ||
    data.projectId !== binding.projectId ||
    selected.projectId !== binding.projectId ||
    !binding.workId ||
    selected.workId !== binding.workId ||
    data.globalProfilePath !== resolve(binding.nativeCodexHome, "harness/profile.json") ||
    typeof selected.commit !== "string" ||
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(selected.commit)
  )
    throw new Error("Home v2 differs from the fixed owner, native Home or selected Work");
  const falseBoundaries = [
    "nativePermissionGranted",
    "captureHomeRawPrompts",
    "qualificationLedgerBoundToHome",
    "newModelBudgetGranted",
    "directoryMoveAuthorized",
    "hermesRuntimeEffectsAuthorized",
    "configuredInjectionIsFreshSessionProof",
  ];
  exactKeys(boundaries, [
    "authority",
    ...falseBoundaries,
    "maxContextChars",
    "nativeContextTokenLimit",
  ]);
  if (
    falseBoundaries.some((key) => boundaries[key] !== false) ||
    boundaries.authority !== "none" ||
    boundaries.maxContextChars !== 6000 ||
    boundaries.nativeContextTokenLimit !== 1500
  )
    throw new Error("Home context boundary or budget differs");
  let checkoutObservation: { path: string; sha256: string } | undefined;
  if (selected.checkoutObservation !== undefined) {
    const pin = row(selected.checkoutObservation);
    exactKeys(pin, ["path", "sha256"]);
    const path = pointer(pin.path);
    if (
      !within(binding.root, path) ||
      typeof pin.sha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(pin.sha256)
    )
      throw new Error("Home checkout observation escapes its fixed owner");
    checkoutObservation = { path, sha256: pin.sha256 };
  }
  if (!Array.isArray(data.canonicalSources) || data.canonicalSources.length > 20)
    throw new Error("Invalid canonical context pointers");
  return {
    schema: "codex.home-context.v2",
    projectId: binding.projectId,
    entrypointCwd: cwd,
    nativeCodexHome: binding.nativeCodexHome,
    globalProfilePath: pointer(data.globalProfilePath),
    codeOwner: { projectId: "codex-harness-engineering", root: binding.root },
    selectedWork: {
      repositoryRoot: canonicalDirectory(selected.repositoryRoot),
      repositoryId: text(selected.repositoryId, 120),
      commit: selected.commit,
      projectId: text(selected.projectId, 120),
      workId: text(selected.workId, 120),
      ...(checkoutObservation ? { checkoutObservation } : {}),
    },
    northStar: text(data.northStar, 1600),
    priorities: list(data.priorities, 10, 300),
    ownership: ownerPointers(data.ownership),
    canonicalSources: data.canonicalSources.map((value) => {
      const entry = row(value);
      return {
        role: text(entry.role),
        path: pointer(entry.path),
        readPolicy: text(entry.readPolicy),
      };
    }),
    recordPolicy: {
      distinctions: list(policy.distinctions, 15, 100),
      materialBoundary: text(policy.materialBoundary),
      tinyWork: text(policy.tinyWork),
      sources: text(policy.sources),
      ontologyPromotion: text(policy.ontologyPromotion),
    },
    boundaries,
  };
}

function readContract(
  binding: HookBinding,
  cwd: string,
): { contract: NativeHomeContextContract; sha256: string } {
  if (
    !binding.entrypointContextPath ||
    binding.qualificationLedgerPath ||
    cwd !== realpathSync(cwd) ||
    !binding.entrypointCwds?.includes(cwd) ||
    binding.root !== realpathSync(binding.root) ||
    within(binding.root, cwd)
  )
    throw new Error("Invalid exact Home owner binding");
  const path = binding.entrypointContextPath;
  if (!within(binding.root, path)) throw new Error("Home contract escapes its project owner");
  const source = sourceFile(path, 64 * 1024);
  return {
    contract: readHomeContextContractValue(source.value, binding, cwd),
    sha256: source.sha256,
  };
}

export interface NativeGitWorkDeliveryPolicy {
  policyVersion: "v1" | "v2";
  maxChars: number;
  versionCapChars: 6000 | 12000;
  qualification: "source-only-not-native-admission";
}
/** Native delivery policy is explicit; an arbitrary caller budget cannot opt into v2. */
export function gitWorkDeliveryPolicy(
  binding: HookBinding,
  requestedMaxChars?: number,
): NativeGitWorkDeliveryPolicy {
  if (
    binding.gitWorkContextVersion !== undefined &&
    binding.gitWorkContextVersion !== "v1" &&
    binding.gitWorkContextVersion !== "v2"
  )
    throw Error("INVALID_GIT_WORK_CONTEXT_VERSION");
  const policyVersion = binding.gitWorkContextVersion ?? "v1";
  if (
    binding.gitWorkMetadataEncoding !== undefined &&
    (binding.gitWorkMetadataEncoding !== "metadata.v1" || policyVersion !== "v2")
  )
    throw Error("INVALID_GIT_WORK_METADATA_ENCODING");
  const versionCapChars = policyVersion === "v2" ? 12000 : 6000;
  return {
    policyVersion,
    maxChars: Math.min(
      versionCapChars,
      Math.max(
        2000,
        Number.isSafeInteger(requestedMaxChars) ? requestedMaxChars! : versionCapChars,
      ),
    ),
    versionCapChars,
    qualification: "source-only-not-native-admission",
  };
}
export interface SelectedGitWorkView {
  status: "source-qualified" | "held";
  holdReasons: string[];
  pointer: {
    repositoryId: string;
    commit: string;
    projectId: string;
    workId: string;
    sourcePathBase: string;
  };
  purpose: unknown;
  currentTask?: unknown;
  decisionOriginals: unknown;
  historicalEffects: unknown;
  /** Complete common semantics supplement the compatibility fields; never a weaker view. */
  semanticContext: CompactGitWorkContext;
  projection: "bounded-derived-pointer-view";
  currentness: "caller-declared-not-remote-verified";
  authority: "none";
  nativePermissionGranted: false;
  semanticAcceptanceEstablished: false;
}
export type SelectedGitWorkContext = SelectedGitWorkView | GitWorkCompactOutput;
export function selectedGitWorkPointer(selected: SelectedGitWorkContext) {
  if ("encoding" in selected) {
    const [repositoryId, commit, projectId, workId] = selected.metadata;
    if (![repositoryId, commit, projectId, workId].every((value) => typeof value === "string"))
      throw Error("INVALID_COMPACT_SOURCE_POINTER");
    return {
      repositoryId: repositoryId as string,
      commit: commit as string,
      projectId: projectId as string,
      workId: workId as string,
      sourcePathBase: `work/${encodeURIComponent(projectId as string)}/${encodeURIComponent(workId as string)}`,
    };
  }
  return "pointer" in selected
    ? selected.pointer
    : {
        repositoryId: selected.repositoryId,
        commit: selected.commit,
        projectId: selected.projectId,
        workId: selected.workId,
        sourcePathBase: selected.sourcePathBase,
      };
}
/** Consume the single common codec only after the native owner has proved its closure. */
export function projectSelectedGitWork(
  context: GitWorkContext,
  selection: { taskId?: string; decisionIds?: string[] },
  codec: Pick<
    NonNullable<Awaited<ReturnType<NativeContextLoader>>["gitWork"]>,
    "compactGitWorkContext" | "decodeCompactGitWorkContext"
  >,
  version: "v1" | "v2" = "v1",
): SelectedGitWorkContext {
  const semanticContext = codec.compactGitWorkContext(context, selection);
  // Reader/parser diagnostics may contain private source bytes. Deliver codes only.
  semanticContext.holdReasons = semanticContext.holdReasons.map((reason) =>
    /^[A-Z][A-Z0-9_]{0,120}(?::|$)/.test(reason)
      ? reason.split(":")[0]!
      : "REQUIRED_SOURCE_UNAVAILABLE",
  );
  if (version === "v2") return semanticContext;
  const decoded =
    semanticContext.status === "source-qualified"
      ? codec.decodeCompactGitWorkContext(semanticContext)
      : undefined;
  const work = decoded?.normative.find((record) => record.kind === "work");
  const purpose = object(work?.purpose) ? work.purpose : undefined;
  const sourceWork = context.records.find((record) => record.kind === "work");
  const task = decoded?.task;
  const source = (address: CompactSourcePointer) => ({
    source: `o${address[0] + 1}`,
    ...(address[1] ? { fragment: address[1] } : {}),
  });
  return {
    status: semanticContext.status,
    holdReasons: [...semanticContext.holdReasons],
    pointer: selectedGitWorkPointer(semanticContext),
    purpose: {
      objective: purpose?.objective,
      fullPurpose: "work.json#/purpose",
      contractsSource: "work.json",
      specRevision: object(sourceWork?.spec) ? sourceWork.spec.revision : undefined,
      planRevision: object(sourceWork?.plan) ? sourceWork.plan.revision : undefined,
    },
    ...(task
      ? {
          currentTask: {
            id: task.id,
            title: context.records.find((record) => record.kind === "task" && record.id === task.id)
              ?.title,
            state: task.state,
            contractSource: `tasks/${encodeURIComponent(task.id)}.json#/logical`,
            specRevision: task.logical.specRevision,
            planRevision: task.logical.planRevision,
          },
        }
      : {}),
    decisionOriginals: {
      governingIndex: "work.json#/governingDecisionIds",
      recordPattern: "decisions/<encodeURIComponent(id)>.json",
      decisionCount: semanticContext.decisions.length,
      directPointers: (decoded?.decisions ?? []).map((record) => ({
        id: record.id,
        status: record.status,
        answer: source(record.answerSource),
        ...(record.selectionContextSource
          ? { selectionContext: source(record.selectionContextSource) }
          : {}),
      })),
      sources: (decoded?.sources ?? []).map((address, index) => ({
        id: `o${index + 1}`,
        ...address,
      })),
      unexpandedDecisionCount: 0,
    },
    historicalEffects: semanticContext.historicalEffects,
    semanticContext,
    projection: "bounded-derived-pointer-view",
    currentness: "caller-declared-not-remote-verified",
    authority: "none",
    nativePermissionGranted: false,
    semanticAcceptanceEstablished: false,
  };
}

function selectedHomeRoute(
  contract: HomeContextContractV2,
  options: {
    selectedWork?: SelectedGitWorkContext;
    selectedRepositoryRoot?: string;
    observeCheckout?: (observation: NativeWorkCheckoutObservation) => void;
  },
): {
  executionReady: boolean;
  effectfulWorkCwd?: string;
  observation?: { path: string; sha256: string };
} {
  const expected = contract.selectedWork;
  if (options.selectedWork) {
    const actual = selectedGitWorkPointer(options.selectedWork);
    if (
      ["repositoryId", "commit", "projectId", "workId"].some(
        (key) => actual[key as keyof typeof actual] !== expected[key as keyof typeof expected],
      ) ||
      options.selectedRepositoryRoot !== expected.repositoryRoot
    )
      throw new Error("Home v2 selected route differs from the exact session Work");
  }
  if (
    !options.selectedWork ||
    options.selectedWork.status === "held" ||
    !expected.checkoutObservation
  )
    return { executionReady: false };
  const pin = expected.checkoutObservation,
    source = sourceFile(pin.path, 16 * 1024);
  if (source.sha256 !== pin.sha256) throw new Error("Home checkout observation drift");
  const data = row(source.value);
  exactKeys(data, [
    "schema",
    "repositoryRoot",
    "repositoryId",
    "commit",
    "projectId",
    "workId",
    "cwd",
    "branch",
  ]);
  if (
    data.schema !== "codex.home-work-checkout-observation.v1" ||
    ["repositoryRoot", "repositoryId", "commit", "projectId", "workId"].some(
      (key) => data[key] !== expected[key as keyof typeof expected],
    ) ||
    typeof data.branch !== "string" ||
    !/^refs\/heads\/[A-Za-z0-9][A-Za-z0-9._/-]{0,240}$/.test(data.branch) ||
    data.branch.includes("..") ||
    data.branch.includes("//") ||
    /[/.]$/.test(data.branch)
  )
    throw new Error("Home checkout observation differs from selected Work");
  const observation: NativeWorkCheckoutObservation = {
    schema: "codex.home-work-checkout-observation.v1",
    repositoryRoot: expected.repositoryRoot,
    repositoryId: expected.repositoryId,
    commit: expected.commit,
    projectId: expected.projectId,
    workId: expected.workId,
    cwd: canonicalDirectory(data.cwd),
    branch: data.branch,
  };
  if (!options.observeCheckout) throw new Error("Native checkout observation is required");
  options.observeCheckout(observation);
  if (sourceFile(pin.path, 16 * 1024).sha256 !== pin.sha256)
    throw new Error("Home checkout observation changed during read");
  return { executionReady: true, effectfulWorkCwd: observation.cwd, observation: pin };
}

/** Complete semantic view, before choosing a wire representation. Not native admission. */
export interface HomeContextViewV2 {
  schema: "codex.home-context.projection.v2";
  deliveryPolicy: NativeGitWorkDeliveryPolicy;
  sourceOwnerProjectRoot: string;
  nativeCodexHome: string;
  codeOwner: HomeContextContractV2["codeOwner"];
  selectedWorkRoute: ReturnType<typeof selectedHomeRoute>;
  actualNativeCwd: string;
  northStar: string;
  priorities: string[];
  ownership: Pointer[];
  preferences: {
    implementation: { language: string; runtime: string };
    canonicalArtifacts: {
      extensions: string[];
      language: string;
      nativeRequiredFormatsAllowed: boolean;
    };
    conversation: { language: string };
    ontology: { ownership: string; organization: string };
    excludedDomains: string[];
  };
  selectedWork: SelectedGitWorkContext | ObjectValue;
  recordPolicy: HomeContextContract["recordPolicy"];
  boundaries: {
    authority: "none";
    nativePermissionGranted: false;
    executionReady: boolean;
    captureHomeRawPrompts: false;
    exactCwdOnly: true;
    excludedDomainsRemainExcluded: true;
    newModelBudgetGranted: false;
    directoryMoveAuthorized: false;
  };
  sources: [{ path: string; sha256: string }, { path: string; sha256: string }];
  omitted: string[];
  selectedCapabilities?: unknown;
  semanticOwners?: unknown;
  projectEntrypoints?: unknown;
  canonicalSourcePaths?: unknown;
}

const homeV2Prefix =
  "Source-owned Home bootstrap. Preserve actualNativeCwd. Only selectedWorkRoute can identify an observed Work checkout; executionReady is source qualification and grants no native permission. Configured capabilities do not prove callable/authenticated tools. This hook does not capture or accept the prompt.\n";
const homeCompactPrefix =
  "Source-only Home v2 (home.v1; decodeHomeContextProjection). Preserve native cwd; only observed checkout routes Work. No native permission or prompt capture; configured capabilities are not authenticated tools.\n";
const homeOptionalKeys = [
  "selectedCapabilities",
  "semanticOwners",
  "projectEntrypoints",
  "canonicalSourcePaths",
] as const;

interface EncodedHomeContextProjectionV2 {
  schema: "codex.home-context.projection.v2";
  encoding: "home.v1";
  maxChars: number;
  locations: [string, string, string];
  checkout: [] | [string, string, string];
  purpose: [string, string[]];
  ownership: [string, string, string][];
  preferences: [[string, string], [string[], string, boolean], string, [string, string], string[]];
  selectedWork: HomeContextViewV2["selectedWork"];
  recordPolicy: [string[], string, string, string, string];
  sources: [string, string, string];
  authority: "none";
  nativePermissionGranted: false;
  selectedCapabilities?: unknown;
  semanticOwners?: unknown;
  projectEntrypoints?: unknown;
  canonicalSourcePaths?: unknown;
}

/**
 * home.v1 is a finite v2 envelope layout, not a Work codec. Tuples keep all values:
 * locations=[native Home, fixed code owner root, actual cwd]; checkout=[] or
 * [observed Work cwd, owner-relative receipt path, SHA256]; purpose=[northStar,priorities];
 * ownership rows=[scope,owner,reference]; preferences=[implementation,artifacts,
 * conversation,ontology,excludedDomains], with each nested record in declaration order;
 * recordPolicy follows HomeContextContract.recordPolicy declaration order;
 * sources=[owner-relative Home source path, Home SHA256, profile SHA256]. The profile
 * path is exactly native Home/harness/profile.json. Fixed policy/boundary constants,
 * the duplicate owner root/executionReady and absent optional labels are reconstructed.
 * selectedWork and present optional values are embedded unchanged, never abbreviated.
 */
function encodeHomeContextProjection(view: HomeContextViewV2): EncodedHomeContextProjectionV2 {
  const {
    implementation: i,
    canonicalArtifacts: a,
    conversation: c,
    ontology: o,
  } = view.preferences;
  const policy = view.recordPolicy,
    route = view.selectedWorkRoute;
  return {
    schema: view.schema,
    encoding: "home.v1",
    maxChars: view.deliveryPolicy.maxChars,
    locations: [view.nativeCodexHome, view.codeOwner.root, view.actualNativeCwd],
    checkout: route.executionReady
      ? [
          route.effectfulWorkCwd!,
          relative(view.codeOwner.root, route.observation!.path),
          route.observation!.sha256,
        ]
      : [],
    purpose: [view.northStar, view.priorities],
    ownership: view.ownership.map(({ scope, owner, reference }) => [scope, owner, reference]),
    preferences: [
      [i.language, i.runtime],
      [a.extensions, a.language, a.nativeRequiredFormatsAllowed],
      c.language,
      [o.ownership, o.organization],
      view.preferences.excludedDomains,
    ],
    selectedWork: view.selectedWork,
    recordPolicy: [
      policy.distinctions,
      policy.materialBoundary,
      policy.tinyWork,
      policy.sources,
      policy.ontologyPromotion,
    ],
    sources: [
      relative(view.codeOwner.root, view.sources[0].path),
      view.sources[0].sha256,
      view.sources[1].sha256,
    ],
    authority: "none",
    nativePermissionGranted: false,
    ...Object.fromEntries(
      homeOptionalKeys.filter((key) => Object.hasOwn(view, key)).map((key) => [key, view[key]]),
    ),
  };
}

/** Pure bounded inverse of home.v1; authenticates neither Work nor checkout facts.
 * Runtime source/tuple/receipt guards retain admission; this never observes/adopts a route. */
export function decodeHomeContextProjection(value: unknown): HomeContextViewV2 {
  try {
    const data = row(value);
    const wireChars = homeCompactPrefix.length + JSON.stringify(data).length;
    if (wireChars > 12000) throw Error();
    exactKeys(
      data,
      [
        "schema",
        "encoding",
        "maxChars",
        "locations",
        "checkout",
        "purpose",
        "ownership",
        "preferences",
        "selectedWork",
        "recordPolicy",
        "sources",
        "authority",
        "nativePermissionGranted",
      ],
      [...homeOptionalKeys],
    );
    if (
      data.schema !== "codex.home-context.projection.v2" ||
      data.encoding !== "home.v1" ||
      data.authority !== "none" ||
      data.nativePermissionGranted !== false ||
      !Number.isSafeInteger(data.maxChars) ||
      Number(data.maxChars) < 2000 ||
      Number(data.maxChars) > 12000 ||
      wireChars > Number(data.maxChars)
    )
      throw Error();
    const tuple = (value: unknown, length: number): unknown[] => {
      if (!Array.isArray(value) || value.length !== length) throw Error();
      return value;
    };
    const boolean = (value: unknown): boolean => {
      if (typeof value !== "boolean") throw Error();
      return value;
    };
    const digest = (value: unknown): string => {
      if (typeof value !== "string" || !/^[a-f0-9]{64}$/.test(value)) throw Error();
      return value;
    };
    const local = (root: string, value: unknown): string => {
      const path = text(value, 1024);
      if (
        isAbsolute(path) ||
        path.split("/").some((part) => !part || part === "." || part === "..") ||
        path.includes("\\")
      )
        throw Error();
      return pointer(resolve(root, path));
    };
    const locations = tuple(data.locations, 3),
      nativeCodexHome = pointer(locations[0]),
      ownerRoot = pointer(locations[1]),
      actualNativeCwd = pointer(locations[2]),
      purpose = tuple(data.purpose, 2),
      preferences = tuple(data.preferences, 5),
      implementation = tuple(preferences[0], 2),
      artifacts = tuple(preferences[1], 3),
      ontology = tuple(preferences[3], 2),
      policy = tuple(data.recordPolicy, 5),
      sources = tuple(data.sources, 3),
      selectedWork = row(data.selectedWork);
    if (
      !["held", "source-qualified"].includes(String(selectedWork.status)) ||
      selectedWork.authority !== "none" ||
      selectedWork.nativePermissionGranted !== false ||
      !Array.isArray(data.checkout) ||
      ![0, 3].includes(data.checkout.length)
    )
      throw Error();
    const checkout = data.checkout,
      executionReady = checkout.length === 3;
    if (executionReady && selectedWork.status !== "source-qualified") throw Error();
    if (!Array.isArray(data.ownership) || data.ownership.length > 20) throw Error();
    return {
      schema: "codex.home-context.projection.v2",
      deliveryPolicy: {
        policyVersion: "v2",
        maxChars: Number(data.maxChars),
        versionCapChars: 12000,
        qualification: "source-only-not-native-admission",
      },
      sourceOwnerProjectRoot: ownerRoot,
      nativeCodexHome,
      codeOwner: { projectId: "codex-harness-engineering", root: ownerRoot },
      selectedWorkRoute: executionReady
        ? {
            executionReady,
            effectfulWorkCwd: pointer(checkout[0]),
            observation: { path: local(ownerRoot, checkout[1]), sha256: digest(checkout[2]) },
          }
        : { executionReady },
      actualNativeCwd,
      northStar: text(purpose[0], 1600),
      priorities: list(purpose[1], 10, 300),
      ownership: data.ownership.map((value) => {
        const [scope, owner, reference] = tuple(value, 3);
        return { scope: text(scope), owner: text(owner), reference: pointer(reference) };
      }),
      preferences: {
        implementation: {
          language: text(implementation[0], 80),
          runtime: text(implementation[1], 80),
        },
        canonicalArtifacts: {
          extensions: list(artifacts[0], 10, 30),
          language: text(artifacts[1], 80),
          nativeRequiredFormatsAllowed: boolean(artifacts[2]),
        },
        conversation: { language: text(preferences[2], 100) },
        ontology: { ownership: text(ontology[0], 100), organization: text(ontology[1], 100) },
        excludedDomains: list(preferences[4], 40, 120),
      },
      selectedWork,
      recordPolicy: {
        distinctions: list(policy[0], 15, 100),
        materialBoundary: text(policy[1]),
        tinyWork: text(policy[2]),
        sources: text(policy[3]),
        ontologyPromotion: text(policy[4]),
      },
      boundaries: {
        authority: "none",
        nativePermissionGranted: false,
        executionReady,
        captureHomeRawPrompts: false,
        exactCwdOnly: true,
        excludedDomainsRemainExcluded: true,
        newModelBudgetGranted: false,
        directoryMoveAuthorized: false,
      },
      sources: [
        { path: local(ownerRoot, sources[0]), sha256: digest(sources[1]) },
        { path: resolve(nativeCodexHome, "harness/profile.json"), sha256: digest(sources[2]) },
      ],
      omitted: homeOptionalKeys.filter((key) => !Object.hasOwn(data, key)),
      ...Object.fromEntries(
        homeOptionalKeys.filter((key) => Object.hasOwn(data, key)).map((key) => [key, data[key]]),
      ),
    };
  } catch {
    throw Error("INVALID_HOME_PROJECTION_ENCODING");
  }
}

/** Prospective source-only sizing, also used after runtime source/checkout guards. */
export function renderHomeContextProjectionV2(view: HomeContextViewV2): string {
  const ordinary = homeV2Prefix + JSON.stringify(view);
  if (ordinary.length <= view.deliveryPolicy.maxChars) return ordinary;
  const encoded = encodeHomeContextProjection(view),
    result = homeCompactPrefix + JSON.stringify(encoded);
  // Oversized required views are returned only to the caller's existing hold path.
  if (
    result.length <= view.deliveryPolicy.maxChars &&
    !isDeepStrictEqual(decodeHomeContextProjection(encoded), view)
  )
    throw Error("HOME_PROJECTION_ROUNDTRIP_MISMATCH");
  return result;
}

/** Read-only bootstrap: never reads the Home prompt or updates a semantic Work. */
export function projectHomeContext(options: {
  binding: HookBinding;
  cwd: string;
  maxChars?: number;
  selectedWork?: SelectedGitWorkContext;
  /** Exact source root from the source-verified registry tuple, not from Work text. */
  selectedRepositoryRoot?: string;
  /** Native adapter reobserves the admitted checkout; this callback grants no permission. */
  observeCheckout?: (observation: NativeWorkCheckoutObservation) => void;
}): HomeContextProjection {
  try {
    const deliveryPolicy = gitWorkDeliveryPolicy(options.binding, options.maxChars);
    const { maxChars } = deliveryPolicy;
    const { contract, sha256 } = readContract(options.binding, options.cwd),
      profileSource = sourceFile(contract.globalProfilePath, 128 * 1024),
      profile = safeProfile(profileSource.value);
    const { selectedCapabilities, semanticOwners, projectEntrypoints, ...preferences } = profile;
    const v2 = contract.schema === "codex.home-context.v2";
    const projectRoute =
      contract.schema === "codex.home-context.v1"
        ? (({ root: _root, effectfulWorkCwd: _effectfulCwd, ...route }) => route)(
            contract.projectRoute,
          )
        : undefined;
    const selectedWorkRoute = v2 ? selectedHomeRoute(contract, options) : undefined;
    const context: ObjectValue = {
      schema: v2 ? "codex.home-context.projection.v2" : "codex.home-context.projection.v1",
      deliveryPolicy,
      sourceOwnerProjectRoot: v2 ? contract.codeOwner.root : contract.sourceProjectRoot,
      ...(v2
        ? {
            nativeCodexHome: contract.nativeCodexHome,
            codeOwner: contract.codeOwner,
            selectedWorkRoute,
          }
        : {}),
      actualNativeCwd: options.cwd,
      northStar: contract.northStar,
      priorities: contract.priorities,
      ownership: contract.ownership,
      preferences,
      ...(options.selectedWork
        ? { selectedWork: options.selectedWork }
        : v2
          ? {
              selectedWork: {
                ...contract.selectedWork,
                status: "held",
                holdReasons: ["EXACT_SELECTED_WORK_REQUIRED"],
                authority: "none",
                nativePermissionGranted: false,
              },
            }
          : { projectRoute }),
      recordPolicy: contract.recordPolicy,
      boundaries: {
        authority: "none",
        nativePermissionGranted: false,
        executionReady: selectedWorkRoute?.executionReady ?? false,
        captureHomeRawPrompts: false,
        exactCwdOnly: true,
        excludedDomainsRemainExcluded: true,
        newModelBudgetGranted: false,
        directoryMoveAuthorized: false,
      },
      sources: [
        { path: options.binding.entrypointContextPath, sha256 },
        { path: contract.globalProfilePath, sha256: profileSource.sha256 },
      ],
    };
    const prefix = v2
      ? homeV2Prefix
      : "Source-owned Home bootstrap. Preserve actualNativeCwd; enter sourceOwnerProjectRoot for effectful Work. Configured capabilities do not prove callable/authenticated tools. This hook does not capture or accept the prompt.\n";
    const optionalContext = Object.entries({
      selectedCapabilities,
      semanticOwners,
      projectEntrypoints,
      canonicalSourcePaths: contract.canonicalSources.map(({ path }) => path),
      ...(options.selectedWork && !v2
        ? {
            defaultProjectRoute: {
              ...projectRoute,
              role: "default-bootstrap-route-not-selected-session-work",
            },
          }
        : {}),
    });
    // Reserve every omission label before fitting optional data so a later
    // omission cannot push otherwise valid essential context over the cap.
    const omitted = optionalContext.map(([key]) => key),
      render = () =>
        v2
          ? renderHomeContextProjectionV2({ ...context, omitted } as unknown as HomeContextViewV2)
          : `${prefix}${JSON.stringify({ ...context, omitted })}`;
    let held = options.selectedWork?.status === "held" || (v2 && !options.selectedWork);
    if (render().length > maxChars && options.selectedWork) {
      // Hold the complete required payload. Never drop clauses, decisions, or
      // original pointers until a weakened projection happens to fit.
      held = true;
      if (v2) {
        context.selectedWorkRoute = { executionReady: false };
        (context.boundaries as ObjectValue).executionReady = false;
      }
      context.selectedWork = {
        ...selectedGitWorkPointer(options.selectedWork),
        status: "held",
        holdReasons: ["NATIVE_CONTEXT_OUTPUT_LIMIT"],
        requiredContextChars: render().length,
        readPolicy: "Read the complete common context from this immutable Git Work source.",
        authority: "none",
        nativePermissionGranted: false,
        semanticAcceptanceEstablished: false,
      };
    }
    if (render().length > maxChars) throw new Error("Essential Home context exceeds the budget");
    for (const [key, value] of optionalContext) {
      const omittedIndex = omitted.indexOf(key);
      context[key] = value;
      omitted.splice(omittedIndex, 1);
      if (render().length > maxChars) {
        delete context[key];
        omitted.splice(omittedIndex, 0, key);
      }
    }
    const result = render();
    if (result.length > maxChars) throw new Error("Home context exceeds the bounded budget");
    if (
      v2 &&
      (sourceFile(options.binding.entrypointContextPath!, 64 * 1024).sha256 !== sha256 ||
        sourceFile(contract.globalProfilePath, 128 * 1024).sha256 !== profileSource.sha256)
    )
      throw new Error("Home v2 sources changed during projection");
    return {
      status: held ? "held" : "current",
      text: result,
      chars: result.length,
      authority: "none",
      nativePermissionGranted: false,
      omitted,
    };
  } catch {
    // No exception text, profile contents or prompt can leak through a repair notice.
    const notice =
      "Home bootstrap context requires source repair. Read the owning project's exact entrypoint binding and architecture/home-context.json, then verify the canonical owner, actual cwd and safe profile source. No context was inferred; authority:none; nativePermissionGranted:false; no Home input was captured. Continue the native user turn normally; this notice grants no effects or model budget.";
    return {
      status: "repair-required",
      text: notice,
      chars: notice.length,
      authority: "none",
      nativePermissionGranted: false,
      omitted: [],
    };
  }
}
