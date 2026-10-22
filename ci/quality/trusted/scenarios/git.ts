import { operationalTimeouts } from "../../operational-timeouts";
import { join } from "node:path";
import { scratch, git, commit, write, bytes, hash, inputFixture, capture } from "./support";
import type {
  Graph,
  Contract,
  GitSourcePin,
  SelectedExternalArtifact,
} from "../../../../packages/work-continuity/src/git-work/generated/v3";
import type { NativeGitIdentityOwner } from "../../../../packages/work-continuity/src/git-work/allocation";
const base = "work/p/w/v3";
const ref = "refs/heads/work/fixture";
async function initialize(root: string) {
  await git(root, ["init", "--initial-branch=fixture"]);
  await git(root, ["config", "user.name", "Fictional P scenario"]);
  await git(root, ["config", "user.email", "scenario@example.invalid"]);
  await git(root, ["config", "commit.gpgsign", "false"]);
}
async function graphFiles(root: string, graph: Graph) {
  for (const w of graph.works)
    await write(
      root,
      `work/${w.projectId}/${w.workId}/v3/work.json`,
      bytes({ schema: "common.git-work-record.v3", kind: "work", value: w }),
    );
  for (const t of graph.tasks)
    await write(
      root,
      `work/${t.projectId}/${t.workId}/v3/tasks/${t.taskId}.json`,
      bytes({ schema: "common.git-work-record.v3", kind: "task", value: t }),
    );
  for (const c of graph.contracts) await write(root, contractPath(c), bytes(c));
}
function contractPath(c: Contract): string {
  return (
    `work/${c.owner.projectId}/${c.owner.workId}/v3` +
    (c.owner.kind === "task" ? `/tasks/${c.owner.taskId}` : "") +
    `/contracts/${c.kind}/${c.id}@${c.revision}.json`
  );
}
export async function gitScenario(source: string, id: string): Promise<unknown> {
  const api = (await import(
    join(source, "packages/work-continuity/src/git-work/v3.ts")
  )) as typeof import("../../../../packages/work-continuity/src/git-work/v3");
  const sourceApi = (await import(
    join(source, "packages/work-continuity/src/git-work/source.ts")
  )) as typeof import("../../../../packages/work-continuity/src/git-work/source");
  const external = (await import(
    join(source, "packages/work-continuity/src/git-work/external-artifacts.ts")
  )) as typeof import("../../../../packages/work-continuity/src/git-work/external-artifacts");
  return scratch("git-scenario", async (root) => {
    await initialize(root);
    const { graph, task } = await inputFixture();
    if (
      [
        "git-source-positive",
        "git-source-hash",
        "git-source-missing",
        "git-source-foreign",
        "git-source-currentness",
      ].includes(id)
    ) {
      await write(root, "original.json", '{"answer":42}\n');
      const initial = await commit(root);
      const snapshot = await api.openGitSourceSnapshot({
        repositoryRoot: root,
        repositoryId: "fixture",
        commit: initial,
        projectId: "p",
        workId: "w",
      });
      if (id === "git-source-currentness") {
        await write(root, "original.json", '{"answer":99}\n');
        const next = await commit(root);
        const old = await snapshot.readSource({ kind: "snapshot", path: "original.json" });
        const current = await sourceApi.resolveGitWorkSelection({
          repositoryRoot: root,
          repositoryId: "fixture",
          commit: "HEAD",
          projectId: "p",
          workId: "w",
        });
        return {
          oldValue: old.value,
          oldCommit: old.commit,
          initial,
          next,
          selected: current.commit,
          actualOldSha: hash(sourceApi.verifiedOriginalBytes(old)),
          sourceSha: old.sha256,
        };
      }
      return snapshot.readSource(
        id === "git-source-foreign"
          ? { kind: "git", repositoryId: "foreign", commit: initial, path: "original.json" }
          : {
              kind: "snapshot",
              path: id === "git-source-missing" ? "absent.json" : "original.json",
              ...(id === "git-source-hash" ? { sha256: "0".repeat(64) } : {}),
            },
      );
    }
    if (
      [
        "git-allocation-positive",
        "git-allocation-conflict",
        "git-allocation-stale",
        "git-allocation-denied",
      ].includes(id)
    ) {
      await graphFiles(root, graph);
      const fence = bytes({ completedEffectIds: ["done"], uncertainEffectIds: ["uncertain"] });
      await write(root, "work/p/w/fences/original.json", fence);
      const initial = await commit(root);
      await git(root, ["update-ref", ref, initial]);
      const request = {
        schema: "common.git-work-create.v3" as const,
        ownerId: "fixture-owner",
        createRequestId: "request",
        kind: "task" as const,
        scope: { kind: "work" as const, projectId: "p", workId: "w" },
        record: { ...graph.tasks[0]!, taskId: { $allocatedId: true } },
        contracts: [],
      };
      const target = {
        repositoryRoot: root,
        ownerRef: ref,
        expectedHead: initial,
        expectedAuthorityRevision: 1,
      };
      const owner: NativeGitIdentityOwner = {
        async withCurrentAuthority(input, use) {
          return use({
            ownerId: "fixture-owner",
            actorId: "fixture-actor",
            scope: { kind: "project", projectId: "p" },
            kinds: ["work", "task", "run"],
            revision: id === "git-allocation-denied" ? 2 : 1,
            authorityRef: "fictional-lease",
            expiresAtUtc: new Date(Date.now() + 60000).toISOString(),
            membership: true,
            gitHookPolicy: {
              mode: "native-qualified-no-git-hooks",
              repositoryRoot: root,
              ownerRef: ref,
              head: input.head,
              ownerId: "fixture-owner",
              authorityRevision: 1,
              qualificationRef: "fictional-no-hooks",
              expiresAtUtc: new Date(Date.now() + 60000).toISOString(),
              requiredNativeControlsSatisfied: true,
            },
          });
        },
        async readGraph(input) {
          const result = structuredClone(graph);
          result.works = [];
          result.tasks = [];
          result.contracts = [];
          const paths = (
            await git(root, ["ls-tree", "-r", "--name-only", input.head, "--", base])
          ).split("\n");
          for (const path of paths) {
            const data = JSON.parse(await git(root, ["show", input.head + ":" + path]));
            if (path.includes("/contracts/")) result.contracts.push(data);
            else if (data.kind === "work") result.works.push(data.value);
            else if (data.kind === "task") result.tasks.push(data.value);
          }
          return result;
        },
      };
      const first = await api.allocateGitWorkIdentity(request, target, owner);
      const second = await capture(() =>
        api.allocateGitWorkIdentity(
          id === "git-allocation-conflict"
            ? { ...request, record: { ...request.record, title: "changed meaning" } }
            : id === "git-allocation-stale"
              ? { ...request, createRequestId: "new-request" }
              : request,
          target,
          owner,
        ),
      );
      const observed = await git(root, ["rev-parse", ref]);
      const recordPath = base + "/tasks/" + first.id + ".json";
      const stored = JSON.parse(await git(root, ["show", observed + ":" + recordPath]));
      return {
        first,
        second,
        stored: stored.value,
        initial,
        observed,
        fenceBytes: await git(root, ["show", observed + ":work/p/w/fences/original.json"]),
        originalFenceBytes: fence.trim(),
      };
    }
    if (
      [
        "git-legacy-positive",
        "git-legacy-fence",
        "git-legacy-migration",
        "git-legacy-migration-conflict",
      ].includes(id)
    ) {
      const identity = {
        schema: "common.git-work.v1" as const,
        domainId: "d",
        projectId: "p",
        workId: "w",
      };
      const work = {
        ...identity,
        kind: "work" as const,
        id: "w",
        repositoryId: "fixture",
        issueId: "issue",
        purpose: {
          objective: "Preserve originals",
          successCriteria: ["safe"],
          inScope: ["fixture"],
          outOfScope: ["native"],
        },
        spec: { id: "spec", revision: 1 },
        plan: { id: "plan", revision: 1 },
        governingDecisionIds: [],
        historicalFenceIds: ["fence"],
      };
      const originals: [string, unknown][] = [
        ["work/p/w/work.json", work],
        [
          "work/p/w/specs/spec@1.json",
          {
            ...identity,
            kind: "spec",
            id: "spec",
            revision: 1,
            outcome: "Source preserved",
            criteria: ["safe"],
          },
        ],
        [
          "work/p/w/plans/plan@1.json",
          {
            ...identity,
            kind: "plan",
            id: "plan",
            revision: 1,
            specId: "spec",
            specRevision: 1,
            objective: "Preserve source",
          },
        ],
        [
          "work/p/w/fences/fence.json",
          {
            ...identity,
            kind: "effect-fence",
            id: "fence",
            completedEffectIds: ["done"],
            uncertainEffectIds: ["uncertain"],
            source: {
              kind: "snapshot",
              path: "work/p/w/inputs/original.json",
              fragment: "#/content",
            },
          },
        ],
        [
          "work/p/w/inputs/original.json",
          {
            ...identity,
            kind: "input",
            id: "original",
            inputKind: "source-owner-record",
            origin: {
              kind: "source-owner-capture",
              ownerId: "fixture",
              locator: "fictional-original",
            },
            content: { completedEffectIds: ["done"], uncertainEffectIds: ["uncertain"] },
          },
        ],
      ];
      for (const [path, value] of originals) await write(root, path, bytes(value));
      const initial = await commit(root);
      await git(root, ["update-ref", ref, initial]);
      const selection = {
        repositoryRoot: root,
        repositoryId: "fixture",
        projectId: "p",
        workId: "w",
        commit: initial,
      };
      const reader = await api.openGitWork(selection);
      const projection = await api.readLegacyGitWorkProjection(selection);
      if (id === "git-legacy-fence") {
        // Fixed legacy writer marker, not a native registration or an actual owner migration.
        await write(root, "work/p/w/v3/writer.json", "{}\n");
        const fenced = await commit(root);
        await api.assertLegacyGitWriterAllowed({ ...selection, commit: fenced });
      }
      if (id.startsWith("git-legacy-migration")) {
        const target = {
          repositoryRoot: root,
          ownerRef: ref,
          expectedHead: initial,
          expectedAuthorityRevision: 1,
        };
        const request = {
          schema: "common.git-legacy-migration-request.v3" as const,
          ownerId: "fixture-owner",
          createRequestId: "migration",
          snapshot: { repositoryId: "fixture", projectId: "p", workId: "w", commit: initial },
          expectedInventoryDigest: projection.inventoryDigest,
          bootstrapRequests: [],
          aliases: structuredClone(projection.aliases),
        };
        const owner = {
          async withCurrentMigrationAuthority<T>(
            input: typeof target & { head: string },
            use: (
              authority: import("../../../../packages/work-continuity/src/git-work/legacy-bridge").CurrentLegacyMigrationAuthority,
            ) => Promise<T>,
          ): Promise<T> {
            return use({
              ownerId: "fixture-owner",
              actorId: "fixture-actor",
              scope: { projectId: "p", workId: "w" },
              operation: "admit-existing-legacy-work",
              revision: 1,
              authorityRef: "fictional-migration",
              expiresAtUtc: new Date(Date.now() + 60000).toISOString(),
              membership: true,
              gitHookPolicy: {
                mode: "native-qualified-no-git-hooks",
                repositoryRoot: root,
                ownerRef: ref,
                head: input.head,
                ownerId: "fixture-owner",
                authorityRevision: 1,
                qualificationRef: "fictional-no-hooks",
                expiresAtUtc: new Date(Date.now() + 60000).toISOString(),
                requiredNativeControlsSatisfied: true,
              },
            });
          },
        };
        const first = await api.admitLegacyGitWork(request, target, owner);
        const second = await capture(() =>
          api.admitLegacyGitWork(
            id.endsWith("conflict")
              ? { ...request, expectedInventoryDigest: "0".repeat(64) }
              : request,
            target,
            owner,
          ),
        );
        const observed = await git(root, ["rev-parse", ref]);
        return {
          first,
          second,
          writer: await capture(() =>
            api.assertLegacyGitWriterAllowed({ ...selection, commit: observed }),
          ),
          originalFence: await readBlob(root, initial, "work/p/w/fences/fence.json"),
          preservedFence: await readBlob(root, observed, "work/p/w/fences/fence.json"),
        };
      }
      const originalFence = await reader.readRecord({ kind: "effect-fence", id: "fence" });
      return {
        work: reader.work,
        fence: originalFence,
        projectionWork: projection.graph.works[0],
        sourceHashes: projection.sources.map((s) => s.sha256),
        originalBytes: await git(root, ["show", initial + ":work/p/w/fences/fence.json"]),
      };
    }
    if (
      [
        "git-artifact-positive",
        "git-artifact-receiver-negative",
        "git-artifact-source-negative",
      ].includes(id)
    ) {
      // One canonical graph snapshot, then immutable source receipts in later commits: no self-hash cycle.
      const consumerContracts = graph.contracts.map((c) => {
        const { digest: _, ...body } = structuredClone(c);
        body.owner = { ...body.owner, workId: "w2" };
        return { ...body, digest: hash(bytes(body).trim()) } as Contract;
      });
      const consumerSpec = consumerContracts.find((c) => c.kind === "spec")!;
      const consumerPlan = consumerContracts.find((c) => c.kind === "plan")!;
      if (consumerPlan.kind !== "plan") throw Error("SCENARIO_PLAN_REQUIRED");
      consumerPlan.body.appliesToSpec = api.v3ContractRef(consumerSpec);
      const { digest: _, ...planBody } = consumerPlan;
      consumerPlan.digest = hash(bytes(planBody).trim());
      graph.contracts.push(...consumerContracts);
      graph.works.push({
        ...structuredClone(graph.works[0]!),
        workId: "w2",
        contracts: {
          issue: api.v3ContractRef(consumerContracts.find((c) => c.kind === "issue")!),
          spec: api.v3ContractRef(consumerSpec),
          plan: api.v3ContractRef(consumerPlan),
        },
      });
      graph.tasks.push({ ...structuredClone(graph.tasks[0]!), workId: "w2", taskId: "consumer" });
      const consumerTask = { projectId: "p", workId: "w2", taskId: "consumer" };
      let snapshot = "";
      // Receipts precede the graph snapshot, avoiding artifact/context self-hash cycles.
      const exactPin = async (path: string, at = snapshot): Promise<GitSourcePin> => ({
        repositoryId: "fixture",
        commit: at,
        path,
        sha256: hash(await readBlob(root, at, path)),
      });
      const persist = async (path: string, value: unknown) => {
        await write(root, path, bytes(value));
        const at = await commit(root);
        return exactPin(path, at);
      };
      const closure = async (t: typeof task) => ({
        work: await exactPin(`work/p/${t.workId}/v3/work.json`),
        task: await exactPin(`work/p/${t.workId}/v3/tasks/${t.taskId}.json`),
        graph: await exactPin(`work/p/${t.workId}/v3/graph.json`),
        contracts: await Promise.all(
          graph.contracts
            .filter((c) => c.owner.workId === t.workId)
            .map((c) => exactPin(contractPath(c))),
        ),
      });
      const packet = api.readGitWorkV3Context({ graph, task, mode: "full" });
      if (!("effective" in packet)) throw Error("SCENARIO_FULL_PACKET_REQUIRED");
      const producer = {
        task,
        workRevision: 1,
        taskRevision: 1,
        workContracts: graph.works[0]!.contracts,
        contracts: graph.works[0]!.contracts,
        effectiveDigest: packet.effective.digest,
      };
      const context = await persist("receipts/context.json", packet);
      const artifactSource = await persist("receipts/artifact.json", {
        schema: "common.git-artifact.v3",
        id: "artifact",
        revision: 1,
        producer,
        body: { answer: 42 },
      });
      const artifact = { id: "artifact", revision: 1, digest: artifactSource.sha256 };
      graph.dependencies.push({
        producer: task,
        consumer: consumerTask,
        required: "verified",
        artifact,
      });
      await graphFiles(root, graph);
      await write(root, base + "/graph.json", bytes(graph));
      await write(root, "work/p/w2/v3/graph.json", bytes(graph));
      snapshot = await commit(root);
      const consumer = api.consumerContractBinding(graph, consumerTask);
      const definition = await persist("receipts/gate.json", {
        schema: "common.git-artifact-gate.v3",
        instanceId: "gate",
        revision: 1,
        consumer,
        selectionOwnerId: "receiver",
        alternatives: [{ id: "selected", producer, artifact, criterionIds: ["safe"] }],
      });
      const selection = {
        instanceId: "gate",
        revision: 1,
        consumer,
        selectionOwnerId: "receiver",
        selectedAlternative: "selected",
        definition,
      };
      const selectionReceipt = await persist("receipts/selection.json", {
        schema: "common.git-artifact-selection.v3",
        selection,
      });
      const milestone = await persist("receipts/milestone.json", {
        schema: "common.git-artifact-producer-milestone.v3",
        producer,
        artifact,
        artifactSource,
        milestone: "verified",
      });
      const criterion = await persist("receipts/criterion.json", {
        schema: "common.git-artifact-criterion.v3",
        selection,
        criterionId: "safe",
        status: "pass",
        evidence: [artifactSource],
      });
      const receiverUse = await persist("receipts/receiver.json", {
        schema: "common.git-artifact-receiver-use.v3",
        selection,
        producer,
        artifact,
        receiverOwnerId: id === "git-artifact-receiver-negative" ? "foreign" : "receiver",
        status: "used",
        evidence: [artifactSource],
      });
      const selected: SelectedExternalArtifact = {
        schema: "common.git-selected-external-artifact.v3",
        producer,
        producerSources: await closure(task),
        consumerSources: await closure(consumerTask),
        producerContext: context,
        artifact,
        artifactSource,
        selection,
        selectionReceipt,
        producerMilestone: milestone,
        criteria: [{ criterionId: "safe", receipt: criterion }],
        receiverUse,
        receiverOwnerId: "receiver",
      };
      if (id === "git-artifact-source-negative")
        selected.artifactSource = { ...artifactSource, sha256: "0".repeat(64) };
      graph.externalArtifacts = [selected];
      const reader = await external.openExactV3SourceReader({
        repositoryRoot: root,
        repositoryId: "fixture",
        commit: snapshot,
        projectId: "p",
        workId: "w",
      });
      await api.qualifySelectedExternalArtifacts(graph, reader);
      return {
        context: await api.readSourceQualifiedGitWorkV3Context({
          graph,
          task: consumerTask,
          mode: "full",
          reader,
        }),
        selection: graph.externalArtifacts[0]!.selection,
        receiverUse: JSON.parse(await readBlob(root, receiverUse.commit, receiverUse.path)),
        effective: api.resolveEffectiveContract(graph, consumerTask),
        artifactDocument: JSON.parse(
          await readBlob(root, artifactSource.commit, artifactSource.path),
        ),
      };
    }
    throw Error("TRUSTED_CI_UNKNOWN_GIT_SCENARIO");
  });
}
async function readBlob(root: string, at: string, path: string): Promise<string> {
  // cat-file avoids trimming: immutable source hashes bind the actual original bytes.
  const oid = await git(root, ["rev-parse", at + ":" + path]);
  const result = await import("../process").then(({ fixedProcess }) =>
    fixedProcess(["/usr/bin/git", "-C", root, "cat-file", "blob", oid], {
      cwd: root,
      timeoutMs: operationalTimeouts.scenarioCommandMs,
      maxBytes: 1000000,
    }),
  );
  if (result.code || result.timedOut) throw Error("SCENARIO_BLOB_READ_FAILED");
  return result.stdout.toString();
}
