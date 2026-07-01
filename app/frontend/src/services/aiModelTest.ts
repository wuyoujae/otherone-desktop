import type { ModelConfig, ProviderConfig } from '../types/apiConfig';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export type TestAiModelResult = {
  latencyMs: number;
};

export async function testAiModel(provider: ProviderConfig, model: ModelConfig) {
  const request = {
    provider: provider.provider,
    baseUrl: provider.baseUrl,
    apiKey: provider.apiKey,
    model: model.name,
    contextLength: model.contextLength,
    temperature: model.temperature,
    topP: model.topP,
    parallelToolCalls: model.parallelToolCalls,
  };

  if (isDesktopRuntime()) {
    return invokeDesktop<TestAiModelResult>('test_ai_model', { request });
  }

  if (canUseWebApi()) {
    return requestWebApi<TestAiModelResult>('/api/ai-model-test', {
      method: 'POST',
      body: request,
    });
  }

  throw new Error('真实模型测试需要桌面端或已配置的 Web API。');
}
