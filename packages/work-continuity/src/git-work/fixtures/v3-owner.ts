/** Private synthetic Git/native-owner fixtures; never an authority adapter for deployment. */
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type { Contract, Graph, CreateRequest, Scope } from "../generated/v3.ts";
import type { NativeGitIdentityOwner, CurrentAllocationAuthority } from "../allocation.ts";
import { canonicalV3 } from "../v3-codec.ts";
import { v3ContractDigest, v3ContractRef } from "../effective-contract.ts";

export const fixtureScope = { projectId: "fixture-project", workId: "legacy-work-slug" };
export const fixtureRef = "refs/heads/work/synthetic-v3";
export const fixtureTask = { ...fixtureScope, taskId: "legacy-task-slug" };
export const fixtureGit = (root: string, ...args: string[]): string =>
  execFileSync("git", ["-C", root, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

export function fixtureContract(
  kind: Contract["kind"],
  id: string,
  owner: Contract["owner"],
  body: unknown,
): Contract {
  const row = { id, kind, owner, revision: 1, body, sourceRefs: [] } as Omit<Contract, "digest">;
  return { ...row, digest: v3ContractDigest(row) } as Contract;
}
export function fixtureGraph(): Graph {
  const owner = { kind: "work" as const, ...fixtureScope };
  const issue = fixtureContract("issue", "issue", owner, { problem: "Synthetic consistency" });
  const spec = fixtureContract("spec", "spec", owner, {
    acceptanceCriteria: [{ id: "AC-1" }, { id: "AC-2" }],
  });
  const plan = fixtureContract("plan", "plan", owner, { appliesToSpec: v3ContractRef(spec) });
  const work = {
    ...fixtureScope,
    title: "Synthetic Work",
    leadActorId: "fixture-actor",
    revision: 1,
    contracts: {
      issue: v3ContractRef(issue),
      spec: v3ContractRef(spec),
      plan: v3ContractRef(plan),
    },
    status: "active" as const,
  };
  const task = {
    ...fixtureTask,
    title: "Synthetic Task",
    assigneeActorId: "fixture-actor",
    revision: 1,
    contractOverrides: {},
    acceptanceCriteria: ["AC-1"],
    resources: [{ resource: "repo/fixture/source", mode: "write" as const }],
    status: "planned" as const,
  };
  return {
    schema: "common.git-work.v3",
    domains: [{ id: "fixture-domain", title: "Synthetic Domain" }],
    projects: [
      { id: fixtureScope.projectId, title: "Synthetic Project", primaryDomainId: "fixture-domain" },
    ],
    works: [work],
    tasks: [task],
    contracts: [issue, spec, plan],
    dependencies: [],
    relations: [],
    delegations: [],
    provenance: [],
    nativeCorrelations: [],
  };
}
export function fixtureTaskRequest(key = "synthetic-create"): CreateRequest {
  return {
    schema: "common.git-work-create.v3",
    ownerId: "fixture-owner",
    createRequestId: key,
    kind: "task",
    scope: { kind: "work", ...fixtureScope },
    record: JSON.parse(
      canonicalV3({ ...fixtureGraph().tasks[0]!, taskId: { $allocatedId: true } }),
    ) as CreateRequest["record"],
    contracts: [],
  };
}
export function fixtureWorkRequest(key = "synthetic-work"): CreateRequest {
  const owner = { kind: "work", projectId: fixtureScope.projectId, workId: { $allocatedId: true } };
  return {
    schema: "common.git-work-create.v3",
    ownerId: "fixture-owner",
    createRequestId: key,
    kind: "work",
    scope: { kind: "project", projectId: fixtureScope.projectId },
    record: {
      projectId: fixtureScope.projectId,
      workId: { $allocatedId: true },
      title: "Synthetic new Work",
      leadActorId: "fixture-actor",
      revision: 1,
      contracts: {
        issue: { $contractRef: 0 },
        spec: { $contractRef: 1 },
        plan: { $contractRef: 2 },
      },
      status: "proposed",
    },
    contracts: [
      { id: "issue", kind: "issue", owner, revision: 1, body: {}, sourceRefs: [] },
      {
        id: "spec",
        kind: "spec",
        owner,
        revision: 1,
        body: { acceptanceCriteria: [{ id: "AC-NEW" }] },
        sourceRefs: [],
      },
      {
        id: "plan",
        kind: "plan",
        owner,
        revision: 1,
        body: { appliesToSpec: { $contractRef: 1 } },
        sourceRefs: [],
      },
    ],
  };
}

export async function seedFixture(root: string): Promise<string> {
  fixtureGit(root, "init", "--initial-branch=work/synthetic-v3");
  fixtureGit(root, "config", "user.name", "Synthetic source test");
  fixtureGit(root, "config", "user.email", "synthetic@example.invalid");
  const graph = fixtureGraph();
  const files: [string, unknown][] = [
    [
      "work/fixture-project/legacy-work-slug/v3/work.json",
      { schema: "common.git-work-record.v3", kind: "work", value: graph.works[0] },
    ],
    [
      "work/fixture-project/legacy-work-slug/v3/tasks/legacy-task-slug.json",
      { schema: "common.git-work-record.v3", kind: "task", value: graph.tasks[0] },
    ],
    // A historical v1 record and uncertain fence are third-state bytes, never migrated.
    [
      "work/fixture-project/legacy-work-slug/work.json",
      {
        schema: "common.git-work.v1",
        workId: fixtureScope.workId,
        historical: "original exact bytes",
      },
    ],
    [
      "work/fixture-project/legacy-work-slug/fences/uncertain.json",
      {
        schema: "common.git-work.v1",
        kind: "effect-fence",
        uncertainEffectIds: ["already-uncertain"],
        completedEffectIds: ["already-completed"],
      },
    ],
  ];
  for (const c of graph.contracts)
    files.push([`work/fixture-project/legacy-work-slug/v3/contracts/${c.kind}/${c.id}@1.json`, c]);
  for (const [path, value] of files) {
    await mkdir(dirname(join(root, path)), { recursive: true });
    await writeFile(join(root, path), canonicalV3(value) + "\n");
  }
  fixtureGit(root, "add", "work");
  fixtureGit(root, "commit", "-m", "Synthetic immutable fixture");
  fixtureGit(root, "checkout", "--detach");
  return fixtureGit(root, "rev-parse", "HEAD");
}

export function fixtureOwner(
  root: string,
  overrides: Partial<CurrentAllocationAuthority> = {},
): NativeGitIdentityOwner {
  return {
    async withCurrentAuthority(input, operation) {
      if (input.repositoryRoot !== root || input.ownerRef !== fixtureRef)
        throw Error("SYNTHETIC_OWNER_TARGET_MISMATCH");
      return operation({
        ownerId: "fixture-owner",
        actorId: "fixture-actor",
        scope: { kind: "project", projectId: fixtureScope.projectId } as Scope,
        kinds: ["work", "task", "run"],
        revision: 1,
        authorityRef: "fixture-current-authority",
        expiresAtUtc: new Date(Date.now() + 60_000).toISOString(),
        membership: true,
        gitHookPolicy: { mode: "native-qualified-no-git-hooks", repositoryRoot: root, ownerRef: input.ownerRef, head: input.head, ownerId: "fixture-owner", authorityRevision: 1, qualificationRef: "synthetic-current-source-hook-policy", expiresAtUtc: new Date(Date.now() + 60_000).toISOString(), requiredNativeControlsSatisfied: true },
        ...overrides,
      });
    },
    async readGraph(input) {
      const graph = fixtureGraph();
      graph.works = [];
      graph.tasks = [];
      graph.contracts = [];
      const paths = fixtureGit(root, "ls-tree", "-r", "--name-only", input.head, "work").split(
        "\n",
      );
      for (const path of paths.filter((p) => p.includes("/v3/"))) {
        const value = JSON.parse(fixtureGit(root, "show", input.head + ":" + path));
        if (path.includes("/contracts/")) graph.contracts.push(value);
        else if (value.kind === "work") graph.works.push(value.value);
        else if (value.kind === "task") graph.tasks.push(value.value);
      }
      return graph;
    },
  };
}
