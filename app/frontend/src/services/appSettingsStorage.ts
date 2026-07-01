import type { AppSettings, EngineSettings, StorageSettings } from '../types/appSettings';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export async function loadAppSettingsFromStorage() {
  if (isDesktopRuntime()) {
    return invokeDesktop<AppSettings>('load_app_settings');
  }

  if (canUseWebApi()) {
    return requestWebApi<AppSettings>('/api/app-settings');
  }

  return null;
}

export async function saveEngineSettingsToStorage(engine: EngineSettings) {
  if (isDesktopRuntime()) {
    return invokeDesktop<AppSettings>('save_engine_settings', { request: { engine } });
  }

  if (canUseWebApi()) {
    return requestWebApi<AppSettings>('/api/app-settings/engine', {
      method: 'PUT',
      body: { engine },
    });
  }

  return null;
}

export async function migrateStorageSettingsToStorage(storage: StorageSettings) {
  if (!isDesktopRuntime()) {
    return null;
  }

  return invokeDesktop<AppSettings>('migrate_storage_settings', {
    request: {
      storage,
      acknowledgedDataLossRisk: true,
    },
  });
}

export async function clearAllOtheroneDataFromStorage() {
  if (!isDesktopRuntime()) {
    return null;
  }

  return invokeDesktop<AppSettings>('clear_all_otherone_data', {
    request: {
      acknowledgedDataLossRisk: true,
    },
  });
}

export async function selectDirectoryFromSystem() {
  if (!isDesktopRuntime()) {
    return null;
  }

  return invokeDesktop<string | null>('select_directory');
}

export async function openDirectoryInSystem(path: string) {
  const directoryPath = path.trim();

  if (!directoryPath || !isDesktopRuntime()) {
    return false;
  }

  await invokeDesktop<void>('open_directory', { path: directoryPath });
  return true;
}

export async function revealFileInSystem(path: string) {
  const filePath = path.trim();

  if (!filePath || !isDesktopRuntime()) {
    return false;
  }

  await invokeDesktop<void>('reveal_file', { path: filePath });
  return true;
}
