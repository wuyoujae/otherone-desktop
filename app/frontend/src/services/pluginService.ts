import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

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
  if (isDesktopRuntime()) return invokeDesktop<PluginEntry[]>('load_plugin_list');
  if (canUseWebApi()) return requestWebApi<PluginEntry[]>('/api/plugins');
  return [];
}

export async function installPlugin(pluginName: string, kind: string): Promise<void> {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('install_plugin', { pluginName, kind });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>('/api/plugins/install', {
      method: 'POST',
      body: { pluginName, kind },
    });
  }
}

export async function importSkillFromDirectory(sourceDir: string): Promise<PluginEntry | null> {
  if (!isDesktopRuntime()) return null;
  return invokeDesktop<PluginEntry>('import_skill_from_directory', { sourceDir });
}

export async function importSkillFromUrl(url: string): Promise<PluginEntry | null> {
  if (isDesktopRuntime()) return invokeDesktop<PluginEntry>('import_skill_from_url', { url });

  if (canUseWebApi()) {
    return requestWebApi<PluginEntry>('/api/plugins/skills/import-url', {
      method: 'POST',
      body: { url },
    });
  }

  return null;
}

export async function importMcpServers(rawConfig: string): Promise<PluginEntry[]> {
  if (isDesktopRuntime()) return invokeDesktop<PluginEntry[]>('import_mcp_servers', { rawConfig });

  if (canUseWebApi()) {
    return requestWebApi<PluginEntry[]>('/api/plugins/mcp/import', {
      method: 'POST',
      body: { rawConfig },
    });
  }

  return [];
}

export async function importMcpServersFromUrl(url: string): Promise<PluginEntry[]> {
  if (isDesktopRuntime()) return invokeDesktop<PluginEntry[]>('import_mcp_servers_from_url', { url });

  if (canUseWebApi()) {
    return requestWebApi<PluginEntry[]>('/api/plugins/mcp/import-url', {
      method: 'POST',
      body: { url },
    });
  }

  return [];
}

export async function uninstallPlugin(pluginName: string, kind: string): Promise<void> {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('uninstall_plugin', { pluginName, kind });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>('/api/plugins/uninstall', {
      method: 'POST',
      body: { pluginName, kind },
    });
  }
}
