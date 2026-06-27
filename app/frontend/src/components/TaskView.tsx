import {
  ArrowUp,
  CalendarClock,
  Check,
  ChevronDown,
  ChevronUp,
  Clock,
  FileText,
  Flag,
  Loader2,
  MessageSquare,
  Pencil,
  Plus,
  Repeat,
  RefreshCw,
  StickyNote,
  Tag,
  Timer,
  Trash2,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  createWorkflowTaskInStorage,
  deleteWorkflowTaskInStorage,
  loadWorkflowTasksForRangeFromStorage,
  updateWorkflowTaskStatusInStorage,
} from '../services/workflowStorage';
import type { WorkflowTask, WorkflowTaskStatus } from '../types/workflow';
import { resolveWorkflowTaskToneClasses } from '../utils/workflowTaskColors';

const iconSize = { width: 16, height: 16 };

const statusMeta: Record<WorkflowTask['status'], { label: string; color: string }> = {
  pending: { label: '待处理', color: '#60a5fa' },
  completed: { label: '已完成', color: '#22c55e' },
};

type WorkflowNoticeKind = 'success' | 'warn' | 'fail' | 'info';

type TaskViewProps = {
  newTaskRequestId?: number;
  onNotice?: (type: WorkflowNoticeKind, title: string, description?: string) => void;
  selectedDate: Date;
  selectedTaskId?: string | null;
};

type WorkflowTaskMetadata = {
  endAt?: string | null;
  originalPrompt?: string;
  priority?: string;
  repeatEndDate?: string | null;
  repeatStartDate?: string | null;
  scheduledAt?: string | null;
  startAt?: string | null;
  summary?: string;
  tags?: string[];
  timeText?: string | null;
};

type TaskTimeDisplay = {
  label: string;
  title: string;
  type: 'point' | 'range' | 'repeat-point' | 'repeat-range' | 'none';
};

function taskTitle(task: WorkflowTask | null) {
  return task?.title.trim() || task?.prompt.trim() || '未命名任务';
}

function parseTaskMetadata(task: WorkflowTask | null): WorkflowTaskMetadata {
  if (!task?.metadataJson.trim()) {
    return {};
  }

  try {
    const parsed = JSON.parse(task.metadataJson);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function priorityLabel(value: string | undefined) {
  switch (value) {
    case 'high':
      return '高';
    case 'medium':
      return '中';
    case 'low':
      return '低';
    default:
      return '未定';
  }
}

function priorityColor(value: string | undefined) {
  switch (value) {
    case 'high':
      return '#ff5555';
    case 'medium':
      return '#fbbf24';
    case 'low':
      return '#9ca3af';
    default:
      return '#60a5fa';
  }
}

function formatDateKey(date: Date) {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function formatClockTime(value: string) {
  const date = new Date(value);

  if (Number.isNaN(date.getTime())) {
    return '';
  }

  const hour = `${date.getHours()}`.padStart(2, '0');
  const minute = `${date.getMinutes()}`.padStart(2, '0');
  return `${hour}:${minute}`;
}

function formatTaskTime(value: string) {
  const date = new Date(value);

  if (Number.isNaN(date.getTime())) {
    return '未定时间';
  }

  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  const hour = `${date.getHours()}`.padStart(2, '0');
  const minute = `${date.getMinutes()}`.padStart(2, '0');
  const currentYear = new Date().getFullYear();
  const datePart = date.getFullYear() === currentYear
    ? `${month}月${day}日`
    : `${date.getFullYear()}年${month}月${day}日`;

  return `${datePart} ${hour}:${minute}`;
}

function taskStartAt(task: WorkflowTask, metadata?: WorkflowTaskMetadata) {
  return task.startAt || task.scheduledAt || metadata?.startAt || metadata?.scheduledAt || null;
}

function taskEndAt(task: WorkflowTask, metadata?: WorkflowTaskMetadata) {
  return task.endAt || metadata?.endAt || null;
}

function hasRepeatDates(task: WorkflowTask, metadata?: WorkflowTaskMetadata) {
  if (task.occurrenceDate) {
    return false;
  }

  const start = task.repeatStartDate || metadata?.repeatStartDate;
  const end = task.repeatEndDate || metadata?.repeatEndDate;
  return Boolean(start && end && start !== end);
}

function taskTimeDisplay(task: WorkflowTask, metadata: WorkflowTaskMetadata): TaskTimeDisplay {
  const startAt = taskStartAt(task, metadata);
  const endAt = taskEndAt(task, metadata);
  const startLabel = startAt ? formatClockTime(startAt) : '';
  const endLabel = endAt ? formatClockTime(endAt) : '';
  const hasRepeat = hasRepeatDates(task, metadata);

  if (startLabel && endLabel) {
    return {
      label: `${startLabel}-${endLabel}`,
      title: `${startLabel}-${endLabel}`,
      type: hasRepeat ? 'repeat-range' : 'range',
    };
  }

  if (startLabel) {
    return {
      label: startLabel,
      title: startLabel,
      type: hasRepeat ? 'repeat-point' : 'point',
    };
  }

  return { label: '未定时间', title: '未定时间', type: 'none' };
}

function taskTimeIcon(type: TaskTimeDisplay['type']) {
  switch (type) {
    case 'range':
      return <Timer style={{ width: 10, height: 10 }} />;
    case 'repeat-point':
    case 'repeat-range':
      return <Repeat style={{ width: 10, height: 10 }} />;
    case 'point':
    case 'none':
    default:
      return <Clock style={{ width: 10, height: 10 }} />;
  }
}

function taskContentItems(task: WorkflowTask | null) {
  if (!task?.content.trim()) {
    return [];
  }

  return task.content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => line.replace(/^[-*•]\s*/, '').trim())
    .filter(Boolean);
}

function metadataTags(metadata: WorkflowTaskMetadata) {
  return Array.isArray(metadata.tags)
    ? metadata.tags.map((tag) => tag.trim()).filter(Boolean)
    : [];
}

function errorToMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function TaskView({ newTaskRequestId = 0, onNotice, selectedDate, selectedTaskId = null }: TaskViewProps) {
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const [paramsOpen, setParamsOpen] = useState(false);
  const [prompt, setPrompt] = useState('');
  const [tasks, setTasks] = useState<WorkflowTask[]>([]);
  const [isLoadingTasks, setIsLoadingTasks] = useState(true);
  const [isCreatingTask, setIsCreatingTask] = useState(false);
  const [moreInfoOpen, setMoreInfoOpen] = useState(false);
  const [updatingTaskIds, setUpdatingTaskIds] = useState<Set<string>>(() => new Set());
  const [deletingTaskIds, setDeletingTaskIds] = useState<Set<string>>(() => new Set());
  const selectedDateKey = useMemo(() => formatDateKey(selectedDate), [selectedDate]);

  useEffect(() => {
    let cancelled = false;

    setIsLoadingTasks(true);
    loadWorkflowTasksForRangeFromStorage(selectedDateKey, selectedDateKey)
      .then((loadedTasks) => {
        if (cancelled) {
          return;
        }

        setTasks(loadedTasks);
        setActiveTaskId((current) => selectedTaskId ?? current ?? null);
      })
      .catch((error) => {
        if (!cancelled) {
          onNotice?.('fail', '读取任务失败', errorToMessage(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoadingTasks(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [newTaskRequestId, onNotice, selectedDateKey, selectedTaskId]);

  const activeTask = useMemo(
    () => (activeTaskId ? tasks.find((task) => task.id === activeTaskId) ?? null : null),
    [activeTaskId, tasks],
  );
  const taskToneClasses = useMemo(() => resolveWorkflowTaskToneClasses(tasks), [tasks]);
  const isEditingTask = Boolean(activeTaskId && activeTask);
  const selectedDateLabel = useMemo(
    () => selectedDate.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric' }),
    [selectedDate],
  );
  const activeTaskMetadata = useMemo(() => parseTaskMetadata(activeTask), [activeTask]);
  const activeTaskContentItems = useMemo(() => taskContentItems(activeTask), [activeTask]);
  const activeTaskTags = useMemo(() => metadataTags(activeTaskMetadata), [activeTaskMetadata]);

  useEffect(() => {
    setPrompt('');
    setParamsOpen(false);
    setMoreInfoOpen(false);
  }, [activeTask?.id]);

  useEffect(() => {
    if (newTaskRequestId === 0) {
      return;
    }

    setActiveTaskId(null);
    setPrompt('');
    setParamsOpen(false);
    setMoreInfoOpen(false);
  }, [newTaskRequestId]);

  useEffect(() => {
    if (!selectedTaskId) {
      return;
    }

    setActiveTaskId(selectedTaskId);
    setPrompt('');
    setParamsOpen(false);
    setMoreInfoOpen(false);
  }, [selectedTaskId]);

  const updateTaskStatus = useCallback(async (task: WorkflowTask, status: WorkflowTaskStatus) => {
    if (updatingTaskIds.has(task.id)) {
      return;
    }

    setUpdatingTaskIds((current) => new Set(current).add(task.id));

    try {
      const updatedTask = await updateWorkflowTaskStatusInStorage(task.id, status);

      if (!updatedTask) {
        onNotice?.('warn', '无法更新任务', '当前环境无法保存任务状态，请在桌面应用中使用。');
        return;
      }

      setTasks((current) => current.map((item) => (item.id === updatedTask.id ? updatedTask : item)));
      onNotice?.('success', status === 'completed' ? '任务已完成' : '任务已恢复');
    } catch (error) {
      onNotice?.('fail', '更新任务失败', errorToMessage(error));
    } finally {
      setUpdatingTaskIds((current) => {
        const next = new Set(current);
        next.delete(task.id);
        return next;
      });
    }
  }, [onNotice, updatingTaskIds]);

  const deleteTask = useCallback(async (task: WorkflowTask) => {
    if (deletingTaskIds.has(task.id)) {
      return;
    }

    setDeletingTaskIds((current) => new Set(current).add(task.id));

    try {
      const deleted = await deleteWorkflowTaskInStorage(task.id);

      if (!deleted) {
        onNotice?.('warn', '无法删除任务', '当前环境无法删除任务，请在桌面应用中使用。');
        return;
      }

      setTasks((current) => current.filter((item) => item.id !== task.id));
      setActiveTaskId((current) => (current === task.id ? null : current));
      onNotice?.('success', '任务已删除', taskTitle(task));
    } catch (error) {
      onNotice?.('fail', '删除任务失败', errorToMessage(error));
    } finally {
      setDeletingTaskIds((current) => {
        const next = new Set(current);
        next.delete(task.id);
        return next;
      });
    }
  }, [deletingTaskIds, onNotice]);

  const createTask = useCallback(async () => {
    const taskPrompt = prompt.trim();

    if (!taskPrompt || isCreatingTask) {
      return;
    }

    setIsCreatingTask(true);

    try {
      const createdTask = await createWorkflowTaskInStorage(taskPrompt);

      if (!createdTask) {
        onNotice?.('warn', '无法创建任务', '当前环境无法保存任务，请在桌面应用中使用。');
        return;
      }

      const refreshedTasks = await loadWorkflowTasksForRangeFromStorage(selectedDateKey, selectedDateKey);
      setTasks(refreshedTasks);
      setActiveTaskId(refreshedTasks.some((task) => task.id === createdTask.id) ? createdTask.id : null);
      setPrompt('');
      onNotice?.('success', '任务已创建', createdTask.title);
    } catch (error) {
      onNotice?.('fail', '创建任务失败', errorToMessage(error));
    } finally {
      setIsCreatingTask(false);
    }
  }, [isCreatingTask, onNotice, prompt, selectedDateKey]);

  const submitTaskPrompt = useCallback(async () => {
    const taskPrompt = prompt.trim();

    if (!taskPrompt || isCreatingTask) {
      return;
    }

    if (isEditingTask) {
      onNotice?.('info', '任务修改暂未接入', '当前只展示编辑入口，下一步再接入 AI 修改任务。');
      return;
    }

    await createTask();
  }, [createTask, isCreatingTask, isEditingTask, onNotice, prompt]);

  const sendDisabled = prompt.trim().length === 0 || isCreatingTask;
  const activeTaskStatus = activeTask ? statusMeta[activeTask.status] ?? statusMeta.pending : null;
  const activePriority = activeTaskMetadata.priority;
  const scheduledTime = activeTask && taskStartAt(activeTask, activeTaskMetadata)
    ? formatTaskTime(taskStartAt(activeTask, activeTaskMetadata)!)
    : '未定时间';

  return (
    <div className="task-view">
      <aside className="task-sidebar">
        <div className="task-sidebar-header">
          <div className="task-sidebar-heading">
            <span className="task-sidebar-title">今日任务</span>
            <span className="task-sidebar-count" title={`${selectedDateLabel} 的任务视图`}>
              {isLoadingTasks ? '读取中' : `${tasks.length} 项`}
            </span>
          </div>
          <button
            className={`task-sidebar-add-btn ${activeTaskId === null ? 'active' : ''}`}
            type="button"
            title="新建任务"
            aria-label="新建任务"
            onClick={() => {
              setActiveTaskId(null);
              setPrompt('');
            }}
          >
            <Plus style={{ width: 15, height: 15 }} />
          </button>
        </div>

        <div className="task-sidebar-list">
          {isLoadingTasks && <div className="task-sidebar-state">正在读取任务...</div>}
          {!isLoadingTasks && tasks.length === 0 && <div className="task-sidebar-state">暂无任务</div>}

          {!isLoadingTasks && tasks.map((task, taskIndex) => {
            const status = statusMeta[task.status] ?? statusMeta.pending;
            const metadata = parseTaskMetadata(task);
            const timeDisplay = taskTimeDisplay(task, metadata);
            const priority = metadata.priority;
            const isCompleted = task.status === 'completed';
            const isUpdating = updatingTaskIds.has(task.id);
            const isDeleting = deletingTaskIds.has(task.id);

            return (
              <div
                className={`task-sidebar-item ${taskToneClasses[taskIndex]} ${activeTaskId === task.id ? 'active' : ''} ${isCompleted ? 'is-completed' : ''}`}
                key={task.id}
                onClick={() => setActiveTaskId(task.id)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    setActiveTaskId(task.id);
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <div className="task-sidebar-item-top">
                  <span className={`task-sidebar-item-time time-${timeDisplay.type}`} title={timeDisplay.title}>
                    {taskTimeIcon(timeDisplay.type)}
                    {timeDisplay.label}
                  </span>
                  <span
                    className="task-sidebar-item-priority"
                    style={{ color: priority ? priorityColor(priority) : status.color }}
                  >
                    <Flag style={{ width: 10, height: 10 }} />
                    {priorityLabel(priority)}
                  </span>
                  <button
                    aria-label="删除任务"
                    className={`task-sidebar-delete-btn ${isDeleting ? 'is-deleting' : ''}`}
                    disabled={isDeleting}
                    onClick={(event) => {
                      event.stopPropagation();
                      void deleteTask(task);
                    }}
                    onKeyDown={(event) => event.stopPropagation()}
                    title="删除任务"
                    type="button"
                  >
                    <Trash2 style={{ width: 12, height: 12 }} />
                  </button>
                </div>
                <span className="task-sidebar-title-row">
                  <button
                    aria-label={isCompleted ? '取消完成任务' : '完成任务'}
                    aria-pressed={isCompleted}
                    className={`task-complete-check ${isCompleted ? 'checked' : ''} ${isUpdating ? 'is-updating' : ''}`}
                    disabled={isUpdating}
                    onClick={(event) => {
                      event.stopPropagation();
                      void updateTaskStatus(task, isCompleted ? 'pending' : 'completed');
                    }}
                    type="button"
                    title={isCompleted ? '取消完成' : '标记完成'}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.stopPropagation();
                      }
                    }}
                  >
                    {isCompleted && <Check style={{ width: 11, height: 11 }} />}
                  </button>
                  <span className="task-sidebar-item-title">{taskTitle(task)}</span>
                </span>
              </div>
            );
          })}
        </div>
      </aside>

      <div className="task-editor">
        <div className="task-prompt-section">
          <div className="task-prompt-box">
            <textarea
              aria-label={isEditingTask ? '任务修改指令' : '任务内容'}
              className="task-prompt-textarea"
              disabled={isCreatingTask}
              rows={1}
              placeholder={isEditingTask ? '修改你的任务，通过自然语言...' : '用自然语言描述你要创建的任务...'}
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
                  event.preventDefault();
                  void submitTaskPrompt();
                }
              }}
            />
            <div className="task-prompt-actions">
              <div className="task-prompt-left">
                {isEditingTask ? (
                  <span className="task-prompt-mode-label">
                    <Pencil style={iconSize} />
                    <span>自然语言修改</span>
                  </span>
                ) : (
                  <button className="task-param-toggle" type="button" onClick={() => setParamsOpen((value) => !value)}>
                    <Pencil style={iconSize} />
                    <span>参数预设</span>
                    {paramsOpen ? <ChevronUp style={{ width: 12, height: 12 }} /> : <ChevronDown style={{ width: 12, height: 12 }} />}
                  </button>
                )}
              </div>
              <button
                className={`task-send-btn ${sendDisabled ? 'disabled' : ''}`}
                disabled={sendDisabled}
                title={isCreatingTask ? '正在解析任务' : isEditingTask ? '提交修改指令' : '创建任务'}
                type="button"
                onClick={() => void submitTaskPrompt()}
              >
                {isCreatingTask ? (
                  <Loader2 style={{ width: 18, height: 18, animation: 'spin 0.8s linear infinite' }} />
                ) : (
                  <ArrowUp style={{ width: 18, height: 18 }} />
                )}
              </button>
            </div>
          </div>
        </div>

        {isEditingTask && activeTask ? (
          <section className={`task-detail-panel ${activeTask.status === 'completed' ? 'is-completed' : ''}`} key={activeTask.id} aria-label="任务详情">
            <div className="task-detail-header">
              <div className="task-detail-title-block">
                <span className="task-detail-kicker">当前任务</span>
                <h2 className="task-detail-title">{taskTitle(activeTask)}</h2>
              </div>
              {activeTaskStatus && (
                <button
                  className={`task-status-pill ${activeTask.status === 'completed' ? 'is-completed' : ''}`}
                  disabled={updatingTaskIds.has(activeTask.id)}
                  type="button"
                  onClick={() => void updateTaskStatus(activeTask, activeTask.status === 'completed' ? 'pending' : 'completed')}
                >
                  <span className={`task-complete-check ${activeTask.status === 'completed' ? 'checked' : ''}`}>
                    {activeTask.status === 'completed' && <Check style={{ width: 12, height: 12 }} />}
                  </span>
                  {activeTaskStatus.label}
                </button>
              )}
            </div>

            <div className="task-detail-summary-grid">
              <div className="task-detail-summary-item">
                <Clock style={{ width: 15, height: 15 }} />
                <span>计划时间</span>
                <strong>{scheduledTime}</strong>
              </div>
              <div className="task-detail-summary-item">
                <Flag style={{ width: 15, height: 15 }} />
                <span>优先级</span>
                <strong style={{ color: priorityColor(activePriority) }}>{priorityLabel(activePriority)}</strong>
              </div>
            </div>

            <div className="task-detail-section">
              <div className="task-detail-section-title">
                <FileText style={{ width: 15, height: 15 }} />
                <span>任务内容</span>
              </div>
              {activeTaskContentItems.length > 0 ? (
                <ul className="task-content-list">
                  {activeTaskContentItems.map((item, index) => (
                    <li key={`${item}-${index}`}>{item}</li>
                  ))}
                </ul>
              ) : (
                <div className="task-detail-empty">暂无任务内容</div>
              )}
            </div>

            <div className={`task-more-panel ${moreInfoOpen ? 'is-open' : ''}`}>
              <button className="task-param-toggle task-more-toggle" type="button" onClick={() => setMoreInfoOpen((value) => !value)}>
                <StickyNote style={iconSize} />
                <span>更多信息</span>
                {moreInfoOpen ? <ChevronUp style={{ width: 12, height: 12 }} /> : <ChevronDown style={{ width: 12, height: 12 }} />}
              </button>
              <div className="task-more-inner">
                <div className="task-detail-grid">
                  <div className="task-detail-section">
                    <div className="task-detail-section-title">
                      <CalendarClock style={{ width: 15, height: 15 }} />
                      <span>更新时间</span>
                    </div>
                    <p className="task-detail-text">{formatTaskTime(activeTask.updatedAt)}</p>
                  </div>
                  <div className="task-detail-section">
                    <div className="task-detail-section-title">
                      <StickyNote style={{ width: 15, height: 15 }} />
                      <span>摘要</span>
                    </div>
                    <p className="task-detail-text">{activeTaskMetadata.summary?.trim() || '暂无摘要'}</p>
                  </div>
                  <div className="task-detail-section">
                    <div className="task-detail-section-title">
                      <Tag style={{ width: 15, height: 15 }} />
                      <span>标签</span>
                    </div>
                    {activeTaskTags.length > 0 ? (
                      <div className="task-tag-list">
                        {activeTaskTags.map((tag) => (
                          <span className="task-tag-pill" key={tag}>{tag}</span>
                        ))}
                      </div>
                    ) : (
                      <div className="task-detail-empty">暂无标签</div>
                    )}
                  </div>
                  <div className="task-detail-section">
                    <div className="task-detail-section-title">
                      <MessageSquare style={{ width: 15, height: 15 }} />
                      <span>原始描述</span>
                    </div>
                    <p className="task-detail-text">{activeTaskMetadata.originalPrompt || activeTask.prompt}</p>
                  </div>
                </div>
              </div>
            </div>
          </section>
        ) : (
          <div className={`task-params-panel ${paramsOpen ? 'is-open' : ''}`}>
            <div className="task-params-inner">
              <div className="task-params-grid" key="create-task">
                <label className="task-param-field">
                  <span className="task-param-label">
                    <RefreshCw style={{ width: 13, height: 13 }} />
                    重复规则
                  </span>
                  <input className="task-param-input" placeholder="如：每周一上午 09:00，或 每天" defaultValue="" />
                </label>
                <label className="task-param-field">
                  <span className="task-param-label">
                    <Tag style={{ width: 13, height: 13 }} />
                    任务名称
                  </span>
                  <input className="task-param-input" placeholder="输入任务名称" defaultValue="" />
                </label>
                <label className="task-param-field full-width">
                  <span className="task-param-label">
                    <StickyNote style={{ width: 13, height: 13 }} />
                    备注
                  </span>
                  <textarea className="task-param-textarea" rows={2} placeholder="添加备注说明..." defaultValue="" />
                </label>
                <label className="task-param-field">
                  <span className="task-param-label">
                    <Flag style={{ width: 13, height: 13 }} />
                    优先级
                  </span>
                  <input className="task-param-input" placeholder="高 / 中 / 低" defaultValue="" />
                </label>
                <label className="task-param-field">
                  <span className="task-param-label">
                    <Clock style={{ width: 13, height: 13 }} />
                    时间
                  </span>
                  <input className="task-param-input" placeholder="如：下午 3 点" defaultValue="" />
                </label>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
