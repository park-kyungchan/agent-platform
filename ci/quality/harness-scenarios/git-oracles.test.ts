import { expect, test } from "bun:test";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  validateGitWorkRecords,
  validateGitWorkTransition,
  readGitWorkContext,
} from "@palantirkc/work-continuity/git-work";
import {
  fixture,
  records,
  task,
  put,
  commit,
  git,
  pathOf,
  objective,
  reference,
} from "./fixtures/work.ts";

test("PT-QC-05: clean parallel Git merge can introduce a cycle; acyclic work and historical completion remain valid", async () => {
  const f = await fixture([...records(), task("left"), task("right")]);
  try {
    const before = await f.open();
    await git(f.root, "checkout", "-qb", "left");
    const left = task("left");
    left.dependsOn = [{ taskId: "right", milestone: "execution-and-verification" }];
    await put(f.root, left);
    const leftOid = await commit(f.root);
    expect((await validateGitWorkTransition(before, await f.open(leftOid))).findings).toEqual([]);
    await git(f.root, "checkout", "-qb", "right", f.oid);
    const right = task("right");
    right.dependsOn = [{ taskId: "left", milestone: "execution-and-verification" }];
    await put(f.root, right);
    const rightOid = await commit(f.root);
    expect((await validateGitWorkRecords(await f.open(rightOid))).findings).toEqual([]);
    await git(f.root, "merge", "--no-edit", "left");
    const joined = await git(f.root, "rev-parse", "HEAD");
    expect(await git(f.root, "ls-files", "--unmerged")).toBe("");
    expect(
      (await validateGitWorkTransition(before, await f.open(joined))).findings.some((v) =>
        v.includes("CYCLE"),
      ),
    ).toBe(true);
    // Repair just the newly discovered edge; both independent Task files survive.
    await put(f.root, task("right"));
    const repaired = await commit(f.root);
    expect((await validateGitWorkTransition(before, await f.open(repaired))).findings).toEqual([]);
    const reader = await f.open(repaired);
    const completed = await reader.readRecord({ kind: "task", id: "dependency" });
    expect(completed).toMatchObject({
      logical: { specRevision: 1, planRevision: 1 },
      state: {
        execution: "complete",
        verification: "passed",
        formalCompletion: "audit-synchronization-pending",
      },
    });
    expect(reader.work.spec.revision).toBe(2);
    expect(reader.work.plan.revision).toBe(2);
  } finally {
    await f.cleanup();
  }
}, 30000);

test("PT-QC-03/05: semantic transition protects originals, consumed fences and completed contracts; raw proof needs typed custody", async () => {
  const f = await fixture();
  try {
    const before = await f.open();
    for (const [kind, id, mutate, finding] of [
      [
        "input",
        "original-0",
        (r: Record<string, unknown>) => {
          r.content = { answer: "different", question: "Exact question 0" };
        },
        "IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED",
      ],
      [
        "effect-fence",
        "history",
        (r: Record<string, unknown>) => {
          r.uncertainEffectIds = [];
        },
        "IMMUTABLE_ORIGINAL_OR_EFFECT_CHANGED",
      ],
      [
        "task",
        "dependency",
        (r: Record<string, unknown>) => {
          r.logical = {
            ...(r.logical as Record<string, unknown>),
            specRevision: 2,
            planRevision: 2,
          };
        },
        "COMPLETED_TASK_CONTRACT_HISTORY_CHANGED",
      ],
    ] as const) {
      await git(f.root, "checkout", "--detach", f.oid);
      const path = join(f.root, pathOf(kind, id));
      const value = JSON.parse(await readFile(path, "utf8"));
      mutate(value);
      await writeFile(path, JSON.stringify(value));
      const changed = await commit(f.root);
      expect(
        (await validateGitWorkTransition(before, await f.open(changed))).findings.some((v) =>
          v.startsWith(finding),
        ),
      ).toBe(true);
    }
    await git(f.root, "checkout", "--detach", f.oid);
    const proofPath = join(f.root, pathOf("input", "delegated-result"));
    await writeFile(proofPath, JSON.stringify({ rawNativeProof: "not a typed Work Input" }));
    const raw = await commit(f.root);
    expect((await validateGitWorkRecords(await f.open(raw))).valid).toBe(false);
    // The original exact committed tree remains a readable positive control after every bad candidate.
    expect((await validateGitWorkRecords(before)).valid).toBe(true);
    expect((await before.readSource(reference("original-0", "#/content/answer"))).value).toBe(
      "Exact answer 0",
    );
  } finally {
    await f.cleanup();
  }
}, 30000);

test("PT-QC-07: complete immutable-tree publication, expected-ref concurrency and historical recovery preserve third state", async () => {
  const f = await fixture();
  try {
    const reviewed = await f.open();
    const reviewedTree = await git(f.root, "rev-parse", `${f.oid}^{tree}`);
    const workPath = join(f.root, pathOf("work", "recovery"));
    const originalBytes = await readFile(workPath);
    await writeFile(workPath, '{"partial-unreviewed-candidate":');
    const context = await readGitWorkContext(reviewed, {
      taskId: "review",
      latestRelevant: "current",
    });
    expect(context.status).toBe("source-qualified");
    expect(context.records.find((r) => r.kind === "work")?.purpose).toMatchObject({ objective });
    expect(await git(f.root, "rev-parse", `${f.oid}^{tree}`)).toBe(reviewedTree);
    await writeFile(workPath, originalBytes);
    await put(f.root, task("independent-third-state"));
    const third = await commit(f.root);
    await git(f.root, "update-ref", "refs/heads/integration", f.oid);
    await git(f.root, "update-ref", "refs/heads/integration", third, f.oid);
    const staleUpdate = await git(
      f.root,
      "update-ref",
      "refs/heads/integration",
      f.oid,
      f.oid,
    ).then(
      () => null,
      (error: unknown) => error,
    );
    expect(staleUpdate).toBeInstanceOf(Error);
    expect(await git(f.root, "rev-parse", "refs/heads/integration")).toBe(third);
    expect((await validateGitWorkTransition(reviewed, await f.open(third))).valid).toBe(true);
    expect(
      (await (await f.open(third)).readRecord({ kind: "task", id: "independent-third-state" })).id,
    ).toBe("independent-third-state");
    expect((await reviewed.readSource(reference("original-4", "#/content/question"))).value).toBe(
      "Exact question 4",
    );
    expect(await git(f.root, "rev-parse", `${f.oid}^{tree}`)).toBe(reviewedTree);
    // This models only Git-owned integration. No private native binding is read or applied.
  } finally {
    await f.cleanup();
  }
}, 30000);
