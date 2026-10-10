import { test, expect, spyOn } from 'bun:test';
import { createHash } from 'node:crypto';
import * as filesystem from 'node:fs/promises';
import { constants } from 'node:fs';
import { mkdtemp, mkdir, readFile, writeFile, symlink, rm, rename, appendFile, readdir, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { HandoffPublicationUncertainError, HandoffReceiveUncertainError, initializeCanonicalHandoff, mutateCanonicalHandoff, packHandoff, readCanonicalHandoff, receiveHandoff, validateHandoff, type HandoffBundle } from '../src/workspace/handoff';

async function fixture(run: (root: string, bundle: HandoffBundle) => Promise<void>) {
  const root = await mkdtemp(join(tmpdir(), 'handoff-'));
  try {
    await writeFile(join(root, 'pending.ts'), 'export const pending = true;\n');
    await writeFile(join(root, 'passed.json'), '{"passed":true}\n');
    const bundle = await packHandoff(root, ['pending.ts', 'passed.json'], { issueId: 'handoff-implementation', sourceRevision: 'saved-1', completedEffectIds: ['module-authored'], checks: [{id:'old-check',status:'passed',evidencePaths:['passed.json']},{id:'new-check',status:'unexecuted',evidencePaths:[]}], nextAction: { id: 'run-focused-check', action: 'Verify actual saved code', path: 'pending.ts' } });
    await run(root, bundle);
  } finally { await rm(root, {recursive:true,force:true}); }
}
test('byte transport preserves actual unfinished state and verifies independent readback', () => fixture(async (root,bundle) => {
  const result = await receiveHandoff(root,'receiver',bundle);
  expect(result.files).toBe(2); expect(result.completedEffectIds).toEqual(['module-authored']);
  expect(await readFile(join(result.root,'pending.ts'),'utf8')).toBe('export const pending = true;\n');
  expect(bundle.checks[1]!.status).toBe('unexecuted');
  await expect(receiveHandoff(root,'receiver',bundle)).rejects.toThrow();
}));
test('transport refuses corruption, traversal, duplicate sources and completed replay', () => fixture(async (_root,bundle) => {
  const bad = structuredClone(bundle); bad.files[0]!.base64 = Buffer.from('wrong').toString('base64');
  expect(()=>validateHandoff(bad)).toThrow('SOURCE_INTEGRITY_MISMATCH');
  const unsafe = structuredClone(bundle); unsafe.files[0]!.path='../outside'; expect(()=>validateHandoff(unsafe)).toThrow('UNSAFE_RELATIVE_PATH');
  const duplicate = structuredClone(bundle); duplicate.files.push(duplicate.files[0]!); expect(()=>validateHandoff(duplicate)).toThrow('DUPLICATE_SOURCE');
  const replay = structuredClone(bundle); replay.nextAction.id='module-authored'; expect(()=>validateHandoff(replay)).toThrow('INVALID_NEXT_ACTION');
  expect(()=>validateHandoff(bundle,1)).toThrow('TRANSFER_TOO_LARGE');
}));
test('source symlink cannot escape the admitted root', () => fixture(async (root,bundle) => {
  await symlink('/etc/hostname',join(root,'outside'));
  await expect(packHandoff(root,['outside'],bundle)).rejects.toThrow('UNSAFE_SOURCE');
}));
test('authoritative writer transfer rejects prior writer and preserves completed identity', () => fixture(async(root,bundle)=>{
  const store=join(root,'store');await mkdir(store);
  const first=await initializeCanonicalHandoff(store,'hermes-source',bundle);
  const moved=await mutateCanonicalHandoff(store,first,'handoff-1',{kind:'transfer-writer',toWriterId:'codex-receiver'});
  expect(moved.epoch).toBe(2);expect(moved.revision).toBe(2);
  const before=await readFile(join(store,'current.json'),'utf8');
  await expect(mutateCanonicalHandoff(store,first,'stale-attempt',{kind:'commit',bundle})).rejects.toThrow('STALE_WRITER');
  expect(await readFile(join(store,'current.json'),'utf8')).toBe(before);
  const loses=structuredClone(bundle);loses.completedEffectIds=[];
  await expect(mutateCanonicalHandoff(store,moved,'history-loss',{kind:'commit',bundle:loses})).rejects.toThrow('COMPLETED_EFFECT_HISTORY_LOST');
  const done=await mutateCanonicalHandoff(store,moved,'check-completed',{kind:'commit',bundle});
  await expect(mutateCanonicalHandoff(store,done,'check-completed',{kind:'commit',bundle})).rejects.toThrow('WRITE_ALREADY_COMPLETED_OR_INVALID');
  expect((await readCanonicalHandoff(store)).completedWriteIds).toEqual(['handoff-1','check-completed']);
}));
test('concurrent writers cannot both publish the same canonical revision',()=>fixture(async(root,bundle)=>{
  const store=join(root,'store');await mkdir(store);const initial=await initializeCanonicalHandoff(store,'owner',bundle);
  const results=await Promise.allSettled([mutateCanonicalHandoff(store,initial,'a',{kind:'commit',bundle}),mutateCanonicalHandoff(store,initial,'b',{kind:'commit',bundle})]);
  expect(results.filter(x=>x.status==='fulfilled')).toHaveLength(1);
  expect((await readCanonicalHandoff(store)).revision).toBe(2);
}));

test('packing admits all paths, count and aggregate bytes before any source open', () => fixture(async (root, bundle) => {
  await writeFile(join(root, 'large'), Buffer.alloc(4 * 1024 * 1024));
  await writeFile(join(root, 'extra'), 'x');
  const mock = spyOn(filesystem, 'open');
  try {
    await expect(packHandoff(root, Array(1025).fill('missing'), bundle)).rejects.toThrow('INVALID_HANDOFF');
    await expect(packHandoff(root, ['pending.ts', '../outside'], bundle)).rejects.toThrow('UNSAFE_RELATIVE_PATH');
    await expect(packHandoff(root, ['pending.ts', 'pending.ts'], bundle)).rejects.toThrow('DUPLICATE_SOURCE');
    await expect(packHandoff(root, ['large', 'extra'], bundle)).rejects.toThrow('TRANSFER_TOO_LARGE');
    await appendFile(join(root, 'large'), 'x');
    await expect(packHandoff(root, ['large'], bundle)).rejects.toThrow('TRANSFER_TOO_LARGE');
    expect(mock).not.toHaveBeenCalled();
  } finally { mock.mockRestore(); }
}));

test.each(['symlink-before-open', 'replacement-before-open', 'replacement-after-open', 'parent-symlink-before-open'] as const)(
  'controlled source race rejects %s before reading substituted content', race => fixture(async (root, bundle) => {
    const directory = join(root, 'selected'), outside = join(root, 'outside');
    await mkdir(directory); await mkdir(outside);
    const path = join(directory, 'pending.ts');
    await writeFile(path, 'ORIGINAL'); await writeFile(join(outside, 'pending.ts'), 'OUTSIDE_SENTINEL');
    const originalOpen = filesystem.open; let contentReads = 0, injected = false;
    const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
      if (String(args[0]) !== path || injected) return originalOpen(...args);
      injected = true;
      const handle = race === 'replacement-after-open' ? await originalOpen(...args) : undefined;
      if (race === 'parent-symlink-before-open') {
        await rename(directory, directory + '-saved'); await symlink(outside, directory);
      } else {
        await rename(path, path + '-saved');
        if (race === 'symlink-before-open') await symlink(join(outside, 'pending.ts'), path);
        else await writeFile(path, 'REPLACEMENT');
      }
      const opened = handle ?? await originalOpen(...args), read = opened.read.bind(opened);
      opened.read = ((...readArgs: any[]) => { contentReads++; return Reflect.apply(read, opened, readArgs); }) as typeof opened.read;
      return opened;
    });
    try {
      await expect(packHandoff(root, ['selected/pending.ts'], { ...bundle, nextAction: { ...bundle.nextAction, path: 'selected/pending.ts' } })).rejects.toThrow();
      expect(injected).toBe(true); expect(contentReads).toBe(0);
    } finally { mock.mockRestore(); }
  })
);

test('growth after fstat is bounded and rejected without readFile allocation', () => fixture(async (root, bundle) => {
  const path = join(root, 'pending.ts'), initialSize = (await stat(path)).size;
  const originalOpen = filesystem.open; let requestedBytes = 0, injected = false;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    const handle = await originalOpen(...args);
    if (String(args[0]) === path && !injected) {
      injected = true;
      const read = handle.read.bind(handle);
      handle.read = (async (...readArgs: any[]) => {
        requestedBytes += readArgs[2];
        await appendFile(path, Buffer.alloc(4 * 1024 * 1024));
        return Reflect.apply(read, handle, readArgs);
      }) as typeof handle.read;
    }
    return handle;
  });
  try {
    await expect(packHandoff(root, ['pending.ts', 'passed.json'], bundle)).rejects.toThrow('SOURCE_CHANGED_DURING_PACK');
    expect(requestedBytes).toBe(initialSize + 1);
  } finally { mock.mockRestore(); }
}));

test('final descriptor scan rejects an earlier source changed while another is collected', () => fixture(async (root, bundle) => {
  const originalOpen = filesystem.open; let injected = false;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    if (String(args[0]) === join(root, 'passed.json') && !injected) {
      injected = true; await writeFile(join(root, 'pending.ts'), 'changed');
    }
    return originalOpen(...args);
  });
  try { await expect(packHandoff(root, ['pending.ts', 'passed.json'], bundle)).rejects.toThrow('SOURCE_CHANGED_DURING_PACK'); }
  finally { mock.mockRestore(); }
}));

test.each(['initialize', 'mutate'] as const)('%s flushes staged bytes before atomic publication and parent directory before readback', kind => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  const initial = kind === 'mutate' ? await initializeCanonicalHandoff(store, 'owner', bundle) : undefined;
  const events: string[] = [], originalOpen = filesystem.open, originalLink = filesystem.link, originalRename = filesystem.rename, originalRead = filesystem.readFile;
  const mocks = [
    spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
      const handle = await originalOpen(...args), sync = handle.sync.bind(handle);
      handle.sync = async () => { events.push(String(args[0]) === store ? 'parent-sync' : 'file-sync'); await sync(); };
      return handle;
    }),
    spyOn(filesystem, 'link').mockImplementation(async (...args: Parameters<typeof filesystem.link>) => { events.push('publish'); return originalLink(...args); }),
    spyOn(filesystem, 'rename').mockImplementation(async (...args: Parameters<typeof filesystem.rename>) => { events.push('publish'); return originalRename(...args); }),
    spyOn(filesystem, 'readFile').mockImplementation((...args: any[]) => { events.push('readback'); return Reflect.apply(originalRead, filesystem, args); })
  ];
  try {
    if (initial) await mutateCanonicalHandoff(store, initial, 'durable', { kind: 'commit', bundle });
    else await initializeCanonicalHandoff(store, 'owner', bundle);
    expect(events.slice(-4)).toEqual(['file-sync', 'publish', 'parent-sync', 'readback']);
    expect((await stat(join(store, 'current.json'))).mode & 0o777).toBe(0o600);
    expect(await readdir(store)).toEqual(['current.json']);
  } finally { for (const mock of mocks) mock.mockRestore(); }
}));

test('staged file sync failure leaves canonical bytes unchanged and releases the lock', () => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  const initial = await initializeCanonicalHandoff(store, 'owner', bundle), before = await readFile(join(store, 'current.json'));
  const originalOpen = filesystem.open;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    const handle = await originalOpen(...args);
    if (String(args[0]).includes('/.current-')) handle.sync = async () => { throw Error('INJECTED_FILE_SYNC_FAILURE'); };
    return handle;
  });
  try { await expect(mutateCanonicalHandoff(store, initial, 'failed', { kind: 'commit', bundle })).rejects.toThrow('INJECTED_FILE_SYNC_FAILURE'); }
  finally { mock.mockRestore(); }
  expect(await readFile(join(store, 'current.json'))).toEqual(before);
  expect(await readdir(store)).toEqual(['current.json']);
  expect((await mutateCanonicalHandoff(store, initial, 'after-failure', { kind: 'commit', bundle })).revision).toBe(2);
}));

test.each(['initialize', 'mutate'] as const)('%s parent sync failure reports uncertain publication and preserves the writer lock', kind => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  const initial = kind === 'mutate' ? await initializeCanonicalHandoff(store, 'owner', bundle) : undefined;
  const originalOpen = filesystem.open;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    const handle = await originalOpen(...args);
    if (String(args[0]) === store) handle.sync = async () => { throw Error('INJECTED_PARENT_SYNC_FAILURE'); };
    return handle;
  });
  let error: unknown;
  try {
    if (initial) await mutateCanonicalHandoff(store, initial, 'uncertain', { kind: 'commit', bundle });
    else await initializeCanonicalHandoff(store, 'owner', bundle);
  } catch (caught) { error = caught; } finally { mock.mockRestore(); }
  expect(error).toBeInstanceOf(HandoffPublicationUncertainError);
  expect((error as HandoffPublicationUncertainError).stage).toBe('directory-sync');
  const observed = await readCanonicalHandoff(store);
  expect(observed.revision).toBe(initial ? 2 : 1);
  expect((error as HandoffPublicationUncertainError).expected.revision).toBe(observed.revision);
  expect(await readdir(store)).toContain('.writer-lock');
  await expect(mutateCanonicalHandoff(store, observed, 'blind-retry', { kind: 'commit', bundle })).rejects.toThrow('WRITER_BUSY_OR_UNCERTAIN');
}));

test('post-publication readback failure is uncertain and cannot silently release the lock', () => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  const initial = await initializeCanonicalHandoff(store, 'owner', bundle), originalRead = filesystem.readFile;
  let reads = 0;
  const mock = spyOn(filesystem, 'readFile').mockImplementation((...args: any[]) => {
    if (String(args[0]) === join(store, 'current.json') && ++reads === 2) return Promise.reject(Error('INJECTED_READBACK_FAILURE'));
    return Reflect.apply(originalRead, filesystem, args);
  });
  try { await expect(mutateCanonicalHandoff(store, initial, 'visible', { kind: 'commit', bundle })).rejects.toThrow('CANONICAL_PUBLICATION_UNCERTAIN:readback'); }
  finally { mock.mockRestore(); }
  expect((await readCanonicalHandoff(store)).completedWriteIds).toEqual(['visible']);
  expect(await readdir(store)).toContain('.writer-lock');
}));

test('initialization never overwrites existing canonical bytes', () => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  await initializeCanonicalHandoff(store, 'owner', bundle);
  const before = await readFile(join(store, 'current.json'));
  await expect(initializeCanonicalHandoff(store, 'other', bundle)).rejects.toThrow();
  expect(await readFile(join(store, 'current.json'))).toEqual(before);
  expect(await readdir(store)).toEqual(['current.json']);
}));

test('lock release failure after durable publication reports the published token', () => fixture(async (root, bundle) => {
  const store = join(root, 'store'); await mkdir(store);
  const initial = await initializeCanonicalHandoff(store, 'owner', bundle), originalRmdir = filesystem.rmdir;
  const mock = spyOn(filesystem, 'rmdir').mockImplementation((...args: Parameters<typeof filesystem.rmdir>) => {
    if (String(args[0]) === join(store, '.writer-lock')) return Promise.reject(Error('INJECTED_LOCK_RELEASE_FAILURE'));
    return originalRmdir(...args);
  });
  try { await expect(mutateCanonicalHandoff(store, initial, 'published', { kind: 'commit', bundle })).rejects.toThrow('CANONICAL_PUBLICATION_UNCERTAIN:lock-release'); }
  finally { mock.mockRestore(); }
  const observed = await readCanonicalHandoff(store);
  expect(observed.completedWriteIds).toEqual(['published']);
  await expect(mutateCanonicalHandoff(store, observed, 'retry', { kind: 'commit', bundle })).rejects.toThrow('WRITER_BUSY_OR_UNCERTAIN');
}));

function nestedBundle(bundle: HandoffBundle) {
  const nested = structuredClone(bundle);
  nested.files[0]!.path = 'nested/deep/pending.ts';
  nested.nextAction.path = nested.files[0]!.path;
  return nested;
}

test.each(['nested', 'receiver-root', 'parent', 'leaf'] as const)(
  'receiver descriptor anchors contain controlled %s substitution at exclusive file open', race => fixture(async (root, bundle) => {
    const parent = join(root, 'admitted'), outside = join(root, 'outside');
    await mkdir(parent); await mkdir(outside);
    await writeFile(join(outside, 'sentinel'), 'UNCHANGED');
    if (race === 'leaf') await writeFile(join(outside, 'pending.ts'), 'OUTSIDE_SENTINEL');
    const nested = nestedBundle(bundle), originalOpen = filesystem.open;
    let injected = false, error: unknown;
    const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
      if (!injected && String(args[0]).endsWith('/pending.ts') && (Number(args[1]) & constants.O_CREAT)) {
        injected = true;
        const path = race === 'nested' ? join(parent, 'receiver/nested/deep')
          : race === 'receiver-root' ? join(parent, 'receiver') : parent;
        if (race === 'leaf') await symlink(join(outside, 'pending.ts'), join(parent, 'receiver/nested/deep/pending.ts'));
        else { await rename(path, path + '-saved'); await symlink(outside, path); }
      }
      return originalOpen(...args);
    });
    try { await receiveHandoff(parent, 'receiver', nested); } catch (caught) { error = caught; } finally { mock.mockRestore(); }
    expect(injected).toBe(true); expect(error).toBeInstanceOf(HandoffReceiveUncertainError);
    expect((error as HandoffReceiveUncertainError).root).toBe(join(parent, 'receiver'));
    expect((error as HandoffReceiveUncertainError).bundleSha256).toBe(validateHandoff(nested).bundleSha256);
    expect((await readdir(outside)).sort()).toEqual(race === 'leaf' ? ['pending.ts', 'sentinel'] : ['sentinel']);
    if (race === 'leaf') expect(await readFile(join(outside, 'pending.ts'), 'utf8')).toBe('OUTSIDE_SENTINEL');
    else await expect(readFile(join(outside, 'pending.ts'))).rejects.toThrow('ENOENT');
    expect(await readFile(join(outside, 'sentinel'), 'utf8')).toBe('UNCHANGED');
  })
);

test('receiver syncs every file then directories bottom-up through publication parent before descriptor readback', () => fixture(async (root, bundle) => {
  const nested = nestedBundle(bundle), events: string[] = [], originalOpen = filesystem.open;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    const handle = await originalOpen(...args), path = await filesystem.realpath(`/proc/self/fd/${handle.fd}`);
    const label = path === root ? 'parent' : path.slice(join(root, 'receiver').length) || 'root';
    const sync = handle.sync.bind(handle), read = handle.read.bind(handle);
    handle.sync = async () => { events.push(`sync:${label}`); await sync(); };
    handle.read = ((...readArgs: any[]) => { events.push(`read:${label}`); return Reflect.apply(read, handle, readArgs); }) as typeof handle.read;
    return handle;
  });
  let result: Awaited<ReturnType<typeof receiveHandoff>>;
  try { result = await receiveHandoff(root, 'receiver', nested); } finally { mock.mockRestore(); }
  expect(events.slice(0, 6)).toEqual(['sync:/nested/deep/pending.ts', 'sync:/passed.json', 'sync:/nested/deep', 'sync:/nested', 'sync:root', 'sync:parent']);
  expect(events.slice(6).every(event => event.startsWith('read:'))).toBe(true);
  expect(result!.completedEffectIds).toEqual(bundle.completedEffectIds);
  expect(result!.nextAction).toEqual(nested.nextAction);
  expect(result!.bundleSha256).toBe(validateHandoff(nested).bundleSha256);
  expect((await stat(result!.root)).mode & 0o777).toBe(0o700);
  expect((await stat(join(result!.root, nested.files[0]!.path))).mode & 0o777).toBe(0o600);
}));

test.each(['file', 'nested', 'root', 'parent', 'readback', 'close'] as const)(
  'receiver %s failure is explicitly uncertain, preserves partial bytes and refuses blind overwrite', failure => fixture(async (root, bundle) => {
    const nested = nestedBundle(bundle), originalOpen = filesystem.open;
    let injected = false, error: unknown;
    const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
      const path = String(args[0]);
      if (failure === 'readback' && path.endsWith('/pending.ts') && !(Number(args[1]) & constants.O_CREAT)) {
        injected = true; throw Error('INJECTED_RECEIVER_READBACK_FAILURE');
      }
      const handle = await originalOpen(...args), publicPath = await filesystem.realpath(`/proc/self/fd/${handle.fd}`);
      const target = failure === 'file' ? join(root, 'receiver/nested/deep/pending.ts')
        : failure === 'nested' ? join(root, 'receiver/nested/deep')
        : failure === 'root' ? join(root, 'receiver') : root;
      if (failure === 'close' && publicPath === root) {
        const close = handle.close.bind(handle);
        handle.close = async () => { await close(); injected = true; throw Error('INJECTED_RECEIVER_CLOSE_FAILURE'); };
      } else if (failure !== 'readback' && publicPath === target) {
        handle.sync = async () => { injected = true; throw Error('INJECTED_RECEIVER_SYNC_FAILURE'); };
      }
      return handle;
    });
    try { await receiveHandoff(root, 'receiver', nested); } catch (caught) { error = caught; } finally { mock.mockRestore(); }
    expect(injected).toBe(true); expect(error).toBeInstanceOf(HandoffReceiveUncertainError);
    expect((error as HandoffReceiveUncertainError).stage).toBe(failure === 'file' ? 'file-sync' : failure === 'readback' ? 'readback' : failure === 'close' ? 'close' : 'directory-sync');
    expect((error as HandoffReceiveUncertainError).bundleSha256).toBe(validateHandoff(nested).bundleSha256);
    const bytes = await readFile(join(root, 'receiver/nested/deep/pending.ts'));
    expect(bytes.toString('base64')).toBe(nested.files[0]!.base64);
    await expect(receiveHandoff(root, 'receiver', nested)).rejects.toThrow('EEXIST');
    expect(await readFile(join(root, 'receiver/nested/deep/pending.ts'))).toEqual(bytes);
  })
);

test('receiver rejects same-byte leaf replacement before descriptor readback without reading the replacement', () => fixture(async (root, bundle) => {
  const nested = nestedBundle(bundle), path = join(root, 'receiver/nested/deep/pending.ts'), originalOpen = filesystem.open;
  let injected = false, contentReads = 0;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    if (String(args[0]).endsWith('/pending.ts') && !(Number(args[1]) & constants.O_CREAT)) {
      injected = true; await rename(path, path + '-saved'); await writeFile(path, Buffer.from(nested.files[0]!.base64, 'base64'));
      const handle = await originalOpen(...args), read = handle.read.bind(handle);
      handle.read = ((...readArgs: any[]) => { contentReads++; return Reflect.apply(read, handle, readArgs); }) as typeof handle.read;
      return handle;
    }
    return originalOpen(...args);
  });
  try { await expect(receiveHandoff(root, 'receiver', nested)).rejects.toThrow('RECEIVER_PUBLICATION_UNCERTAIN:readback'); }
  finally { mock.mockRestore(); }
  expect(injected).toBe(true); expect(contentReads).toBe(0);
}));

test('receiver snapshots validated paths and completed metadata before the first await', () => fixture(async (root, bundle) => {
  const expected = structuredClone(bundle), originalOpen = filesystem.open; let injected = false;
  const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
    if (!injected && String(args[0]) === root) {
      injected = true; bundle.files[0]!.path = '../outside'; bundle.completedEffectIds.push('NOT_ADMITTED'); bundle.nextAction.id = 'NOT_ADMITTED';
    }
    return originalOpen(...args);
  });
  let result: Awaited<ReturnType<typeof receiveHandoff>>;
  try { result = await receiveHandoff(root, 'receiver', bundle); } finally { mock.mockRestore(); }
  expect(injected).toBe(true); expect(result!.bundleSha256).toBe(validateHandoff(expected).bundleSha256);
  expect(result!.completedEffectIds).toEqual(expected.completedEffectIds); expect(result!.nextAction).toEqual(expected.nextAction);
  expect((await readFile(join(result!.root, 'pending.ts'))).toString('base64')).toBe(expected.files[0]!.base64);
}));

test('receiver preserves an existing empty destination and an existing symlink without adopting either', () => fixture(async (root, bundle) => {
  await mkdir(join(root, 'existing')); await mkdir(join(root, 'outside')); await symlink(join(root, 'outside'), join(root, 'linked'));
  await expect(receiveHandoff(root, 'existing', bundle)).rejects.toThrow('EEXIST');
  await expect(receiveHandoff(root, 'linked', bundle)).rejects.toThrow('EEXIST');
  expect(await readdir(join(root, 'existing'))).toEqual([]); expect(await readdir(join(root, 'outside'))).toEqual([]);
}));

test('receiver preserves a destination when mkdir takes effect and then reports failure', () => fixture(async (root, bundle) => {
  const originalMkdir = filesystem.mkdir; let injected = false, error: unknown;
  // Keep all native overloads and arguments; inject only after the native effect.
  const injectedMkdir: typeof filesystem.mkdir = new Proxy(originalMkdir, {
    async apply(target, thisArg, args) {
      const result = await Reflect.apply(target, thisArg, args);
      if (String(args[0]).endsWith('/receiver')) { injected = true; throw Error('INJECTED_AFTER_MKDIR'); }
      return result;
    }
  });
  const mock = spyOn(filesystem, 'mkdir').mockImplementation(injectedMkdir);
  try { await receiveHandoff(root, 'receiver', bundle); } catch (caught) { error = caught; } finally { mock.mockRestore(); }
  expect(injected).toBe(true); expect(error).toBeInstanceOf(HandoffReceiveUncertainError);
  expect((error as HandoffReceiveUncertainError).stage).toBe('destination-create');
  expect(await readdir(join(root, 'receiver'))).toEqual([]);
  await expect(receiveHandoff(root, 'receiver', bundle)).rejects.toThrow('EEXIST');
}));

test.each(['in-place', 'replacement', 'missing'] as const)(
  'receiver final all-file scan rejects earlier %s mutation during later readback and retains partial state', race => fixture(async (root) => {
    // Adapt the independent FIRST -> WRONG probe, including same-size in-place corruption.
    const file = (path: string, text: string) => ({ path, sha256: createHash('sha256').update(text).digest('hex'), byteLength: Buffer.byteLength(text), base64: Buffer.from(text).toString('base64') });
    const bundle: HandoffBundle = { schema: 'workspace.source-transfer.v1', issueId: 'synthetic-issue', sourceRevision: 'synthetic-saved-revision', files: [file('first.txt', 'FIRST'), file('second.txt', 'SECOND')], completedEffectIds: ['already-completed'], checks: [], nextAction: { id: 'unfinished', action: 'Continue pending source work', path: 'first.txt' } };
    const first = join(root, 'receiver/first.txt'), second = join(root, 'receiver/second.txt'), originalOpen = filesystem.open;
    let firstReadbackClosed = false, injected = false, returnedSuccess = false, error: unknown;
    const mock = spyOn(filesystem, 'open').mockImplementation(async (...args: Parameters<typeof filesystem.open>) => {
      const path = String(args[0]), readback = !(Number(args[1]) & constants.O_CREAT);
      if (!injected && path.endsWith('/second.txt') && readback) {
        expect(firstReadbackClosed).toBe(true);
        injected = true;
        if (race !== 'in-place') await rename(first, first + '-saved');
        if (race !== 'missing') await writeFile(first, 'WRONG');
      }
      const handle = await originalOpen(...args);
      if (path.endsWith('/first.txt') && readback) {
        const close = handle.close.bind(handle);
        handle.close = async () => { await close(); firstReadbackClosed = true; };
      }
      return handle;
    });
    try { await receiveHandoff(root, 'receiver', bundle); returnedSuccess = true; }
    catch (caught) { error = caught; } finally { mock.mockRestore(); }
    expect(injected).toBe(true); expect(firstReadbackClosed).toBe(true); expect(returnedSuccess).toBe(false);
    expect(error).toBeInstanceOf(HandoffReceiveUncertainError);
    expect((error as HandoffReceiveUncertainError).stage).toBe('readback');
    expect((error as HandoffReceiveUncertainError).root).toBe(join(root, 'receiver'));
    expect((error as HandoffReceiveUncertainError).bundleSha256).toBe(validateHandoff(bundle).bundleSha256);
    if (race === 'missing') await expect(readFile(first)).rejects.toThrow('ENOENT');
    else expect(await readFile(first, 'utf8')).toBe('WRONG');
    if (race !== 'in-place') expect(await readFile(first + '-saved', 'utf8')).toBe('FIRST');
    expect(await readFile(second, 'utf8')).toBe('SECOND');
    await expect(receiveHandoff(root, 'receiver', bundle)).rejects.toThrow('EEXIST');
    if (race === 'missing') await expect(readFile(first)).rejects.toThrow('ENOENT');
    else expect(await readFile(first, 'utf8')).toBe('WRONG');
    expect(await readFile(second, 'utf8')).toBe('SECOND');
    expect(bundle.completedEffectIds).toEqual(['already-completed']);
    console.log(JSON.stringify({ status: 'earlier-file-mutation-rejected', race, injected, firstReadbackClosed, returnedSuccess, stage: (error as HandoffReceiveUncertainError).stage, partialStateRetained: true, blindRetryRejected: true }));
  })
);
