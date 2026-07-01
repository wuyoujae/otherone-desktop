export type PlatformRuntime = 'desktop' | 'web';

export function isDesktopRuntime() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function getPlatformRuntime(): PlatformRuntime {
  return isDesktopRuntime() ? 'desktop' : 'web';
}

export function getWebApiBaseUrl() {
  const baseUrl = import.meta.env.VITE_OTHERONE_WEB_API_BASE_URL?.trim() ?? '';

  if (!baseUrl) {
    return null;
  }

  return baseUrl.replace(/\/+$/, '');
}

export function hasWebApiBaseUrl() {
  return getWebApiBaseUrl() !== null;
}

export function createPlatformUnavailableError(feature: string) {
  return new Error(`${feature}需要桌面端或已配置的 Web API。`);
}
