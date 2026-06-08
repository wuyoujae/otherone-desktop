import type { ReasoningEffort } from './apiConfig';

export type StorageSettings = {
  dataRoot: string;
  artifactRoot: string;
  dialogueRoot: string;
};

export type EngineSettings = {
  systemPrompt: string;
  maxIterations: number;
  contextWindow: number;
  thresholdPercentage: number;
  compactionKeepRatio: number;
  compactModelId: string;
  defaultReasoningEffort: ReasoningEffort;
};

export type AppSettings = {
  storage: StorageSettings;
  engine: EngineSettings;
};

export const defaultEngineSettings: EngineSettings = {
  systemPrompt: '',
  maxIterations: 8,
  contextWindow: 16000,
  thresholdPercentage: 0.8,
  compactionKeepRatio: 0.35,
  compactModelId: '',
  defaultReasoningEffort: 'medium',
};
