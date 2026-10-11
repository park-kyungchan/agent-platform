// codex-native-provider-registration-sha256: 9c9dbd94db4b205087dee5854d24e50391be89ee2f41e8b51c2fac114aaec01b
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from '../legacy/harness/owner-context-loader.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
