import { createPlatformUnavailableError, isDesktopRuntime } from './runtime';

type TauriArgs = Record<string, unknown>;

export async function invokeDesktop<T>(command: string, args?: TauriArgs) {
  if (!isDesktopRuntime()) {
    throw createPlatformUnavailableError(command);
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export async function listenDesktop<T>(eventName: string, onEvent: (event: T) => void) {
  if (!isDesktopRuntime()) {
    return () => undefined;
  }

  const { listen } = await import('@tauri-apps/api/event');
  const unlisten = await listen<T>(eventName, (event) => {
    onEvent(event.payload);
  });
  return unlisten;
}
