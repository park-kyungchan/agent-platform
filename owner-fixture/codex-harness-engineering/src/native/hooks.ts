// codex-native-provider-registration-sha256: 1e36ba51e6f941c5d7b47fd76c3e8c9c831244254adebc438e2307a465b640b3
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from '../legacy/harness/owner-context-loader.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
