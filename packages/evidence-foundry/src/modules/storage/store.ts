import { Database } from "bun:sqlite";
import {
  mkdir,
  readFile,
  readdir,
  link,
  unlink,
  rm,
  stat,
  open,
  chmod,
  cp,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { gzipSync, gunzipSync } from "node:zlib";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import schema from "../../../schemas/storage.schema.json";
import type { AuditRecord, CommitBatch, BackupManifest } from "../../generated/storage";
import { assertObjectRecord, assertLinkRecord } from "@palantirkc/ontology/operational";
import type { ObjectRecord, LinkRecord } from "@palantirkc/ontology/operational";

export type StoredRecord = ObjectRecord | LinkRecord | AuditRecord;
export const hash = (bytes: Uint8Array | string) =>
  new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
export function canonical(value: unknown): string {
  if (value === null || typeof value === "string" || typeof value === "boolean")
    return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (
    typeof value === "object" &&
    value &&
    (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null)
  )
    return (
      "{" +
      Object.entries(value)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([k, v]) => JSON.stringify(k) + ":" + canonical(v))
        .join(",") +
      "}"
    );
  throw new Error("Only finite plain JSON is supported");
}
const ajv = new Ajv2020({ strict: true, allErrors: true });
addFormats(ajv);
const validateStorage = ajv.compile(schema);
function validateRecord(value: unknown): asserts value is StoredRecord {
  canonical(value);
  if ((value as StoredRecord)?.kind === "object") assertObjectRecord(value);
  else if ((value as StoredRecord)?.kind === "link") assertLinkRecord(value);
  else if (!validateStorage(value) || (value as StoredRecord).kind !== "audit")
    throw new Error("Invalid stored record: " + JSON.stringify(validateStorage.errors));
}
async function syncDirectory(path: string) {
  const fd = await open(path, "r");
  try {
    await fd.sync();
  } finally {
    await fd.close();
  }
}
async function immutable(path: string, bytes: Uint8Array | string) {
  await mkdir(join(path, ".."), { recursive: true, mode: 0o700 });
  const temporary = path + ".pending-" + crypto.randomUUID();
  const fd = await open(temporary, "wx", 0o600);
  try {
    await fd.writeFile(bytes);
    await fd.sync();
  } finally {
    await fd.close();
  }
  try {
    await link(temporary, path);
    await syncDirectory(join(path, ".."));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
    if (hash(await readFile(path)) !== hash(bytes))
      throw new Error("Immutable content corruption: " + path);
  } finally {
    await unlink(temporary).catch(() => {});
  }
}
export interface CommitContext {
  objects: Map<string, ObjectRecord>;
  links: Map<string, LinkRecord>;
  audits: AuditRecord[];
}
export interface CommitOptions {
  beforePublish?: () => void;
  afterPublish?: () => void;
}
export class Store {
  private db!: Database;
  private projectedHead = "";
  constructor(readonly root: string) {
    this.root = resolve(root);
  }
  async initialize() {
    await mkdir(this.root, { recursive: true, mode: 0o700 });
    await chmod(this.root, 0o700);
    for (const directory of ["blobs", "records", "commits", "indexes", "runs"])
      await mkdir(join(this.root, directory), { recursive: true, mode: 0o700 });
    await this.refresh();
    return this;
  }
  blobPath(sha256: string) {
    this.checkHash(sha256);
    return join(this.root, "blobs", sha256.slice(0, 2), sha256 + ".gz");
  }
  recordPath(sha256: string) {
    this.checkHash(sha256);
    return join(this.root, "records", sha256.slice(0, 2), sha256 + ".json");
  }
  private checkHash(value: string) {
    if (!/^[a-f0-9]{64}$/.test(value)) throw new Error("Invalid SHA-256");
  }
  async putBlob(bytes: Uint8Array | string) {
    const data = typeof bytes === "string" ? Buffer.from(bytes) : bytes;
    const sha256 = hash(data);
    const path = this.blobPath(sha256);
    await immutable(path, gzipSync(data));
    await this.readBlob(sha256);
    return { sha256, byteLength: data.byteLength };
  }
  async readBlob(sha256: string): Promise<Uint8Array> {
    const bytes = gunzipSync(await readFile(this.blobPath(sha256)));
    if (hash(bytes) !== sha256) throw new Error("Blob hash corruption: " + sha256);
    return bytes;
  }
  async readRecord(sha256: string): Promise<StoredRecord> {
    const bytes = await readFile(this.recordPath(sha256));
    if (hash(bytes) !== sha256) throw new Error("Record hash corruption: " + sha256);
    const value: unknown = JSON.parse(bytes.toString());
    validateRecord(value);
    return value;
  }
  async commits(): Promise<{ batch: CommitBatch; sha256: string }[]> {
    const files = (await readdir(join(this.root, "commits")))
      .filter((p) => /^\d{12}-[a-f0-9]{64}\.json$/.test(p))
      .sort();
    const result: { batch: CommitBatch; sha256: string }[] = [];
    let previousHash: string | null = null;
    for (const file of files) {
      const bytes = await readFile(join(this.root, "commits", file));
      const sha256 = hash(bytes);
      if (file !== `${String(result.length + 1).padStart(12, "0")}-${sha256}.json`)
        throw new Error("Commit hash or sequence corruption: " + file);
      const batch = JSON.parse(bytes.toString()) as CommitBatch;
      if (
        !validateStorage(batch) ||
        batch.kind !== "commit" ||
        batch.sequence !== result.length + 1 ||
        batch.previousHash !== previousHash
      )
        throw new Error("Invalid commit chain: " + file);
      result.push({ batch, sha256 });
      previousHash = sha256;
    }
    return result;
  }
  async snapshot(): Promise<CommitContext> {
    const objects = new Map<string, ObjectRecord>();
    const links = new Map<string, LinkRecord>();
    const audits: AuditRecord[] = [];
    for (const { batch } of await this.commits())
      for (const reference of batch.records) {
        const record = await this.readRecord(reference.sha256);
        if (record.kind !== reference.kind || record.id !== reference.id)
          throw new Error("Commit reference mismatch");
        if (record.kind === "audit") audits.push(record);
        else {
          const map = record.kind === "object" ? objects : links;
          const prior = map.get(record.id);
          if (record.version !== (prior?.version ?? 0) + 1)
            throw new Error("Record version history corruption");
          (map as Map<string, ObjectRecord | LinkRecord>).set(record.id, record);
        }
      }
    return { objects, links, audits };
  }
  async locked<T>(work: () => Promise<T>): Promise<T> {
    // The kernel releases flock when the holder exits, including parent crashes.
    // A tiny stdin-held subprocess avoids stale PID locks and blocking the Bun event loop.
    const holder = Bun.spawn(
      [
        "flock",
        "--exclusive",
        "--timeout",
        "30",
        join(this.root, "writer.lock"),
        "sh",
        "-c",
        "printf ready; cat >/dev/null",
      ],
      { stdin: "pipe", stdout: "pipe", stderr: "pipe" },
    );
    const reader = holder.stdout.getReader();
    const ready = await reader.read();
    if (ready.done || new TextDecoder().decode(ready.value) !== "ready") {
      const code = await holder.exited;
      throw new Error(
        "Writer lock unavailable: exit " + code + " " + (await new Response(holder.stderr).text()),
      );
    }
    try {
      return await work();
    } finally {
      holder.stdin.end();
      await holder.exited;
      reader.releaseLock();
    }
  }
  async transaction(
    build: (context: CommitContext) => Promise<StoredRecord[]> | StoredRecord[],
    options: CommitOptions = {},
  ) {
    return this.locked(async () => {
      const context = await this.snapshot();
      const records = await build(context);
      if (!records.length) throw new Error("An empty commit is not allowed");
      const ids = new Set<string>();
      for (const record of records) {
        validateRecord(record);
        await this.verifyReferences(record);
        if (record.kind === "audit" && context.audits.some((a) => a.id === record.id))
          throw new Error("Duplicate audit identity");
        if (ids.has(record.id)) throw new Error("Duplicate record in batch");
        ids.add(record.id);
        if (record.kind !== "audit") {
          const previous = (record.kind === "object" ? context.objects : context.links).get(
            record.id,
          );
          if (record.version !== (previous?.version ?? 0) + 1)
            throw new Error("Version conflict: " + record.id);
        }
      }
      const refs = [];
      for (const record of records) {
        const bytes = canonical(record);
        const sha256 = hash(bytes);
        await immutable(this.recordPath(sha256), bytes);
        refs.push({ id: record.id, kind: record.kind, sha256 });
      }
      const commits = await this.commits();
      const sequence = commits.length + 1;
      const batch: CommitBatch = {
        schemaVersion: "1.0.0",
        kind: "commit",
        id: crypto.randomUUID(),
        sequence,
        previousHash: commits.at(-1)?.sha256 ?? null,
        createdAt: new Date().toISOString(),
        records: refs as CommitBatch["records"],
      };
      options.beforePublish?.();
      const bytes = canonical(batch);
      const sha256 = hash(bytes);
      await immutable(
        join(this.root, "commits", `${String(sequence).padStart(12, "0")}-${sha256}.json`),
        bytes,
      );
      options.afterPublish?.();
      await this.refresh();
      return { batch, sha256 };
    });
  }
  private openDatabase() {
    this.db?.close();
    this.db = new Database(join(this.root, "indexes", "projection.sqlite"), { create: true });
    this.db.exec(`PRAGMA journal_mode=WAL; PRAGMA busy_timeout=30000;
      CREATE TABLE IF NOT EXISTS objects(id TEXT PRIMARY KEY, object_type TEXT, project_id TEXT, scope TEXT, version INTEGER, json TEXT);
      CREATE TABLE IF NOT EXISTS links(id TEXT PRIMARY KEY, from_id TEXT, to_id TEXT, link_type TEXT, json TEXT);
      CREATE TABLE IF NOT EXISTS history(id TEXT, version INTEGER, kind TEXT, hash TEXT, sequence INTEGER);
      CREATE TABLE IF NOT EXISTS audits(id TEXT PRIMARY KEY,json TEXT);
      CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT);
      CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(id UNINDEXED, text);`);
  }
  async refresh(force = false) {
    const commits = await this.commits();
    const head = commits.at(-1)?.sha256 ?? "empty";
    const exists = await Bun.file(join(this.root, "indexes", "projection.sqlite")).exists();
    if (!this.db || !exists) {
      this.openDatabase();
      this.projectedHead = "";
    }
    const indexed = (
      this.db.query("SELECT value FROM meta WHERE key='head'").get() as { value: string } | null
    )?.value;
    if (!force && this.projectedHead === head && indexed === head) return;
    // Validate the complete canonical chain before touching a working projection.
    const snapshots: { record: StoredRecord; sha256: string; sequence: number }[] = [];
    for (const { batch } of commits)
      for (const reference of batch.records) {
        const record = await this.readRecord(reference.sha256);
        if (record.id !== reference.id || record.kind !== reference.kind)
          throw new Error("Commit reference mismatch");
        snapshots.push({ record, sha256: reference.sha256, sequence: batch.sequence });
      }
    const project = this.db.transaction(() => {
      this.db.exec(
        "DELETE FROM objects; DELETE FROM links; DELETE FROM history; DELETE FROM audits; DELETE FROM search;",
      );
      for (const { record, sha256, sequence } of snapshots) {
        const json = canonical(record);
        if (record.kind === "object")
          this.db
            .query("INSERT OR REPLACE INTO objects VALUES(?,?,?,?,?,?)")
            .run(
              record.id,
              record.objectType,
              record.projectId,
              record.scope,
              record.version,
              json,
            );
        else if (record.kind === "link")
          this.db
            .query("INSERT OR REPLACE INTO links VALUES(?,?,?,?,?)")
            .run(record.id, record.from, record.to, record.linkType, json);
        else this.db.query("INSERT OR REPLACE INTO audits VALUES(?,?)").run(record.id, json);
        this.db
          .query("INSERT INTO history VALUES(?,?,?,?,?)")
          .run(
            record.id,
            record.kind === "audit" ? 0 : record.version,
            record.kind,
            sha256,
            sequence,
          );
      }
      for (const row of this.db.query("SELECT id,json FROM objects").all() as {
        id: string;
        json: string;
      }[])
        this.db.query("INSERT INTO search VALUES(?,?)").run(row.id, row.json);
      this.db.query("INSERT OR REPLACE INTO meta VALUES('head',?)").run(head);
    });
    project();
    this.projectedHead = head;
  }
  async query(options: {
    projectId: string;
    objectType?: string;
    search?: string;
    includeShared?: boolean;
  }) {
    if (!options.projectId) throw new Error("Project scope required");
    await this.refresh();
    const clauses = [
      options.includeShared === false ? "o.project_id=?" : "(o.project_id=? OR o.scope='shared')",
    ];
    const args: string[] = [options.projectId];
    if (options.objectType) {
      clauses.push("o.object_type=?");
      args.push(options.objectType);
    }
    if (options.search) {
      clauses.push("o.id IN (SELECT id FROM search WHERE search MATCH ?)");
      args.push('"' + options.search.replaceAll('"', '""') + '"');
    }
    return (
      this.db
        .query("SELECT o.json FROM objects o WHERE " + clauses.join(" AND ") + " ORDER BY o.id")
        .all(...args) as { json: string }[]
    ).map((row) => JSON.parse(row.json) as ObjectRecord);
  }
  async history(id: string) {
    await this.refresh();
    return (
      this.db
        .query("SELECT hash,sequence,kind,version FROM history WHERE id=? ORDER BY sequence")
        .all(id) as { hash: string; sequence: number; kind: string; version: number }[]
    ).map((row) => row);
  }
  async audit(operation: string, status: AuditRecord["status"], details: Record<string, unknown>) {
    return this.transaction(() => [
      {
        schemaVersion: "1.0.0",
        kind: "audit",
        id: crypto.randomUUID(),
        createdAt: new Date().toISOString(),
        operation,
        status,
        details,
      },
    ]);
  }
  private artifactReferences(record: StoredRecord): { sha256: string; byteLength: number }[] {
    const artifacts =
      record.kind === "object" && record.objectType === "SourceCapture"
        ? record.properties.artifacts
        : record.kind === "audit"
          ? record.details.artifacts
          : undefined;
    if (!Array.isArray(artifacts)) return [];
    return artifacts.map((value) => {
      if (
        !value ||
        typeof value !== "object" ||
        typeof value.sha256 !== "string" ||
        typeof value.byteLength !== "number"
      )
        throw new Error("Invalid artifact blob reference");
      return { sha256: value.sha256, byteLength: value.byteLength };
    });
  }
  private async verifyReferences(record: StoredRecord) {
    for (const reference of this.artifactReferences(record)) {
      const bytes = await this.readBlob(reference.sha256);
      if (bytes.byteLength !== reference.byteLength)
        throw new Error("Artifact byte length mismatch: " + reference.sha256);
    }
  }
  async verifyIntegrity() {
    const snapshot = await this.snapshot();
    const blobs = await this.allFiles("blobs");
    for (const { batch } of await this.commits())
      for (const ref of batch.records)
        await this.verifyReferences(await this.readRecord(ref.sha256));
    for (const file of blobs)
      if (file.endsWith(".gz")) await this.readBlob(file.split("/").at(-1)!.slice(0, -3));
    return {
      commits: (await this.commits()).length,
      objects: snapshot.objects.size,
      links: snapshot.links.size,
      blobs: blobs.filter((f) => f.endsWith(".gz")).length,
    };
  }
  async allFiles(directory: string): Promise<string[]> {
    const walk = async (path: string): Promise<string[]> => {
      const out: string[] = [];
      for (const item of await readdir(path, { withFileTypes: true })) {
        if (item.isDirectory()) out.push(...(await walk(join(path, item.name))));
        else out.push(join(path, item.name));
      }
      return out;
    };
    return walk(join(this.root, directory));
  }
  async stats() {
    const snapshot = await this.snapshot();
    const referenced = new Set<string>();
    const visit = (v: unknown) => {
      if (Array.isArray(v)) v.forEach(visit);
      else if (v && typeof v === "object")
        for (const [k, x] of Object.entries(v)) {
          if (k === "sha256" && typeof x === "string") referenced.add(x);
          else visit(x);
        }
    };
    for (const object of snapshot.objects.values()) visit(object.properties);
    // Historical records retain references, even when the current object changed.
    for (const { batch } of await this.commits())
      for (const ref of batch.records) visit(await this.readRecord(ref.sha256));
    const blobs = (await this.allFiles("blobs")).filter((f) => f.endsWith(".gz"));
    let compressedBytes = 0;
    for (const file of blobs) compressedBytes += (await stat(file)).size;
    const unreferenced = blobs
      .map((f) => f.split("/").at(-1)!.slice(0, -3))
      .filter((h) => !referenced.has(h));
    return {
      root: this.root,
      commits: (await this.commits()).length,
      objects: snapshot.objects.size,
      links: snapshot.links.size,
      blobCount: blobs.length,
      compressedBytes,
      unreferenced,
    };
  }
  async cleanup(hashes: string[]) {
    return this.locked(async () => {
      const stats = await this.stats();
      for (const value of hashes) {
        this.checkHash(value);
        if (!stats.unreferenced.includes(value))
          throw new Error("Cleanup refused for referenced or missing blob: " + value);
        await this.readBlob(value);
      }
      for (const value of hashes) await unlink(this.blobPath(value));
      return { removed: hashes };
    });
  }
  async backup(destination: string) {
    const target = resolve(destination);
    if (target === this.root || target.startsWith(this.root + "/"))
      throw new Error("Backup must be outside store");
    return this.locked(async () => {
      await this.verifyIntegrity();
      await mkdir(target, { mode: 0o700 });
      const files: { path: string; sha256: string }[] = [];
      for (const directory of ["blobs", "records", "commits", "runs"])
        for (const file of await this.allFiles(directory)) {
          const relative = file.slice(this.root.length + 1);
          await mkdir(join(target, relative, ".."), { recursive: true, mode: 0o700 });
          await cp(file, join(target, relative), { errorOnExist: true, force: false });
          files.push({ path: relative, sha256: hash(await readFile(file)) });
        }
      await immutable(
        join(target, "backup.json"),
        canonical({
          schemaVersion: "1.0.0",
          kind: "backup",
          createdAt: new Date().toISOString(),
          head: (await this.commits()).at(-1)?.sha256 ?? null,
          files,
        }),
      );
      return { destination: target, fileCount: files.length };
    });
  }
  static async restore(source: string, destination: string) {
    const manifest: unknown = JSON.parse(await readFile(join(source, "backup.json"), "utf8"));
    if (!validateStorage(manifest) || (manifest as BackupManifest).kind !== "backup")
      throw new Error("Invalid backup manifest");
    const backup = manifest as BackupManifest;
    const paths = new Set<string>();
    const validated: { path: string; bytes: Buffer }[] = [];
    for (const file of backup.files) {
      if (
        !/^(blobs|records|commits|runs)\//.test(file.path) ||
        file.path.split("/").some((p) => p === ".." || p === "." || p === "")
      )
        throw new Error("Invalid backup path");
      if (paths.has(file.path)) throw new Error("Duplicate backup path");
      paths.add(file.path);
      const bytes = await readFile(join(source, file.path));
      if (hash(bytes) !== file.sha256) throw new Error("Backup hash corruption");
      validated.push({ path: file.path, bytes });
    }
    await mkdir(destination, { mode: 0o700 });
    for (const file of validated) await immutable(join(destination, file.path), file.bytes);
    const store = await new Store(destination).initialize();
    try {
      await store.verifyIntegrity();
      if (
        (await store.commits()).at(-1)?.sha256 !== backup.head &&
        !(backup.head === null && (await store.commits()).length === 0)
      )
        throw new Error("Backup committed head mismatch");
      return store;
    } catch (error) {
      store.close();
      throw error;
    }
  }
  close() {
    this.db?.close();
  }
}
