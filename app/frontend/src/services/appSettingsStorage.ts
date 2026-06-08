import type { AppSettings, EngineSettings, StorageSettings } from '../types/appSettings';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function loadAppSettingsFromStorage() {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppSettings>('load_app_settings');
}

export async function saveEngineSettingsToStorage(engine: EngineSettings) {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppSettings>('save_engine_settings', { request: { engine } });
}

export async function migrateStorageSettingsToStorage(storage: StorageSettings) {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AppSettings>('migrate_storage_settings', {
    request: {
      storage,
      acknowledgedDataLossRisk: true,
    },
  });
}
