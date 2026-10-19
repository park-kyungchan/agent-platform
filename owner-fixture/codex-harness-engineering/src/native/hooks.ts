// codex-native-provider-registration-sha256: 8c0ffbcfab88f3a673b24d572e8bad263d61dce4b96db7fc1f7b160fb01f46ec
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from '../legacy/harness/owner-context-loader.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
