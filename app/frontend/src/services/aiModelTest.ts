import type { ModelConfig, ProviderConfig } from '../types/apiConfig';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type TestAiModelResult = {
  latencyMs: number;
};

export async function testAiModel(provider: ProviderConfig, model: ModelConfig) {
  if (!isTauriRuntime()) {
    throw new Error('真实模型测试需要在 Tauri 桌面端运行。');
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<TestAiModelResult>('test_ai_model', {
    request: {
      provider: provider.provider,
      baseUrl: provider.baseUrl,
      apiKey: provider.apiKey,
      model: model.name,
      contextLength: model.contextLength,
      temperature: model.temperature,
      topP: model.topP,
      parallelToolCalls: model.parallelToolCalls,
    },
  });
}
