import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import * as work from "@palantirkc/work-continuity/git-work";
import {
  handleHook,
  readHookBindings,
  type NativeContextLoader,
  type SessionGitWorkSelection,
} from "@palantirkc/native-codex-adapter/hooks";
import type { HomeContextContract } from "@palantirkc/native-codex-adapter/home-context";
import {
  readHermesCurrentWorkContextV2,
  type HermesCurrentWorkEvent,
  type HermesCurrentWorkSelection,
} from "@palantirkc/native-hermes-adapter/platform-current-work/context";
import { logical, scope, selection } from "./work.ts";

export async function codex(
  root: string,
  oid: string,
  taskId = "review",
  decisionId = "decision-0",
) {
  const home = join(root, "fictional-home"),
    owner = join(root, "fictional-owner");
  await mkdir(join(home, ".codex/harness"), { recursive: true });
  await mkdir(owner, { recursive: true });
  const profile = join(home, ".codex/harness/profile.json"),
    context = join(owner, "home-context.json");
  await writeFile(
    profile,
    JSON.stringify({
      implementation: { language: "TypeScript", runtime: "Bun" },
      canonicalArtifacts: {
        extensions: [".ts", ".json"],
        language: "English",
        nativeRequiredFormatsAllowed: true,
      },
      conversation: { language: "English" },
      ontology: { ownership: "shared", organization: "domain-modules" },
      excludedDomains: [],
      selectedCapabilities: {},
      nativeBinding: { semanticOwners: [], projectEntrypoints: [] },
    }),
  );
  const contract: HomeContextContract = {
    schema: "codex.home-context.v1",
    projectId: scope.projectId,
    entrypointCwd: home,
    sourceProjectRoot: owner,
    globalProfilePath: profile,
    northStar: "Preserve originals and useful source progress",
    priorities: ["Keep effect fences"],
    ownership: [],
    canonicalSources: [],
    projectRoute: {
      projectId: scope.projectId,
      root: owner,
      workId: "legacy",
      effectfulWorkCwd: owner,
      contextCommand: "context project oracle",
      checkpointPath: join(owner, "absent-checkpoint.json"),
      purposeRef: "purpose.json",
      nextSessionRef: "next.json",
      boundaryRef: "boundary.json",
      kickoffPath: join(owner, "kickoff.json"),
    },
    recordPolicy: {
      distinctions: ["Decision", "Verification", "AcceptanceDecision"],
      materialBoundary: "Retain originals",
      tinyWork: "Proportionate",
      sources: "No authority",
      ontologyPromotion: "Owner only",
    },
    boundaries: {
      authority: "none",
      nativePermissionGranted: false,
      captureHomeRawPrompts: false,
      qualificationLedgerBoundToHome: false,
      newModelBudgetGranted: false,
      directoryMoveAuthorized: false,
      hermesRuntimeEffectsAuthorized: false,
      configuredInjectionIsFreshSessionProof: false,
      maxContextChars: 6000,
      nativeContextTokenLimit: 1500,
    },
  };
  await writeFile(context, JSON.stringify(contract));
  const selected: SessionGitWorkSelection = {
    schema: "codex.session-git-work-selection.v1",
    nativeSessionId: "fictional-session",
    cwd: home,
    ownerRoot: owner,
    projectId: scope.projectId,
    workId: scope.workId,
    snapshot: selection(root, oid),
    taskId,
    decisionId,
    latestRelevant: "current",
  };
  const registryPath = join(owner, "bindings.json");
  await writeFile(
    registryPath,
    JSON.stringify({
      schemaVersion: "1.0.0",
      bindings: [
        {
          projectId: scope.projectId,
          root: owner,
          workId: "legacy",
          checkpointPath: contract.projectRoute.checkpointPath,
          entrypointCwds: [home],
          entrypointContextPath: context,
          gitWorkContextVersion: "v2",
          gitWorkMetadataEncoding: "metadata.v1",
        },
      ],
      contextSelections: [selected],
    }),
  );
  const unavailable = () => {
    throw Error("Absent old checkpoint/pane must not be used");
  };
  let checks = 0;
  const provider: Awaited<ReturnType<NativeContextLoader>> = {
    index: {
      boundedProjection: unavailable,
      readCheckpoint: unavailable,
      recordBoundary: unavailable,
    },
    sections: { readWorkContextSection: unavailable },
    gitWork: work,
    verifyCurrentSourceClosure: async () => {
      checks++;
    },
  };
  return {
    provider,
    checks: () => checks,
    read: (hook_event_name: "SessionStart" | "UserPromptSubmit") =>
      handleHook(
        {
          hook_event_name,
          session_id: "fictional-session",
          cwd: home,
          ...(hook_event_name === "SessionStart"
            ? { source: "resume" }
            : { prompt: "PRIVATE-UNRELATED-PROMPT" }),
        },
        { bindings: readHookBindings(registryPath) },
        async () => provider,
      ),
  };
}
export function hermes(root: string, oid: string, taskId = "review", verify?: () => Promise<void>) {
  const event: HermesCurrentWorkEvent = {
    runtime: "native-hermes",
    profileId: "fictional-profile",
    sessionId: "fictional-session",
    agentId: null,
    nativeProjectId: "fictional-project",
    nativeTaskId: "fictional-task",
    nativeRunId: null,
  };
  const selected: HermesCurrentWorkSelection = {
    schema: "hermes.current-git-work-selection.v1",
    native: event,
    logical: logical(taskId),
    snapshot: selection(root, oid),
    latestRelevant: "current",
  };
  const index = {
    schema: "native-harness.runtime-index.v1",
    entries: [
      {
        ...event,
        logical: logical(taskId),
        observation: {
          class: "declared",
          observedAtUtc: null,
          evidenceRefs: [],
          currentness: "current",
        },
        sourceOnly: true,
      },
    ],
  };
  let checks = 0;
  return {
    checks: () => checks,
    read: (maxChars?: number) =>
      readHermesCurrentWorkContextV2(event, selected, index, {
        gitWork: work,
        verifySourceClosure: async () => {
          checks++;
          await verify?.();
        },
        metadataEncoding: "metadata.v1",
        ...(maxChars ? { maxChars } : {}),
      }),
  };
}
