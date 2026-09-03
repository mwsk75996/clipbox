// ----------
// Custom Desktop Titlebar
// Description: Native-style titlebar that starts native dragging on the first real pointer movement and toggles maximize on the second mouse-down without waiting for a browser double-click event.
// ----------

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, Copy as RestoreIcon, X, ClipboardList } from "lucide-react";
import clipboxLogo from "@/assets/clipbox-logo.png";

interface TitlebarProps {
  entriesCount?: number;
}

export function Titlebar({ entriesCount }: TitlebarProps) {
  const [isMaximized, setIsMaximized] = React.useState(false);
  const dragCleanupRef = React.useRef<(() => void) | null>(null);
  const appWindow = React.useMemo(() => {
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      return getCurrentWindow();
    }
    return null;
  }, []);

  const syncMaximizedState = React.useCallback(async () => {
    if (!appWindow) return;

    try {
      setIsMaximized(await appWindow.isMaximized());
    } catch {
      // Fallback for web preview
    }
  }, [appWindow]);

  React.useEffect(() => {
    void syncMaximizedState();

    let resizeTimer: number | undefined;
    const handleResize = () => {
      window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => {
        void syncMaximizedState();
      }, 120);
    };

    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      window.clearTimeout(resizeTimer);
      dragCleanupRef.current?.();
    };
  }, [syncMaximizedState]);

  const toggleMaximize = React.useCallback(() => {
    setIsMaximized((current) => !current);
    invoke("toggle_maximize_window").catch((err) => {
      console.warn("Could not toggle maximize", err);
      void syncMaximizedState();
    });
  }, [syncMaximizedState]);

  const handleTitlebarMouseDown = (event: React.MouseEvent) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) {
      return;
    }

    dragCleanupRef.current?.();

    // Toggle on the second press instead of waiting for the delayed dblclick event.
    if (event.detail === 2) {
      toggleMaximize();
      return;
    }

    if (event.detail !== 1) return;

    const startX = event.screenX;
    const startY = event.screenY;

    const cleanup = () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", cleanup);
      dragCleanupRef.current = null;
    };

    const handleMouseMove = (moveEvent: MouseEvent) => {
      if (moveEvent.screenX === startX && moveEvent.screenY === startY) return;

      cleanup();
      invoke("begin_window_drag").catch((err) => {
        console.warn("Could not start dragging", err);
      });
    };

    dragCleanupRef.current = cleanup;
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", cleanup);
  };

  const handleMinimize = (e: React.MouseEvent) => {
    e.stopPropagation();
    appWindow?.minimize().catch((err) => {
      console.warn("Could not minimize window", err);
    });
  };

  const handleToggleMaximize = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!appWindow) return;
    toggleMaximize();
  };

  const handleClose = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("close_window");
    } catch (err) {
      console.warn("Could not close window", err);
    }
  };

  return (
    <div
      data-window-chrome
      onMouseDown={handleTitlebarMouseDown}
      className="h-9 w-full bg-card border-b flex items-center justify-between select-none z-[60] sticky top-0 cursor-default"
    >
      {/* App Branding (Draggable) */}
      <div className="flex items-center gap-2 px-3 h-full select-none shrink-0 pointer-events-none">
        <img
          src={clipboxLogo}
          alt="Clipbox Logo"
          width={18}
          height={18}
          className="h-[18px] w-[18px] max-h-[18px] max-w-[18px] rounded object-contain select-none pointer-events-none shrink-0"
        />
        <span className="text-xs font-semibold tracking-wide text-foreground/85 pointer-events-none">
          Clipbox
        </span>
      </div>

      {/* Middle Drag Region with Centered Entries Count Badge */}
      <div className="flex-1 h-full flex items-center justify-center cursor-default select-none pointer-events-none">
        {typeof entriesCount === "number" && (
          <div className="flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-secondary/70 border border-border/80 text-[11px] font-medium text-muted-foreground shadow-sm pointer-events-none">
            <ClipboardList className="size-3 text-muted-foreground" />
            <span>
              {entriesCount} {entriesCount === 1 ? "entry" : "entries"}
            </span>
          </div>
        )}
      </div>

      {/* Window Controls (Not draggable) */}
      <div
        className="flex items-center h-full pointer-events-auto shrink-0 min-w-[100px] justify-end"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          onClick={handleMinimize}
          className="h-full w-11 flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted/70 transition-colors focus:outline-none"
          title="Minimize"
        >
          <Minus className="size-3.5" />
        </button>

        <button
          type="button"
          onClick={handleToggleMaximize}
          className="h-full w-11 flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-muted/70 transition-colors focus:outline-none"
          title={isMaximized ? "Restore Down" : "Maximize"}
        >
          {isMaximized ? (
            <RestoreIcon className="size-3" />
          ) : (
            <Square className="size-3" />
          )}
        </button>

        <button
          type="button"
          onClick={handleClose}
          className="h-full w-11 flex items-center justify-center text-muted-foreground hover:text-white hover:bg-red-600 transition-colors focus:outline-none"
          title="Close"
        >
          <X className="size-3.5" />
        </button>
      </div>
    </div>
  );
}
