import { test, expect } from 'bun:test';
import { desktopIdentity, parseUpdateArguments } from '../src/update/cli';
import type { DesktopSnapshot } from '../src/adapters/desktop-update';
test('backend-only is read-only by default and has no Desktop parameters', () => {
  expect(parseUpdateArguments(['backend-only']).options.apply).toBe(false);
  expect(() => parseUpdateArguments(['backend-only', '--desktop-cdp', 'http://localhost:9222'])).toThrow('coordinated');
});
test('native effects retain their current operation authority', () => {
  expect(() => parseUpdateArguments(['backend-only', '--apply'])).toThrow('--authorization');
  expect(parseUpdateArguments(['backend-only', '--apply', '--authorization', 'User requested backend update']).options.apply).toBe(true);
});
test('coordinated cannot pass an unsupported target SHA to Desktop', () => {
  expect(() => parseUpdateArguments(['coordinated', '--sha', 'a'.repeat(40)])).toThrow('completion determines');
});
test('ambiguous actions and incomplete native overlay staging are rejected', () => {
  expect(() => parseUpdateArguments(['backend-only', '--plan', '--apply', '--authorization', 'request'])).toThrow('Choose one');
  expect(() => parseUpdateArguments(['stage-overlay', '--sha', 'a'.repeat(40), '--patch', 'file'])).toThrow('--apply');
});
test('release targets are explicit stable tags in either update mode', () => {
  expect(parseUpdateArguments(['coordinated', '--release', 'v0.22.0']).options.release).toBe('v0.22.0');
  expect(parseUpdateArguments(['backend-only', '--release', 'v0.22.0']).options.release).toBe('v0.22.0');
  for (const tag of ['latest', 'main', '0.22.0', 'v0.22.0-rc.1', 'v00.22.0']) {
    expect(() => parseUpdateArguments(['coordinated', '--release', tag])).toThrow('stable release tag');
  }
  expect(() => parseUpdateArguments(['coordinated', '--release'])).toThrow('incomplete');
});
test('release targets cannot conflict with SHA or leak into non-update commands', () => {
  expect(() => parseUpdateArguments(['backend-only', '--release', 'v0.22.0', '--sha', 'a'.repeat(40)])).toThrow('Choose one target');
  expect(() => parseUpdateArguments(['verify', '--sha', 'a'.repeat(40), '--release', 'v0.22.0'])).toThrow('only to an update mode');
  expect(() => parseUpdateArguments(['stage-overlay', '--sha', 'a'.repeat(40), '--release', 'v0.22.0'])).toThrow('only to an update mode');
});
test('Desktop identity preserves unknown bundle settlement and known running mismatches', () => {
  const sha = 'a'.repeat(40);
  const snapshot = { nativeHomeVerified: true, checkoutDirty: false, checkoutSha: sha, checkoutStamp: { commit: sha, dirty: false },
    installedArtifact: { commit: sha, dirty: false, source: 'local' }, processes: [{ pid: 1 }],
    runningArtifact: { commit: sha, bundleSwapPending: null, bundleOutOfSync: null } } as DesktopSnapshot;
  expect(desktopIdentity(snapshot).runningSha).toBeNull();
  expect(desktopIdentity(snapshot).verified).toBe(true);
  expect(desktopIdentity({ ...snapshot, runningArtifact: { commit: 'b'.repeat(40), bundleSwapPending: null, bundleOutOfSync: null } }).runningSha).toBe('b'.repeat(40));
  expect(desktopIdentity({ ...snapshot, runningArtifact: { commit: sha, bundleSwapPending: false, bundleOutOfSync: false } }).runningSha).toBe(sha);
  expect(desktopIdentity({ ...snapshot, runningArtifact: { commit: sha, bundleSwapPending: true, bundleOutOfSync: false } }).verified).toBe(false);
});
