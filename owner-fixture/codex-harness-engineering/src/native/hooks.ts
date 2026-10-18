// codex-native-provider-registration-sha256: 61815b982ea76b40fcbafcb87ec6fbc02f8f94b3581cb52455914ad6a6fa8569
import {handleHook as coreHandleHook} from '@palantirkc/native-codex-adapter/hooks';
import {loadCodexOwnerContext} from '../legacy/harness/owner-context-loader.ts';
export * from '@palantirkc/native-codex-adapter/hooks';
export function handleHook(value:unknown,options:Parameters<typeof coreHandleHook>[1]){return coreHandleHook(value,options,loadCodexOwnerContext);}
