import { afterEach, expect, spyOn, test } from 'bun:test';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import * as fileSystem from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { executeRemotePython, type HermesTarget, type RemotePythonLimits } from '../src/adapters/ssh';
import { applyNativeOperation } from '../src/adapters/lifecycle';
import { operation } from './domain.fixtures';

const target: HermesTarget = {
  schemaVersion: 1, hostId: 'fixture', ssh: { user: 'fixture', address: 'invalid.example', hostKeyAlias: 'fixture-key', port: 2222 },
  project: 'fixture', gatewayService: 'fixture', profile: 'default', expectedProfiles: ['default'],
  nativeHome: '/fixture', nativeUid: 10000, nativeGid: 10000, nativePython: '/fixture/python', nativeRepository: '/fixture', gatewaySlot: 'fixture',
};
const nativeSpawn = Bun.spawn;
const roots: string[] = [];
let restore: (() => void) | undefined;
let restoreFileSystem: (() => void) | undefined;
afterEach(async () => {
  restore?.(); restore = undefined;
  restoreFileSystem?.(); restoreFileSystem = undefined;
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true })));
});
const limits: RemotePythonLimits = { timeoutMs: 2000, terminationGraceMs: 20, reapTimeoutMs: 500 };
async function directory() {
  const root = await mkdtemp(join(tmpdir(), 'vps-ssh-inert-')); roots.push(root); return root;
}
function inertTransport(script: string) {
  let argumentsObserved: unknown[] = [];
  const stub = spyOn(Bun, 'spawn').mockImplementation((...args: any[]) => {
    argumentsObserved = args;
    // Tests replace only the SSH executable with an inert, directly owned process.
    return nativeSpawn([process.execPath, '-e', script], { stdin: args[1].stdin, stdout: 'pipe', stderr: 'pipe' });
  });
  restore = () => stub.mockRestore();
  return () => argumentsObserved;
}
async function run(script: string, options: RemotePythonLimits = {}) {
  const observed = inertTransport(script), root = await directory();
  const receipt = await executeRemotePython(target, 'fixture', 'PRIVATE_SOURCE_SENTINEL', root, { ...limits, ...options });
  const path = join(root, 'fixture-execution.json');
  const bytes = await readFile(path, 'utf8');
  expect(JSON.parse(bytes)).toEqual({ ...receipt, state: 'settled' });
  expect((await stat(path)).mode & 0o777).toBe(0o600);
  expect(bytes).not.toContain('PRIVATE_SOURCE_SENTINEL');
  expect(bytes).not.toContain('PRIVATE_STDERR_SENTINEL');
  return { receipt, bytes, observed: observed() };
}

test('preserves successful JSON and UTF-8 stdout while retaining strict native SSH arguments', async () => {
  const stdout = '{"message":"한글 🐱"}\n';
  const { receipt, observed } = await run(`await Bun.stdin.text(); process.stdout.write(${JSON.stringify(stdout)}); process.stderr.write('PRIVATE_STDERR_SENTINEL');`);
  expect(observed[0]).toEqual(['ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'HostKeyAlias=fixture-key', '-o', 'ConnectTimeout=10', '-p', '2222', 'fixture@invalid.example', 'python3 -']);
  expect(receipt.stdout).toBe(stdout);
  expect(receipt.stderr).toBe('');
  expect(receipt.output.stdoutBytesObserved).toBe(Buffer.byteLength(stdout));
  expect(receipt.output.stdoutComplete).toBe(true);
  expect(receipt.output.stderrComplete).toBe(true);
  expect(receipt).toMatchObject({ stage: 'completed', reason: 'completed', exitCode: 0, transportExitCode: 0, ownedChildCreated: true, reaped: true, remoteOutcome: 'unverified', automaticReplayPermitted: false });
  expect(receipt.cleanup).toEqual({ termSent: false, killSent: false, signalFailed: false, streamCancellationRequested: false, streamsCancelled: false });
});

test('nonzero SSH exit persists sanitized failure and no raw failure output', async () => {
  const { receipt, bytes } = await run("process.stdout.write('PRIVATE_FAILURE_STDOUT'); process.stderr.write('PRIVATE_STDERR_SENTINEL'); process.exit(42);");
  expect(receipt).toMatchObject({ stage: 'transport', reason: 'nonzero-exit', exitCode: 42, transportExitCode: 42, reaped: true, remoteOutcome: 'uncertain', automaticReplayPermitted: false });
  expect(receipt.stdout).toBe('');
  expect(bytes).not.toContain('PRIVATE_FAILURE_STDOUT');
  expect(receipt.stderr).toContain('automatic replay prohibited');
});

test.each(['stdout', 'stderr'] as const)('%s overflow terminates only the owned transport and leaves remote outcome uncertain', async stream => {
  const { receipt } = await run(`process.${stream}.write('x'.repeat(100_000)); setInterval(() => {}, 1000);`, { maxStdoutBytes: 1024, maxStderrBytes: 1024 });
  expect(receipt.reason).toBe(`${stream}-limit-exceeded`);
  expect(receipt.stage).toBe('stream');
  expect(receipt.exitCode).not.toBe(0);
  expect(receipt.stdout).toBe('');
  expect(receipt.reaped).toBe(true);
  expect(receipt.cleanup.termSent).toBe(true);
  expect(receipt.cleanup.streamsCancelled).toBe(true);
  expect(receipt.remoteOutcome).toBe('uncertain');
  expect(receipt.automaticReplayPermitted).toBe(false);
});

test('total deadline escalates an inert SIGTERM-resistant child and records actual reap separately', async () => {
  const start = performance.now();
  const { receipt } = await run("process.on('SIGTERM', () => {}); process.stdout.write('ready'); setInterval(() => {}, 1000);", { timeoutMs: 200 });
  expect(performance.now() - start).toBeLessThan(2000);
  expect(receipt).toMatchObject({ reason: 'deadline-exceeded', exitCode: 124, remoteOutcome: 'uncertain', reaped: true });
  expect(receipt.output.stdoutBytesObserved).toBe(5);
  expect(receipt.cleanup).toMatchObject({ termSent: true, killSent: true, streamsCancelled: true });
  expect(receipt.transportSignal).toBe('SIGKILL');
  expect(receipt.stdout).toBe('');
});

test('spawn rejection has a durable sanitized receipt without claiming a child or remote effect', async () => {
  const root = await directory();
  const stub = spyOn(Bun, 'spawn').mockImplementation(() => {
    const pending = JSON.parse(readFileSync(join(root, 'spawn-failure-execution.json'), 'utf8'));
    expect(pending).toMatchObject({ state: 'launch-pending', ownedChildCreated: null, remoteOutcome: 'uncertain', automaticReplayPermitted: false });
    throw new Error('PRIVATE_EXCEPTION_SENTINEL');
  });
  restore = () => stub.mockRestore();
  const receipt = await executeRemotePython(target, 'spawn-failure', '', root, limits);
  expect(receipt).toMatchObject({ stage: 'spawn', reason: 'spawn-failed', ownedChildCreated: false, reaped: false, exitCode: 127, transportExitCode: null, remoteOutcome: 'not-started' });
  expect(await readFile(join(root, 'spawn-failure-execution.json'), 'utf8')).not.toContain('PRIVATE_EXCEPTION_SENTINEL');
});

test('deadline also bounds hanging pipes after the directly owned child has exited', async () => {
  const root = await directory();
  let cancelled = 0;
  const stub = spyOn(Bun, 'spawn').mockImplementation(() => {
    const child = nativeSpawn([process.execPath, '-e', 'process.exit(0)'], { stdout: 'ignore', stderr: 'ignore' });
    const pipe = () => new ReadableStream<Uint8Array>({ cancel() { cancelled++; } });
    return {
      stdout: pipe(), stderr: pipe(), exited: child.exited, kill: child.kill.bind(child),
      get signalCode() { return child.signalCode; },
    } as ReturnType<typeof nativeSpawn>;
  });
  restore = () => stub.mockRestore();
  const receipt = await executeRemotePython(target, 'hanging-pipes', '', root, { ...limits, timeoutMs: 200 });
  expect(receipt).toMatchObject({ reason: 'deadline-exceeded', transportExitCode: 0, reaped: true, exitCode: 124, remoteOutcome: 'uncertain' });
  expect(receipt.cleanup).toEqual({ termSent: false, killSent: false, signalFailed: false, streamCancellationRequested: true, streamsCancelled: true });
  expect(receipt.output.stdoutComplete).toBe(false);
  expect(receipt.output.stderrComplete).toBe(false);
  expect(cancelled).toBe(2);
});

test('unconfirmed reap and pipe cleanup stay bounded and a late exit cannot rewrite the receipt', async () => {
  const root = await directory();
  let reportExit: (code: number) => void;
  const exit = new Promise<number>(resolve => { reportExit = resolve; });
  const stub = spyOn(Bun, 'spawn').mockImplementation(() => ({
    stdout: new ReadableStream({ cancel: () => new Promise(() => {}) }),
    stderr: new ReadableStream({ cancel: () => new Promise(() => {}) }),
    exited: exit, signalCode: null, kill() { throw new Error('PRIVATE_EXCEPTION_SENTINEL'); },
  } as unknown as ReturnType<typeof nativeSpawn>));
  restore = () => stub.mockRestore();
  const started = performance.now();
  const receipt = await executeRemotePython(target, 'unconfirmed-cleanup', '', root, { ...limits, timeoutMs: 10, terminationGraceMs: 10, reapTimeoutMs: 10 });
  expect(performance.now() - started).toBeLessThan(1000);
  expect(receipt).toMatchObject({ reason: 'deadline-exceeded', reaped: false, transportExitCode: null, remoteOutcome: 'uncertain' });
  expect(receipt.cleanup).toMatchObject({ signalFailed: true, streamCancellationRequested: true, streamsCancelled: false });
  reportExit!(0); await Bun.sleep(0);
  expect(receipt.reaped).toBe(false);
  const saved = await readFile(join(root, 'unconfirmed-cleanup-execution.json'), 'utf8');
  expect(JSON.parse(saved)).toEqual({ ...receipt, state: 'settled' });
  expect(saved).not.toContain('PRIVATE_EXCEPTION_SENTINEL');
});

test('invalid execution limits reject before any child creation', async () => {
  const root = await directory();
  const stub = spyOn(Bun, 'spawn'); restore = () => stub.mockRestore();
  for (const timeoutMs of [0, -1, NaN, Infinity, 1.5, 2_147_483_648]) {
    await expect(executeRemotePython(target, 'invalid', '', root, { timeoutMs })).rejects.toThrow('bounded integers');
  }
  expect(stub).not.toHaveBeenCalled();
});

test('an explicit longer foreground budget permits valid output after a shorter diagnostic window', async () => {
  const { receipt } = await run("await Bun.sleep(150); process.stdout.write('{\"completed\":true}');", { timeoutMs: 1000 });
  expect(receipt.exitCode).toBe(0);
  expect(JSON.parse(receipt.stdout)).toEqual({ completed: true });
});

test.each([42, 0])('lifecycle retains an uncertainty fence after exit %s with an unusable native result', async exit => {
  const root = await directory(), request = operation(); request.idempotencyKey = 'inert-' + randomUUID();
  const key = createHash('sha256').update(request.hostId + '\0' + request.idempotencyKey).digest('hex');
  const resultPath = resolve(import.meta.dir, '../state/operations', key + '.json'); roots.push(resultPath);
  const stub = spyOn(Bun, 'spawn').mockImplementation(() => nativeSpawn([process.execPath, '-e', `process.stdout.write('invalid-json'); process.exit(${exit});`], { stdout: 'pipe', stderr: 'pipe' }));
  restore = () => stub.mockRestore();
  const boundTarget = { ...target, hostId: request.hostId, gatewayService: request.target.service };
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow('automatic replay prohibited');
  const result = JSON.parse(await readFile(resultPath, 'utf8'));
  expect(result).toMatchObject({ status: 'uncertain', automaticReplayPermitted: false, verificationStillRequired: true });
  expect(result.details.remoteOutcome).toBe('uncertain');
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow('automatic replay prohibited');
  expect(stub).toHaveBeenCalledTimes(1);
});

test('lifecycle keeps its request fence when execution receipt persistence rejects before launch', async () => {
  const root = await directory(), request = operation(); request.idempotencyKey = 'inert-' + randomUUID();
  const key = createHash('sha256').update(request.hostId + '\0' + request.idempotencyKey).digest('hex');
  const lockPath = resolve(import.meta.dir, '../state/operations', key + '.lock'); roots.push(lockPath);
  // A regular file as run directory makes persistence fail without spawning anything.
  const badDirectory = join(root, 'not-a-directory'); await Bun.write(badDirectory, 'fixture');
  const stub = spyOn(Bun, 'spawn'); restore = () => stub.mockRestore();
  const boundTarget = { ...target, hostId: request.hostId, gatewayService: request.target.service };
  await expect(applyNativeOperation(boundTarget, request, badDirectory)).rejects.toThrow();
  expect((await stat(lockPath)).isFile()).toBe(true);
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow();
  expect(stub).not.toHaveBeenCalled();
});

test.each(['partial-write', 'sync-rejection'] as const)('post-transport %s leaves a recoverable receipt and no partial final result', async fault => {
  const root = await directory(), request = operation(); request.idempotencyKey = 'inert-' + randomUUID();
  const key = createHash('sha256').update(request.hostId + '\0' + request.idempotencyKey).digest('hex');
  const resultPath = resolve(import.meta.dir, '../state/operations', key + '.json');
  const lockPath = resolve(import.meta.dir, '../state/operations', key + '.lock'); roots.push(resultPath, lockPath);
  const receiptPath = join(root, 'native-operation-execution.json');
  const transport = spyOn(Bun, 'spawn').mockImplementation(() => nativeSpawn([process.execPath, '-e', "process.stdout.write('invalid-json'); process.exit(42);"], { stdout: 'pipe', stderr: 'pipe' }));
  restore = () => transport.mockRestore();
  const originalOpen = fileSystem.open;
  let injected = false;
  const filesystem = spyOn(fileSystem, 'open').mockImplementation(async (path: any, ...args: any[]) => {
    const handle = await (originalOpen as any)(path, ...args);
    // Fault the result destination whether publication uses a temp or the old direct path.
    if (String(path) === resultPath || String(path).startsWith(resultPath + '.')) {
      roots.push(String(path)); injected = true;
      expect(JSON.parse(await readFile(receiptPath, 'utf8'))).toMatchObject({ state: 'settled', remoteOutcome: 'uncertain' });
      if (fault === 'partial-write') {
        const write = handle.writeFile.bind(handle);
        handle.writeFile = async () => { await write('{"partial":'); throw new Error('Injected partial write'); };
      } else handle.sync = async () => { throw new Error('Injected sync rejection'); };
    }
    return handle;
  });
  restoreFileSystem = () => filesystem.mockRestore();
  const boundTarget = { ...target, hostId: request.hostId, gatewayService: request.target.service };
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow('Injected');
  expect(injected).toBe(true);
  expect(await Bun.file(resultPath).exists()).toBe(false);
  const fence = JSON.parse(await readFile(lockPath, 'utf8'));
  expect(fence).toMatchObject({ status: 'uncertain', receiptPath, automaticReplayPermitted: false });
  expect(JSON.parse(await readFile(fence.receiptPath, 'utf8'))).toMatchObject({ state: 'settled', reason: 'nonzero-exit', remoteOutcome: 'uncertain' });
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow('original receipt pointer');
  expect(transport).toHaveBeenCalledTimes(1);
});

test('third-state final bytes created after transport admission are preserved and block replay', async () => {
  const root = await directory(), request = operation(); request.idempotencyKey = 'inert-' + randomUUID();
  const key = createHash('sha256').update(request.hostId + '\0' + request.idempotencyKey).digest('hex');
  const resultPath = resolve(import.meta.dir, '../state/operations', key + '.json');
  const lockPath = resolve(import.meta.dir, '../state/operations', key + '.lock'); roots.push(resultPath, lockPath);
  const thirdState = '{"foreign":"preserve these exact bytes"}\n';
  const transport = spyOn(Bun, 'spawn').mockImplementation(() => nativeSpawn([process.execPath, '-e', `await Bun.write(${JSON.stringify(resultPath)}, ${JSON.stringify(thirdState)}); process.exit(42);`], { stdout: 'pipe', stderr: 'pipe' }));
  restore = () => transport.mockRestore();
  const originalOpen = fileSystem.open;
  const filesystem = spyOn(fileSystem, 'open').mockImplementation(async (path: any, ...args: any[]) => {
    if (String(path).startsWith(resultPath + '.')) roots.push(String(path));
    return (originalOpen as any)(path, ...args);
  });
  restoreFileSystem = () => filesystem.mockRestore();
  const boundTarget = { ...target, hostId: request.hostId, gatewayService: request.target.service };
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow();
  expect(await readFile(resultPath, 'utf8')).toBe(thirdState);
  expect(JSON.parse(await readFile(lockPath, 'utf8')).status).toBe('uncertain');
  await expect(applyNativeOperation(boundTarget, request, root)).rejects.toThrow('different request');
  expect(transport).toHaveBeenCalledTimes(1);
  expect(await readFile(resultPath, 'utf8')).toBe(thirdState);
});
