import {
  ArrowUp,
  ChevronDown,
  ChevronUp,
  Clock,
  Flag,
  Pencil,
  RefreshCw,
  StickyNote,
  Tag,
} from 'lucide-react';
import { useState, useMemo } from 'react';

const iconSize = { width: 16, height: 16 };

// ═══════════════════════════════════════
// Mock data
// ═══════════════════════════════════════

type TaskItem = {
  id: string;
  title: string;
  time: string;
  priority: 'high' | 'medium' | 'low';
  repeat?: string;
  notes?: string;
};

const todayTasks: TaskItem[] = [
  {
    id: 't1',
    title: '整理项目文档结构',
    time: '09:00',
    priority: 'high',
    repeat: '每周一上午 09:00',
    notes: '将前端组件按功能模块重新划分目录，移除冗余代码',
  },
  {
    id: 't2',
    title: '完成 API 密钥模块测试',
    time: '11:30',
    priority: 'medium',
    repeat: '每天 11:30',
    notes: '对 OpenRouter 与 Anthropic 供应商的 API 测试流程做端到端验证',
  },
  {
    id: 't3',
    title: '设计产物面板交互原型',
    time: '14:00',
    priority: 'high',
    notes: '基于现有的手风琴组件产出一版高保真交互稿',
  },
  {
    id: 't4',
    title: '修复日历视图滑动动画',
    time: '16:30',
    priority: 'low',
    repeat: '每周三下午 16:30',
  },
];

// ═══════════════════════════════════════
// Helpers
// ═══════════════════════════════════════

const priorityMeta: Record<TaskItem['priority'], { label: string; color: string }> = {
  high: { label: '高', color: '#ff5555' },
  medium: { label: '中', color: '#fbbf24' },
  low: { label: '低', color: '#9ca3af' },
};

// ═══════════════════════════════════════
// Component
// ═══════════════════════════════════════

type TaskViewProps = {
  selectedDate: Date;
};

export function TaskView({ selectedDate }: TaskViewProps) {
  const [activeTaskId, setActiveTaskId] = useState(todayTasks[0].id);
  const [paramsOpen, setParamsOpen] = useState(false);
  const [prompt, setPrompt] = useState('');

  const activeTask = useMemo(
    () => todayTasks.find((t) => t.id === activeTaskId) ?? todayTasks[0],
    [activeTaskId],
  );

  return (
    <div className="task-view">
      {/* ---- Left sidebar ---- */}
      <aside className="task-sidebar">
        <div className="task-sidebar-header">
          <span className="task-sidebar-title">今日任务</span>
          <span className="task-sidebar-count">{todayTasks.length} 项</span>
        </div>

        <div className="task-sidebar-list">
          {todayTasks.map((task) => {
            const meta = priorityMeta[task.priority];
            return (
              <button
                className={`task-sidebar-item ${activeTaskId === task.id ? 'active' : ''}`}
                key={task.id}
                type="button"
                onClick={() => setActiveTaskId(task.id)}
              >
                <div className="task-sidebar-item-top">
                  <span className="task-sidebar-item-time">{task.time}</span>
                  <span
                    className="task-sidebar-item-priority"
                    style={{ color: meta.color }}
                  >
                    <Flag style={{ width: 10, height: 10 }} />
                    {meta.label}
                  </span>
                </div>
                <span className="task-sidebar-item-title">{task.title}</span>
                {task.repeat && (
                  <span className="task-sidebar-item-repeat">
                    <RefreshCw style={{ width: 10, height: 10 }} />
                    {task.repeat}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </aside>

      {/* ---- Right editor ---- */}
      <div className="task-editor">
        {/* Prompt area */}
        <div className="task-prompt-section">
          <div className="task-prompt-box">
            <textarea
              className="task-prompt-textarea"
              rows={1}
              placeholder="用自然语言描述你要创建或修改的任务…"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
            />
            <div className="task-prompt-actions">
              <div className="task-prompt-left">
                <button
                  className="task-param-toggle"
                  type="button"
                  onClick={() => setParamsOpen((v) => !v)}
                >
                  <Pencil style={iconSize} />
                  <span>参数预设</span>
                  {paramsOpen ? (
                    <ChevronUp style={{ width: 12, height: 12 }} />
                  ) : (
                    <ChevronDown style={{ width: 12, height: 12 }} />
                  )}
                </button>
              </div>
              <button
                className={`task-send-btn ${prompt.trim().length === 0 ? 'disabled' : ''}`}
                type="button"
              >
                <ArrowUp style={{ width: 18, height: 18 }} />
              </button>
            </div>
          </div>
        </div>

        {/* Parameter presets — collapsible */}
        <div className={`task-params-panel ${paramsOpen ? 'is-open' : ''}`}>
          <div className="task-params-inner">
            <div className="task-params-grid">
              <label className="task-param-field">
                <span className="task-param-label">
                  <RefreshCw style={{ width: 13, height: 13 }} />
                  重复规则
                </span>
                <input
                  className="task-param-input"
                  placeholder="如：每周一上午 09:00，或 每天"
                  defaultValue={activeTask.repeat ?? ''}
                />
              </label>

              <label className="task-param-field">
                <span className="task-param-label">
                  <Tag style={{ width: 13, height: 13 }} />
                  任务名称
                </span>
                <input
                  className="task-param-input"
                  placeholder="输入任务名称"
                  defaultValue={activeTask.title}
                />
              </label>

              <label className="task-param-field full-width">
                <span className="task-param-label">
                  <StickyNote style={{ width: 13, height: 13 }} />
                  备注
                </span>
                <textarea
                  className="task-param-textarea"
                  rows={2}
                  placeholder="添加备注说明…"
                  defaultValue={activeTask.notes ?? ''}
                />
              </label>

              <label className="task-param-field">
                <span className="task-param-label">
                  <Flag style={{ width: 13, height: 13 }} />
                  优先级
                </span>
                <input
                  className="task-param-input"
                  placeholder="高 / 中 / 低"
                  defaultValue={priorityMeta[activeTask.priority].label}
                />
              </label>

              <label className="task-param-field">
                <span className="task-param-label">
                  <Clock style={{ width: 13, height: 13 }} />
                  时间
                </span>
                <input
                  className="task-param-input"
                  placeholder="如：下午 3 点"
                  defaultValue={activeTask.time}
                />
              </label>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
