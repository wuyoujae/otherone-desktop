import type { ReasoningEffort } from '../types/apiConfig';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop, listenDesktop } from './platform/tauri';
import { canUseWebApi, listenWebApiEventStream, requestWebApi } from './platform/webApi';

export type SendChatMessageRequest = {
  sessionId?: string;
  modelId: string;
  prompt: string;
  prompts?: string[];
  reasoningEffort: ReasoningEffort;
  contextCompressionEnabled: boolean;
  branchModeEnabled: boolean;
  targetModeEnabled: boolean;
  memoryEnabled: boolean;
};

export type SendChatMessageResponse = {
  sessionId: string;
};

export type EnqueueChatMessageRequest = {
  sessionId: string;
  prompt: string;
  prompts?: string[];
};

export type ChatStreamEvent = {
  sessionId: string;
  eventType: 'user_entry' | 'queued_user_prompts' | 'assistant_delta' | 'assistant_thinking_delta' | 'thinking' | 'tool_calls' | 'tool_call' | 'complete' | 'error' | 'cancelled';
  content: string;
  rawChunk?: unknown;
  error?: string;
  toolLabel?: string;
  toolExpandable?: boolean;
  toolDetail?: string;
  toolStatus?: string;
};

export async function sendChatMessageToStorage(payload: SendChatMessageRequest) {
  if (isDesktopRuntime()) {
    return invokeDesktop<SendChatMessageResponse>('send_chat_message', { request: payload });
  }

  if (canUseWebApi()) {
    return requestWebApi<SendChatMessageResponse>('/api/chat/messages', {
      method: 'POST',
      body: payload,
    });
  }

  throw new Error('真实对话需要桌面端或已配置的 Web API。');
}

export async function enqueueChatMessageToStorage(payload: EnqueueChatMessageRequest) {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('enqueue_chat_message', { request: payload });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>('/api/chat/messages/enqueue', {
      method: 'POST',
      body: payload,
    });
    return;
  }

  throw new Error('真实对话需要桌面端或已配置的 Web API。');
}

export async function cancelChatMessage(sessionId: string) {
  if (isDesktopRuntime()) {
    await invokeDesktop<void>('cancel_chat_message', { sessionId });
    return;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>('/api/chat/messages/cancel', {
      method: 'POST',
      body: { sessionId },
    });
    return;
  }

  throw new Error('取消对话需要桌面端或已配置的 Web API。');
}

export async function listenToChatStream(onEvent: (event: ChatStreamEvent) => void) {
  if (isDesktopRuntime()) {
    console.log('[chatStorage] 正在注册 chat_stream_event 监听器...');
    const unlisten = await listenDesktop<ChatStreamEvent>('chat_stream_event', onEvent);
    console.log('[chatStorage] chat_stream_event 监听器注册成功');
    return unlisten;
  }

  if (canUseWebApi()) {
    return listenWebApiEventStream<ChatStreamEvent>('/api/chat/stream', onEvent);
  }

  console.warn('[chatStorage] 未配置 Web API，跳过事件监听');
  return () => undefined;
}
