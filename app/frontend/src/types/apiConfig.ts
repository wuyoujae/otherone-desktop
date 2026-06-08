export type ProviderKind = 'OpenAI' | 'Anthropic' | 'OpenRouter' | 'Fetch' | 'Local' | 'OpenAI Compatible';

export type ReasoningEffort = 'none' | 'low' | 'medium' | 'high';

export type ToolChoicePolicy = 'default' | 'auto' | 'none' | 'required';

export type ModelConfig = {
  id: string;
  name: string;
  contextLength: number;
  contextWindow: number;
  thresholdPercentage: number;
  maxIterations: number;
  temperature: number;
  topP: number;
  stream: boolean;
  parallelToolCalls: boolean;
  toolChoice: ToolChoicePolicy;
  extraParams: string;
  reasoningEffort: ReasoningEffort;
  defaultModel: boolean;
};

export type ProviderConfig = {
  id: string;
  name: string;
  provider: ProviderKind;
  officialUrl: string;
  baseUrl: string;
  apiKey: string;
  models: ModelConfig[];
};

export type ModelOption = {
  id: string;
  label: string;
  providerName: string;
};
