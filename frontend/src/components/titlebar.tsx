// ----------
// Custom Desktop Titlebar
// Description: Native-style titlebar with manual window dragging and explicit double-click maximize, with responsive custom controls.
// ----------

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { Minus, Square, Copy as RestoreIcon, X, ClipboardList } from "lucide-react";
import clipboxLogo from "@/assets/clipbox-logo.png";

interface TitlebarProps {
  entriesCount?: number;
  onCloseRequest?: () => void;
}

// Titlebar height is h-9 (36px). Restored windows are positioned so the
// cursor lands mid-titlebar during a pull-down restore-drag.
const TITLEBAR_GRAB_OFFSET = 18;

export function Titlebar({ entriesCount, onCloseRequest }: TitlebarProps) {
  const [isMaximized, setIsMaximized] = React.useState(false);
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
    };
  }, [syncMaximizedState]);

  const toggleMaximize = React.useCallback(() => {
    setIsMaximized((current) => !current);
    appWindow?.toggleMaximize().catch((err) => {
      console.warn("Could not toggle maximize", err);
      void syncMaximizedState();
    });
  }, [appWindow, syncMaximizedState]);

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

  // Restores a maximized window under the cursor, then hands the in-progress
  // press to the system move loop. toggleMaximize is used instead of unmaximize
  // because only core:window:allow-toggle-maximize is granted (see capabilities).
  const beginRestoreDrag = React.useCallback(
    async (cursorX: number, cursorY: number) => {
      if (!appWindow) return;

      try {
        await appWindow.toggleMaximize();
        setIsMaximized(false);
      } catch (err) {
        console.warn("Could not restore window for drag", err);
        void syncMaximizedState();
        return;
      }

      // The window restores to its previous bounds, which is rarely under the
      // cursor. Reposition it first so the held press grabs the titlebar
      // instead of dragging air.
      try {
        const [size, scaleFactor] = await Promise.all([
          appWindow.innerSize(),
          appWindow.scaleFactor(),
        ]);
        const restoredWidth = size.width / scaleFactor;
        await appWindow.setPosition(
          new LogicalPosition(
            Math.round(cursorX - restoredWidth / 2),
            Math.max(0, Math.round(cursorY - TITLEBAR_GRAB_OFFSET))
          )
        );
      } catch (err) {
        console.warn("Could not reposition window for drag", err);
      }

      invoke("start_dragging").catch((err) => {
        console.warn("Could not start window drag", err);
      });
    },
    [appWindow, syncMaximizedState]
  );

  const handleTitlebarMouseDown = (e: React.MouseEvent) => {
    if (
      e.button !== 0 ||
      (e.target as HTMLElement).closest("button, input, select, textarea, [role='button']")
    ) {
      return;
    }

    // Manual drag only: the titlebar deliberately has no data-tauri-drag-region
    // (native drag regions eat mouse events on Windows, see tauri-apps/tauri#10767).
    // Double-click is handled explicitly so the drag move loop can't swallow it.
    if (e.detail === 2) {
      handleToggleMaximize(e);
      return;
    }

    if (e.detail !== 1 || !appWindow) return;

    if (!isMaximized) {
      invoke("start_dragging").catch((err) => {
        console.warn("Could not start window drag", err);
      });
      return;
    }

    // Maximized: arm a pull-down restore. It only fires once the pointer
    // actually moves, so a plain click never unmaximizes the window.
    const startX = e.screenX;
    const startY = e.screenY;
    const cleanup = () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };
    const handleMove = (moveEvent: MouseEvent) => {
      if (moveEvent.buttons !== 1) {
        cleanup();
        return;
      }
      // Native restore-drag also needs a few pixels before it kicks in.
      if (Math.hypot(moveEvent.screenX - startX, moveEvent.screenY - startY) <= 4) return;

      cleanup();
      void beginRestoreDrag(moveEvent.screenX, moveEvent.screenY);
    };
    const handleUp = () => cleanup();

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
  };

  const handleClose = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (onCloseRequest) {
      onCloseRequest();
      return;
    }
    // Standalone fallback (e.g. error boundary): hide without prompting.
    invoke("hide_window").catch((err) => {
      console.warn("Could not hide window", err);
    });
  };

  return (
    <div
      data-window-chrome
      onMouseDown={handleTitlebarMouseDown}
      className="h-9 w-full bg-card border-b flex items-center justify-between select-none z-[60] sticky top-0 cursor-default"
    >
      {/* App Branding (Draggable) */}
      <div
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
        data-tauri-drag-region="false"
        className="native-window-no-drag flex items-center h-full pointer-events-auto shrink-0 min-w-[100px] justify-end"
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
