// ----------
// Image Lightbox Modal Component
// Description: Full-resolution image preview lightbox with fit/actual size zoom toggle, metadata badges (source app/screen capture, dimensions, timestamp), copy to clipboard, and native save as file action.
// ----------

import * as React from "react";
import {
  Camera,
  Image as ImageIcon,
  Copy,
  Check,
  Download,
  ZoomIn,
  ZoomOut,
  X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import type { ClipboardEntry } from "../App";

interface ImageLightboxProps {
  entry: ClipboardEntry | null;
  isOpen: boolean;
  onClose: () => void;
  formatTimestamp: (timestamp: number) => string;
}

export function ImageLightbox({
  entry,
  isOpen,
  onClose,
  formatTimestamp,
}: ImageLightboxProps) {
  const [isZoomed, setIsZoomed] = React.useState(false);
  const [copied, setCopied] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [saveMessage, setSaveMessage] = React.useState<string | null>(null);

  // Reset zoom state when modal opens with a new entry
  React.useEffect(() => {
    if (isOpen) {
      setIsZoomed(false);
      setCopied(false);
      setSaveMessage(null);
    }
  }, [isOpen, entry?.id]);

  // Keyboard shortcut listener for Esc
  React.useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen || !entry || !entry.imageData) {
    return null;
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

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Image Preview Lightbox"
      className="fixed inset-0 z-50 flex flex-col bg-black/85 backdrop-blur-md animate-in fade-in-0 duration-200"
      onClick={onClose}
    >
      {/* Top Action Bar */}
      <div
        className="h-14 w-full bg-card/90 border-b border-border/60 px-4 flex items-center justify-between gap-3 shrink-0 select-none shadow-md backdrop-blur-sm"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Left: Metadata Badges */}
        <div className="flex items-center gap-2 flex-wrap overflow-hidden">
          {isScreenCapture ? (
            <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-xs font-semibold bg-blue-500/15 text-blue-400 border border-blue-500/30 select-none">
              <Camera className="size-3.5 text-blue-400" />
              Screen Capture
            </span>
          ) : (
            <div className="flex items-center gap-2">
              <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-xs font-semibold bg-blue-500/15 text-blue-400 border border-blue-500/30 select-none">
                <ImageIcon className="size-3.5 text-blue-400" />
                Image
              </span>
              <div className="flex items-center gap-1.5 text-xs font-medium text-foreground">
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
                <span>{appName}</span>
              </div>
              <span className="text-xs text-muted-foreground">·</span>
              <span className="text-xs text-muted-foreground">
                {entry.windowTitle || "Copied Image"}
              </span>
            </div>
          )}

          {entry.imageDimensions && (
            <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[11px] font-mono font-medium bg-muted text-muted-foreground border select-none">
              {entry.imageDimensions}
            </span>
          )}

          <span className="text-xs text-muted-foreground">·</span>
          <span className="text-xs text-muted-foreground">
            {formatTimestamp(entry.copiedAt)}
          </span>
        </div>

        {/* Right: Actions */}
        <div className="flex items-center gap-1.5 shrink-0">
          {saveMessage && (
            <span className="text-xs font-medium text-emerald-400 animate-in fade-in-0 slide-in-from-right-2 duration-150 mr-2">
              {saveMessage}
            </span>
          )}

          <Button
            variant="outline"
            size="sm"
            onClick={() => setIsZoomed((prev) => !prev)}
            title={isZoomed ? "Fit to view" : "View actual size"}
            className="h-8 gap-1.5 text-xs font-medium"
          >
            {isZoomed ? (
              <>
                <ZoomOut className="size-3.5" />
                <span>Fit</span>
              </>
            ) : (
              <>
                <ZoomIn className="size-3.5" />
                <span>100%</span>
              </>
            )}
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
        className="flex-1 overflow-auto flex items-center justify-center p-4 select-none"
        onClick={onClose}
      >
        <div
          className="relative max-h-full max-w-full flex items-center justify-center transition-all duration-150"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Subtle Checkerboard backdrop for transparent PNG images */}
          <div
            className="rounded-lg shadow-2xl overflow-hidden border border-border/40 bg-[radial-gradient(#27272a_1px,transparent_1px)] [background-size:16px_16px] bg-card/60"
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
              onClick={() => setIsZoomed((prev) => !prev)}
              className={`transition-all duration-150 rounded-lg select-none ${
                isZoomed
                  ? "max-w-none max-h-none cursor-zoom-out"
                  : "max-w-[calc(92vw)] max-h-[calc(80vh)] object-contain cursor-zoom-in"
              }`}
            />
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
          <span>Click image to toggle zoom</span>
          <span>•</span>
          <span>Press <kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">Esc</kbd> or click backdrop to close</span>
        </div>
      </div>
    </div>
  );
}
