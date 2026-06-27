import { useCallback, useEffect, useState } from 'react';
import { CustomSelect, ToggleSwitch } from './CustomControls';
import { MemoryTreeScene } from './MemoryTreeScene';
import { readMemoryTreeFromStorage } from '../services/memoryStorage';
import type { ModelOption } from '../types/apiConfig';
import type { MemoryTreeResponse } from '../types/memory';

type PersonalizationPageProps = {
  memoryEnabled: boolean;
  memoryModelId: string;
  modelOptions: ModelOption[];
  onMemoryEnabledChange: (enabled: boolean) => void;
  onMemoryModelChange: (modelId: string) => void;
};

const noMemoryModelOption = { label: '未配置模型', value: 'none' };
const emptyMemoryTree: MemoryTreeResponse = { storagePath: '', points: [] };

export function PersonalizationPage({
  memoryEnabled,
  memoryModelId,
  modelOptions,
  onMemoryEnabledChange,
  onMemoryModelChange,
}: PersonalizationPageProps) {
  const selectOptions = modelOptions.length
    ? modelOptions.map((model) => ({ label: model.label, value: model.id }))
    : [noMemoryModelOption];
  const selectedValue = selectOptions.some((option) => option.value === memoryModelId)
    ? memoryModelId
    : selectOptions[0].value;
  const [memoryTree, setMemoryTree] = useState<MemoryTreeResponse>(emptyMemoryTree);
  const [isLoadingMemoryTree, setIsLoadingMemoryTree] = useState(false);
  const [memoryTreeError, setMemoryTreeError] = useState<string | null>(null);

  const loadMemoryTree = useCallback(async () => {
    setIsLoadingMemoryTree(true);
    setMemoryTreeError(null);

    try {
      const response = await readMemoryTreeFromStorage();
      setMemoryTree(response ?? emptyMemoryTree);
    } catch (error) {
      setMemoryTreeError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoadingMemoryTree(false);
    }
  }, []);

  useEffect(() => {
    void loadMemoryTree();
  }, [loadMemoryTree]);

  return (
    <section id="view-personalization" className="view-container personalization-view active">
      <div className="personalization-shell">
        <nav className="personalization-nav" aria-label="个性化设置">
          <div className="personalization-nav-left">
            <div className="personalization-model-control">
              <CustomSelect
                label="记忆辅助模型"
                options={selectOptions}
                value={selectedValue}
                onChange={(value) => onMemoryModelChange(value === 'none' ? '' : value)}
              />
            </div>
          </div>

          <div className="personalization-nav-right">
            <ToggleSwitch
              checked={memoryEnabled}
              label={memoryEnabled ? '记忆开启' : '记忆关闭'}
              onChange={onMemoryEnabledChange}
            />
          </div>
        </nav>
        <div className="personalization-content">
          <MemoryTreeScene
            points={memoryTree.points}
            loading={isLoadingMemoryTree}
            error={memoryTreeError}
            storagePath={memoryTree.storagePath}
            onRefresh={loadMemoryTree}
          />
        </div>
      </div>
    </section>
  );
}
