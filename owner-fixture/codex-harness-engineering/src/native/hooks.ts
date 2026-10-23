// codex-native-provider-registration-sha256: 93e2a5183e8617ada1261e476a03265bbb4deb12c3c09d8aa853d72d6e8678be
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from './context-owner.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
