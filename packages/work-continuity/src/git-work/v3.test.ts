import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, rm, writeFile, chmod } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Graph, Contract, Task, Run } from "./generated/v3.ts";
import { assertV3, canonicalV3, digestV3, encodeV3, decodeV3 } from "./v3.ts";
import {
  resolveEffectiveContract,
  readGitWorkV3Context,
  expandGitWorkV3Context,
  validateGitWorkV3Graph,
  v3ContractRef,
  v3ContractDigest,
} from "./v3.ts";
import { allocateGitWorkIdentity, previewGitWorkV3Allocation } from "./v3.ts";
import {
  fixtureGraph,
  fixtureTask,
  fixtureScope,
  fixtureContract,
  fixtureTaskRequest,
  fixtureWorkRequest,
  fixtureRef,
  fixtureGit,
  fixtureOwner,
  seedFixture,
} from "./fixtures/v3-owner.ts";
import { runHarness } from "../../../../scripts/harness.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((p) => rm(p, { recursive: true, force: true })));
});
async function repository() {
  const root = await mkdtemp(join(tmpdir(), "v3-source-owner-test-"));
  roots.push(root);
  const head = await seedFixture(root);
  return {
    root,
    head,
    target: {
      repositoryRoot: root,
      ownerRef: fixtureRef,
      expectedHead: head,
      expectedAuthorityRevision: 1,
    },
  };
}
function override(g: Graph, edit: (body: Record<string, unknown>) => void = () => {}) {
  const task = g.tasks[0]!;
  const owner = { kind: "task" as const, ...fixtureTask };
  const spec = fixtureContract("spec", "task-spec", owner, {
    refinesSpec: g.works[0]!.contracts.spec,
    acceptanceCriteria: [{ id: "TASK-1", covers: ["AC-1"] }],
  });
  edit(spec.body);
  const { digest: _, ...raw } = spec;
  spec.digest = v3ContractDigest(raw);
  const plan = fixtureContract("plan", "task-plan", owner, {
    methodRef: g.works[0]!.contracts.plan,
    appliesToSpec: v3ContractRef(spec),
  });
  g.contracts.push(spec, plan);
  task.contractOverrides = { spec: v3ContractRef(spec), plan: v3ContractRef(plan) };
  task.acceptanceCriteria = ["TASK-1"];
  return task;
}

describe("v3 generated schema and immutable contract semantics", () => {
  test("schema codecs retain scoped legacy slugs and reject ambiguous JSON", () => {
    class SyntheticArraySubclass extends Array<number> {}
    const graph = fixtureGraph();
    expect(decodeV3("Graph", encodeV3("Graph", graph))).toEqual(graph);
    expect(() => decodeV3("Graph", JSON.stringify(graph))).toThrow("CANONICAL_TRANSPORT");
    for (const value of [
      NaN,
      Infinity,
      -0,
      1.5,
      undefined,
      { x: undefined },
      new Date(),
      SyntheticArraySubclass.of(1),
      [, 1],
      Object.defineProperty({}, "x", {
        get() {
          throw Error("accessor executed");
        },
        enumerable: true,
      }),
    ])
      expect(() => canonicalV3(value)).toThrow();
    expect(() => canonicalV3({ x: "x".repeat(4_194_305) })).toThrow("BOUND");
    const cycle: { self?: unknown } = {};
    cycle.self = cycle;
    expect(() => canonicalV3(cycle)).toThrow("CYCLE");
  });
  test("unknown graph families and malformed native bindings reject in generated validation", () => {
    expect(() => assertV3("Graph", { ...fixtureGraph(), secondScheduler: [] })).toThrow(
      "INVALID_V3",
    );
    const graph = fixtureGraph();
    graph.nativeCorrelations.push({
      task: fixtureTask,
      binding: {
        runtimeId: "fixture",
        nativeSessionId: null,
        nativeTaskId: null,
        nativeRunId: null,
        paneId: null,
      },
    });
    expect(() => validateGitWorkV3Graph(graph)).not.toThrow();
    (graph.nativeCorrelations[0]!.binding as unknown as Record<string, unknown>).nativeRunId = 3;
    expect(() => assertV3("Graph", graph)).toThrow("INVALID_V3");
  });
  test("all context paths share exactly the same resolver and require pinned recovery", () => {
    const graph = fixtureGraph();
    override(graph);
    const effective = resolveEffectiveContract(graph, fixtureTask);
    for (const mode of ["full", "compact", "dispatch", "resume"] as const) {
      const context = readGitWorkV3Context({
        graph,
        task: fixtureTask,
        mode,
        expectedDigest: effective.digest,
      });
      expect(expandGitWorkV3Context(context).effective).toEqual(effective);
      expect(context.nativePermissionGranted).toBe(false);
      expect(context.authority).toBe("none");
    }
    expect(() => readGitWorkV3Context({ graph, task: fixtureTask, mode: "resume" })).toThrow(
      "PINNED",
    );
    graph.tasks[0]!.resources.push({ resource: "repo/new-claim", mode: "read" });
    expect(() =>
      readGitWorkV3Context({
        graph,
        task: fixtureTask,
        mode: "resume",
        expectedDigest: effective.digest,
      }),
    ).toThrow("STALE");
  });
  test("compact transport detects digest/body tampering and source omission", () => {
    const graph = fixtureGraph();
    const compact = readGitWorkV3Context({ graph, task: fixtureTask, mode: "compact" });
    expect(Buffer.byteLength(canonicalV3(compact))).toBeLessThan(
      Buffer.byteLength(canonicalV3(expandGitWorkV3Context(compact))),
    );
    if (compact.schema !== "common.git-work-context.compact.v3")
      throw Error("fixture compact schema");
    expect(() => expandGitWorkV3Context({ ...compact, effectiveDigest: "0".repeat(64) })).toThrow(
      "DIGEST",
    );
    const packet = expandGitWorkV3Context(compact);
    packet.contractSources.pop();
    expect(() => expandGitWorkV3Context(packet)).toThrow("SOURCE_CLOSURE");
  });
  test.each(["kind", "owner", "revision", "digest"])(
    "exact contract reference %s is enforced",
    (field) => {
      const graph = fixtureGraph();
      const ref = graph.works[0]!.contracts.issue;
      if (field === "kind") ref.kind = "spec";
      else if (field === "owner") ref.owner = { kind: "task", ...fixtureTask };
      else if (field === "revision") ref.revision++;
      else ref.digest = "0".repeat(64);
      expect(() => validateGitWorkV3Graph(graph)).toThrow();
    },
  );
  test("content, contract identity and revision are bound by the same digest", () => {
    const c = fixtureGraph().contracts[0]!;
    const { digest, ...raw } = c;
    expect(v3ContractDigest(raw)).toBe(digest);
    expect(v3ContractDigest({ ...raw, revision: 2 })).not.toBe(digest);
    expect(v3ContractDigest({ ...raw, id: "other" })).not.toBe(digest);
    const graph = fixtureGraph();
    (graph.contracts[0]!.body as Record<string, unknown>).problem = "changed";
    expect(() => validateGitWorkV3Graph(graph)).toThrow("DIGEST");
  });
  test("effective Plan cannot silently follow a Task Spec override", () => {
    const graph = fixtureGraph();
    const task = override(graph);
    delete task.contractOverrides.plan;
    expect(() => validateGitWorkV3Graph(graph)).toThrow("PLAN_SPEC");
  });
  test.each([undefined, [], ["missing"], ["AC-1", "AC-1"]].map((covers) => ({ covers })))(
    "Task-local criteria require exact nonduplicate Work coverage: %j",
    ({ covers }) => {
      const graph = fixtureGraph();
      override(graph, (body) => {
        body.acceptanceCriteria = [{ id: "TASK-1", ...(covers === undefined ? {} : { covers }) }];
      });
      expect(() => validateGitWorkV3Graph(graph)).toThrow("COVERAGE");
    },
  );
  test("typed refinement, duplicate criteria and selections reject", () => {
    const graph = fixtureGraph();
    override(graph, (body) => {
      body.refinesSpec = graph.works[0]!.contracts.issue;
    });
    expect(() => validateGitWorkV3Graph(graph)).toThrow("REFINEMENT");
    const duplicate = fixtureGraph();
    override(duplicate, (body) => {
      body.acceptanceCriteria = [
        { id: "TASK-1", covers: ["AC-1"] },
        { id: "TASK-1", covers: ["AC-2"] },
      ];
    });
    expect(() => validateGitWorkV3Graph(duplicate)).toThrow("DUPLICATE_CRITERION");
    const selection = fixtureGraph();
    selection.tasks[0]!.acceptanceCriteria.push("AC-1");
    expect(() => validateGitWorkV3Graph(selection)).toThrow("DUPLICATE_CRITERION_SELECTION");
  });
  test("Work Plan/criterion validation runs even without Tasks", () => {
    const graph = fixtureGraph();
    graph.tasks = [];
    const plan = graph.contracts.find((c) => c.kind === "plan")!;
    plan.body.appliesToSpec = graph.works[0]!.contracts.issue;
    const { digest: _, ...raw } = plan;
    plan.digest = v3ContractDigest(raw);
    graph.works[0]!.contracts.plan = v3ContractRef(plan);
    expect(() => validateGitWorkV3Graph(graph)).toThrow("WORK_PLAN_SPEC");
  });
  test("hierarchy, refinement, dependency and delegation cycles reject; relatedTo cycles remain legal", () => {
    const domain = fixtureGraph();
    domain.domains[0]!.parentDomainId = domain.domains[0]!.id;
    expect(() => validateGitWorkV3Graph(domain)).toThrow("DOMAIN_PARENT_CYCLE");
    const graph = fixtureGraph();
    graph.tasks.push({ ...structuredClone(graph.tasks[0]!), taskId: "sibling" });
    graph.tasks[0]!.parentTaskId = "sibling";
    graph.tasks[1]!.parentTaskId = fixtureTask.taskId;
    expect(() => validateGitWorkV3Graph(graph)).toThrow("TASK_PARENT_CYCLE");
    delete graph.tasks[0]!.parentTaskId;
    delete graph.tasks[1]!.parentTaskId;
    const sibling = { ...fixtureTask, taskId: "sibling" };
    graph.dependencies = [
      { producer: fixtureTask, consumer: sibling, required: "verified" },
      { producer: sibling, consumer: fixtureTask, required: "accepted" },
    ];
    expect(() => validateGitWorkV3Graph(graph)).toThrow("DEPENDENCY_CYCLE");
    graph.dependencies = [];
    const a = { kind: "task" as const, ...fixtureTask },
      b = { kind: "task" as const, ...sibling };
    graph.relations = [
      { kind: "relatedTo", from: a, to: b },
      { kind: "relatedTo", from: b, to: a },
    ];
    expect(() => validateGitWorkV3Graph(graph)).not.toThrow();
    const authorityRef = {
      owner: "fixture",
      uri: "fixture:authority",
      revision: "1",
      digest: "1".repeat(64),
    };
    graph.delegations = [
      { from: a, to: b, authorityRef },
      { from: b, to: a, authorityRef },
    ];
    expect(() => validateGitWorkV3Graph(graph)).toThrow("DELEGATION_CYCLE");
  });
  test("ancestry and transitive dependency resources participate; unqualified artifacts reject", () => {
    const graph = fixtureGraph();
    const base = graph.tasks[0]!;
    graph.tasks.push(
      { ...structuredClone(base), taskId: "parent", revision: 2 },
      { ...structuredClone(base), taskId: "producer" },
      { ...structuredClone(base), taskId: "upstream" },
      { ...structuredClone(base), taskId: "unrelated", status: "blocked" },
    );
    base.parentTaskId = "parent";
    graph.dependencies = [
      {
        producer: { ...fixtureTask, taskId: "producer" },
        consumer: fixtureTask,
        required: "verified",
      },
      {
        producer: { ...fixtureTask, taskId: "upstream" },
        consumer: { ...fixtureTask, taskId: "producer" },
        required: "accepted",
      },
    ];
    graph.dependencies[1]!.artifact = {
      id: "immutable-artifact",
      revision: 1,
      digest: "1".repeat(64),
    };
    expect(() => resolveEffectiveContract(graph, fixtureTask)).toThrow("EXACT_SELECTED_ARTIFACT");
    delete graph.dependencies[1]!.artifact;
    const initial = resolveEffectiveContract(graph, fixtureTask);
    expect(initial.dependencies.length).toBe(2);
    expect(initial.parentTaskChain[0]!.revision).toBe(2);
    graph.dependencies.reverse();
    graph.tasks[4]!.status = "running";
    expect(resolveEffectiveContract(graph, fixtureTask).digest).toBe(initial.digest);
    graph.tasks[1]!.revision++;
    expect(resolveEffectiveContract(graph, fixtureTask).digest).not.toBe(initial.digest);
    graph.tasks[1]!.revision--;
    graph.tasks[3]!.resources.push({ resource: "artifact/resource", mode: "read" });
    expect(resolveEffectiveContract(graph, fixtureTask).digest).not.toBe(initial.digest);
  });
  test("cross-Work artifact and missing graph endpoints reject", () => {
    const graph = fixtureGraph();
    graph.dependencies.push({
      producer: { ...fixtureTask, workId: "other" },
      consumer: fixtureTask,
      required: "verified",
    });
    expect(() => validateGitWorkV3Graph(graph)).toThrow("CROSS_WORK");
    graph.dependencies[0]!.artifact = { id: "artifact", revision: 1, digest: "1".repeat(64) };
    expect(() => validateGitWorkV3Graph(graph)).toThrow("ENDPOINT");
  });
});

describe("durable authenticated Git owner allocation", () => {
  test("restart/idempotency preserves exact v1/fence bytes, request mapping and publication", async () => {
    const { root, target } = await repository();
    const historical = fixtureGit(
      root,
      "show",
      target.expectedHead + ":work/fixture-project/legacy-work-slug/work.json",
    );
    const request = fixtureTaskRequest();
    const first = await allocateGitWorkIdentity(request, target, fixtureOwner(root));
    expect(first.id).toMatch(/^[a-f0-9-]{36}$/);
    expect(first.nativePermissionGranted).toBe(false);
    const restarted = await allocateGitWorkIdentity(
      request,
      { ...target, expectedHead: first.head },
      fixtureOwner(root),
    );
    expect(restarted.id).toBe(first.id);
    expect(restarted.status).toBe("existing");
    expect(restarted.head).toBe(first.head);
    expect(
      fixtureGit(root, "show", first.head + ":work/fixture-project/legacy-work-slug/work.json"),
    ).toBe(historical);
    const changed = fixtureGit(
      root,
      "diff-tree",
      "--no-commit-id",
      "--name-only",
      "-r",
      first.head,
    ).split("\n");
    expect(changed.length).toBe(3);
    expect(
      changed.every((p) => p.startsWith("work/.identity-") || p.endsWith(first.id + ".json")),
    ).toBe(true);
    expect(fixtureGit(root, "status", "--porcelain")).toBe("");
  });
  test("new Work and its three typed contracts publish together", async () => {
    const { root, target } = await repository();
    const first = await allocateGitWorkIdentity(fixtureWorkRequest(), target, fixtureOwner(root));
    const graph = await fixtureOwner(root).readGraph({
      repositoryRoot: root,
      ownerRef: fixtureRef,
      head: first.head,
    });
    expect(() => validateGitWorkV3Graph(graph)).not.toThrow();
    expect(graph.works.some((w) => w.workId === first.id)).toBe(true);
    expect(
      fixtureGit(root, "diff-tree", "--no-commit-id", "--name-only", "-r", first.head).split("\n")
        .length,
    ).toBe(6);
  });
  test.each(["meaning", "kind", "scope"])(
    "same-owner request %s conflict never writes",
    async (field) => {
      const { root, target } = await repository();
      const request = fixtureTaskRequest();
      const first = await allocateGitWorkIdentity(request, target, fixtureOwner(root));
      let conflict = structuredClone(request);
      if (field === "meaning")
        (conflict.record as Record<string, unknown>).title = "conflicting meaning";
      else if (field === "kind")
        conflict = { ...fixtureWorkRequest(), createRequestId: request.createRequestId };
      else conflict.scope = { kind: "work", ...fixtureScope, workId: "other" };
      await expect(
        allocateGitWorkIdentity(
          conflict,
          { ...target, expectedHead: first.head },
          fixtureOwner(root),
        ),
      ).rejects.toThrow("CREATE_REQUEST_CONFLICT");
      expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(first.head);
    },
  );
  test.each(["owner", "membership", "revision", "expiry", "scope", "kind"])(
    "wrong current %s authority rejects before publication",
    async (field) => {
      const { root, target } = await repository();
      const overrides =
        field === "owner"
          ? { ownerId: "other" }
          : field === "membership"
            ? { membership: false as unknown as true }
            : field === "revision"
              ? { revision: 2 }
              : field === "expiry"
                ? { expiresAtUtc: "2020-01-01T00:00:00Z" }
                : field === "scope"
                  ? { scope: { kind: "project" as const, projectId: "other" } }
                  : { kinds: [] };
      await expect(
        allocateGitWorkIdentity(fixtureTaskRequest(), target, fixtureOwner(root, overrides)),
      ).rejects.toThrow("AUTHORITY");
      expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(target.expectedHead);
    },
  );
  test("owner graph cannot fabricate missing record membership", async () => {
    const { root, target } = await repository();
    const owner = fixtureOwner(root);
    const readGraph = owner.readGraph;
    owner.readGraph = async (input) => {
      const g = await readGraph(input);
      g.works[0]!.title = "fabricated";
      return g;
    };
    await expect(allocateGitWorkIdentity(fixtureTaskRequest(), target, owner)).rejects.toThrow(
      "GRAPH_WORK_MISMATCH",
    );
    expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(target.expectedHead);
  });
  test("six independently restarted processes allocate exactly one ID/owner transaction", async () => {
    const { root, target } = await repository();
    const path = join(root, "worker-input.json");
    await writeFile(path, JSON.stringify({ request: fixtureTaskRequest(), target }));
    const worker = new URL("./fixtures/v3-worker.ts", import.meta.url).pathname;
    const children = Array.from({ length: 6 }, () =>
      Bun.spawn([process.execPath, "--no-env-file", "--no-install", worker, path], {
        stdout: "pipe",
        stderr: "pipe",
      }),
    );
    const results = await Promise.all(
      children.map(async (p) => {
        const result = JSON.parse(await new Response(p.stdout).text());
        const errors = await new Response(p.stderr).text();
        expect(await p.exited, errors).toBe(0);
        return result;
      }),
    );
    expect(new Set(results.map((r) => r.id)).size).toBe(1);
    expect(fixtureGit(root, "rev-list", "--count", target.expectedHead + ".." + fixtureRef)).toBe(
      "1",
    );
  }, 30_000);
  test("opaque UUID collision with a historical record rejects without overwriting", async () => {
    const { root, target } = await repository();
    const forcedSyntheticId = "11111111-1111-4111-8111-111111111111";
    await writeFile(
      join(root, "work/fixture-project/legacy-work-slug/tasks-collision-" + forcedSyntheticId),
      "third-state",
    );
    await mkdir(join(root, "work/.identity-ids"), { recursive: true });
    await writeFile(
      join(root, "work/.identity-ids/" + forcedSyntheticId + ".json"),
      "historical collision bytes",
    );
    fixtureGit(root, "add", "work");
    fixtureGit(root, "commit", "-m", "Synthetic collision");
    target.expectedHead = fixtureGit(root, "rev-parse", "HEAD");
    fixtureGit(root, "update-ref", fixtureRef, target.expectedHead);
    const path = join(root, "worker-input.json");
    await writeFile(
      path,
      JSON.stringify({ request: fixtureTaskRequest(), target, forcedSyntheticId }),
    );
    const p = Bun.spawn(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        new URL("./fixtures/v3-worker.ts", import.meta.url).pathname,
        path,
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    expect(JSON.parse(await new Response(p.stdout).text()).error).toContain("ID_COLLISION");
    expect(await p.exited).toBe(1);
    expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(target.expectedHead);
  });
  test("uncertain-success CAS is observed once and never replayed", async () => {
    const { root, target } = await repository();
    const path = join(root, "worker-input.json");
    await writeFile(
      path,
      JSON.stringify({
        request: fixtureTaskRequest(),
        target,
        syntheticLostCasAcknowledgement: true,
      }),
    );
    const p = Bun.spawn(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        new URL("./fixtures/v3-worker.ts", import.meta.url).pathname,
        path,
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    const result = JSON.parse(await new Response(p.stdout).text());
    if (result.status !== "reconciled")
      console.error(
        "SYNTHETIC_CAS_STATUS_" +
          String(result.status ?? result.error)
            .toUpperCase()
            .replace(/[^A-Z_]/g, "_"),
      );
    expect(await p.exited).toBe(0);
    expect(result.status).toBe("reconciled");
    expect(fixtureGit(root, "rev-list", "--count", target.expectedHead + ".." + fixtureRef)).toBe(
      "1",
    );
  });
  test("Run binds the pinned effective packet and independent attempt; reserve invents no native IDs", async () => {
    const { root, target } = await repository();
    const row: Run = {
      runId: "placeholder",
      task: fixtureTask,
      attempt: 1,
      status: "reserved",
      effectiveContractDigest: resolveEffectiveContract(fixtureGraph(), fixtureTask).digest,
      sourceSnapshotId: "fixture-exact-source",
      parentAuthorityRef: "fixture-current-authority",
      nativeBindings: [],
      effectRefs: [],
    };
    const request = {
      ...fixtureTaskRequest(),
      kind: "run" as const,
      scope: { kind: "task" as const, ...fixtureTask },
      record: { ...row, runId: { $allocatedId: true } },
    };
    const result = await allocateGitWorkIdentity(request, target, fixtureOwner(root));
    expect(result.nativePermissionGranted).toBe(false);
    await expect(
      allocateGitWorkIdentity(
        { ...request, createRequestId: "new-run" },
        { ...target, expectedHead: result.head },
        fixtureOwner(root),
      ),
    ).rejects.toThrow("NEXT_RUN_ATTEMPT");
    await expect(
      allocateGitWorkIdentity(
        {
          ...request,
          createRequestId: "bad-run",
          record: { ...request.record, attempt: 2, effectiveContractDigest: "0".repeat(64) },
        },
        { ...target, expectedHead: result.head },
        fixtureOwner(root),
      ),
    ).rejects.toThrow("EFFECTIVE_CONTRACT");
  });
  test("bounded CLI dry-run cannot claim native authority or accept an authority JSON substitute", async () => {
    const { root, target } = await repository();
    const request = fixtureTaskRequest();
    const preview = await previewGitWorkV3Allocation(request, target);
    expect(preview.id).toBeNull();
    expect(preview.publication).toBe("none");
    const path = join(root, "cli.json");
    await writeFile(path, JSON.stringify({ mode: "dry-run", request, target }));
    const result = (await runHarness(["allocate-v3", path])) as { id: unknown };
    expect(result.id).toBeNull();
    await writeFile(path, JSON.stringify({ mode: "apply", request, target }));
    await expect(runHarness(["allocate-v3", path])).rejects.toThrow(
      "NATIVE_OWNER_ADAPTER_REQUIRED",
    );
    expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(target.expectedHead);
  });
  test("checked-out/dirty owner branches require native-owner reconciliation instead of mutation", async () => {
    const { root, target } = await repository();
    fixtureGit(root, "checkout", "work/synthetic-v3");
    await writeFile(join(root, "third-state.txt"), "unrelated uncommitted state");
    await expect(
      allocateGitWorkIdentity(fixtureTaskRequest(), target, fixtureOwner(root)),
    ).rejects.toThrow("OWNER_REF_CHECKED_OUT");
    expect(await readFile(join(root, "third-state.txt"), "utf8")).toBe(
      "unrelated uncommitted state",
    );
    expect(fixtureGit(root, "rev-parse", fixtureRef)).toBe(target.expectedHead);
  });
});
