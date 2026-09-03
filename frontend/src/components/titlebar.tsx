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

  const isPointerDownRef = React.useRef(false);
  const dragStartedRef = React.useRef(false);
  const startPosRef = React.useRef({ x: 0, y: 0 });

  const handlePointerDown = (e: React.PointerEvent) => {
    // Left click only
    if (e.button !== 0) return;
    // Don't drag from buttons
    if ((e.target as HTMLElement).closest("button")) return;

    isPointerDownRef.current = true;
    dragStartedRef.current = false;
    startPosRef.current = { x: e.screenX, y: e.screenY };
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!isPointerDownRef.current || dragStartedRef.current) return;

    const dx = e.screenX - startPosRef.current.x;
    const dy = e.screenY - startPosRef.current.y;
    // Threshold of 4 pixels to distinguish click from intentional drag
    if (Math.hypot(dx, dy) > 4) {
      dragStartedRef.current = true;
      isPointerDownRef.current = false;
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        invoke("start_dragging").catch(() => {});
      }
    }
  };

  const handlePointerUp = () => {
    isPointerDownRef.current = false;
    dragStartedRef.current = false;
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    if (dragStartedRef.current) return;
    handleToggleMaximize();
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
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onDoubleClick={handleDoubleClick}
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
        data-tauri-drag-region="false"
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
