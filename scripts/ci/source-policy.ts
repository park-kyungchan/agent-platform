import { lstat, readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';

const root = resolve(import.meta.dir, '../..');
const exact = new Set([
  '.gitignore', 'AGENTS.md', 'README.md', 'project.json', 'package.json', 'bun.lock', 'tsconfig.json', '.github/workflows/source.yml',
  'scripts/hermes-main-update', 'scripts/ci/infrastructure-release.ts', 'scripts/ci/source-policy.ts', 'scripts/ci/deployment-syntax.ts',
  ...['targets', 'native-agent-workflow', 'update-workflow', 'port-and-lifecycle-policy', 'backend-update-policy', 'desktop-update-target', 'runtime-path-index'].map(name => `architecture/${name}.json`),
  'releases/palantirkc-ontology-0.2.0.tgz', 'releases/palantirkc-ontology-0.2.0.json',
  'releases/palantirkc-infrastructure-operations-0.0.0-2f8ca5da92d1.tgz', 'releases/palantirkc-infrastructure-operations-0.0.0-2f8ca5da92d1.json',
  ...['README.md', '.dockerignore', 'Dockerfile', 'compose.yaml', 'supervisord.conf', 'sshd_config', 'versions.json', 'entrypoint.sh', 'profile.sh', 'healthcheck.sh', 'herdr-codex', 'codex-herdr', 'herdr-runtime.service'].map(name => `deployments/herdr-runtime/${name}`),
  ...['README.md', 'contract.json', '00-herdr-runtime-key-only.conf', 'sshd-policy.sh', 'firewall-policy.sh', 'herdr-network-firewall.service'].map(name => `deployments/herdr-network/${name}`),
  ...['README.md', 'facts.schema.json', 'hermes-backend', 'hermes-backend-shell', 'hermes-backend.lib.sh', 'codex-orchestrator', 'orchestrator-instructions.md', 'continue-harness-work'].map(name => `deployments/herdr-entrypoint/${name}`),
]);
const sourcePath = (path: string) => exact.has(path) || /^src\/(?:cli|(?:domain|adapters|update)\/[a-z][a-z0-9-]*)\.ts$/.test(path) || /^tests\/[a-z][a-z0-9.-]*\.ts$/.test(path);
// Match the common source guard before any source-path allowlist exception.
const namedSshKey = /(?:^|\/)id_(?:rsa|dsa|ecdsa|ed25519)(?:[._-]|$)/;
const forbidden = /(?:^|\/)(?:runs|state|\.local|\.private|\.cache|\.codex|\.hermes|\.ssh|node_modules|home|native-home|runtime-home|sessions|backups|credentials|secrets)(?:\/|$)|(?:^|\/)(?:\.env(?:\..*)?|auth\.json|credentials\.json|authorized_keys|known_hosts|ssh_host_.*|.*\.(?:pem|key|p12|pfx|db|sqlite(?:3)?|log))$/;
// Direct PATH commands need executable Git modes. Docker COPY --chmod and
// install-driven runtime/network sources establish their modes at deployment.
const launchers = new Set(['scripts/hermes-main-update',
  ...['hermes-backend', 'hermes-backend-shell', 'codex-orchestrator', 'continue-harness-work'].map(name => `deployments/herdr-entrypoint/${name}`)]);
const secretMarker = /-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----|\b(?:gh[pousr]_[A-Za-z0-9]{24,}|github_pat_[A-Za-z0-9_]{24,}|sk-(?:proj-)?[A-Za-z0-9_-]{32,}|tskey-auth-[A-Za-z0-9_-]{20,})\b/;
const fixture = Bun.argv.slice(2);
if (!(fixture.length === 0 || fixture.length === 1 && fixture[0] === '--fixture')) throw new Error('Use source-policy.ts [--fixture]');
async function walk(directory: string, prefix = ''): Promise<string[]> {
  const result: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (!prefix && ['node_modules', '.ci-install-cache', '.git'].includes(entry.name)) continue;
    const relative = prefix + entry.name;
    if (entry.isDirectory()) result.push(...await walk(resolve(directory, entry.name), relative + '/'));
    else result.push(relative);
  }
  return result;
}
let paths: string[];
const trackedModes = new Map<string, string>();
if (fixture.length) paths = await walk(root);
else {
  const child = Bun.spawn(['git', '-C', root, 'ls-files', '-z', '--stage'], { stdout: 'pipe', stderr: 'pipe' });
  const [text, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  if (code !== 0) throw new Error('Tracked source inventory unavailable');
  paths = text.split('\0').filter(Boolean).map(row => {
    const match = /^([0-7]{6}) [0-9a-f]{40,64} ([0-3])\t(.+)$/s.exec(row);
    if (!match || match[2] !== '0') throw new Error('Unresolved or malformed tracked source inventory');
    trackedModes.set(match[3]!, match[1]!); return match[3]!;
  });
  if (!paths.length) throw new Error('Tracked source inventory is empty; initial baseline has not been committed/staged');
}
for (const launcher of launchers) if (!paths.includes(launcher)) throw new Error(`Required command launcher missing: ${launcher}`);
for (const path of paths.sort()) {
  if (namedSshKey.test(path)) throw new Error(`Credential path rejected: ${path}`);
  if (forbidden.test(path) || !sourcePath(path) || path.includes('..') || path.includes('\\')) throw new Error(`Source-boundary path rejected: ${path}`);
  const absolute = resolve(root, path), metadata = await lstat(absolute);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 2_000_000) throw new Error(`Source file shape rejected: ${path}`);
  if (launchers.has(path)) {
    if ((metadata.mode & 0o7777) !== 0o755) throw new Error(`Command launcher requires working-tree mode 0755: ${path}`);
    if (!fixture.length && trackedModes.get(path) !== '100755') throw new Error(`Command launcher requires tracked Git mode 100755: ${path}`);
  }
  if (!path.endsWith('.tgz') && secretMarker.test(await readFile(absolute, 'utf8'))) throw new Error(`Credential marker rejected in: ${path}`);
}
console.log(JSON.stringify({ status: 'passed', inventory: fixture.length ? 'explicit-finite-local-fixture' : 'git-tracked-source', files: paths.length, executableLaunchers: launchers.size, gitExecutableModes: fixture.length ? 'not-available-fixture' : 'verified-index-100755', credentialCheck: 'bounded-path-and-known-secret-markers; not a universal-secret-audit', lifecycleEffects: false }));
