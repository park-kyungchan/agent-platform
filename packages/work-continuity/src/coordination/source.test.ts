import { afterEach, expect, test } from "bun:test";
import { chmod, writeFile, symlink, readFile, lstat, access } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  normalizeContent,
  normalizeLocation,
  normalizeBinding,
  contentDigest,
  locationDigest,
  providerBytes,
  providerDigest,
  sourceHash,
  type SourceBindingInput,
} from "./source-contract.ts";
import { withQualifiedSource, type QualifiedSourceView } from "./source-runtime.ts";
import { materializeSourceDistribution } from "./source-distribution.ts";
import { sourceFixture, cleanupSourceFixtures } from "./fixtures/source-owner.ts";
afterEach(cleanupSourceFixtures);
const consume = (
  binding: SourceBindingInput,
  owner: Awaited<ReturnType<typeof sourceFixture>>["owner"],
) => withQualifiedSource(binding, owner, async (view) => (await view.load("api")).value);

test("content-addressed distribution preserves exact declared bytes and is exclusive; JSON grants no owner", async () => {
  const f = await sourceFixture();
  expect(f.delivery.contentDigest).toBe(contentDigest(f.content));
  expect(f.delivery.root).toBe(join(f.targetParent, f.delivery.contentDigest));
  expect((await lstat(join(f.delivery.root, "src/api.ts"))).mode & 0o777).toBe(0o444);
  expect(await readFile(join(f.delivery.root, "src/api.ts"), "utf8")).toBe(
    await readFile(join(f.sourceRoot, "src/api.ts"), "utf8"),
  );
  await expect(materializeSourceDistribution(f.content, f.distributionOwner)).rejects.toThrow(
    "SOURCE_TARGET_EXISTS_RECONCILE_NO_RETRY",
  );
  await expect(materializeSourceDistribution(f.content, undefined!)).rejects.toThrow(
    "OWNER_REQUIRED",
  );
  expect(await consume(f.binding, f.owner)).toBe("source-v1");
});
test("typed normalization is order-independent and rejects unknown fields, duplicate/colliding/unsafe/private members", async () => {
  const f = await sourceFixture(),
    copy = JSON.parse(JSON.stringify(f.content));
  copy.files.reverse();
  expect(contentDigest(copy)).toBe(contentDigest(f.content));
  for (const path of [
    "../escape",
    "src//bad",
    ".codex/config.toml",
    "state/private.json",
    ".env",
    "secrets/token",
    "work/user/original.json",
    "node_modules/undeclared/index.js",
  ]) {
    const value = structuredClone(f.content);
    value.files[0]!.path = path;
    expect(() => normalizeContent(value)).toThrow();
  }
  expect(() => normalizeContent({ ...f.content, authority: "approved" })).toThrow("EXACT_RECORD");
  expect(() =>
    normalizeContent({ ...f.content, files: [...f.content.files, f.content.files[0]!] }),
  ).toThrow("DUPLICATE");
  expect(() => normalizeLocation({ ...f.binding.location, root: join(f.root, "latest") })).toThrow(
    "EXACT_PHYSICAL",
  );
  expect(() => normalizeBinding({ ...f.binding, reader: "forged" } as SourceBindingInput)).toThrow(
    "EXACT_RECORD",
  );
});
test.each(["content", "location", "registration", "actual-tool"] as const)(
  "wrong %s refuses before any payload import",
  async (part) => {
    const f = await sourceFixture(),
      b = structuredClone(f.binding);
    if (part === "content") b.content.files[0]!.sha256 = "c".repeat(64);
    if (part === "location") b.location.root = f.sourceRoot;
    if (part === "registration") b.registration.generation = "other";
    if (part === "actual-tool") f.state.toolHash = "f".repeat(64);
    await expect(consume(b, f.owner)).rejects.toThrow();
  },
);
test.each(["uid", "gid", "host", "mountNamespace", "profile", "role"] as const)(
  "actual namespace/principal %s must join independently of matching declared hashes",
  async (field) => {
    const f = await sourceFixture(),
      b = structuredClone(f.binding);
    if (field === "uid" || field === "gid") b.location.identity[field]++;
    else b.location.identity[field] += "-other";
    b.registration.locationDigest = locationDigest(b.location);
    f.state.currentSelection = f.selection(b);
    await writeFile(b.registrationPath, providerBytes(b.registration));
    await expect(consume(b, f.owner)).rejects.toThrow("NAMESPACE_PRINCIPAL_PROFILE_ROLE");
  },
);
test.each(["bytes", "extra", "symlink", "root-alias"] as const)(
  "immutable source %s mismatch fails before load and does not clear a cache",
  async (mutation) => {
    const f = await sourceFixture(),
      b = structuredClone(f.binding);
    const root = b.location.root;
    if (mutation === "bytes") {
      await chmod(join(root, "src/api.ts"), 0o600);
      await writeFile(join(root, "src/api.ts"), 'export const value="wrong";');
      await chmod(join(root, "src/api.ts"), 0o444);
    }
    if (mutation === "extra") {
      await chmod(root, 0o755);
      await writeFile(join(root, "extra.ts"), 'throw Error("not admitted")');
      await chmod(root, 0o555);
    }
    if (mutation === "symlink") {
      await chmod(root, 0o755);
      await symlink("/etc/passwd", join(root, "forbidden"));
      await chmod(root, 0o555);
    }
    if (mutation === "root-alias") {
      const alias = join(f.root, "alias");
      await symlink(root, alias);
      b.location.root = alias;
      b.registration.locationDigest = locationDigest(b.location);
      f.state.currentSelection = f.selection(b);
      await writeFile(b.registrationPath, providerBytes(b.registration));
    }
    await expect(consume(b, f.owner)).rejects.toThrow();
  },
);
test("source view closes after native lease and current-provider drift revokes use before/after load", async () => {
  const f = await sourceFixture();
  let retained: QualifiedSourceView | undefined;
  await withQualifiedSource(f.binding, f.owner, (source) => {
    retained = source;
    return source.proof;
  });
  await expect(retained!.read("src/api.ts")).rejects.toThrow("VIEW_CLOSED");
  await expect(
    withQualifiedSource(f.binding, f.owner, async (source) => {
      f.state.currentGeneration = "generation-2";
      return source.load("api");
    }),
  ).rejects.toThrow("REVOKED");
});
test("registration drift or payload mutation after load rejects the whole native read occurrence", async () => {
  const f = await sourceFixture();
  await expect(
    withQualifiedSource(f.binding, f.owner, async (source) => {
      await source.load("api");
      await writeFile(
        f.binding.registrationPath,
        providerBytes({ ...f.binding.registration, generation: "changed" }),
      );
      return "must-not-return";
    }),
  ).rejects.toThrow("REGISTRATION_CHANGED");
});
test("preimported unqualified modules and changed loaded images require a fresh process", async () => {
  const f = await sourceFixture();
  await import(pathToFileURL(join(f.binding.location.root, "src/api.ts")).href);
  await expect(consume(f.binding, f.owner)).rejects.toThrow("FIRST_IMPORT_OR_LOADED_IMAGE");
});
test("immutable audit/dependency and original recovery keep selected source after main/provider advance", async () => {
  const f = await sourceFixture(),
    old = await f.admitRecovery();
  f.state.main = "d".repeat(40);
  f.state.currentGeneration = "generation-9";
  f.state.currentSelection = { ...f.state.currentSelection!, contentDigest: "e".repeat(64) };
  expect(await consume(old, f.owner)).toBe("source-v1");
  await expect(consume(f.binding, f.owner)).rejects.toThrow("SELECTION_MISMATCH");
  const wrong = structuredClone(old);
  wrong.recovery!.originalReceipt.sha256 = sourceHash("foreign receipt");
  await expect(consume(wrong, f.owner)).rejects.toThrow("SELECTION_MISMATCH");
  f.state.active = false;
  await expect(consume(old, f.owner)).rejects.toThrow("REVOKED");
});
test("new snapshot content cannot silently reuse old recovery binding", async () => {
  const f = await sourceFixture(),
    old = await f.admitRecovery();
  const changed = structuredClone(old);
  changed.content.commit = "c".repeat(40);
  expect(() => normalizeBinding(changed)).toThrow("JOIN_MISMATCH");
  expect(providerDigest(old.registration)).toBe(old.recovery!.registrationDigest);
});
test("source snapshot mutation leaves its own incomplete stage for reconciliation and never retries/overwrites it", async () => {
  const f = await sourceFixture();
  const changed = normalizeContent({ ...f.content, commit: "f".repeat(40) });
  await expect(materializeSourceDistribution(changed, f.distributionOwner)).rejects.toThrow(
    "SNAPSHOT_MISMATCH",
  );
  const content = structuredClone(f.content);
  // Source proof preflight now precedes output writes. Inject drift only after
  // the new stage exists to retain a real uncertain/incomplete-output control.
  let mutatedAfterStage = false;
  f.state.beforeSnapshotCheck = async () => {
    if (
      !mutatedAfterStage &&
      (await lstat(join(f.targetParent, ".staging-" + contentDigest(content))).then(
        () => true,
        () => false,
      ))
    ) {
      mutatedAfterStage = true;
      await writeFile(
        join(f.sourceRoot, "src/api.ts"),
        'export const value="changed-after-stage";',
      );
    }
  };
  // Keep selected Git identity unchanged while obtaining a distinct digest.
  content.entries.push({ id: "second-api", path: "src/api.ts" });
  await expect(materializeSourceDistribution(content, f.distributionOwner)).rejects.toThrow(
    "INCOMPLETE_RECONCILE",
  );
  expect(
    (await lstat(join(f.targetParent, ".staging-" + contentDigest(content)))).isDirectory(),
  ).toBe(true);
  expect(mutatedAfterStage).toBe(true);
});
test("computed/undeclared imports and dependency escapes are refused before evaluation", async () => {
  const f = await sourceFixture({
    text: 'const name="node:fs"; export const value=await import(name);',
  });
  await expect(consume(f.binding, f.owner)).rejects.toThrow("COMPUTED_LOADER_UNSUPPORTED");
  const other = await sourceFixture({ text: 'import "./missing.ts"; export const value="bad";' });
  await expect(consume(other.binding, other.owner)).rejects.toThrow();
});

test("dirty source cannot be relabeled as a selected Git commit by matching checkout hashes alone", async () => {
  const f = await sourceFixture(),
    content = structuredClone(f.content),
    text = 'export const value="dirty";\n';
  await writeFile(join(f.sourceRoot, "src/api.ts"), text);
  const pin = content.files.find((row) => row.path === "src/api.ts")!;
  pin.sha256 = sourceHash(text);
  pin.bytes = Buffer.byteLength(text);
  await expect(materializeSourceDistribution(content, f.distributionOwner)).rejects.toThrow(
    "IMMUTABLE_GIT_BLOB",
  );
  await expect(
    access(join(f.targetParent, ".staging-" + contentDigest(content))),
  ).rejects.toThrow();
});
