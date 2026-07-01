import type { MemoryTreeResponse } from '../types/memory';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export async function readMemoryTreeFromStorage() {
  if (isDesktopRuntime()) {
    return invokeDesktop<MemoryTreeResponse>('read_memory_tree');
  }

  if (canUseWebApi()) {
    return requestWebApi<MemoryTreeResponse>('/api/memory/tree');
  }

  return null;
}
