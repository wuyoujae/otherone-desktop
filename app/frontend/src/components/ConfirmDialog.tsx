import { TriangleAlert } from 'lucide-react';
import { useEffect, useRef } from 'react';

type ConfirmDialogProps = {
  cancelLabel?: string;
  confirmLabel?: string;
  description: string;
  open: boolean;
  title: string;
  tone?: 'default' | 'warning' | 'danger';
  onCancel: () => void;
  onConfirm: () => void;
};

export function ConfirmDialog({
  cancelLabel = '取消',
  confirmLabel = '确认',
  description,
  open,
  title,
  tone = 'default',
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  const confirmButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return undefined;

    confirmButtonRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCancel();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onCancel, open]);

  if (!open) {
    return null;
  }

  return (
    <div className="confirm-dialog-backdrop" role="presentation" onMouseDown={onCancel}>
      <section
        aria-describedby="confirm-dialog-desc"
        aria-labelledby="confirm-dialog-title"
        aria-modal="true"
        className={`confirm-dialog confirm-dialog-${tone}`}
        role="dialog"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="confirm-dialog-icon" aria-hidden="true">
          <TriangleAlert style={{ width: 18, height: 18 }} />
        </div>
        <div className="confirm-dialog-content">
          <h2 id="confirm-dialog-title">{title}</h2>
          <p id="confirm-dialog-desc">{description}</p>
          <div className="confirm-dialog-actions">
            <button className="confirm-dialog-btn secondary" type="button" onClick={onCancel}>
              {cancelLabel}
            </button>
            <button
              className="confirm-dialog-btn primary"
              type="button"
              ref={confirmButtonRef}
              onClick={onConfirm}
            >
              {confirmLabel}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
