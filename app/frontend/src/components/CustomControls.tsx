import { Check, ChevronDown } from 'lucide-react';
import type { CSSProperties } from 'react';
import { useEffect, useRef, useState } from 'react';

type SelectOption<T extends string> = {
  label: string;
  value: T;
};

type CustomSelectProps<T extends string> = {
  label?: string;
  onChange: (value: T) => void;
  options: SelectOption<T>[];
  value: T;
};

export function CustomSelect<T extends string>({ label, onChange, options, value }: CustomSelectProps<T>) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const selected = options.find((option) => option.value === value) ?? options[0];

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };

    document.addEventListener('pointerdown', handlePointerDown);
    return () => document.removeEventListener('pointerdown', handlePointerDown);
  }, []);

  return (
    <div className={`custom-select ${open ? 'is-open' : ''}`} ref={rootRef}>
      {label && <span className="custom-control-label">{label}</span>}
      <button className="custom-select-trigger" type="button" onClick={() => setOpen((current) => !current)}>
        <span>{selected?.label ?? '未选择'}</span>
        <ChevronDown style={{ width: 15, height: 15 }} />
      </button>
      <div className="custom-select-popover">
        {options.map((option) => (
          <button
            className={`custom-select-option ${option.value === value ? 'active' : ''}`}
            key={option.value}
            type="button"
            onClick={() => {
              onChange(option.value);
              setOpen(false);
            }}
          >
            <span>{option.label}</span>
            {option.value === value && <Check style={{ width: 14, height: 14 }} />}
          </button>
        ))}
      </div>
    </div>
  );
}

type SegmentedControlProps<T extends string> = {
  onChange: (value: T) => void;
  options: SelectOption<T>[];
  value: T;
};

export function SegmentedControl<T extends string>({ onChange, options, value }: SegmentedControlProps<T>) {
  return (
    <div className="segmented-control">
      {options.map((option) => (
        <button
          className={option.value === value ? 'active' : ''}
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

type ToggleSwitchProps = {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
};

export function ToggleSwitch({ checked, label, onChange }: ToggleSwitchProps) {
  return (
    <button className={`toggle-switch ${checked ? 'checked' : ''}`} type="button" onClick={() => onChange(!checked)}>
      <span className="toggle-track">
        <span className="toggle-thumb" />
      </span>
      <span>{label}</span>
    </button>
  );
}

type CustomSliderProps = {
  label: string;
  max: number;
  onChange: (value: number) => void;
  step: number;
  value: number;
};

export function CustomSlider({ label, max, onChange, step, value }: CustomSliderProps) {
  const progress = max === 0 ? 0 : (value / max) * 100;

  return (
    <label className="custom-slider">
      <span className="custom-slider-header">
        <span>{label}</span>
        <code>{value.toFixed(step < 0.1 ? 2 : 1)}</code>
      </span>
      <span className="custom-slider-track" style={{ '--slider-progress': `${progress}%` } as CSSProperties}>
        <input
          type="range"
          min={0}
          max={max}
          step={step}
          value={value}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </span>
    </label>
  );
}
