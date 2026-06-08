import type { SessionDetail, SessionSummary } from '../types/session';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function loadSessionsFromStorage() {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SessionSummary[]>('load_sessions');
}

export async function readSessionFromStorage(sessionId: string) {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SessionDetail>('read_session', { sessionId });
}

export async function updateSessionTitleInStorage(sessionId: string, title: string) {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('update_session_title', { payload: { sessionId, title } });
}
