// ----------
// Custom Desktop Titlebar
// Description: Native-style draggable window titlebar supporting app branding, centered entries count badge, double-click to maximize, minimize, maximize/restore, close, and programmatic window dragging.
// ----------

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Minus, Square, Copy as RestoreIcon, X, ClipboardList } from "lucide-react";
import clipboxLogo from "@/assets/clipbox-logo.png";

interface TitlebarProps {
  entriesCount?: number;
}

export function Titlebar({ entriesCount }: TitlebarProps) {
  const [isMaximized, setIsMaximized] = React.useState(false);

  const checkMaximized = React.useCallback(async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const max = await invoke<boolean>("is_window_maximized");
        setIsMaximized(max);
      }
    } catch {
      // Fallback for web preview
    }
  }, []);

  React.useEffect(() => {
    checkMaximized();
    window.addEventListener("resize", checkMaximized);
    return () => window.removeEventListener("resize", checkMaximized);
  }, [checkMaximized]);

  const lastToggleRef = React.useRef(0);

  const handleMouseDown = (event: React.MouseEvent) => {
    // Only drag on left mouse button
    if (event.button !== 0) return;

    // Ignore clicks on buttons
    if ((event.target as HTMLElement).closest("button")) return;

    // Ignore double/triple clicks on mousedown (handled cleanly by onDoubleClick)
    if (event.detail > 1) return;

    // Programmatically start window dragging via native Tauri IPC
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        invoke("start_dragging").catch(() => {});
      }
    } catch (err) {
      console.warn("Could not start dragging", err);
    }
  };

  const handleMinimize = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("minimize_window");
    } catch (err) {
      console.warn("Could not minimize window", err);
    }
  };

  const handleToggleMaximize = async (e?: React.MouseEvent) => {
    e?.stopPropagation();
    const now = Date.now();
    // Debounce rapid successive toggles to prevent animation double-trigger
    if (now - lastToggleRef.current < 400) return;
    lastToggleRef.current = now;

    try {
      await invoke("toggle_maximize_window");
      await checkMaximized();
    } catch (err) {
      console.warn("Could not toggle maximize", err);
    }
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
      data-tauri-drag-region
      onMouseDown={handleMouseDown}
      onDoubleClick={handleToggleMaximize}
      className="h-9 w-full bg-card border-b flex items-center justify-between select-none z-[60] sticky top-0 cursor-default"
    >
      {/* App Branding (Draggable) */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 px-3 h-full select-none shrink-0"
      >
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
      <div
        data-tauri-drag-region
        className="flex-1 h-full flex items-center justify-center cursor-default select-none"
      >
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
