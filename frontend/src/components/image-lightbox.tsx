// ----------
// Image Lightbox Modal Component
// Description: Full-resolution interactive image lightbox with cursor-centered mouse wheel zoom, click-and-drag panning, quick zoom controls, double-click toggle, and metadata badges.
// ----------

import * as React from "react";
import { createPortal } from "react-dom";
import {
  Camera,
  Image as ImageIcon,
  Copy,
  Check,
  Download,
  X,
  Pencil,
  Plus,
  Minus,
  RotateCcw,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import type { ClipboardEntry } from "../App";
import { ImageAnnotator } from "./image-editor/image-annotator";

interface OcrBox {
  t: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

const isOcrBox = (value: unknown): value is OcrBox => {
  if (typeof value !== "object" || value === null) return false;
  const box = value as Record<string, unknown>;
  return (
    typeof box.t === "string" &&
    typeof box.x === "number" &&
    typeof box.y === "number" &&
    typeof box.w === "number" &&
    typeof box.h === "number" &&
    [box.x, box.y, box.w, box.h].every((n) => Number.isFinite(n))
  );
};

interface ImageLightboxProps {
  entry: ClipboardEntry | null;
  isOpen: boolean;
  onClose: () => void;
  formatTimestamp: (timestamp: number) => string;
  searchQuery: string;
}

export function ImageLightbox({
  entry,
  isOpen,
  onClose,
  formatTimestamp,
  searchQuery,
}: ImageLightboxProps) {
  const [isEditing, setIsEditing] = React.useState(false);
  const [copied, setCopied] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null);

  // Zoom & Pan interactive state
  const [zoom, setZoom] = React.useState<number>(1);
  const [pan, setPan] = React.useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = React.useState<boolean>(false);

  const stageRef = React.useRef<HTMLDivElement>(null);
  const dragStartPosRef = React.useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const dragStartPanRef = React.useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const hasMovedRef = React.useRef<boolean>(false);

  const prevEntryIdRef = React.useRef<number | undefined>(undefined);
  const prevIsOpenRef = React.useRef(false);

  // OCR word boxes matching the current feed search, overlaid on the image.
  // Memoized since renders happen on every zoom/pan frame.
  const matchedBoxes = React.useMemo<OcrBox[]>(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query || !entry?.ocrBoxes) return [];
    try {
      const parsed: unknown = JSON.parse(entry.ocrBoxes);
      if (!Array.isArray(parsed)) return [];
      return parsed.filter(
        (box): box is OcrBox => isOcrBox(box) && box.t.toLowerCase().includes(query)
      );
    } catch {
      return [];
    }
  }, [entry?.ocrBoxes, searchQuery]);

  // Reset zoom, pan, and edit state ONLY when modal opens afresh or when switching to a different clip
  React.useEffect(() => {
    if (isOpen && (!prevIsOpenRef.current || prevEntryIdRef.current !== entry?.id)) {
      setIsEditing(false);
      setCopied(false);
      setSaveMessage(null);
      setZoom(1);
      setPan({ x: 0, y: 0 });
    }
    prevIsOpenRef.current = isOpen;
    prevEntryIdRef.current = entry?.id;
  }, [isOpen, entry?.id]);

  const handleResetZoom = React.useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const handleZoomIn = React.useCallback(() => {
    setZoom((z) => Math.min(10, z * 1.25));
  }, []);

  const handleZoomOut = React.useCallback(() => {
    setZoom((z) => Math.max(0.2, z / 1.25));
  }, []);

  // Keyboard shortcut listener for Esc and Zoom (+, -, 0)
  React.useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        handleZoomIn();
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        handleZoomOut();
      } else if (e.key === "0") {
        e.preventDefault();
        handleResetZoom();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose, handleZoomIn, handleZoomOut, handleResetZoom]);

  if (!isOpen || !entry || !entry.imageData) {
    return null;
  }

  if (isEditing) {
    return (
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Image Editor"
        className="fixed top-9 inset-x-0 bottom-0 z-40 flex flex-col bg-background animate-in fade-in-0 duration-200"
      >
        <ImageAnnotator
          initialDataUrl={entry.imageData}
          sourceEntryId={entry.id}
          initialDimensions={entry.imageDimensions}
          onClose={() => setIsEditing(false)}
        />
      </div>
    );
  }

  const appName = entry.sourceApp || entry.sourceProcess || "Unknown Application";
  const isScreenCapture =
    appName.toLowerCase().includes("screen capture") ||
    appName.toLowerCase().includes("screenshot");

  const handleCopy = async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("copy_image_to_clipboard", {
          dataUrl: entry.imageData,
        });
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy image", err);
    }
  };

  const handleSaveAs = async () => {
    setSaving(true);
    setSaveMessage(null);

    const defaultFilename = isScreenCapture
      ? `clipbox-screenshot-${entry.id}.png`
      : `clipbox-image-${entry.id}.png`;

    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const savedPath = await invoke<string | null>("save_image_to_file", {
          dataUrl: entry.imageData,
          defaultFilename,
        });

        if (savedPath) {
          const filenameOnly = savedPath.split(/[/\\]/).pop() || savedPath;
          setSaveMessage(`Saved as ${filenameOnly}`);
          setTimeout(() => setSaveMessage(null), 3000);
        }
      } else {
        // Web fallback
        const link = document.createElement("a");
        link.href = entry.imageData!;
        link.download = defaultFilename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        setSaveMessage("Saved!");
        setTimeout(() => setSaveMessage(null), 3000);
      }
    } catch (err) {
      console.error("Failed to save image", err);
    } finally {
      setSaving(false);
    }
  };

  // Cursor-centered smooth mouse wheel zoom
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const stage = stageRef.current;
    if (!stage) return;

    const zoomFactor = e.deltaY < 0 ? 1.18 : 1 / 1.18;
    const newZoom = Math.min(10, Math.max(0.2, zoom * zoomFactor));

    const rect = stage.getBoundingClientRect();
    const mx = e.clientX - rect.left - rect.width / 2;
    const my = e.clientY - rect.top - rect.height / 2;

    const scaleRatio = newZoom / zoom;
    setPan((prev) => ({
      x: mx - (mx - prev.x) * scaleRatio,
      y: my - (my - prev.y) * scaleRatio,
    }));
    setZoom(newZoom);
  };

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    setIsDragging(true);
    hasMovedRef.current = false;
    dragStartPosRef.current = { x: e.clientX, y: e.clientY };
    dragStartPanRef.current = { ...pan };
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {}
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!isDragging) return;
    const dx = e.clientX - dragStartPosRef.current.x;
    const dy = e.clientY - dragStartPosRef.current.y;
    if (Math.hypot(dx, dy) > 3) {
      hasMovedRef.current = true;
    }
    setPan({
      x: dragStartPanRef.current.x + dx,
      y: dragStartPanRef.current.y + dy,
    });
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if ((e.currentTarget as HTMLElement)?.hasPointerCapture?.(e.pointerId)) {
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {}
    }
    setIsDragging(false);

    // If pure click without dragging:
    if (!hasMovedRef.current) {
      const target = e.target as HTMLElement;
      // If clicked backdrop (not the image container), close the lightbox
      if (!target.closest("[data-image-container]")) {
        onClose();
      }
    }
  };

  const handleDoubleClickImage = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (zoom === 1 && pan.x === 0 && pan.y === 0) {
      const stage = stageRef.current;
      if (!stage) return;
      const rect = stage.getBoundingClientRect();
      const mx = e.clientX - rect.left - rect.width / 2;
      const my = e.clientY - rect.top - rect.height / 2;
      const newZoom = 2.5;
      const scaleRatio = newZoom / zoom;
      setPan({
        x: mx - (mx - pan.x) * scaleRatio,
        y: my - (my - pan.y) * scaleRatio,
      });
      setZoom(newZoom);
    } else {
      handleResetZoom();
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Image Preview Lightbox"
      className="fixed top-9 inset-x-0 bottom-0 z-40 flex flex-col bg-black/85 backdrop-blur-md animate-in fade-in-0 duration-200"
    >
      {/* Top Action Bar */}
      <div
        data-window-chrome
        className="native-window-drag-region h-14 w-full bg-card/90 border-b border-border/60 px-4 flex items-center justify-between gap-3 shrink-0 select-none shadow-md backdrop-blur-sm"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Left: Metadata Badges */}
        <div className="flex min-w-0 flex-1 items-center gap-2 flex-nowrap overflow-hidden">
          {isScreenCapture ? (
            <span className="inline-flex shrink-0 items-center gap-1.5 px-2 py-0.5 rounded-md text-xs font-semibold bg-blue-500/15 text-blue-400 border border-blue-500/30 select-none">
              <Camera className="size-3.5 text-blue-400" />
              Screen Capture
            </span>
          ) : (
            <div className="flex min-w-0 flex-1 items-center gap-2">
              <span className="inline-flex shrink-0 items-center gap-1.5 px-2 py-0.5 rounded-md text-xs font-semibold bg-blue-500/15 text-blue-400 border border-blue-500/30 select-none">
                <ImageIcon className="size-3.5 text-blue-400" />
                Image
              </span>
              <div className="flex min-w-0 items-center gap-1.5 text-xs font-medium text-foreground">
                {entry.appIcon ? (
                  <img
                    src={entry.appIcon}
                    alt={appName}
                    width={16}
                    height={16}
                    className="size-4 object-contain rounded-sm select-none pointer-events-none shrink-0"
                  />
                ) : (
                  <span className="size-4 rounded bg-muted border flex items-center justify-center text-[9px] font-semibold text-muted-foreground uppercase shrink-0 select-none">
                    {appName.charAt(0)}
                  </span>
                )}
                <span className="truncate">{appName}</span>
              </div>
              <span className="text-xs text-muted-foreground shrink-0">·</span>
              <span className="text-xs text-muted-foreground truncate">
                {entry.windowTitle || "Copied Image"}
              </span>
            </div>
          )}

          {entry.imageDimensions && (
            <span className="inline-flex shrink-0 items-center px-1.5 py-0.5 rounded text-[11px] font-mono font-medium bg-muted text-muted-foreground border select-none">
              {entry.imageDimensions}
            </span>
          )}

          <span className="text-xs text-muted-foreground shrink-0">·</span>
          <span className="text-xs text-muted-foreground shrink-0 whitespace-nowrap">
            {formatTimestamp(entry.copiedAt)}
          </span>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center gap-2 shrink-0">
          {/* Interactive Zoom Controls */}
          <div className="flex items-center bg-muted/60 border rounded-lg p-0.5">
            <Button
              variant="ghost"
              size="icon"
              onClick={handleZoomOut}
              disabled={zoom <= 0.2}
              className="size-7 rounded"
              title="Zoom Out (-)"
            >
              <Minus className="size-3.5" />
            </Button>

            <button
              type="button"
              onClick={handleResetZoom}
              title="Click to reset zoom & center (0)"
              className="px-2 py-0.5 text-xs font-mono font-medium text-muted-foreground hover:text-foreground hover:bg-background/60 rounded transition-colors"
            >
              {Math.round(zoom * 100)}%
            </button>

            <Button
              variant="ghost"
              size="icon"
              onClick={handleZoomIn}
              disabled={zoom >= 10}
              className="size-7 rounded"
              title="Zoom In (+)"
            >
              <Plus className="size-3.5" />
            </Button>

            {(zoom !== 1 || pan.x !== 0 || pan.y !== 0) && (
              <Button
                variant="ghost"
                size="sm"
                onClick={handleResetZoom}
                className="h-7 px-2 text-[11px] font-medium text-primary hover:bg-primary/10 ml-0.5"
                title="Fit to view"
              >
                <RotateCcw className="size-3 mr-1" />
                <span>Fit</span>
              </Button>
            )}
          </div>

          <Button
            variant="outline"
            size="sm"
            onClick={() => setIsEditing(true)}
            title="Annotate & Crop Image"
            className="h-8 gap-1.5 text-xs font-medium border-primary/40 text-primary hover:bg-primary/10"
          >
            <Pencil className="size-3.5" />
            <span>Annotate</span>
          </Button>

          <Button
            variant="outline"
            size="sm"
            onClick={handleSaveAs}
            disabled={saving}
            title="Save Image As"
            className="h-8 gap-1.5 text-xs font-medium"
          >
            <Download className="size-3.5" />
            <span>Save As</span>
          </Button>

          <Button
            variant={copied ? "default" : "outline"}
            size="sm"
            onClick={handleCopy}
            title="Copy Image to Clipboard"
            className="h-8 gap-1.5 text-xs font-medium"
          >
            {copied ? (
              <>
                <Check className="size-3.5 text-emerald-400" />
                <span>Copied</span>
              </>
            ) : (
              <>
                <Copy className="size-3.5" />
                <span>Copy</span>
              </>
            )}
          </Button>

          <Button
            variant="ghost"
            size="icon"
            onClick={onClose}
            title="Close (Esc)"
            className="size-8 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground ml-1"
          >
            <X className="size-4" />
            <span className="sr-only">Close</span>
          </Button>
        </div>
      </div>

      {/* Main Image Stage */}
      <div
        ref={stageRef}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        className={`flex-1 min-h-0 overflow-hidden relative flex items-center justify-center select-none touch-none ${
          isDragging ? "cursor-grabbing" : "cursor-grab"
        }`}
      >
        <div
          data-image-container
          onDoubleClick={handleDoubleClickImage}
          style={{
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
            transformOrigin: "center center",
            transition: isDragging ? "none" : "transform 0.08s ease-out",
          }}
          className="relative flex items-center justify-center pointer-events-auto"
        >
          {/* Subtle Checkerboard backdrop for transparent PNG images */}
          <div
            className="relative rounded-lg shadow-2xl overflow-hidden border border-border/40 bg-[radial-gradient(#27272a_1px,transparent_1px)] [background-size:16px_16px] bg-card/60"
            style={{
              backgroundImage: `
                linear-gradient(45deg, rgba(255,255,255,0.03) 25%, transparent 25%),
                linear-gradient(-45deg, rgba(255,255,255,0.03) 25%, transparent 25%),
                linear-gradient(45deg, transparent 75%, rgba(255,255,255,0.03) 75%),
                linear-gradient(-45deg, transparent 75%, rgba(255,255,255,0.03) 75%)
              `,
              backgroundSize: "20px 20px",
              backgroundPosition: "0 0, 0 10px, 10px -10px, -10px 0px",
            }}
          >
            <img
              src={entry.imageData}
              alt={entry.content}
              draggable={false}
              className="max-w-[calc(88vw)] max-h-[calc(68vh)] object-contain select-none pointer-events-none rounded-lg"
            />
            {matchedBoxes.length > 0 && (
              <div className="absolute inset-0 pointer-events-none">
                {matchedBoxes.map((box, index) => (
                  <div
                    key={index}
                    className="absolute bg-yellow-400/30 border border-yellow-400/70 rounded-[1px]"
                    style={{
                      left: `${box.x * 100}%`,
                      top: `${box.y * 100}%`,
                      width: `${box.w * 100}%`,
                      height: `${box.h * 100}%`,
                    }}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Bottom Hint Footer */}
      <div
        className="h-8 w-full bg-card/80 border-t border-border/40 px-4 flex items-center justify-between text-[11px] text-muted-foreground select-none shrink-0"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2">
          <span>Entry #{entry.id}</span>
          {entry.imageDimensions && (
            <>
              <span>•</span>
              <span>{entry.imageDimensions}</span>
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span>Mouse wheel to zoom</span>
          <span>•</span>
          <span>Click & drag to pan</span>
          <span>•</span>
          <span>Double-click to toggle 2.5x</span>
          <span>•</span>
          <span>Press <kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">Esc</kbd> or click background to close</span>
        </div>
      </div>

      {/* Floating Save Confirmation Toast (portaled to body so no
          overlay stacking context can trap it) */}
      {saveMessage &&
        createPortal(
          <div className="fixed bottom-16 left-1/2 -translate-x-1/2 z-[100] pointer-events-none animate-in fade-in-0 slide-in-from-bottom-2 duration-200">
            <div className="bg-popover/95 backdrop-blur-md text-popover-foreground border shadow-xl rounded-full px-4 py-2 flex items-center gap-2 text-xs font-medium whitespace-nowrap">
              <Check className="size-4 text-emerald-400 shrink-0" />
              <span>{saveMessage}</span>
            </div>
          </div>,
          document.body
        )}
    </div>
  );
}
