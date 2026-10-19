import { afterEach, expect, test } from 'bun:test';
import { lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const source = await readFile(new URL('../scripts/hermes-main-update', import.meta.url), 'utf8');
const roots: string[] = [];
const shellQuote = (value: string): string => "'" + value.replaceAll("'", "'\\''") + "'";
const extract = (name: string): string => {
  const found = source.match(new RegExp('^' + name + '\\(\\) \\{\\n.*?^\\}', 'ms'));
  if (!found) throw new Error(`Missing native function ${name}`);
  return found[0];
};
afterEach(async () => {
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true })));
});

const canaryScript = (root: string, url: string, body: string): string => `set -Eeuo pipefail
${extract('die')}
${extract('need')}
record_phase(){ printf '%s:%s:%s\\n' "$1" "$2" "\${3:-}" >> ${shellQuote(join(root, 'phases.log'))}; }
trap ${shellQuote("printf 'GLOBAL_EXIT_TRAP_PRESERVED\\n' >> " + shellQuote(join(root, 'exit-trap.log')))} EXIT
CANARY_URL=${shellQuote(url)}
${extract('gateway_canary')}
${body}
`;

type SinkObservation = { path: string; mode: number; uid: number; regular: boolean };
async function readOptional(path: string): Promise<string> {
  return readFile(path, 'utf8').catch((error: NodeJS.ErrnoException) => {
    if (error.code === 'ENOENT') return '';
    throw error;
  });
}

async function runCanary(httpStatus: string, curlExit = 0, compoundDeploy = false, options: { mktempFails?: boolean; cleanupFails?: boolean } = {}) {
  const root = await mkdtemp(join(tmpdir(), 'native-hermes-canary-'));
  roots.push(root);
  const bin = join(root, 'bin');
  const calls = join(root, 'curl-calls.jsonl');
  const phases = join(root, 'phases.log');
  const script = join(root, 'canary.sh');
  const sinks = join(root, 'sinks');
  const observations = join(root, 'sink-observations.jsonl');
  await mkdir(bin);
  await mkdir(sinks);
  await writeFile(join(sinks, 'unrelated.keep'), 'synthetic unrelated file');
  await writeFile(join(bin, 'curl'), `#!${process.execPath}
import { appendFileSync, lstatSync, writeFileSync } from 'node:fs';
const argv = process.argv.slice(2);
appendFileSync(process.env.CURL_CALLS, JSON.stringify(argv) + '\\n');
const path = argv[argv.indexOf('--output') + 1];
const metadata = lstatSync(path);
appendFileSync(process.env.SINK_OBSERVATIONS, JSON.stringify({path, mode: metadata.mode & 0o777, uid: metadata.uid, regular: metadata.isFile()}) + '\\n');
writeFileSync(path, 'SYNTHETIC_RESPONSE_BODY_MUST_NOT_BE_LOGGED');
process.stdout.write(process.env.CURL_STATUS ?? '');
process.exit(Number(process.env.CURL_EXIT));
`, { mode: 0o700 });
  if (options.mktempFails) await writeFile(join(bin, 'mktemp'), '#!/bin/sh\nexit 1\n', { mode: 0o700 });
  if (options.cleanupFails) await writeFile(join(bin, 'rm'), '#!/bin/sh\nexit 1\n', { mode: 0o700 });
  const condition = source.match(/  if ! docker compose[^\n]*\\\n(?:[^\n]*\\\n)*[^\n]*gateway_canary; then/);
  if (!condition) throw new Error('Missing native deploy gates');
  const body = compoundDeploy ? `
ROOT=${shellQuote(root)}; COMPOSE=${shellQuote(join(root, 'unused-compose.yml'))}; SERVICES=(hermes-serve)
docker(){ printf 'DOCKER_PASSED\\n'; }
wait_for_candidate(){ printf 'HEALTH_PASSED\\n'; }
validate_runtime_image(){ printf 'VALIDATION_PASSED\\n'; }
${condition[0]}
  printf 'ROLLBACK_REQUIRED\\n'; exit 1
else
  printf 'DEPLOY_GATES_PASSED\\n'
fi
` : 'gateway_canary';
  await writeFile(script, canaryScript(root, 'https://example.invalid/hermes/api/status', body));
  const child = Bun.spawn(['bash', script], {
    env: { ...process.env, PATH: bin + ':' + process.env.PATH, TMPDIR: sinks, CURL_CALLS: calls, SINK_OBSERVATIONS: observations, CURL_STATUS: httpStatus, CURL_EXIT: String(curlExit) },
    stdout: 'pipe', stderr: 'pipe',
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  const callText = await readOptional(calls);
  const sinkText = await readOptional(observations);
  const sinkObservations = sinkText.trim() ? sinkText.trim().split('\n').map(row => JSON.parse(row) as SinkObservation) : [];
  const remainingSinks = await readdir(sinks);
  expect(await readFile(join(root, 'exit-trap.log'), 'utf8')).toBe('GLOBAL_EXIT_TRAP_PRESERVED\n');
  expect(stdout + stderr + await readFile(phases, 'utf8')).not.toContain('SYNTHETIC_RESPONSE_BODY_MUST_NOT_BE_LOGGED');
  for (const observation of sinkObservations) {
    expect(observation.regular).toBe(true);
    expect(observation.mode).toBe(0o600);
    expect(observation.uid).toBe(process.getuid!());
    expect(observation.path.startsWith(join(sinks, 'hermes-gateway-canary.'))).toBe(true);
  }
  if (!options.cleanupFails) expect(remainingSinks).toEqual(['unrelated.keep']);
  expect(await readFile(join(sinks, 'unrelated.keep'), 'utf8')).toBe('synthetic unrelated file');
  return {
    stdout, stderr, code, sinkObservations, remainingSinks,
    phases: await readFile(phases, 'utf8'),
    calls: callText.trim() ? callText.trim().split('\n').map(row => JSON.parse(row) as string[]) : [],
  };
}

test('silent curl failure records the exit and status and preserves the failing gate', async () => {
  const result = await runCanary('200', 22);
  expect(result.code).toBe(1);
  expect(result.stdout).toContain('Gateway canary starting: https://example.invalid/hermes/api/status');
  expect(result.stdout).toContain('curl_exit=22 http_code=200');
  expect(result.stderr).toContain('Gateway canary curl failed: exit=22 http_code=200');
  expect(result.phases).toContain('gateway-canary:running:');
  expect(result.phases).toContain('gateway-canary:failed:curl_exit=22 http_code=200');
  expect(result.phases).not.toContain('gateway-canary:passed:');
});

test('HTTP rejection records a failed phase even when curl succeeded', async () => {
  const result = await runCanary('503');
  expect(result.code).toBe(1);
  expect(result.stdout).toContain('curl_exit=0 http_code=503');
  expect(result.stderr).toContain('Unexpected gateway canary HTTP status: 503');
  expect(result.phases).toContain('gateway-canary:failed:curl_exit=0 http_code=503; rejected HTTP status');
});

test.each(['200', '301', '302', '401', '403'])('existing accepted HTTP status %s still passes', async httpStatus => {
  const result = await runCanary(httpStatus);
  expect(result.code).toBe(0);
  expect(result.stderr).toBe('');
  expect(result.stdout).toContain(`Gateway canary passed: https://example.invalid/hermes/api/status -> HTTP ${httpStatus}`);
  expect(result.phases).toContain(`gateway-canary:passed:curl_exit=0 http_code=${httpStatus}`);
  expect(result.calls).toEqual([[
    '--silent', '--show-error', '--output', result.sinkObservations[0]!.path, '--write-out', '%{http_code}',
    '--max-time', '10', '--retry', '5', '--retry-delay', '2', '--retry-connrefused',
    'https://example.invalid/hermes/api/status',
  ]]);
});

test('malformed or verbose curl output is bounded and remains a rejection', async () => {
  const output = 'unexpected-output\n' + 'x'.repeat(10_000);
  const result = await runCanary(output);
  expect(result.code).toBe(1);
  expect(result.stdout).toContain('curl_exit=0 http_code=invalid');
  expect(result.stderr).toBe('Unexpected gateway canary HTTP status: invalid\n');
  expect(result.stdout.length).toBeLessThan(300);
  expect(result.phases).not.toContain('unexpected-output');
});

test('all successful native deploy gates reach completion without rollback', async () => {
  const result = await runCanary('200', 0, true);
  expect(result.code).toBe(0);
  expect(result.stdout).toContain('DOCKER_PASSED\nHEALTH_PASSED\nVALIDATION_PASSED\n');
  expect(result.stdout).toContain('DEPLOY_GATES_PASSED');
  expect(result.stdout).not.toContain('ROLLBACK_REQUIRED');
});

test('silent canary failure still selects the existing native rollback branch', async () => {
  const result = await runCanary('000', 7, true);
  expect(result.code).toBe(1);
  expect(result.stdout).toContain('curl_exit=7 http_code=000');
  expect(result.stdout).toContain('ROLLBACK_REQUIRED');
  expect(result.stdout).not.toContain('DEPLOY_GATES_PASSED');
});

test('response sink creation failure prevents curl and selects rollback without replacing the EXIT trap', async () => {
  const result = await runCanary('200', 0, true, { mktempFails: true });
  expect(result.code).toBe(1);
  expect(result.calls).toEqual([]);
  expect(result.sinkObservations).toEqual([]);
  expect(result.phases).toContain('gateway-canary:failed:response sink creation failed; curl not invoked');
  expect(result.phases).not.toContain('gateway-canary:passed:');
  expect(result.stdout).toContain('ROLLBACK_REQUIRED');
});

test('response sink cleanup failure rejects an otherwise successful curl and selects rollback', async () => {
  const result = await runCanary('200', 0, true, { cleanupFails: true });
  expect(result.code).toBe(1);
  expect(result.calls).toHaveLength(1);
  expect(result.sinkObservations).toHaveLength(1);
  expect(result.remainingSinks).toHaveLength(2);
  expect(result.phases).toContain('gateway-canary:failed:response sink cleanup failed; curl_exit=0');
  expect(result.phases).not.toContain('gateway-canary:passed:');
  expect(result.stdout).toContain('ROLLBACK_REQUIRED');
});

test('real curl retries a nonempty 503 response then accepts 200 and removes its private sink', async () => {
  const root = await mkdtemp(join(tmpdir(), 'native-hermes-canary-loopback-'));
  roots.push(root);
  const sinks = join(root, 'sinks');
  await mkdir(sinks);
  await writeFile(join(sinks, 'unrelated.keep'), 'synthetic unrelated file');
  const observedSinks: SinkObservation[] = [];
  let requests = 0;
  const server = Bun.serve({
    hostname: '127.0.0.1', port: 0,
    async fetch() {
      requests++;
      const names = (await readdir(sinks)).filter(name => name !== 'unrelated.keep');
      for (const name of names) {
        const path = join(sinks, name);
        const metadata = await lstat(path);
        observedSinks.push({ path, mode: metadata.mode & 0o777, uid: metadata.uid, regular: metadata.isFile() });
      }
      return new Response(requests === 1 ? 'SYNTHETIC_TRANSIENT_BODY' : 'SYNTHETIC_SUCCESS_BODY', {
        status: requests === 1 ? 503 : 200, headers: { Connection: 'close' },
      });
    },
  });
  try {
    const script = join(root, 'canary.sh');
    const url = `http://127.0.0.1:${server.port}/synthetic-status`;
    await writeFile(script, canaryScript(root, url, 'curl(){ /usr/bin/curl "$@"; }\ngateway_canary'));
    const child = Bun.spawn(['bash', script], {
      env: { ...process.env, TMPDIR: sinks }, stdout: 'pipe', stderr: 'pipe',
    });
    const timeout = setTimeout(() => child.kill(), 8_000);
    try {
      const [stdout, stderr, code] = await Promise.all([
        new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
      ]);
      expect(code).toBe(0);
      expect(stderr).toBe('');
      expect(stdout).toContain('curl_exit=0 http_code=200');
      expect(stdout).not.toContain('SYNTHETIC_TRANSIENT_BODY');
      expect(stdout).not.toContain('SYNTHETIC_SUCCESS_BODY');
      expect(requests).toBe(2);
      expect(observedSinks).toHaveLength(2);
      expect(new Set(observedSinks.map(item => item.path)).size).toBe(1);
      for (const item of observedSinks) {
        expect(item.regular).toBe(true);
        expect(item.mode).toBe(0o600);
        expect(item.uid).toBe(process.getuid!());
      }
      expect(await readdir(sinks)).toEqual(['unrelated.keep']);
      expect(await readFile(join(sinks, 'unrelated.keep'), 'utf8')).toBe('synthetic unrelated file');
      expect(await readFile(join(root, 'exit-trap.log'), 'utf8')).toBe('GLOBAL_EXIT_TRAP_PRESERVED\n');
    } finally {
      clearTimeout(timeout);
      if (child.exitCode === null) child.kill();
    }
  } finally {
    await server.stop(true);
  }
}, 10_000);
