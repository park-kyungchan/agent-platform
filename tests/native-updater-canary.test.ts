import { afterEach, expect, test } from 'bun:test';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
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

async function runCanary(httpStatus: string, curlExit = 0, compoundDeploy = false) {
  const root = await mkdtemp(join(tmpdir(), 'native-hermes-canary-'));
  roots.push(root);
  const bin = join(root, 'bin');
  const calls = join(root, 'curl-calls.jsonl');
  const phases = join(root, 'phases.log');
  const script = join(root, 'canary.sh');
  await mkdir(bin);
  await writeFile(join(bin, 'curl'), `#!${process.execPath}
import { appendFileSync } from 'node:fs';
appendFileSync(process.env.CURL_CALLS, JSON.stringify(process.argv.slice(2)) + '\\n');
process.stdout.write(process.env.CURL_STATUS ?? '');
process.exit(Number(process.env.CURL_EXIT));
`, { mode: 0o700 });
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
  await writeFile(script, `set -Eeuo pipefail
${extract('die')}
${extract('need')}
record_phase(){ printf '%s:%s:%s\\n' "$1" "$2" "\${3:-}" >> ${shellQuote(phases)}; }
CANARY_URL=https://example.invalid/hermes/api/status
${extract('gateway_canary')}
${body}
`);
  const child = Bun.spawn(['bash', script], {
    env: { ...process.env, PATH: bin + ':' + process.env.PATH, CURL_CALLS: calls, CURL_STATUS: httpStatus, CURL_EXIT: String(curlExit) },
    stdout: 'pipe', stderr: 'pipe',
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  return {
    stdout, stderr, code,
    phases: await readFile(phases, 'utf8'),
    calls: (await readFile(calls, 'utf8')).trim().split('\n').map(row => JSON.parse(row) as string[]),
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
    '--silent', '--show-error', '--output', '/dev/null', '--write-out', '%{http_code}',
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
