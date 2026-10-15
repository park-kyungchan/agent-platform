import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';

const root = resolve(import.meta.dir, '../..');
const flags = Bun.argv.slice(2);
if (!(flags.length === 0 || flags.length === 1 && ['--require-compose', '--optional-compose'].includes(flags[0]!))) throw new Error('Use deployment-syntax.ts [--require-compose|--optional-compose]');
const requireCompose = flags[0] !== '--optional-compose';
const scripts = [
  'scripts/hermes-main-update',
  ...['hermes-backend', 'hermes-backend-shell', 'hermes-backend.lib.sh', 'codex-orchestrator', 'continue-harness-work'].map(name => `deployments/herdr-entrypoint/${name}`),
  ...['sshd-policy.sh', 'firewall-policy.sh'].map(name => `deployments/herdr-network/${name}`),
  ...['entrypoint.sh', 'profile.sh', 'healthcheck.sh', 'herdr-codex', 'codex-herdr'].map(name => `deployments/herdr-runtime/${name}`),
];
async function run(args: string[]): Promise<number> {
  const child = Bun.spawn(args, { cwd: root, stdout: 'pipe', stderr: 'pipe' });
  // Errors are intentionally bounded to command identity, without environment/config output.
  await Promise.all([new Response(child.stdout).arrayBuffer(), new Response(child.stderr).arrayBuffer()]);
  return child.exited;
}
let checked = 0;
for (const path of scripts) {
  await stat(resolve(root, path));
  const text = await readFile(resolve(root, path), 'utf8');
  const shell = text.startsWith('#!/bin/sh') ? 'sh' : 'bash';
  if (await run([shell, '-n', path]) !== 0) throw new Error(`Shell syntax failed: ${path}`);
  checked++;
}
const composePath = 'deployments/herdr-runtime/compose.yaml';
let compose = 'skipped-no-source';
if (await Bun.file(resolve(root, composePath)).exists()) {
  if (Bun.which('docker') && await run(['docker', 'compose', 'version']) === 0) {
    if (await run(['docker', 'compose', '-f', composePath, 'config', '--quiet', '--no-interpolate']) !== 0) throw new Error('Native Compose source config validation failed');
    compose = 'passed-native-config-only';
  } else {
    if (requireCompose) throw new Error('Required native Docker Compose plugin unavailable; no lifecycle/install fallback attempted');
    compose = 'unavailable-explicit-local-optional-mode';
  }
}
if (requireCompose && compose === 'skipped-no-source') throw new Error('Required deployment Compose source unavailable');
console.log(JSON.stringify({ status: 'passed', shellFiles: checked, compose, requireCompose, lifecycleEffects: false }));
