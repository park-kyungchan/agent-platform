import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { gzipSync, gunzipSync } from 'node:zlib';

// Updating these immutable pins requires review; canonical domain code stays in agent-platform.
const source = {
  repository: 'https://github.com/park-kyungchan/agent-platform',
  commitSha: '2f8ca5da92d12376496f563fefb627ab80ab51b7',
  repositoryTreeSha: '9beaf9dbf024c6d7a2d58875844192d3a978f9ed',
  packagePath: 'packages/infrastructure-operations',
  packageTreeSha: '04764657cdd2f5d1d2b3c9f651198b57e75c6a57',
} as const;
const members = ['AGENTS.md', 'package.json', 'releases/palantirkc-ontology-0.2.0.tgz',
  ...['evaluate', 'index', 'registry', 'schemas', 'types', 'validation'].map(name => `src/domain/${name}.ts`)].sort();
const releaseName = 'palantirkc-infrastructure-operations-0.0.0-2f8ca5da92d1.tgz';
const root = resolve(import.meta.dir, '../..');
const archivePath = resolve(root, 'releases', releaseName);
const provenancePath = archivePath.replace(/\.tgz$/, '.json');
const hash = (bytes: Uint8Array, algorithm = 'sha256') => createHash(algorithm).update(bytes).digest('hex');
const gitHash = (type: string, bytes: Buffer) => hash(Buffer.concat([Buffer.from(`${type} ${bytes.length}\0`), bytes]), 'sha1');
function requireValue(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
interface Member { archivePath: string; sourcePath: string; gitMode: '100644'; gitBlobSha: string; sha256: string; bytes: number }
interface Provenance {
  schemaVersion: 1; kind: 'immutable-common-package-release';
  ontologyPackage: { name: '@palantirkc/ontology'; version: '0.2.0' };
  source: typeof source;
  artifact: { name: string; format: 'deterministic-ustar-gzip'; sha256: string; bytes: number };
  members: Member[];
}
function treeHash(rows: Map<string, Buffer>): string {
  interface Node { files: Map<string, Buffer>; children: Map<string, Node> }
  const newNode = (): Node => ({ files: new Map(), children: new Map() });
  const top = newNode();
  for (const [path, bytes] of rows) {
    const parts = path.split('/'); let node = top;
    for (const part of parts.slice(0, -1)) {
      if (!node.children.has(part)) node.children.set(part, newNode());
      node = node.children.get(part)!;
    }
    node.files.set(parts.at(-1)!, bytes);
  }
  const digest = (node: Node): string => {
    const entries = [...node.files].map(([name, bytes]) => ({ name, mode: '100644', sha: gitHash('blob', bytes), sort: name }));
    entries.push(...[...node.children].map(([name, child]) => ({ name, mode: '40000', sha: digest(child), sort: name + '/' })));
    entries.sort((a, b) => Buffer.compare(Buffer.from(a.sort), Buffer.from(b.sort)));
    return gitHash('tree', Buffer.concat(entries.map(row => Buffer.concat([Buffer.from(`${row.mode} ${row.name}\0`), Buffer.from(row.sha, 'hex')]))));
  };
  return digest(top);
}
function tar(rows: Map<string, Buffer>): Buffer {
  const output: Buffer[] = [];
  for (const [path, bytes] of rows) {
    const header = Buffer.alloc(512);
    const name = 'package/' + path;
    requireValue(Buffer.byteLength(name) <= 100, 'Archive member name exceeds USTAR field');
    header.write(name);
    const octal = (offset: number, length: number, value: number) => header.write(value.toString(8).padStart(length - 1, '0') + '\0', offset, length, 'ascii');
    octal(100, 8, 0o644); octal(108, 8, 0); octal(116, 8, 0);
    octal(124, 12, bytes.length); octal(136, 12, 0);
    header.fill(32, 148, 156); header.write('0', 156); header.write('ustar\0', 257); header.write('00', 263);
    header.write([...header].reduce((a, b) => a + b, 0).toString(8).padStart(6, '0') + '\0 ', 148, 8, 'ascii');
    output.push(header, bytes, Buffer.alloc((512 - bytes.length % 512) % 512));
  }
  return Buffer.concat([...output, Buffer.alloc(1024)]);
}
function untar(gzip: Buffer): Map<string, Buffer> {
  requireValue(gzip.length < 2_000_000, 'Archive exceeds bounded package size');
  const bytes = gunzipSync(gzip, { maxOutputLength: 4_000_000 });
  const rows = new Map<string, Buffer>(); let cursor = 0;
  const text = (start: number, length: number) => bytes.subarray(cursor + start, cursor + start + length).toString('utf8').replace(/\0.*$/s, '');
  while (cursor + 512 <= bytes.length) {
    const header = bytes.subarray(cursor, cursor + 512);
    if (header.every(value => value === 0)) {
      requireValue(bytes.subarray(cursor).every(value => value === 0) && bytes.length - cursor >= 1024, 'Invalid archive ending');
      requireValue(rows.size === members.length, 'Missing archive members'); return rows;
    }
    const sum = [...header].reduce((total, value, index) => total + (index >= 148 && index < 156 ? 32 : value), 0);
    requireValue(sum === parseInt(text(148, 8), 8), 'Invalid tar checksum');
    const archiveName = text(0, 100), path = archiveName.slice('package/'.length);
    requireValue(archiveName.startsWith('package/') && members.includes(path) && !rows.has(path), 'Unexpected or duplicate archive member');
    requireValue(text(156, 1) === '0' && text(257, 6) === 'ustar' && text(263, 2) === '00', 'Unsupported archive format/link');
    requireValue(text(157, 100) === '' && text(265, 32) === '' && text(297, 32) === '' && text(345, 155) === '', 'Unexpected tar metadata');
    requireValue(parseInt(text(100, 8), 8) === 0o644 && [108, 116, 136].every(offset => parseInt(text(offset, offset === 136 ? 12 : 8), 8) === 0), 'Non-deterministic member metadata');
    const size = parseInt(text(124, 12), 8);
    requireValue(Number.isSafeInteger(size) && size >= 0 && cursor + 512 + size <= bytes.length, 'Invalid tar member size');
    rows.set(path, bytes.subarray(cursor + 512, cursor + 512 + size));
    cursor += 512 + Math.ceil(size / 512) * 512;
  }
  throw new Error('Missing archive terminator');
}
async function git(repo: string, args: string[]): Promise<Buffer> {
  const child = Bun.spawn(['git', '-C', repo, ...args], { stdout: 'pipe', stderr: 'pipe', env: { ...process.env, GIT_NO_LAZY_FETCH: '1', GIT_TERMINAL_PROMPT: '0' } });
  const [bytes, code] = await Promise.all([new Response(child.stdout).arrayBuffer(), child.exited]);
  requireValue(code === 0, 'Pinned local Git object unavailable; no network fetch attempted');
  return Buffer.from(bytes);
}
async function verify(): Promise<void> {
  const archive = await readFile(archivePath);
  const record = JSON.parse(await readFile(provenancePath, 'utf8')) as Provenance;
  requireValue(record.schemaVersion === 1 && record.kind === 'immutable-common-package-release', 'Unknown provenance schema');
  requireValue(JSON.stringify(record.source) === JSON.stringify(source), 'Common source pins differ');
  requireValue(record.artifact.name === releaseName && record.artifact.format === 'deterministic-ustar-gzip' && record.artifact.bytes === archive.length && record.artifact.sha256 === hash(archive), 'Archive identity differs');
  const rows = untar(archive);
  requireValue(JSON.stringify([...rows.keys()]) === JSON.stringify(members), 'Archive ordering/member set differs');
  requireValue(record.members.length === members.length && treeHash(rows) === source.packageTreeSha, 'Package Git tree differs');
  for (const [index, path] of members.entries()) {
    const bytes = rows.get(path)!, member = record.members[index]!;
    requireValue(member.archivePath === 'package/' + path && member.sourcePath === source.packagePath + '/' + path && member.gitMode === '100644' && member.gitBlobSha === gitHash('blob', bytes) && member.sha256 === hash(bytes) && member.bytes === bytes.length, 'Source member identity differs');
  }
  requireValue(gzipSync(tar(rows)).equals(archive), 'Archive is not the deterministic reviewed representation');
  const localOntology = await readFile(resolve(root, 'releases/palantirkc-ontology-0.2.0.tgz'));
  requireValue(rows.get('releases/palantirkc-ontology-0.2.0.tgz')!.equals(localOntology), 'Native and common Ontology pins differ');
  const packageManifest = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8'));
  requireValue(packageManifest.dependencies?.['@palantirkc/infrastructure-operations'] === 'file:releases/' + releaseName, 'Native dependency does not use the reviewed immutable release');
  console.log(JSON.stringify({ status: 'passed', verification: 'offline-content-and-recorded-source-pins', commitSha: source.commitSha, packageTreeSha: source.packageTreeSha, archiveSha256: hash(archive), members: rows.size, remoteProvenanceFetch: false }));
}
const [command = 'verify', flag, repo] = Bun.argv.slice(2);
if (command === 'pack') {
  requireValue(flag === '--common-repo' && !!repo && Bun.argv.length === 5, 'Use pack --common-repo <local-repository>');
  requireValue((await git(repo, ['rev-parse', `${source.commitSha}^{tree}`])).toString().trim() === source.repositoryTreeSha, 'Common repository tree differs');
  requireValue((await git(repo, ['rev-parse', `${source.commitSha}:${source.packagePath}`])).toString().trim() === source.packageTreeSha, 'Common package tree differs');
  const rows = new Map<string, Buffer>();
  for (const path of members) rows.set(path, await git(repo, ['show', `${source.commitSha}:${source.packagePath}/${path}`]));
  requireValue(treeHash(rows) === source.packageTreeSha, 'Pinned package is not the exact finite source set');
  const archive = gzipSync(tar(rows));
  const record: Provenance = { schemaVersion: 1, kind: 'immutable-common-package-release', ontologyPackage: { name: '@palantirkc/ontology', version: '0.2.0' }, source,
    artifact: { name: releaseName, format: 'deterministic-ustar-gzip', sha256: hash(archive), bytes: archive.length },
    members: [...rows].map(([path, bytes]) => ({ archivePath: 'package/' + path, sourcePath: source.packagePath + '/' + path, gitMode: '100644', gitBlobSha: gitHash('blob', bytes), sha256: hash(bytes), bytes: bytes.length })) };
  await writeFile(archivePath, archive, { flag: 'wx' });
  await writeFile(provenancePath, JSON.stringify(record, null, 2) + '\n', { flag: 'wx' });
  await verify();
} else {
  requireValue(command === 'verify' && flag === undefined, 'Use verify | pack --common-repo <local-repository>');
  await verify();
}
