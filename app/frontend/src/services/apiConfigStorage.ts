import type { ProviderConfig } from '../types/apiConfig';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function loadApiConfigsFromStorage() {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<ProviderConfig[]>('load_api_configs');
}

export async function saveApiConfigsToStorage(providers: ProviderConfig[]) {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('save_api_configs', { providers });
}
