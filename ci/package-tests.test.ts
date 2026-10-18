import { expect, test } from "bun:test";
import { runIndependentPackageTests } from "./package-tests";

test("package checks run once within the fixed concurrency bound", async () => {
  for (const concurrency of [1, 2] as const) {
    let active = 0;
    let peak = 0;
    const completed: number[] = [];
    await runIndependentPackageTests(
      Array.from({ length: 7 }, (_, index) => async () => {
        active++;
        peak = Math.max(peak, active);
        await Promise.resolve();
        completed.push(index);
        active--;
      }),
      concurrency,
    );
    expect(peak).toBe(concurrency);
    expect(active).toBe(0);
    expect(completed.sort()).toEqual([0, 1, 2, 3, 4, 5, 6]);
  }
});

test("failure stops new admissions and waits for already started checks", async () => {
  let release = (): void => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const failure = Error("Synthetic package refusal");
  const events: string[] = [];
  const execution = runIndependentPackageTests([
    () => {
      events.push("refused");
      return Promise.reject(failure);
    },
    async () => {
      events.push("started");
      await held;
      events.push("drained");
    },
    () => {
      events.push("must-not-start");
      return Promise.resolve();
    },
  ]);
  let finished = false;
  const observed = execution.catch((error: unknown) => {
    finished = true;
    return error;
  });
  await Promise.resolve();
  await Promise.resolve();
  expect(events).toEqual(["refused", "started"]);
  expect(finished).toBe(false);
  release();
  expect(await observed).toBe(failure);
  expect(events).toEqual(["refused", "started", "drained"]);
});

test("empty work is finite and unsupported concurrency refuses before admission", async () => {
  await runIndependentPackageTests([]);
  let admitted = false;
  await expect(
    runIndependentPackageTests(
      [
        () => {
          admitted = true;
          return Promise.resolve();
        },
      ],
      3 as 2,
    ),
  ).rejects.toThrow("one or two");
  expect(admitted).toBe(false);
});
