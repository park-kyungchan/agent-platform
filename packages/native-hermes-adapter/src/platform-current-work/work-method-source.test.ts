import { afterEach, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fixture, rebind, ref, sha, verified } from "./tests/work-method.fixture.ts";
import { readHermesWorkMethod, readHermesWorkMethodSourceDocument } from "./work-method.ts";
import {
  readHermesWorkMethodSource,
  type WorkMethodSourceMap,
  type WorkMethodSessionSourceMap,
  type WorkMethodSourceBinding,
} from "./work-method-source.ts";

const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});
async function sourceFixture(text = '\uFEFFWhole WHY 한국어 😀 "\\\n\t'.repeat(4000)) {
  const root = await mkdtemp(join(tmpdir(), "work-method-source-"));
  roots.push(root);
  const owner = fixture();
  const map: WorkMethodSourceMap = {
    schema: "hermes.work-method-source-map.v1",
    logical: owner.admission.packet.logical,
    actorId: owner.selection.actorId,
    routeId: owner.selection.routeId,
    native: owner.selection.native,
    cwd: owner.selection.cwd,
    entries: [
      { id: "original", path: "original.json", sha256: sha(text), bytes: Buffer.byteLength(text) },
    ],
    provenance: { original: "Synthetic complete source, not native user data" },
  };
  await writeFile(join(root, "original.json"), text);
  const mapPath = join(root, "map.json");
  const saveMap = async () => {
    const bytes = JSON.stringify(map, null, 2) + "\n";
    await writeFile(mapPath, bytes);
    binding.mapSha256 = sha(bytes);
    request.sourceMapSha256 = binding.mapSha256;
  };
  const binding = { runtimeMapPath: mapPath, mapSha256: "", dataBase: root };
  const request = {
    schema: "hermes.work-method-source-request.v1",
    event: owner.event,
    selection: {
      cwd: owner.selection.cwd,
      native: owner.selection.native,
      actorId: map.actorId,
      routeId: map.routeId,
      logical: map.logical,
    },
    sourceMapSha256: "",
    target: { kind: "mapped-source", id: "original" } as Record<string, unknown>,
    cursor: { offsetBytes: 0, expectedDocumentSha256: null as string | null },
    outputVersion: "v2",
    maxChars: 12000,
  };
  await saveMap();
  return { root, owner, map, binding, request, saveMap, text };
}
async function collect(f: {
  binding: WorkMethodSourceBinding;
  request: {
    maxChars: number;
    cursor: { offsetBytes: number; expectedDocumentSha256: string | null };
  };
}) {
  let previous = 0,
    result = "",
    calls = 0;
  while (true) {
    const response = await readHermesWorkMethodSource(f.request, f.binding, verified);
    expect(response.status).toBe("source-qualified");
    expect(JSON.stringify(response).length).toBeLessThanOrEqual(f.request.maxChars);
    expect(response.chunk!.offsetBytes).toBe(previous);
    expect(response.ownerAdmissionEstablishedByDelivery).toBe(false);
    expect(response.receivedOrUsedEstablishedByDelivery).toBe(false);
    result += response.chunk!.text;
    previous = response.chunk!.nextOffsetBytes;
    calls++;
    if (response.chunk!.last) {
      expect(Buffer.byteLength(result)).toBe(response.binding!.totalBytes);
      expect(sha(result)).toBe(response.binding!.documentSha256);
      return { text: result, calls };
    }
    expect(previous).toBeGreaterThan(f.request.cursor.offsetBytes);
    f.request.cursor = {
      offsetBytes: previous,
      expectedDocumentSha256: response.binding!.documentSha256,
    };
    if (calls > 1000) throw Error("Source did not progress");
  }
}
test("complete UTF8 originals roundtrip with BOM, multibyte scalars, escaping and cap", async () => {
  const f = await sourceFixture();
  const result = await collect(f);
  expect(result.calls).toBeGreaterThan(1);
  expect(result.text).toBe(f.text);
});

async function sessionFixture(text = '\uFEFFstable WHY 한국어 😀 "\\\n'.repeat(1200)) {
  const root = await mkdtemp(join(tmpdir(), "work-method-session-source-"));
  roots.push(root);
  // Only a synthetic stable session tuple is known when this map is frozen.
  const map: WorkMethodSessionSourceMap = {
    schema: "hermes.work-method-session-source-map.v1",
    scope: {
      kind: "session-source",
      runtime: "native-hermes",
      profileId: "default",
      sessionId: "native-session-A",
    },
    logical: { domainId: "d", projectId: "p", workId: "w", taskId: "bootstrap" },
    actorId: "actor-A",
    routeId: "owner-task",
    entries: [
      { id: "original", path: "original", sha256: sha(text), bytes: Buffer.byteLength(text) },
    ],
  };
  const binding = { runtimeMapPath: join(root, "map.json"), dataBase: root, mapSha256: "" };
  const request = {
    schema: "hermes.work-method-session-source-request.v1",
    scope: map.scope,
    logical: map.logical,
    actorId: map.actorId,
    routeId: map.routeId,
    sourceMapSha256: "",
    target: { kind: "mapped-source", id: "original" } as Record<string, unknown>,
    cursor: { offsetBytes: 0, expectedDocumentSha256: null as string | null },
    outputVersion: "v2",
    maxChars: 12000,
  };
  await writeFile(join(root, "original"), text);
  const saveMap = async () => {
    const bytes = JSON.stringify(map, null, 2) + "\n";
    await writeFile(binding.runtimeMapPath, bytes);
    binding.mapSha256 = request.sourceMapSha256 = sha(bytes);
  };
  await saveMap();
  return { root, map, request, binding, saveMap, text };
}

test("session source reads complete originals without a speculative event or packet", async () => {
  const f = await sessionFixture();
  expect((await collect(f)).text).toBe(f.text);
  f.request.cursor = { offsetBytes: 0, expectedDocumentSha256: null };
  f.request.target.id = "@map";
  const map = JSON.parse((await collect(f)).text);
  expect(Object.keys(map).sort()).toEqual(
    ["schema", "scope", "logical", "actorId", "routeId", "entries"].sort(),
  );
  expect(map.scope).toEqual(f.map.scope);
});

test("session request/map pairing and every stable scope field are closed and exact", async () => {
  const f = await sessionFixture("source"),
    old = await sourceFixture("old");
  const variants = [
    { ...f.request, event: fixture().event },
    { ...f.request, scope: { ...f.request.scope, sessionId: null } },
    { ...f.request, scope: { ...f.request.scope, profileId: "foreign" } },
    { ...f.request, scope: { ...f.request.scope, turnId: null } },
    { ...f.request, actorId: "foreign" },
    { ...f.request, routeId: "foreign" },
    { ...f.request, logical: { ...f.request.logical, taskId: "foreign" } },
    { ...f.request, maxChars: 12001 },
    { ...f.request, target: { kind: "mapped-source", id: "original", event: {} } },
  ];
  for (const request of variants) {
    const held = await readHermesWorkMethodSource(request, f.binding, verified);
    expect(held.status).toBe("held");
    expect(held.chunk).toBeNull();
  }
  expect(
    (
      await readHermesWorkMethodSource(
        { ...old.request, sourceMapSha256: f.binding.mapSha256 },
        f.binding,
        verified,
      )
    ).holdReasons,
  ).toEqual(["SOURCE_MAP_SCOPE_MISMATCH"]);
  expect(
    (
      await readHermesWorkMethodSource(
        { ...f.request, sourceMapSha256: old.binding.mapSha256 },
        old.binding,
        verified,
      )
    ).holdReasons,
  ).toEqual(["SOURCE_MAP_SCOPE_MISMATCH"]);
  const bad = { ...f.map, native: fixture().selection.native };
  const bytes = JSON.stringify(bad) + "\n";
  await writeFile(f.binding.runtimeMapPath, bytes);
  f.binding.mapSha256 = f.request.sourceMapSha256 = sha(bytes);
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
});

test("a frozen session map admits only later fully joined owner data and detects document drift", async () => {
  const f = await sessionFixture("source"),
    mapBytes = await readFile(f.binding.runtimeMapPath);
  // Event and receiver evidence are constructed after the immutable source map.
  const owner = fixture(),
    input = owner.admission;
  owner.event.extra = { turn_id: "later-turn" };
  owner.selection.native.turnId = "later-turn";
  for (const native of [
    input.packet.native,
    input.expectedActor.native,
    input.receipt.actor.native,
  ])
    native.turnId = "later-turn";
  for (let n = 0; n < 5; n++) {
    const prerequisite = {
      producer: { ...input.packet.logical, taskId: "producer-" + n },
      receiver: input.packet.logical,
      milestone: "execution-and-verification" as const,
      producerReceipt: ref("producer-" + n),
      receiverReceipt: ref("synthetic-U0"),
    };
    input.packet.prerequisites.push(prerequisite);
    input.prerequisites.push({
      ...prerequisite,
      currentness: "current",
      producerSatisfied: true,
      receiverUsed: true,
    });
  }
  input.receipt.useEvidence = [ref("synthetic-U1-after-Q")];
  rebind(owner);
  f.request.target = { kind: "validated-owner-document", event: owner.event, request: owner };
  const first = await readHermesWorkMethodSource(f.request, f.binding, verified);
  const document = JSON.parse((await collect(f)).text);
  expect(document.contracts).toEqual({
    standing: input.standing,
    packet: input.packet,
    receipt: input.receipt,
  });
  expect(document.contracts.packet.prerequisites).toHaveLength(5);
  expect(document.admission.status).toBe("owner-eligible");
  expect(document.nativePermissionGranted).toBe(false);
  expect(await readFile(f.binding.runtimeMapPath)).toEqual(mapBytes);
  input.packet.task.text += " with later current context";
  rebind(owner);
  f.request.cursor = {
    offsetBytes: first.chunk!.nextOffsetBytes,
    expectedDocumentSha256: first.binding!.documentSha256,
  };
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_DOCUMENT_CHANGED",
  ]);
});

test("session source delivery cannot bypass full-event, actor, packet or current-event joins", async () => {
  const f = await sessionFixture("source");
  const changes: ((owner: ReturnType<typeof fixture>) => void)[] = [
    (o) => {
      o.selection.cwd = "/foreign";
    },
    (o) => {
      o.selection.native.turnId = "foreign";
    },
    (o) => {
      o.admission.receipt.actor.native.sessionId = "foreign";
    },
    (o) => {
      o.admission.packet.native.turnId = "foreign";
      rebind(o);
    },
    (o) => {
      o.admission.packet.task.sha256 = sha("foreign");
    },
    (o) => {
      o.selection.packetSha256 = sha("foreign");
    },
    (o) => {
      o.admission.currentEvent = {
        eventId: null,
        actorId: o.selection.actorId,
        packetSha256: o.selection.packetSha256,
        taskSha256: o.admission.packet.task.sha256,
        source: ref("event"),
        native: { ...o.admission.packet.native, sessionId: "foreign" },
      };
    },
  ];
  for (const change of changes) {
    const owner = fixture();
    change(owner);
    f.request.target = { kind: "validated-owner-document", event: owner.event, request: owner };
    const held = await readHermesWorkMethodSource(f.request, f.binding, verified);
    expect(held.status).toBe("held");
    expect(held.chunk).toBeNull();
  }
  const owner = fixture();
  for (const event of [
    { ...owner.event, session_id: "foreign" },
    { ...owner.event, extra: { turn_id: "foreign" } },
  ]) {
    f.request.target = { kind: "validated-owner-document", event, request: owner };
    expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
  }
});

test("valid session delivery preserves held Common decisions and never grants authority", async () => {
  const f = await sessionFixture("source"),
    owner = fixture();
  owner.admission.sources[0]!.currentness = "stale";
  f.request.target = { kind: "validated-owner-document", event: owner.event, request: owner };
  const document = JSON.parse((await collect(f)).text);
  expect(document.admission.status).toBe("held");
  expect(document.contracts.receipt.used).toBe(true);
  expect(document.nativePermissionGranted).toBe(false);
  f.request.cursor = { offsetBytes: 0, expectedDocumentSha256: null };
  owner.admission.receipt.used = false;
  rebind(owner);
  expect(JSON.parse((await collect(f)).text).admission.status).toBe("held");
});
test("map itself is read through its independently compiled pin", async () => {
  const f = await sourceFixture("small");
  f.request.target.id = "@map";
  const result = await collect(f);
  expect(result.text).toBe(await readFile(f.binding.runtimeMapPath, "utf8"));
});
test("continuation digest and UTF8 offsets cannot mix or skip bytes", async () => {
  const f = await sourceFixture("a😀b".repeat(1000));
  for (const cursor of [
    { offsetBytes: 2, expectedDocumentSha256: sha(f.text) },
    { offsetBytes: 1, expectedDocumentSha256: null },
    { offsetBytes: 0, expectedDocumentSha256: sha("foreign") },
    { offsetBytes: -1, expectedDocumentSha256: null },
    { offsetBytes: Buffer.byteLength(f.text) + 1, expectedDocumentSha256: sha(f.text) },
  ]) {
    f.request.cursor = cursor;
    const result = await readHermesWorkMethodSource(f.request, f.binding, verified);
    expect(result.status).toBe("held");
    expect(result.chunk).toBeNull();
  }
});
test("unknown fields, arbitrary paths, map pins and foreign identity or logical scope hold", async () => {
  const f = await sourceFixture("source");
  const variants = [
    { ...f.request, implementationPath: "/private.ts" },
    { ...f.request, target: { kind: "mapped-source", id: "../original.json" } },
    { ...f.request, target: { kind: "mapped-source", id: "original", path: "/foreign" } },
    { ...f.request, sourceMapSha256: sha("foreign") },
    {
      ...f.request,
      selection: { ...f.request.selection, logical: { ...f.map.logical, taskId: "foreign" } },
    },
    {
      ...f.request,
      selection: { ...f.request.selection, native: { ...f.map.native, sessionId: "foreign" } },
    },
    { ...f.request, event: { ...f.request.event, session_id: "foreign" } },
  ];
  for (const request of variants) {
    const result = await readHermesWorkMethodSource(request, f.binding, verified);
    expect(result.status).toBe("held");
    expect(result.chunk).toBeNull();
  }
  expect((await readHermesWorkMethodSource(f.request, undefined, verified)).holdReasons).toEqual([
    "SOURCE_MAP_NOT_REGISTERED",
  ]);
});
test("map path traversal, duplicates, lengths, totals and duplicate JSON keys hold", async () => {
  const f = await sourceFixture("source");
  const entry = structuredClone(f.map.entries[0]!);
  for (const path of ["../original.json", "/foreign", "a/../original.json", "a\\b", "a//b"]) {
    f.map.entries = [{ ...entry, path }];
    await f.saveMap();
    expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
  }
  f.map.entries = [entry, entry];
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
  f.map.entries = [{ ...entry, bytes: 1048577 }];
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
  f.map.entries = Array.from({ length: 33 }, (_, n) => ({ ...entry, id: "s" + n, bytes: 1048576 }));
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_MAP_TOTAL_LIMIT",
  ]);
  f.map.entries = Array.from({ length: 2049 }, (_, n) => ({ ...entry, id: "s" + n }));
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_MAP_LIMIT",
  ]);
  const bad = JSON.stringify(f.map).replace('"entries":', '"entries":[],"entries":') + "\n";
  await writeFile(f.binding.runtimeMapPath, bad);
  f.binding.mapSha256 = f.request.sourceMapSha256 = sha(bad);
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
});
test("symlink files and parent directories cannot escape the owned data base", async () => {
  const f = await sourceFixture("source");
  const outside = await mkdtemp(join(tmpdir(), "outside-source-"));
  roots.push(outside);
  await writeFile(join(outside, "source"), f.text);
  await rm(join(f.root, "original.json"));
  await symlink(join(outside, "source"), join(f.root, "original.json"));
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_FILE_NOT_CURRENT",
  ]);
  await symlink(outside, join(f.root, "link"));
  f.map.entries[0]!.path = "link/source";
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
});
test("file, map and source closure drift never publishes a chunk", async () => {
  const f = await sourceFixture("source");
  await writeFile(join(f.root, "original.json"), "changed");
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_FILE_NOT_CURRENT",
  ]);
  await writeFile(join(f.root, "original.json"), f.text);
  await writeFile(f.binding.runtimeMapPath, "{}");
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_MAP_NOT_CURRENT",
  ]);
  await f.saveMap();
  let count = 0;
  const result = await readHermesWorkMethodSource(f.request, f.binding, {
    verifySourceClosure: async () => {
      if (++count === 2) throw Error("private source path");
    },
  });
  expect(result.holdReasons).toEqual(["SOURCE_CLOSURE_NOT_CURRENT"]);
  expect(result.chunk).toBeNull();
  expect(JSON.stringify(result)).not.toContain("private source path");
});
test("map and file are rechecked after the final source-verification boundary", async () => {
  for (const name of ["original.json", "map.json"]) {
    const f = await sourceFixture("source");
    let checks = 0;
    const result = await readHermesWorkMethodSource(f.request, f.binding, {
      verifySourceClosure: async () => {
        if (++checks === 2) await writeFile(join(f.root, name), "changed");
      },
    });
    expect(result.status).toBe("held");
    expect(result.chunk).toBeNull();
    expect(result.holdReasons).toEqual([
      name === "map.json" ? "SOURCE_MAP_NOT_CURRENT" : "SOURCE_FILE_NOT_CURRENT",
    ]);
  }
});
test("invalid UTF8, metadata oversize and excessive output requests hold", async () => {
  const f = await sourceFixture("source");
  f.request.maxChars = 12001;
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "INVALID_OUTPUT_POLICY",
  ]);
  f.request.maxChars = 1024;
  for (const key of ["domainId", "projectId", "workId", "taskId"] as const)
    f.map.logical[key] = "x".repeat(256);
  await f.saveMap();
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).holdReasons).toEqual([
    "SOURCE_METADATA_OUTPUT_LIMIT",
  ]);
  const bytes = Buffer.from([0xc0, 0xaf]);
  await writeFile(join(f.root, "original.json"), bytes);
  f.map.entries[0]!.sha256 = sha(bytes);
  f.map.entries[0]!.bytes = bytes.length;
  await f.saveMap();
  f.request.maxChars = 12000;
  expect((await readHermesWorkMethodSource(f.request, f.binding, verified)).status).toBe("held");
});
test("derived document preserves all8routes/5prerequisites and legacy oversized response stays held", async () => {
  const f = await sourceFixture("source");
  const input = f.owner.admission;
  input.packet.lens.wholeObjective += " complete WHY ".repeat(400);
  const route = input.standing.routes[0]!;
  for (const kind of [
    "start",
    "delegate",
    "followup",
    "resume",
    "refresh",
    "pre-compact",
    "post-compact",
  ] as const)
    input.standing.routes.push({ ...route, id: kind, kind });
  for (let i = 0; i < 5; i++) {
    const prerequisite = {
      producer: { ...input.packet.logical, taskId: "producer-" + i },
      receiver: input.packet.logical,
      milestone: "execution-and-verification" as const,
      producerReceipt: {
        kind: "snapshot" as const,
        path: "producer-" + i,
        sha256: sha("producer-" + i),
      },
      receiverReceipt: {
        kind: "snapshot" as const,
        path: "receiver-" + i,
        sha256: sha("receiver-" + i),
      },
    };
    input.packet.prerequisites.push(prerequisite);
    input.prerequisites.push({
      ...prerequisite,
      currentness: "current",
      producerSatisfied: true,
      receiverUsed: true,
    });
  }
  rebind(f.owner);
  const legacy = await readHermesWorkMethod(f.owner, verified);
  expect(legacy.holdReasons).toEqual(["WORK_METHOD_OUTPUT_EXCEEDS_BOUND"]);
  f.request.target = { kind: "validated-owner-document", request: f.owner };
  const result = JSON.parse((await collect(f)).text);
  expect(result.contracts).toEqual({
    standing: input.standing,
    packet: input.packet,
    receipt: input.receipt,
  });
  expect(result.contracts.standing.routes).toHaveLength(8);
  expect(result.contracts.packet.prerequisites).toHaveLength(5);
  expect(result.admission.status).toBe("owner-eligible");
  input.standing.routes[0]!.maxPacketBytes = 100;
  const held = await readHermesWorkMethodSourceDocument(f.owner, verified);
  expect(held.admission.status).toBe("held");
  expect(held.admission.reasons).toContain("PACKET_EXCEEDS_ROUTE_BOUND");
  expect(held.nativePermissionGranted).toBe(false);
});
