import type { SessionDetail, SessionSummary } from '../types/session';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export async function loadSessionsFromStorage() {
  if (isDesktopRuntime()) {
    return invokeDesktop<SessionSummary[]>('load_sessions');
  }

  if (canUseWebApi()) {
    return requestWebApi<SessionSummary[]>('/api/sessions');
  }

  return [];
}

export async function readSessionFromStorage(sessionId: string) {
  if (isDesktopRuntime()) {
    return invokeDesktop<SessionDetail>('read_session', { sessionId });
  }

  if (canUseWebApi()) {
    return requestWebApi<SessionDetail>(`/api/sessions/${encodeURIComponent(sessionId)}`);
  }

  return null;
}

export async function updateSessionTitleInStorage(sessionId: string, title: string) {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('update_session_title', { payload: { sessionId, title } });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>(`/api/sessions/${encodeURIComponent(sessionId)}/title`, {
      method: 'PATCH',
      body: { title },
    });
  }
}
