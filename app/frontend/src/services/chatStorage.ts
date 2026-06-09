import type { ReasoningEffort } from '../types/apiConfig';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type SendChatMessageRequest = {
  sessionId?: string;
  modelId: string;
  prompt: string;
  reasoningEffort: ReasoningEffort;
  contextCompressionEnabled: boolean;
  branchModeEnabled: boolean;
  targetModeEnabled: boolean;
};

export type SendChatMessageResponse = {
  sessionId: string;
};

export type ChatStreamEvent = {
  sessionId: string;
  eventType: 'user_entry' | 'assistant_delta' | 'assistant_thinking_delta' | 'thinking' | 'tool_calls' | 'tool_call' | 'complete' | 'error' | 'cancelled';
  content: string;
  rawChunk?: unknown;
  error?: string;
  toolLabel?: string;
  toolExpandable?: boolean;
  toolDetail?: string;
  toolStatus?: string;
};

export async function sendChatMessageToStorage(payload: SendChatMessageRequest) {
  if (!isTauriRuntime()) {
    throw new Error('真实对话需要在桌面应用中运行。');
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SendChatMessageResponse>('send_chat_message', { request: payload });
}

export async function cancelChatMessage(sessionId: string) {
  if (!isTauriRuntime()) {
    throw new Error('取消对话需要在桌面应用中运行。');
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('cancel_chat_message', { sessionId });
}

export async function listenToChatStream(onEvent: (event: ChatStreamEvent) => void) {
  if (!isTauriRuntime()) {
    console.warn('[chatStorage] 非 Tauri 环境，跳过事件监听');
    return () => undefined;
  }

  const { listen } = await import('@tauri-apps/api/event');
  console.log('[chatStorage] 正在注册 chat_stream_event 监听器...');
  const unlisten = await listen<ChatStreamEvent>('chat_stream_event', (event) => {
    onEvent(event.payload);
  });
  console.log('[chatStorage] chat_stream_event 监听器注册成功');
  return unlisten;
}
