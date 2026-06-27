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

export async function selectDirectoryFromSystem() {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string | null>('select_directory');
}

export async function openDirectoryInSystem(path: string) {
  const directoryPath = path.trim();

  if (!directoryPath || !isTauriRuntime()) {
    return false;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('open_directory', { path: directoryPath });
  return true;
}

export async function revealFileInSystem(path: string) {
  const filePath = path.trim();

  if (!filePath || !isTauriRuntime()) {
    return false;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('reveal_file', { path: filePath });
  return true;
}
