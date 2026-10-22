import { afterEach, expect, test } from "bun:test";
import { withCodexSource } from "@palantirkc/native-codex-adapter/source/v1";
import { withHermesSource } from "@palantirkc/native-hermes-adapter/source-provider/v1";
import { sourceFixture, cleanupSourceFixtures } from "./fixtures/source-owner.ts";
afterEach(cleanupSourceFixtures);
test.each(["codex-context", "codex-native-source", "hermes-context-source"] as const)(
  "explicit %s consumes the same qualified source implementation and retains its native owner",
  async (provider) => {
    const f = await sourceFixture({ provider });
    const consume = (modules: Readonly<Record<string, Readonly<Record<string, unknown>>>>) =>
      modules.api!.value;
    const value =
      provider === "hermes-context-source"
        ? await withHermesSource(f.binding, f.owner, consume)
        : await withCodexSource(f.binding, f.owner, consume);
    expect(value).toBe("source-v1");
    if (provider === "hermes-context-source")
      await expect(withCodexSource(f.binding, f.owner, consume)).rejects.toThrow(
        "CODEX_SOURCE_PROVIDER_REQUIRED",
      );
    else
      await expect(withHermesSource(f.binding, f.owner, consume)).rejects.toThrow(
        "HERMES_SOURCE_PROVIDER_REQUIRED",
      );
  },
);
