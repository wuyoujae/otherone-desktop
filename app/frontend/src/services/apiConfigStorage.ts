import type { ProviderConfig } from '../types/apiConfig';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export async function loadApiConfigsFromStorage() {
  if (isDesktopRuntime()) {
    return invokeDesktop<ProviderConfig[]>('load_api_configs');
  }

  if (canUseWebApi()) {
    return requestWebApi<ProviderConfig[]>('/api/api-configs');
  }

  return [];
}

export async function saveApiConfigsToStorage(providers: ProviderConfig[]) {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('save_api_configs', { providers });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>('/api/api-configs', {
      method: 'PUT',
      body: { providers },
    });
  }
}
