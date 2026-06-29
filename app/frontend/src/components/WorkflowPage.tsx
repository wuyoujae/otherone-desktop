import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  ListTodo,
  Plus,
  X,
} from 'lucide-react';
import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import { loadWorkflowTasksForRangeFromStorage } from '../services/workflowStorage';
import type { ModelOption } from '../types/apiConfig';
import type { WorkflowTask } from '../types/workflow';
import { resolveWorkflowTaskToneClasses } from '../utils/workflowTaskColors';
import { CustomSelect } from './CustomControls';
import { TaskView } from './TaskView';

const iconSize = { width: 16, height: 16 };
const noWorkflowModelOption = { label: '未配置 Todo 模型', value: 'none' };

type WorkflowView = 'calendar' | 'task';

type WorkflowPageProps = {
  modelOptions: ModelOption[];
  onNotice?: (type: 'success' | 'warn' | 'fail' | 'info', title: string, description?: string) => void;
  onClose: () => void;
  onWorkflowModelChange: (modelId: string) => void;
  workflowModelId: string;
};

const WEEKDAYS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const WEEKDAYS_SHORT = ['一', '二', '三', '四', '五', '六', '日'];

// ----- helpers -----

function isSameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

function formatDateKey(date: Date) {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function parseTaskDate(value?: string | null) {
  if (!value) {
    return null;
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function taskStartAt(task: WorkflowTask) {
  return task.startAt || task.scheduledAt || null;
}

function taskStartClock(task: WorkflowTask) {
  const start = parseTaskDate(taskStartAt(task));

  if (!start) {
    return null;
  }

  const hour = `${start.getHours()}`.padStart(2, '0');
  const minute = `${start.getMinutes()}`.padStart(2, '0');
  return `${hour}:${minute}`;
}

function taskEndClock(task: WorkflowTask) {
  const end = parseTaskDate(task.endAt);

  if (!end) {
    return null;
  }

  const hour = `${end.getHours()}`.padStart(2, '0');
  const minute = `${end.getMinutes()}`.padStart(2, '0');
  return `${hour}:${minute}`;
}

function taskOccursOnDate(task: WorkflowTask, day: Date) {
  const dayKey = formatDateKey(day);
  const occurrenceDate = task.occurrenceDate;

  if (occurrenceDate) {
    return occurrenceDate === dayKey;
  }

  const start = parseTaskDate(taskStartAt(task));

  if (start) {
    return formatDateKey(start) === dayKey;
  }

  const repeatStart = task.repeatStartDate;
  const repeatEnd = task.repeatEndDate;

  if (repeatStart && repeatEnd && repeatStart !== repeatEnd) {
    return dayKey >= repeatStart && dayKey <= repeatEnd;
  }

  return false;
}

function taskTitle(task: WorkflowTask) {
  return task.title.trim() || task.prompt.trim() || '未命名任务';
}

function taskPreview(task: WorkflowTask) {
  return task.content
    .split('\n')
    .map((line) => line.replace(/^[-*]\s*/, '').trim())
    .filter(Boolean)
    .slice(0, 2)
    .join(' / ') || task.prompt.trim();
}

type CalendarTaskBlock = {
  tasks: WorkflowTask[];
  toneClasses: string[];
  time: string;
};

type CalendarTaskPopover = {
  left: number;
  task: WorkflowTask;
  time: string;
  top: number;
};

function buildCalendarBlocks(day: Date, tasks: WorkflowTask[]): CalendarTaskBlock[] {
  const blocks = new Map<string, WorkflowTask[]>();

  tasks.forEach((task) => {
    if (!taskOccursOnDate(task, day)) {
      return;
    }

    const time = taskStartClock(task);

    if (!time) {
      return;
    }

    const current = blocks.get(time) ?? [];
    current.push(task);
    blocks.set(time, current);
  });

  return Array.from(blocks.entries())
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([time, blockTasks]) => {
      const sortedTasks = blockTasks.sort((a, b) => taskTitle(a).localeCompare(taskTitle(b), 'zh-CN'));

      return {
        time,
        tasks: sortedTasks,
        toneClasses: resolveWorkflowTaskToneClasses(sortedTasks),
      };
    });
}

function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

function startDayOfMonth(year: number, month: number): number {
  const jsDay = new Date(year, month, 1).getDay();
  return jsDay === 0 ? 6 : jsDay - 1;
}

// ----- DatePicker popup -----

type DatePickerProps = {
  anchorRef: React.RefObject<HTMLButtonElement | null>;
  onChange: (date: Date) => void;
  onClose: () => void;
  open: boolean;
  selected: Date;
};

function DatePicker({ anchorRef, onChange, onClose, open, selected }: DatePickerProps) {
  const [viewYear, setViewYear] = useState(selected.getFullYear());
  const [viewMonth, setViewMonth] = useState(selected.getMonth());
  const popupRef = useRef<HTMLDivElement | null>(null);

  const today = new Date();
  const monthDays = daysInMonth(viewYear, viewMonth);
  const startDay = startDayOfMonth(viewYear, viewMonth);
  const totalCells = Math.ceil((startDay + monthDays) / 7) * 7;

  useEffect(() => {
    if (open && anchorRef.current && popupRef.current) {
      const rect = anchorRef.current.getBoundingClientRect();
      const popup = popupRef.current;
      const left = rect.left + rect.width / 2;
      popup.style.top = `${rect.bottom + 8}px`;
      popup.style.left = `${left}px`;
    }
  }, [open, anchorRef]);

  const prevMonth = () => {
    if (viewMonth === 0) {
      setViewYear((y) => y - 1);
      setViewMonth(11);
    } else {
      setViewMonth((m) => m - 1);
    }
  };

  const nextMonth = () => {
    if (viewMonth === 11) {
      setViewYear((y) => y + 1);
      setViewMonth(0);
    } else {
      setViewMonth((m) => m + 1);
    }
  };

  const monthNames = Array.from({ length: 12 }, (_, i) => {
    const d = new Date(2024, i, 1);
    return d.toLocaleDateString('zh-CN', { month: 'long' });
  });

  return (
    <>
      <div className={`datepicker-backdrop ${open ? 'is-open' : ''}`} onClick={onClose} />
      <div
        className={`datepicker-popup ${open ? 'is-open' : ''}`}
        ref={popupRef}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="datepicker-header">
          <button className="datepicker-nav-btn" type="button" onClick={prevMonth}>
            <ChevronLeft style={{ width: 16, height: 16 }} />
          </button>
          <span className="datepicker-month-label">
            {viewYear}年 {monthNames[viewMonth]}
          </span>
          <button className="datepicker-nav-btn" type="button" onClick={nextMonth}>
            <ChevronRight style={{ width: 16, height: 16 }} />
          </button>
        </div>

        <div className="datepicker-grid">
          {WEEKDAYS_SHORT.map((w) => (
            <div className="datepicker-weekday" key={w}>
              {w}
            </div>
          ))}
          {Array.from({ length: totalCells }, (_, i) => {
            const dayNum = i - startDay + 1;
            const isValid = dayNum >= 1 && dayNum <= monthDays;
            const date = new Date(viewYear, viewMonth, dayNum);
            const isToday = isSameDay(date, today);
            const isSel = isSameDay(date, selected);

            return (
              <button
                className={`datepicker-day ${isToday ? 'is-today' : ''} ${isSel ? 'is-selected' : ''}`}
                disabled={!isValid}
                key={i}
                type="button"
                onClick={() => {
                  if (isValid) {
                    onChange(date);
                    onClose();
                  }
                }}
              >
                {isValid ? dayNum : ''}
              </button>
            );
          })}
        </div>
      </div>
    </>
  );
}

// ----- Main component -----

export function WorkflowPage({
  modelOptions,
  onClose,
  onNotice,
  onWorkflowModelChange,
  workflowModelId,
}: WorkflowPageProps) {
  const today = useMemo(() => new Date(), []);
  const [selectedDate, setSelectedDate] = useState(today);
  const [viewMode, setViewMode] = useState<WorkflowView>('calendar');
  const [animDir, setAnimDir] = useState<1 | -1 | 0>(0);
  const [datepickerOpen, setDatepickerOpen] = useState(false);
  const [focusMode, setFocusMode] = useState(true);
  const [newTaskRequestId, setNewTaskRequestId] = useState(0);
  const [selectedTaskRequestId, setSelectedTaskRequestId] = useState<string | null>(null);
  const [calendarTasks, setCalendarTasks] = useState<WorkflowTask[]>([]);
  const [calendarLoading, setCalendarLoading] = useState(false);
  const [calendarPopover, setCalendarPopover] = useState<CalendarTaskPopover | null>(null);
  const [calendarPopoverClosing, setCalendarPopoverClosing] = useState(false);
  const dateButtonRef = useRef<HTMLButtonElement | null>(null);
  const calendarPopoverCloseTimerRef = useRef<number | null>(null);
  const workflowModelOptions = modelOptions.length
    ? modelOptions.map((model) => ({ label: model.label, value: model.id }))
    : [noWorkflowModelOption];
  const selectedWorkflowModelId = workflowModelOptions.some((option) => option.value === workflowModelId)
    ? workflowModelId
    : workflowModelOptions[0].value;

  // Compute the 7 days centered on the selected date
  const visibleDays = useMemo(() => {
    const days: Date[] = [];
    const start = new Date(selectedDate);
    start.setDate(selectedDate.getDate() - 3);
    for (let i = 0; i < 7; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      days.push(d);
    }
    return days;
  }, [selectedDate]);

  const centerIndex = 3;

  useEffect(() => {
    if (viewMode !== 'calendar' || visibleDays.length === 0) {
      return;
    }

    let cancelled = false;
    const startDate = formatDateKey(visibleDays[0]);
    const endDate = formatDateKey(visibleDays[visibleDays.length - 1]);

    setCalendarLoading(true);
    loadWorkflowTasksForRangeFromStorage(startDate, endDate)
      .then((tasks) => {
        if (!cancelled) {
          setCalendarTasks(tasks);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setCalendarTasks([]);
          onNotice?.('fail', '日历任务加载失败', error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setCalendarLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [onNotice, viewMode, visibleDays]);

  const goToToday = useCallback(() => {
    setAnimDir(0);
    setSelectedDate(new Date());
  }, []);

  const openNewTaskPrompt = useCallback(() => {
    setSelectedTaskRequestId(null);
    setViewMode('task');
    setNewTaskRequestId((id) => id + 1);
  }, []);

  const openTaskPrompt = useCallback(() => {
    setSelectedTaskRequestId(null);
    setViewMode('task');
  }, []);

  const openTaskFromCalendar = useCallback((taskId: string, taskDate: Date) => {
    setSelectedTaskRequestId(taskId);
    setSelectedDate(taskDate);
    setCalendarPopoverClosing(false);
    setCalendarPopover(null);
    setViewMode('task');
  }, []);

  const showCalendarPopover = useCallback((task: WorkflowTask, time: string, element: HTMLElement) => {
    if (calendarPopoverCloseTimerRef.current !== null) {
      window.clearTimeout(calendarPopoverCloseTimerRef.current);
      calendarPopoverCloseTimerRef.current = null;
    }

    const rect = element.getBoundingClientRect();
    const popoverWidth = 174;
    const viewportPadding = 12;
    const left = Math.min(
      Math.max(rect.left + 12, viewportPadding),
      Math.max(viewportPadding, window.innerWidth - popoverWidth - viewportPadding),
    );
    const top = rect.bottom + 6;

    setCalendarPopoverClosing(false);
    setCalendarPopover({ left, task, time, top });
  }, []);

  const hideCalendarPopover = useCallback(() => {
    if (calendarPopoverCloseTimerRef.current !== null) {
      window.clearTimeout(calendarPopoverCloseTimerRef.current);
    }

    setCalendarPopoverClosing(true);
    calendarPopoverCloseTimerRef.current = window.setTimeout(() => {
      setCalendarPopover(null);
      setCalendarPopoverClosing(false);
      calendarPopoverCloseTimerRef.current = null;
    }, 180);
  }, []);

  useEffect(() => () => {
    if (calendarPopoverCloseTimerRef.current !== null) {
      window.clearTimeout(calendarPopoverCloseTimerRef.current);
    }
  }, []);

  const shiftDays = useCallback(
    (dir: 1 | -1) => {
      if (animDir !== 0) return;
      const next = new Date(selectedDate);
      next.setDate(next.getDate() + dir);
      // Update date immediately so new content renders with the animation
      setSelectedDate(next);
      setAnimDir(dir);
      // Clear animation flag after it finishes
      setTimeout(() => setAnimDir(0), 280);
    },
    [selectedDate, animDir],
  );

  const year = selectedDate.getFullYear();
  const month = selectedDate.getMonth() + 1;
  const day = selectedDate.getDate();
  const dateDisplay = `${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
  const weekdayCN = WEEKDAYS[selectedDate.getDay() === 0 ? 6 : selectedDate.getDay() - 1];

  // Right arrow (dir=1): new content enters from the right → enter-from-right
  // Left arrow (dir=-1): new content enters from the left → enter-from-left
  const animClass = animDir === 1 ? 'enter-from-right' : animDir === -1 ? 'enter-from-left' : '';

  return (
    <div className="workflow-page">
      {/* ---- Top navigation ---- */}
      <div className="workflow-nav">
        <div className="workflow-nav-left">
          <div className="workflow-view-toggle">
            <button
              className={`workflow-toggle-btn ${viewMode === 'calendar' ? 'active' : ''}`}
              type="button"
              onClick={() => setViewMode('calendar')}
            >
              <CalendarDays style={iconSize} />
              <span>日历</span>
            </button>
            <button
              className={`workflow-toggle-btn ${viewMode === 'task' ? 'active' : ''}`}
              type="button"
              onClick={openTaskPrompt}
            >
              <ListTodo style={iconSize} />
              <span>任务</span>
            </button>
          </div>
        </div>

        <div className="workflow-nav-center">
          <button
            className="workflow-date-arrow"
            type="button"
            aria-label="前一天"
            onClick={() => shiftDays(-1)}
          >
            <ChevronLeft style={{ width: 20, height: 20 }} />
          </button>

          <button
            ref={dateButtonRef}
            className="workflow-date-display"
            type="button"
            onClick={() => setDatepickerOpen(true)}
          >
            <span className="workflow-date-year">{year}</span>
            <span className="workflow-date-md">{dateDisplay}</span>
            <span className="workflow-date-weekday">{weekdayCN}</span>
          </button>

          <button
            className="workflow-date-arrow"
            type="button"
            aria-label="后一天"
            onClick={() => shiftDays(1)}
          >
            <ChevronRight style={{ width: 20, height: 20 }} />
          </button>

          <DatePicker
            anchorRef={dateButtonRef}
            onChange={(date) => {
              setAnimDir(0);
              setSelectedDate(date);
            }}
            onClose={() => setDatepickerOpen(false)}
            open={datepickerOpen}
            selected={selectedDate}
          />
        </div>

        <div className="workflow-nav-right">
          <div className="workflow-model-control">
            <CustomSelect
              label="Todo AI 模型"
              options={workflowModelOptions}
              value={selectedWorkflowModelId}
              onChange={(value) => onWorkflowModelChange(value === 'none' ? '' : value)}
            />
          </div>
          <button
            className="workflow-today-btn"
            type="button"
            onClick={goToToday}
            title="回到今天"
          >
            今天
          </button>
          <button
            className={`workflow-icon-btn ${focusMode ? 'active' : ''}`}
            type="button"
            title={focusMode ? '关闭专注模式' : '打开专注模式'}
            aria-label={focusMode ? '关闭专注模式' : '打开专注模式'}
            onClick={() => setFocusMode((v) => !v)}
          >
            {focusMode ? <Eye style={{ width: 18, height: 18 }} /> : <EyeOff style={{ width: 18, height: 18 }} />}
          </button>
          <button
            className="workflow-icon-btn"
            type="button"
            title="新增任务"
            aria-label="新增任务"
            onClick={openNewTaskPrompt}
          >
            <Plus style={{ width: 18, height: 18 }} />
          </button>
          <button className="workflow-icon-btn" type="button" title="关闭" aria-label="关闭工作流" onClick={onClose}>
            <X style={{ width: 18, height: 18 }} />
          </button>
        </div>
      </div>

      {/* ---- Calendar view ---- */}
      {viewMode === 'calendar' && (
        <div className={`workflow-calendar ${animClass}`}>
          {/* Column headers — animates in sync with body */}
          <div className="workflow-weekday-row">
            {visibleDays.map((d, i) => {
              const isCenter = focusMode && i === centerIndex;
              const isToday = isSameDay(d, today);
              return (
                <div
                  className={`workflow-weekday-cell ${isCenter ? 'is-center' : ''} ${!focusMode || isCenter ? '' : 'is-dimmed'} ${isToday ? 'is-today' : ''}`}
                  key={i}
                >
                  <span className="workflow-weekday-name">
                    {WEEKDAYS[d.getDay() === 0 ? 6 : d.getDay() - 1]}
                  </span>
                  <span className="workflow-weekday-date">
                    {d.getMonth() + 1}/{d.getDate()}
                  </span>
                </div>
              );
            })}
          </div>

          {/* Swimlane body — scrollable, animates in sync with header */}
          <div className="workflow-swimlane-body">
            <div className="workflow-swimlane-grid">
              {visibleDays.map((d, i) => {
                const isCenter = focusMode && i === centerIndex;
                const isToday = isSameDay(d, today);
                const blocks = buildCalendarBlocks(d, calendarTasks);
                return (
                  <div className={`workflow-swimlane-column ${isCenter ? 'is-center' : ''}`} key={i}>
                    <div className="workflow-slot-inner">
                      {calendarLoading ? (
                        <div className="workflow-calendar-empty">加载中</div>
                      ) : blocks.length === 0 ? (
                        <div className="workflow-calendar-empty">{isToday ? '今天没有任务' : '暂无任务'}</div>
                      ) : (
                        blocks.map((block) => (
                          <div className="workflow-calendar-time-block" key={block.time}>
                            <div className="workflow-calendar-time-label">{block.time}</div>
                            <div className="workflow-calendar-task-stack">
                              {block.tasks.map((task, taskIndex) => (
                                <button
                                  className={`workflow-calendar-task ${block.toneClasses[taskIndex]} ${task.status === 'completed' ? 'is-completed' : ''}`}
                                  key={task.id}
                                  type="button"
                                  title={taskTitle(task)}
                                  onClick={() => openTaskFromCalendar(task.id, d)}
                                  onFocus={(event) => showCalendarPopover(task, block.time, event.currentTarget)}
                                  onBlur={hideCalendarPopover}
                                  onMouseEnter={(event) => showCalendarPopover(task, block.time, event.currentTarget)}
                                  onMouseLeave={hideCalendarPopover}
                                >
                                  <span className="workflow-calendar-task-title">{taskTitle(task)}</span>
                                  {taskEndClock(task) && (
                                    <span className="workflow-calendar-task-time">{taskStartClock(task)}-{taskEndClock(task)}</span>
                                  )}
                                </button>
                              ))}
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                    {/* Mask only when focus mode is on and column is not center */}
                    {focusMode && !isCenter && <div className="workflow-column-mask" />}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}

      {/* ---- Task view ---- */}
      {viewMode === 'task' && (
        <TaskView
          selectedDate={selectedDate}
          selectedTaskId={selectedTaskRequestId}
          newTaskRequestId={newTaskRequestId}
          onNotice={onNotice}
          workflowModelId={selectedWorkflowModelId === 'none' ? '' : selectedWorkflowModelId}
        />
      )}

      {calendarPopover && (
        <div
          className={`workflow-calendar-task-popover ${calendarPopoverClosing ? 'is-closing' : 'is-open'}`}
          role="tooltip"
          style={{ left: calendarPopover.left, top: calendarPopover.top }}
        >
          <strong>{taskTitle(calendarPopover.task)}</strong>
          <span>{calendarPopover.time}{taskEndClock(calendarPopover.task) ? `-${taskEndClock(calendarPopover.task)}` : ''}</span>
          <span>{taskPreview(calendarPopover.task)}</span>
        </div>
      )}
    </div>
  );
}
