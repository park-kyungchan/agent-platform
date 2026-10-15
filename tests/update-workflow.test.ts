import { test, expect } from 'bun:test';
import { runUpdate, type UpdateOperations, type UpdateCheckpoint, type UpdateMode } from '../src/update/workflow';
const oldSha = '1'.repeat(40), target = '2'.repeat(40), movedMain = '3'.repeat(40);
function fixture(mode: UpdateMode = 'backend-only') {
  const calls: string[] = [], saved: UpdateCheckpoint<string>[] = [];
  const operations: UpdateOperations<string> = {
    async inspectBackend() { calls.push('inspect-backend'); return { sha: oldSha, imageId: 'old', healthy: true, verified: true }; },
    async resolveMain() { calls.push('resolve-main'); return target; },
    async inspectDesktop() { calls.push('inspect-desktop'); return { installedSha: target, runningSha: target, running: true, verified: true }; },
    async updateDesktop() { calls.push('update-desktop'); return { installedSha: target, runningSha: target, running: true, verified: true, receiptRef: 'correlated-receipt' }; },
    async prepareBackend(sha) { calls.push('prepare:' + sha); return 'candidate:' + sha; },
    async deployBackend(candidate) { calls.push('deploy:' + candidate); },
    async verifyBackend(sha) { calls.push('verify:' + sha); return { sha, imageId: 'new', healthy: true, verified: true }; },
    async save(state) { saved.push(state); },
  };
  return { calls, saved, operations, request: { id: 'test', mode, apply: true } };
}
test('backend-only never touches Desktop and pins main once', async () => {
  const f = fixture(); await runUpdate(f.request, f.operations);
  expect(f.calls.filter(x => x.includes('desktop'))).toEqual([]);
  expect(f.calls.filter(x => x === 'resolve-main')).toHaveLength(1);
  expect(f.calls).toContain('prepare:' + target);
  expect(f.saved.at(-1)?.status).toBe('complete');
});
test('coordinated pins completed Desktop SHA even when main moves', async () => {
  const f = fixture('coordinated'); f.operations.resolveMain = async () => movedMain;
  const result = await runUpdate(f.request, f.operations);
  expect(result.targetSha).toBe(target);
  expect(result.desktop?.receiptRef).toBe('correlated-receipt');
  expect(f.calls.indexOf('update-desktop')).toBeLessThan(f.calls.indexOf('prepare:' + target));
});
test('requested release mismatch blocks backend preparation even after a successful Desktop update', async () => {
  const f = fixture('coordinated');
  await expect(runUpdate({ ...f.request, expectedReleaseSha: movedMain }, f.operations)).rejects.toThrow('requested release');
  expect(f.calls.some(value => value.startsWith('prepare:') || value.startsWith('deploy:'))).toBe(false);
  expect(f.saved.at(-1)?.targetSha).toBeNull();
});
test('controlled pre-update eligibility never substitutes for clean post-update verification', async () => {
  const f = fixture('coordinated');
  f.operations.inspectDesktop = async () => ({ installedSha: oldSha, runningSha: null, running: false, verified: false, preUpdateEligible: true });
  const completed = await runUpdate({ ...f.request, expectedReleaseSha: target }, { ...f.operations,
    inspectDesktop: async () => f.calls.includes('update-desktop') ? { installedSha: target, runningSha: null, running: false, verified: true } : f.operations.inspectDesktop() });
  expect(completed.status).toBe('complete');
  const failed = fixture('coordinated'); failed.operations.inspectDesktop = f.operations.inspectDesktop;
  failed.operations.updateDesktop = async () => ({ installedSha: target, runningSha: null, running: false, verified: false, preUpdateEligible: true });
  await expect(runUpdate(failed.request, failed.operations)).rejects.toThrow('not verified');
  expect(failed.calls.some(value => value.startsWith('prepare:'))).toBe(false);
});
test('read-only planning never updates or prepares either side', async () => {
  const f = fixture('coordinated'); await runUpdate({ ...f.request, apply: false }, f.operations);
  expect(f.calls).toEqual(['inspect-backend', 'inspect-desktop']);
});
test('already-current verified backend avoids rebuild and restart', async () => {
  const f = fixture(); f.operations.inspectBackend = async () => ({ sha: target, imageId: 'new', healthy: true, verified: true });
  await runUpdate(f.request, f.operations);
  expect(f.calls.some(x => x.startsWith('prepare:') || x.startsWith('deploy:'))).toBe(false);
});
test('an explicit already-current release verifies Desktop and backend without invoking either writer', async () => {
  const f = fixture('coordinated');
  f.operations.inspectBackend = async () => { f.calls.push('inspect-backend'); return { sha: target, imageId: 'new', healthy: true, verified: false }; };
  f.operations.inspectDesktop = async () => { f.calls.push('inspect-desktop'); return { installedSha: target, runningSha: null, running: true, verified: true }; };
  f.operations.verifyExistingDesktop = async () => { f.calls.push('verify-existing-desktop'); return { installedSha: target, runningSha: null, running: true, verified: true }; };
  const result = await runUpdate({ ...f.request, expectedReleaseSha: target }, f.operations);
  expect(result.status).toBe('complete');
  expect(result.targetSource).toBe('verified-existing-desktop-release');
  expect(result.desktop?.receiptRef).toBeUndefined();
  expect(result.desktop?.runningSha).toBeNull();
  expect(f.calls).toContain('verify-existing-desktop');
  expect(f.calls).toContain('verify:' + target);
  expect(f.calls.some(value => value === 'update-desktop' || value.startsWith('prepare:') || value.startsWith('deploy:'))).toBe(false);
});
test('a verified existing Desktop release updates only a backend that is behind', async () => {
  const f = fixture('coordinated');
  f.operations.verifyExistingDesktop = async () => { f.calls.push('verify-existing-desktop'); return { installedSha: target, runningSha: target, running: true, verified: true }; };
  const result = await runUpdate({ ...f.request, expectedReleaseSha: target }, f.operations);
  expect(result.targetSource).toBe('verified-existing-desktop-release');
  expect(f.calls).not.toContain('update-desktop');
  expect(f.calls.indexOf('verify-existing-desktop')).toBeLessThan(f.calls.indexOf('prepare:' + target));
  expect(f.calls).toContain('deploy:candidate:' + target);
});
test('failed native existing-release verification or artifact mismatch blocks all writers', async () => {
  for (const outcome of ['failure', 'wrong-sha', 'unverified'] as const) {
    const f = fixture('coordinated');
    f.operations.verifyExistingDesktop = async () => {
      if (outcome === 'failure') throw new Error('native-installed-integrity-failed');
      return { installedSha: outcome === 'wrong-sha' ? movedMain : target, runningSha: null, running: false, verified: outcome !== 'unverified' };
    };
    await expect(runUpdate({ ...f.request, expectedReleaseSha: target }, f.operations)).rejects.toThrow();
    expect(f.calls.some(value => value === 'update-desktop' || value.startsWith('prepare:') || value.startsWith('deploy:'))).toBe(false);
    expect(f.saved.at(-1)?.targetSha).toBeNull();
  }
});
test('the existing-release fast path preserves main updates and rejects known running mismatches', async () => {
  const main = fixture('coordinated');
  main.operations.verifyExistingDesktop = async () => { throw new Error('Main cannot use explicit release verification'); };
  const result = await runUpdate(main.request, main.operations);
  expect(main.calls).toContain('update-desktop');
  expect(result.targetSource).toBe('completed-desktop-artifact');
  const mismatch = fixture('coordinated');
  mismatch.operations.inspectDesktop = async () => ({ installedSha: target, runningSha: oldSha, running: true, verified: true });
  mismatch.operations.verifyExistingDesktop = async () => { mismatch.calls.push('verify-existing-desktop'); return { installedSha: target, runningSha: target, running: true, verified: true }; };
  await expect(runUpdate({ ...mismatch.request, expectedReleaseSha: target }, mismatch.operations)).rejects.toThrow('not verified');
  expect(mismatch.calls).not.toContain('verify-existing-desktop');
  expect(mismatch.calls).not.toContain('update-desktop');
});
test('resume keeps the target and completed Desktop update across deploy failure', async () => {
  const f = fixture('coordinated'); f.operations.deployBackend = async () => { throw new Error('temporary-deploy-failure'); };
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow('temporary-deploy-failure');
  const checkpoint = f.saved.at(-1)!;
  const resumed = fixture('coordinated'); resumed.operations.resolveMain = async () => movedMain;
  const result = await runUpdate({ ...resumed.request, resume: checkpoint }, resumed.operations);
  expect(result.desktop?.receiptRef).toBe('correlated-receipt');
  expect(resumed.calls).not.toContain('update-desktop');
  expect(resumed.calls).not.toContain('resolve-main');
  expect(resumed.calls).toContain('prepare:' + target);
});
test('resume re-verifies native integrity while retaining the verified-existing release target after a backend failure', async () => {
  const first = fixture('coordinated');
  first.operations.verifyExistingDesktop = async () => ({ installedSha: target, runningSha: target, running: true, verified: true });
  first.operations.deployBackend = async () => { throw new Error('temporary-backend-failure'); };
  await expect(runUpdate({ ...first.request, expectedReleaseSha: target }, first.operations)).rejects.toThrow();
  const second = fixture('coordinated');
  second.operations.verifyExistingDesktop = async () => { second.calls.push('verify-existing-desktop'); return { installedSha: target, runningSha: target, running: true, verified: true }; };
  const result = await runUpdate({ ...second.request, expectedReleaseSha: target, resume: first.saved.at(-1)! }, second.operations);
  expect(result.targetSha).toBe(target);
  expect(result.targetSource).toBe('verified-existing-desktop-release');
  expect(second.calls).not.toContain('update-desktop');
  expect(second.calls).not.toContain('resolve-main');
  expect(second.calls.filter(value => value === 'verify-existing-desktop')).toHaveLength(1);
  expect(second.calls.indexOf('verify-existing-desktop')).toBeLessThan(second.calls.indexOf('prepare:' + target));
});
test('failed, mismatched or unavailable native integrity re-verification on resume blocks backend effects', async () => {
  const first = fixture('coordinated');
  first.operations.verifyExistingDesktop = async () => ({ installedSha: target, runningSha: target, running: true, verified: true });
  first.operations.deployBackend = async () => { throw new Error('temporary-backend-failure'); };
  await expect(runUpdate({ ...first.request, expectedReleaseSha: target }, first.operations)).rejects.toThrow();
  for (const outcome of ['failure', 'wrong-sha', 'unavailable'] as const) {
    const resumed = fixture('coordinated');
    if (outcome !== 'unavailable') resumed.operations.verifyExistingDesktop = async () => {
      if (outcome === 'failure') throw new Error('native-installed-integrity-failed');
      return { installedSha: movedMain, runningSha: movedMain, running: true, verified: true };
    };
    await expect(runUpdate({ ...resumed.request, expectedReleaseSha: target, resume: first.saved.at(-1)! }, resumed.operations)).rejects.toThrow();
    expect(resumed.calls.some(value => value === 'update-desktop' || value.startsWith('prepare:') || value.startsWith('deploy:'))).toBe(false);
    expect(resumed.saved.at(-1)?.targetSha).toBe(target);
    expect(resumed.saved.at(-1)?.targetSource).toBe('verified-existing-desktop-release');
  }
});
test('Desktop failure prevents backend promotion', async () => {
  const f = fixture('coordinated'); f.operations.updateDesktop = async () => { throw new Error('native-update-failed'); };
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow('native-update-failed');
  expect(f.calls.some(x => x.startsWith('prepare:') || x.startsWith('deploy:'))).toBe(false);
});
test('an unverified running artifact blocks coordinated mutation', async () => {
  const f = fixture('coordinated'); f.operations.inspectDesktop = async () => ({ installedSha: target, runningSha: oldSha, running: true, verified: false });
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow('not verified');
  expect(f.calls).not.toContain('update-desktop');
});
test('a target drift on resume is rejected before backend changes', async () => {
  const f = fixture('coordinated'); f.operations.deployBackend = async () => { throw new Error('failed'); };
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow();
  const next = fixture('coordinated'); next.operations.inspectDesktop = async () => ({ installedSha: movedMain, runningSha: movedMain, running: true, verified: true });
  await expect(runUpdate({ ...next.request, resume: f.saved.at(-1)! }, next.operations)).rejects.toThrow('Desktop changed');
  expect(next.calls.some(x => x.startsWith('prepare:'))).toBe(false);
});
test('backend mismatch after deployment remains failed', async () => {
  const f = fixture(); f.operations.verifyBackend = async () => ({ sha: oldSha, imageId: 'old', healthy: true, verified: true });
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow('serving identity');
  expect(f.saved.at(-1)?.acceptance).toBe('not-recorded');
});
test('resume after image deployment still completes owed native standby convergence', async () => {
  const first = fixture(); first.operations.deployBackend = async () => { throw new Error('lost-after-image-deploy'); };
  await expect(runUpdate(first.request, first.operations)).rejects.toThrow();
  const second = fixture(); second.operations.inspectBackend = async () => ({ sha: target, imageId: 'new', healthy: true, verified: false });
  await runUpdate({ ...second.request, resume: first.saved.at(-1)! }, second.operations);
  expect(second.calls).toContain('deploy:candidate:' + target);
  expect(second.calls.some(value => value.startsWith('prepare:'))).toBe(false);
});
test('an interrupted Desktop action never starts another native updater automatically', async () => {
  const first = fixture('coordinated'); first.operations.updateDesktop = async () => { throw new Error('lost-after-native-invocation'); };
  await expect(runUpdate(first.request, first.operations)).rejects.toThrow();
  const second = fixture('coordinated');
  await expect(runUpdate({ ...second.request, resume: first.saved.at(-1)! }, second.operations)).rejects.toThrow('Interrupted Desktop');
  expect(second.calls).not.toContain('update-desktop');
});
test('an existing correlated Desktop result is recovered without rerunning its updater', async () => {
  const first = fixture('coordinated'); first.operations.updateDesktop = async () => { throw new Error('parent-interrupted'); };
  await expect(runUpdate(first.request, first.operations)).rejects.toThrow();
  const second = fixture('coordinated'); second.operations.recoverDesktop = async () => ({ installedSha: target, runningSha: null, running: true, verified: true, receiptRef: 'existing-correlated-result' });
  second.operations.inspectDesktop = async () => ({ installedSha: target, runningSha: null, running: true, verified: true });
  const result = await runUpdate({ ...second.request, resume: first.saved.at(-1)! }, second.operations);
  expect(second.calls).not.toContain('update-desktop');
  expect(result.targetSha).toBe(target);
  expect(result.desktop?.runningSha).toBeNull();
});
test('an interrupted Desktop resume checkpoints running status before native recovery instead of taking the no-op release path', async () => {
  const first = fixture('coordinated'); first.operations.updateDesktop = async () => { throw new Error('parent-interrupted'); };
  await expect(runUpdate({ ...first.request, expectedReleaseSha: target }, first.operations)).rejects.toThrow();
  const second = fixture('coordinated');
  second.operations.verifyExistingDesktop = async () => { throw new Error('Interrupted intent must retain native recovery'); };
  second.operations.recoverDesktop = async () => {
    expect(second.saved.at(-1)?.phase).toBe('desktop-update');
    expect(second.saved.at(-1)?.status).toBe('running');
    expect(second.saved.at(-1)?.failure).toBeNull();
    return { installedSha: target, runningSha: null, running: false, verified: true, receiptRef: 'recovered-original-native-receipt' };
  };
  const result = await runUpdate({ ...second.request, expectedReleaseSha: target, resume: first.saved.at(-1)! }, second.operations);
  expect(result.targetSource).toBe('completed-desktop-artifact');
  expect(second.calls).not.toContain('update-desktop');
});
test('read-only planning preserves an interrupted Desktop recovery phase', async () => {
  const first = fixture('coordinated'); first.operations.updateDesktop = async () => { throw new Error('parent-interrupted'); };
  await expect(runUpdate({ ...first.request, expectedReleaseSha: target }, first.operations)).rejects.toThrow();
  const plan = fixture('coordinated');
  const planned = await runUpdate({ ...plan.request, apply: false, expectedReleaseSha: target, resume: first.saved.at(-1)! }, plan.operations);
  expect(planned.phase).toBe('desktop-update');
  const resumed = fixture('coordinated');
  resumed.operations.verifyExistingDesktop = async () => { throw new Error('An unresolved native intent cannot be skipped'); };
  await expect(runUpdate({ ...resumed.request, expectedReleaseSha: target, resume: planned }, resumed.operations)).rejects.toThrow('Interrupted Desktop');
  expect(resumed.calls).not.toContain('update-desktop');
});
test('current image metadata still requires a fresh native serving verification', async () => {
  const f = fixture(); f.operations.inspectBackend = async () => ({ sha: target, imageId: 'new', healthy: true, verified: false });
  f.operations.verifyBackend = async () => ({ sha: target, imageId: 'new', healthy: true, verified: false });
  await expect(runUpdate(f.request, f.operations)).rejects.toThrow('serving identity');
  expect(f.calls.some(value => value.startsWith('prepare:') || value.startsWith('deploy:'))).toBe(false);
});
