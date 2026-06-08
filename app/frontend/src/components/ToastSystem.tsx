import { CheckCircle, ShieldAlert, Sparkles, TriangleAlert, X } from 'lucide-react';
import type { CSSProperties, ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';

export type ToastKind = 'info' | 'success' | 'warn' | 'error';

export type ToastNotice = {
  id: string;
  type: ToastKind;
  title: string;
  description?: string;
};

type ToastViewportProps = {
  messages: ToastNotice[];
  onDismiss: (id: string) => void;
};

const toastIcons: Record<ToastKind, ReactNode> = {
  info: <Sparkles style={{ width: 18, height: 18 }} />,
  success: <CheckCircle style={{ width: 18, height: 18 }} />,
  warn: <TriangleAlert style={{ width: 18, height: 18 }} />,
  error: <ShieldAlert style={{ width: 18, height: 18 }} />,
};

export function ToastViewport({ messages, onDismiss }: ToastViewportProps) {
  const [paused, setPaused] = useState(false);
  const timerRef = useRef<number | null>(null);
  const active = messages[0];
  const visibleMessages = messages.slice(0, 4);

  useEffect(() => {
    if (timerRef.current) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }

    if (!active || paused) {
      return undefined;
    }

    timerRef.current = window.setTimeout(() => {
      onDismiss(active.id);
    }, 3000);

    return () => {
      if (timerRef.current) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [active, onDismiss, paused]);

  if (!active) {
    return null;
  }

  return (
    <div className="toast-envelope-viewport" aria-live="polite" aria-atomic="false">
      {visibleMessages.map((message, index) => {
        const isActive = index === 0;
        const toastStyle = {
          '--toast-depth': `${index * -14}px`,
          '--toast-layer': 10 - index,
          '--toast-opacity': 1 - index * 0.16,
          '--toast-scale': 1 - index * 0.035,
        } as CSSProperties;

        return (
          <div
            className={`toast-envelope-item toast-${message.type} ${isActive ? 'active' : 'queued'}`}
            key={message.id}
            style={toastStyle}
            onMouseEnter={isActive ? () => setPaused(true) : undefined}
            onMouseLeave={isActive ? () => setPaused(false) : undefined}
          >
            <div className="toast-envelope-inner">
              <div className="toast-icon">{toastIcons[message.type]}</div>
              <div className="toast-content">
                <div className="toast-title">{message.title}</div>
                {message.description && <div className="toast-desc">{message.description}</div>}
              </div>
              {isActive && (
                <button
                  className="toast-close"
                  type="button"
                  aria-label="关闭消息"
                  onClick={() => onDismiss(message.id)}
                >
                  <X style={{ width: 14, height: 14 }} />
                </button>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
