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
import { TaskView } from './TaskView';

const iconSize = { width: 16, height: 16 };

type WorkflowView = 'calendar' | 'task';

type WorkflowPageProps = {
  onClose: () => void;
};

const WEEKDAYS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const WEEKDAYS_SHORT = ['一', '二', '三', '四', '五', '六', '日'];

// 24 hours
const HOURS = Array.from({ length: 24 }, (_, i) => i);

// ----- helpers -----

function isSameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
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

export function WorkflowPage({ onClose }: WorkflowPageProps) {
  const today = useMemo(() => new Date(), []);
  const [selectedDate, setSelectedDate] = useState(today);
  const [viewMode, setViewMode] = useState<WorkflowView>('calendar');
  const [animDir, setAnimDir] = useState<1 | -1 | 0>(0);
  const [datepickerOpen, setDatepickerOpen] = useState(false);
  const [focusMode, setFocusMode] = useState(true);
  const dateButtonRef = useRef<HTMLButtonElement | null>(null);

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

  const goToToday = useCallback(() => {
    setAnimDir(0);
    setSelectedDate(new Date());
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
              onClick={() => setViewMode('task')}
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
          <button className="workflow-icon-btn" type="button" title="新增任务" aria-label="新增任务">
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
                return (
                  <div className={`workflow-swimlane-column ${isCenter ? 'is-center' : ''}`} key={i}>
                    <div className="workflow-slot-inner">
                      {HOURS.map((hour) => (
                        <div className="workflow-timeslot" key={hour}>
                          <span className="workflow-timeslot-label">
                            {String(hour).padStart(2, '0')}:00
                          </span>
                        </div>
                      ))}
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
        <TaskView selectedDate={selectedDate} />
      )}
    </div>
  );
}
