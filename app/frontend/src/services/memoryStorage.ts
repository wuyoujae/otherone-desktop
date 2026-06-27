import type { MemoryTreeResponse } from '../types/memory';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function readMemoryTreeFromStorage() {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<MemoryTreeResponse>('read_memory_tree');
}
