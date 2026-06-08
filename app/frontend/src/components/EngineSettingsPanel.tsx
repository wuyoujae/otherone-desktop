import { Database, Save, Settings2 } from 'lucide-react';
import type { EngineSettings } from '../types/appSettings';
import type { ModelOption, ReasoningEffort } from '../types/apiConfig';
import { CustomSelect, CustomSlider } from './CustomControls';

type EngineSettingsPanelProps = {
  engine: EngineSettings;
  isSaving: boolean;
  modelOptions: ModelOption[];
  onChange: (engine: EngineSettings) => void;
  onSave: () => void;
};

const reasoningOptions: Array<{ label: string; value: ReasoningEffort }> = [
  { label: '不要思考', value: 'none' },
  { label: 'Low', value: 'low' },
  { label: 'Medium', value: 'medium' },
  { label: 'High', value: 'high' },
];

export function EngineSettingsPanel({
  engine,
  isSaving,
  modelOptions,
  onChange,
  onSave,
}: EngineSettingsPanelProps) {
  const compactModelOptions = [
    { label: '使用当前对话模型', value: '' },
    ...modelOptions.map((model) => ({
      label: `${model.label} · ${model.providerName}`,
      value: model.id,
    })),
  ];

  const updateEngine = (patch: Partial<EngineSettings>) => {
    onChange({ ...engine, ...patch });
  };

  return (
    <div className="engine-settings">
      <div className="settings-title-row">
        <div>
          <div className="settings-title">模型与引擎</div>
          <p className="settings-subtitle">配置 otherone-agent 的循环、上下文压缩和默认推理行为。</p>
        </div>
        <button className="setting-btn model-save-btn" type="button" disabled={isSaving} onClick={onSave}>
          <Save style={{ width: 16, height: 16 }} />
          {isSaving ? '保存中' : '保存引擎配置'}
        </button>
      </div>

      <section className="engine-config-block">
        <div className="engine-config-header">
          <Settings2 style={{ width: 18, height: 18 }} />
          <h3>Agent 执行</h3>
        </div>
        <div className="model-form-grid">
          <label className="model-field">
            <span>最大 Agent 循环</span>
            <input
              className="model-input"
              min={1}
              max={128}
              type="number"
              value={engine.maxIterations}
              onChange={(event) => updateEngine({ maxIterations: Number(event.target.value) })}
            />
          </label>
          <label className="model-field">
            <span>上下文窗口</span>
            <input
              className="model-input"
              min={1024}
              type="number"
              value={engine.contextWindow}
              onChange={(event) => updateEngine({ contextWindow: Number(event.target.value) })}
            />
          </label>
          <label className="model-field full-width">
            <span>系统提示词</span>
            <textarea
              className="model-textarea"
              rows={4}
              value={engine.systemPrompt}
              onChange={(event) => updateEngine({ systemPrompt: event.target.value })}
              placeholder="为空时不注入系统提示词"
            />
          </label>
        </div>
      </section>

      <section className="engine-config-block">
        <div className="engine-config-header">
          <Database style={{ width: 18, height: 18 }} />
          <h3>上下文与压缩</h3>
        </div>
        <div className="model-slider-grid">
          <CustomSlider
            label="压缩触发阈值"
            max={0.98}
            step={0.01}
            value={engine.thresholdPercentage}
            onChange={(value) => updateEngine({ thresholdPercentage: value })}
          />
          <CustomSlider
            label="压缩保留比例"
            max={0.95}
            step={0.01}
            value={engine.compactionKeepRatio}
            onChange={(value) => updateEngine({ compactionKeepRatio: value })}
          />
        </div>
        <div className="model-form-grid engine-select-grid">
          <label className="model-field">
            <span>默认推理模式</span>
            <CustomSelect
              options={reasoningOptions}
              value={engine.defaultReasoningEffort}
              onChange={(value) => updateEngine({ defaultReasoningEffort: value })}
            />
          </label>
          <label className="model-field">
            <span>压缩摘要模型</span>
            <CustomSelect
              options={compactModelOptions}
              value={engine.compactModelId}
              onChange={(value) => updateEngine({ compactModelId: value })}
            />
          </label>
          <div className="engine-storage-mode">
            <span>上下文加载</span>
            <strong>LocalFile</strong>
          </div>
          <div className="engine-storage-mode">
            <span>对话存储</span>
            <strong>LocalFile</strong>
          </div>
        </div>
      </section>
    </div>
  );
}
