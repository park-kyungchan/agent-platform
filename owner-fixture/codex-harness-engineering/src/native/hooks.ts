// codex-native-provider-registration-sha256: 00b30c4a4f81b05b85981a8c90e9a8d92958f7ecf39e1f530a00b788589058fa
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from '../legacy/harness/owner-context-loader.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
