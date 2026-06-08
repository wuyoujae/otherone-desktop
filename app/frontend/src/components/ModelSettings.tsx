import {
  Box,
  Check,
  ChevronDown,
  ChevronRight,
  Cpu,
  ExternalLink,
  Eye,
  EyeOff,
  Plus,
  PlusCircle,
  Save,
  TestTube2,
  Trash2,
  X,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';
import { createModelConfig, createProviderConfig } from '../data/defaultApiConfigs';
import type { ModelConfig, ProviderConfig, ProviderKind, ToolChoicePolicy } from '../types/apiConfig';
import { CustomSelect, CustomSlider, ToggleSwitch } from './CustomControls';
import { openExternalUrl } from '../utils/openExternalUrl';

type ModelSettingsProps = {
  isSaving: boolean;
  onProvidersChange: (providers: ProviderConfig[]) => void;
  onSave: () => void;
  onTestProvider: (provider: ProviderConfig) => void;
  providers: ProviderConfig[];
  storageStatus: string;
  testingProviderId: string;
};

const iconSize = { width: 16, height: 16 };

const providerOptions: Array<{ label: string; value: ProviderKind }> = [
  { label: 'OpenAI', value: 'OpenAI' },
  { label: 'Anthropic', value: 'Anthropic' },
  { label: 'OpenRouter', value: 'OpenRouter' },
  { label: 'Fetch', value: 'Fetch' },
  { label: 'Local', value: 'Local' },
  { label: 'OpenAI Compatible', value: 'OpenAI Compatible' },
];

const toolChoiceOptions: Array<{ label: string; value: ToolChoicePolicy }> = [
  { label: 'Auto', value: 'auto' },
  { label: 'Default', value: 'default' },
  { label: 'None', value: 'none' },
  { label: 'Required', value: 'required' },
];

export function ModelSettings({
  isSaving,
  onProvidersChange,
  onSave,
  onTestProvider,
  providers,
  storageStatus,
  testingProviderId,
}: ModelSettingsProps) {
  const [visibleKeys, setVisibleKeys] = useState<Record<string, boolean>>({});
  const [openProviders, setOpenProviders] = useState<Record<string, boolean>>({});
  const providerRefs = useRef<Record<string, HTMLElement | null>>({});
  const pendingScrollProviderId = useRef<string | null>(null);

  useEffect(() => {
    setOpenProviders((current) => {
      const next = { ...current };
      providers.forEach((provider) => {
        if (next[provider.id] === undefined) {
          next[provider.id] = false;
        }
      });
      return next;
    });
  }, [providers]);

  useEffect(() => {
    const targetId = pendingScrollProviderId.current;

    if (!targetId) {
      return;
    }

    pendingScrollProviderId.current = null;
    requestAnimationFrame(() => {
      providerRefs.current[targetId]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
  }, [providers]);

  const updateProvider = (providerId: string, patch: Partial<ProviderConfig>) => {
    onProvidersChange(
      providers.map((provider) => (provider.id === providerId ? { ...provider, ...patch } : provider)),
    );
  };

  const updateModel = (providerId: string, modelId: string, patch: Partial<ModelConfig>) => {
    onProvidersChange(
      providers.map((provider) =>
        provider.id === providerId
          ? {
              ...provider,
              models: provider.models.map((model) => (model.id === modelId ? { ...model, ...patch } : model)),
            }
          : provider,
      ),
    );
  };

  const addProvider = () => {
    const nextProvider = createProviderConfig(providers.length + 1);
    pendingScrollProviderId.current = nextProvider.id;
    setOpenProviders((current) => ({ ...current, [nextProvider.id]: true }));
    onProvidersChange([...providers, nextProvider]);
  };

  const removeProvider = (providerId: string) => {
    onProvidersChange(providers.filter((provider) => provider.id !== providerId));
  };

  const addModel = (providerId: string) => {
    onProvidersChange(
      providers.map((provider) =>
        provider.id === providerId
          ? { ...provider, models: [...provider.models, createModelConfig(provider.models.length + 1)] }
          : provider,
      ),
    );
  };

  const removeModel = (providerId: string, modelId: string) => {
    onProvidersChange(
      providers.map((provider) =>
        provider.id === providerId
          ? { ...provider, models: provider.models.filter((model) => model.id !== modelId) }
          : provider,
      ),
    );
  };

  const setDefaultModel = (providerId: string, modelId: string) => {
    onProvidersChange(
      providers.map((provider) => ({
        ...provider,
        models: provider.models.map((model) => ({
          ...model,
          defaultModel: provider.id === providerId && model.id === modelId,
        })),
      })),
    );
  };

  const toggleProvider = (providerId: string) => {
    setOpenProviders((current) => ({ ...current, [providerId]: !current[providerId] }));
  };

  return (
    <div className="model-settings">
      <div className="settings-title-row">
        <div>
          <div className="settings-title">API 与模型配置</div>
          <p className="settings-subtitle">配置供应商公共信息，并为每个供应商维护多个模型参数。</p>
        </div>
        <div className="settings-title-actions">
          <button className="setting-btn model-save-btn" type="button" disabled={isSaving} onClick={onSave}>
            <Save style={{ width: 16, height: 16 }} />
            {isSaving ? '保存中' : '保存配置'}
          </button>
          <button className="setting-btn model-add-provider-btn" type="button" onClick={addProvider}>
            <PlusCircle style={{ width: 17, height: 17 }} />
            新增配置块
          </button>
        </div>
      </div>

      <div className="settings-storage-status">{storageStatus}</div>

      <div className="provider-config-list">
        {providers.map((provider) => {
          const open = openProviders[provider.id] ?? true;

          return (
            <section
              className={`provider-config-block ${open ? 'is-open' : 'is-collapsed'}`}
              key={provider.id}
              ref={(element) => {
                providerRefs.current[provider.id] = element;
              }}
            >
              <div className="provider-config-header">
                <button
                  className="provider-collapse-btn"
                  type="button"
                  aria-label={open ? '折叠配置块' : '展开配置块'}
                  onClick={() => toggleProvider(provider.id)}
                >
                  {open ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
                </button>
                <div className="provider-config-title">
                  <Box style={{ width: 20, height: 20 }} />
                  <input
                    aria-label="供应商名称"
                    className="inline-title-input"
                    value={provider.name}
                    onChange={(event) => updateProvider(provider.id, { name: event.target.value })}
                  />
                </div>
                <button
                  className="provider-link"
                  type="button"
                  aria-label="打开供应商官网"
                  title="打开供应商官网"
                  disabled={!provider.officialUrl.trim()}
                  onClick={() => void openExternalUrl(provider.officialUrl)}
                >
                  <ExternalLink style={{ width: 14, height: 14 }} />
                </button>
                <button
                  className="model-icon-btn test"
                  type="button"
                  aria-label="测试 AI 模型"
                  title="测试 AI 模型"
                  disabled={testingProviderId === provider.id}
                  onClick={() => onTestProvider(provider)}
                >
                  <TestTube2 style={iconSize} />
                </button>
                <button
                  className="model-icon-btn danger"
                  type="button"
                  aria-label="删除配置块"
                  onClick={() => removeProvider(provider.id)}
                >
                  <Trash2 style={iconSize} />
                </button>
              </div>

              <div className="provider-collapse-body">
                <div className="provider-collapse-inner">
                  <div className="model-form-grid provider-form-grid">
                    <Field label="API 供应商类型">
                      <CustomSelect
                        options={providerOptions}
                        value={provider.provider}
                        onChange={(value) => updateProvider(provider.id, { provider: value })}
                      />
                    </Field>
                    <Field label="供应商官网">
                      <input
                        className="model-input"
                        value={provider.officialUrl}
                        onChange={(event) => updateProvider(provider.id, { officialUrl: event.target.value })}
                        placeholder="https://platform.example.com"
                      />
                    </Field>
                    <Field label="Base URL" full>
                      <input
                        className="model-input"
                        value={provider.baseUrl}
                        onChange={(event) => updateProvider(provider.id, { baseUrl: event.target.value })}
                        placeholder="https://api.example.com/v1"
                      />
                    </Field>
                    <Field label="API Key" full>
                      <div className="api-key-input-wrap">
                        <input
                          className="model-input"
                          type={visibleKeys[provider.id] ? 'text' : 'password'}
                          value={provider.apiKey}
                          onChange={(event) => updateProvider(provider.id, { apiKey: event.target.value })}
                          placeholder="sk-..."
                        />
                        <button
                          className="model-icon-btn input-action"
                          type="button"
                          aria-label={visibleKeys[provider.id] ? '隐藏 API Key' : '显示 API Key'}
                          onClick={() =>
                            setVisibleKeys((current) => ({ ...current, [provider.id]: !current[provider.id] }))
                          }
                        >
                          {visibleKeys[provider.id] ? <EyeOff style={iconSize} /> : <Eye style={iconSize} />}
                        </button>
                      </div>
                    </Field>
                  </div>

                  <div className="models-section">
                    <div className="models-header">
                      <h3>模型块</h3>
                      <button className="model-ghost-btn" type="button" onClick={() => addModel(provider.id)}>
                        <Plus style={{ width: 14, height: 14 }} />
                        新增模型
                      </button>
                    </div>

                    {provider.models.map((model) => (
                      <article className="model-config-block" key={model.id}>
                        <div className="model-config-header">
                          <div className="model-config-name">
                            <Cpu style={iconSize} />
                            <input
                              aria-label="模型名称"
                              className="inline-model-input"
                              value={model.name}
                              onChange={(event) => updateModel(provider.id, model.id, { name: event.target.value })}
                            />
                            {model.defaultModel && <span className="model-default-badge">默认</span>}
                          </div>
                          <div className="model-config-actions">
                            <button
                              className="model-text-btn"
                              type="button"
                              onClick={() => setDefaultModel(provider.id, model.id)}
                            >
                              <Check style={{ width: 14, height: 14 }} />
                              设为默认
                            </button>
                            <button
                              className="model-icon-btn danger"
                              type="button"
                              aria-label="删除模型"
                              onClick={() => removeModel(provider.id, model.id)}
                            >
                              <X style={{ width: 15, height: 15 }} />
                            </button>
                          </div>
                        </div>

                        <div className="model-form-grid">
                          <Field label="上下文长度" full>
                            <input
                              className="model-input"
                              type="number"
                              min={1024}
                              value={model.contextLength}
                              onChange={(event) =>
                                updateModel(provider.id, model.id, { contextLength: Number(event.target.value) })
                              }
                            />
                          </Field>
                          <Field label="Context Window">
                            <input
                              className="model-input"
                              type="number"
                              min={1024}
                              value={model.contextWindow}
                              onChange={(event) =>
                                updateModel(provider.id, model.id, { contextWindow: Number(event.target.value) })
                              }
                            />
                          </Field>
                          <Field label="Tool Choice">
                            <CustomSelect
                              options={toolChoiceOptions}
                              value={model.toolChoice}
                              onChange={(value) => updateModel(provider.id, model.id, { toolChoice: value })}
                            />
                          </Field>
                          <Field label="额外参数 JSON" full>
                            <textarea
                              className="model-textarea"
                              rows={3}
                              value={model.extraParams}
                              onChange={(event) =>
                                updateModel(provider.id, model.id, { extraParams: event.target.value })
                              }
                              placeholder='{"maxTokens": 2048}'
                            />
                          </Field>
                        </div>

                        <div className="model-slider-grid">
                          <CustomSlider
                            label="Temperature"
                            max={2}
                            step={0.1}
                            value={model.temperature}
                            onChange={(value) => updateModel(provider.id, model.id, { temperature: value })}
                          />
                          <CustomSlider
                            label="Top P"
                            max={1}
                            step={0.05}
                            value={model.topP}
                            onChange={(value) => updateModel(provider.id, model.id, { topP: value })}
                          />
                        </div>

                        <div className="model-toggle-row">
                          <ToggleSwitch
                            checked={model.stream}
                            label="流式响应"
                            onChange={(checked) => updateModel(provider.id, model.id, { stream: checked })}
                          />
                          <ToggleSwitch
                            checked={model.parallelToolCalls}
                            label="并行工具调用"
                            onChange={(checked) =>
                              updateModel(provider.id, model.id, { parallelToolCalls: checked })
                            }
                          />
                        </div>
                      </article>
                    ))}
                  </div>
                </div>
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}

type FieldProps = {
  children: ReactNode;
  full?: boolean;
  label: string;
};

function Field({ children, full = false, label }: FieldProps) {
  return (
    <label className={`model-field ${full ? 'full-width' : ''}`}>
      <span>{label}</span>
      {children}
    </label>
  );
}
