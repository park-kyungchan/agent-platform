import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
import type { HookBinding, NativeContextLoader } from "./hooks.ts";
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
function sourceFile(path: string, maxBytes: number): { value: unknown; sha256: string } {
  if (
    resolve(path) !== path ||
    realpathSync(path) !== path ||
    !lstatSync(path).isFile() ||
    lstatSync(path).size > maxBytes
  )
    throw new Error("Unsafe or oversized context source");
  const bytes = readFileSync(path);
  if (bytes.length > maxBytes) throw new Error("Context source changed size");
  return { value: JSON.parse(bytes.toString("utf8")), sha256: hash(bytes) };
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
/** Validate a prospective Home value without reading or publishing global/native files. */
export function readHomeContextContractValue(
  value: unknown,
  binding: HookBinding,
  cwd: string,
): HomeContextContract {
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
    route = row(data.projectRoute),
    boundaries = row(data.boundaries),
    policy = row(data.recordPolicy);
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

function readContract(
  binding: HookBinding,
  cwd: string,
): { contract: HomeContextContract; sha256: string } {
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

/** Read-only bootstrap: never reads the Home prompt or updates a semantic Work. */
export function projectHomeContext(options: {
  binding: HookBinding;
  cwd: string;
  maxChars?: number;
  selectedWork?: SelectedGitWorkContext;
}): HomeContextProjection {
  try {
    const deliveryPolicy = gitWorkDeliveryPolicy(options.binding, options.maxChars);
    const { maxChars } = deliveryPolicy;
    const { contract, sha256 } = readContract(options.binding, options.cwd),
      profileSource = sourceFile(contract.globalProfilePath, 128 * 1024),
      profile = safeProfile(profileSource.value);
    const { selectedCapabilities, semanticOwners, projectEntrypoints, ...preferences } = profile;
    const { root: _root, effectfulWorkCwd: _effectfulCwd, ...projectRoute } = contract.projectRoute;
    const context: ObjectValue = {
      schema: "codex.home-context.projection.v1",
      deliveryPolicy,
      sourceOwnerProjectRoot: contract.sourceProjectRoot,
      actualNativeCwd: options.cwd,
      northStar: contract.northStar,
      priorities: contract.priorities,
      ownership: contract.ownership,
      preferences,
      ...(options.selectedWork ? { selectedWork: options.selectedWork } : { projectRoute }),
      recordPolicy: contract.recordPolicy,
      boundaries: {
        authority: "none",
        nativePermissionGranted: false,
        executionReady: false,
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
    const prefix =
      "Source-owned Home bootstrap. Preserve actualNativeCwd; enter sourceOwnerProjectRoot for effectful Work. Configured capabilities do not prove callable/authenticated tools. This hook does not capture or accept the prompt.\n";
    const optionalContext = Object.entries({
      selectedCapabilities,
      semanticOwners,
      projectEntrypoints,
      canonicalSourcePaths: contract.canonicalSources.map(({ path }) => path),
      ...(options.selectedWork
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
      render = () => `${prefix}${JSON.stringify({ ...context, omitted })}`;
    let held = options.selectedWork?.status === "held";
    if (render().length > maxChars && options.selectedWork) {
      // Hold the complete required payload. Never drop clauses, decisions, or
      // original pointers until a weakened projection happens to fit.
      held = true;
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
