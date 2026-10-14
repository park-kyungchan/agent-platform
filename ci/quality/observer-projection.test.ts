import { test } from "bun:test";
import { resolve } from "node:path";
import { synchronizeObserverProjection } from "./observer-projection";
test("native observer embeds the exact canonical validator projection", async () => {
  await synchronizeObserverProjection(resolve(import.meta.dir, "../.."));
});
