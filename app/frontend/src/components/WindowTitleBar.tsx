import { Maximize2, Minimize2, Minus, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { isDesktopRuntime } from '../services/platform/runtime';

const titlebarIconSize = { width: 14, height: 14 };

export function WindowTitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);

  const syncMaximizedState = useCallback(async () => {
    if (!isDesktopRuntime()) return;

    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      setIsMaximized(await getCurrentWindow().isMaximized());
    } catch {
      // Browser preview does not expose Tauri window APIs.
    }
  }, []);

  useEffect(() => {
    if (!isDesktopRuntime()) return;

    let disposed = false;
    let unlistenResize: (() => void) | null = null;

    import('@tauri-apps/api/window')
      .then(async ({ getCurrentWindow }) => {
        const appWindow = getCurrentWindow();
        setIsMaximized(await appWindow.isMaximized());

        unlistenResize = await appWindow.onResized(async () => {
          if (!disposed) {
            setIsMaximized(await appWindow.isMaximized());
          }
        });
      })
      .catch(() => {
        // Browser preview does not expose Tauri window APIs.
      });

    return () => {
      disposed = true;
      unlistenResize?.();
    };
  }, []);

  const runWindowAction = useCallback(
    async (action: 'close' | 'drag' | 'minimize' | 'toggleMaximize') => {
      if (!isDesktopRuntime()) return;

      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const appWindow = getCurrentWindow();

        if (action === 'drag') {
          await appWindow.startDragging();
          return;
        }

        if (action === 'minimize') {
          await appWindow.minimize();
          return;
        }

        if (action === 'toggleMaximize') {
          await appWindow.toggleMaximize();
          await syncMaximizedState();
          return;
        }

        await appWindow.close();
      } catch {
        // Window controls are no-ops in browser preview.
      }
    },
    [syncMaximizedState],
  );

  return (
    <header className="window-titlebar">
      <div
        className="window-titlebar-drag-region"
        aria-label="拖动窗口"
        onDoubleClick={() => void runWindowAction('toggleMaximize')}
        onPointerDown={(event) => {
          if (event.button !== 0 || event.detail > 1) return;
          void runWindowAction('drag');
        }}
      />

      <div className="window-controls" aria-label="窗口控制">
        <button
          className="window-control-button"
          type="button"
          aria-label="最小化窗口"
          title="最小化窗口"
          onClick={() => void runWindowAction('minimize')}
        >
          <Minus style={titlebarIconSize} />
        </button>
        <button
          className="window-control-button"
          type="button"
          aria-label={isMaximized ? '还原窗口' : '最大化窗口'}
          title={isMaximized ? '还原窗口' : '最大化窗口'}
          onClick={() => void runWindowAction('toggleMaximize')}
        >
          {isMaximized ? <Minimize2 style={titlebarIconSize} /> : <Maximize2 style={titlebarIconSize} />}
        </button>
        <button
          className="window-control-button close"
          type="button"
          aria-label="关闭窗口"
          title="关闭窗口"
          onClick={() => void runWindowAction('close')}
        >
          <X style={titlebarIconSize} />
        </button>
      </div>
    </header>
  );
}
