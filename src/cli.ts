#!/usr/bin/env bun
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { join, resolve } from 'node:path';
import { observeHermes } from './adapters/hermes';
import type { HermesTarget } from './adapters/ssh';
import { applyNativeOperation } from './adapters/lifecycle';
import { assertObservation, assertOperationRequest, createDomainObject, evaluateInfrastructure, verificationFromEvaluation } from './domain';
import type { OperationRequestProperties } from './domain';

const root = resolve(import.meta.dir, '..');
const args = Bun.argv.slice(2);
const command = args.shift() ?? 'doctor';
const target: HermesTarget = JSON.parse(await readFile(join(root, 'architecture/targets.json'), 'utf8'));
const name = new Date().toISOString().replaceAll(/[:.]/g, '-') + '-' + command;
const runDirectory = join(root, 'runs', name);
const save = async (filename: string, value: unknown) => {
  await mkdir(runDirectory, { recursive: true });
  await writeFile(join(runDirectory, filename), JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
};
try {
  if (command === 'update') {
    const { runUpdateCli } = await import('./update/cli');
    await runUpdateCli(args, root);
  } else if (command === 'apply') {
    if (!args[0] || args[1] !== '--confirm-exact-target' || !args[2]) throw new Error('Usage: apply <authorized-request.json> --confirm-exact-target <container-id>');
    const request = JSON.parse(await readFile(args[0], 'utf8'));
    assertOperationRequest(request);
    if (request.status !== 'authorized' || !request.authority || request.target.containerId !== args[2]) throw new Error('Explicit authorized request and exact target confirmation required; a schema does not establish actual user permission');
    console.log(JSON.stringify(await applyNativeOperation(target, request, runDirectory), null, 2));
  } else if (command === 'verify') {
    if (!args[0]) throw new Error('Usage: verify <observation.json>');
    const input = JSON.parse(await readFile(args[0], 'utf8'));
    const observation = input.observation ?? input.properties?.observation ?? input;
    assertObservation(observation);
    const evaluation = evaluateInfrastructure(observation);
    console.log(JSON.stringify(evaluation, null, 2));
    process.exitCode = !evaluation.controlPlanePassed ? 2 : evaluation.allAdaptersHealthy ? 0 : 3;
  } else if (['doctor', 'inventory', 'plan'].includes(command)) {
    const observation = await observeHermes(target, command, runDirectory);
    assertObservation(observation);
    const rawPath = join(runDirectory, 'observation.json');
    await save('observation.json', observation);
    const raw = await readFile(rawPath);
    const observationId = `vps-operations:observation:${name}`;
    await save('observation-record.json', createDomainObject('Observation', observationId, { hostId: target.hostId, source: 'live-ssh', observedAtUtc: observation.observedAtUtc, evidence: { locator: rawPath, sha256: createHash('sha256').update(raw).digest('hex'), byteLength: raw.length, mediaType: 'application/json' }, observation }));
    const evaluation = evaluateInfrastructure(observation);
    const verification = verificationFromEvaluation(evaluation, { hostId: target.hostId, observationRef: observationId, verifiedAtUtc: new Date().toISOString(), limitations: ['Fresh read-only evidence does not guarantee behavior after a future container restart', 'No model work or external business messages were submitted'] });
    await save('verification-record.json', createDomainObject('VerificationResult', `vps-operations:verification:${name}`, verification));
    if (command === 'plan') {
      const kind = args[0];
      if (!['start', 'stop', 'restart'].includes(kind ?? '')) throw new Error('Usage: plan <start|stop|restart>');
      const runtime = observation.runtimes.find(row => row.service === target.gatewayService);
      if (!runtime) throw new Error('Target runtime absent');
      const holder = observation.lock.holders.find(row => row.containerId === runtime.containerId);
      const request: OperationRequestProperties = {
        hostId: target.hostId, kind: `native-gateway-${kind}` as OperationRequestProperties['kind'],
        target: { containerId: runtime.containerId, service: target.gatewayService, processIdentity: holder ? { hostPid: holder.pid, localPid: holder.localPid, startTicks: holder.startTicks, pidNamespace: holder.namespace, containerId: runtime.containerId, uid: target.nativeUid } : null, lockIdentity: observation.lock },
        preserveProcesses: [], scope: 'infrastructure', reason: `Proposed exact native root gateway ${kind}`, status: 'proposed', authority: null,
        requestedAtUtc: new Date().toISOString(), idempotencyKey: name,
        constraints: { revalidateProcessIdentity: true, exactLockInodeRequired: kind !== 'start', installedNativeLifecycle: true, broadContainerRestart: false, destructiveDeletion: false, exposeCredentials: false },
      };
      assertOperationRequest(request);
      await save('operation-request.json', createDomainObject('OperationRequest', `vps-operations:operation:${name}`, request));
      await save('operation-payload.json', request);
      console.log(JSON.stringify({ proposedOperation: join(runDirectory, 'operation-payload.json'), executed: false }, null, 2));
    } else {
      console.log(JSON.stringify({ runDirectory, ...(command === 'inventory' ? { inventory: observation.inventory, ports: observation.ports } : { observedAtUtc: observation.observedAtUtc, ...evaluation, ports: observation.ports }) }, null, 2));
      if (command === 'doctor') process.exitCode = !evaluation.controlPlanePassed ? 2 : evaluation.allAdaptersHealthy ? 0 : 3;
    }
  } else throw new Error('Commands: inventory, doctor, verify <observation.json>, plan <start|stop|restart>, apply <authorized-request.json> --confirm-exact-target <id>, update <backend-only|coordinated|verify|stage-overlay>');
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  await save('failure.json', { schemaVersion: 1, recordedAtUtc: new Date().toISOString(), command, message });
  console.error(message);
  process.exitCode = 1;
}
