const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type PluginEntry = {
  id: string;
  name: string;
  description: string;
  kind: string;
  source: string;
  installed: boolean;
  filePath: string;
  hasBinary?: boolean;
  binPath?: string;
  binDir?: string;
};

export async function loadPluginList(): Promise<PluginEntry[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<PluginEntry[]>('load_plugin_list');
}

export async function installPlugin(pluginName: string, kind: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('install_plugin', { pluginName, kind });
}

export async function uninstallPlugin(pluginName: string, kind: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('uninstall_plugin', { pluginName, kind });
}
