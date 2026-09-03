// ----------
// File Entry Preview Card Component
// Description: Renders metadata previews for copied files and folders (CF_HDROP), showing individual file icons, filenames, full paths, formatted sizes, expandable multi-file lists, and quick path copy actions.
// ----------

import * as React from "react";
import {
  Folder,
  File,
  FileText,
  FileArchive,
  FileCode,
  FileAudio,
  FileVideo,
  FileImage,
  Files,
  ChevronDown,
  ChevronUp,
  Link,
  Check,
  FolderOpen,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import type { ClipboardEntry } from "../App";

export interface FileItem {
  name: string;
  path: string;
  extension: string;
  size: number;
  isDirectory: boolean;
}

interface FileEntryCardProps {
  entry: ClipboardEntry;
  onCopyPaths: (e: React.MouseEvent) => void;
  isCopiedPaths: boolean;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function getFileIcon(extension: string, isDirectory: boolean, className = "size-4") {
  if (isDirectory) return <Folder className={`${className} text-amber-500 shrink-0`} />;
  const ext = extension.toLowerCase();
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"].includes(ext)) {
    return <FileImage className={`${className} text-blue-400 shrink-0`} />;
  }
  if (["mp4", "mkv", "avi", "mov", "webm"].includes(ext)) {
    return <FileVideo className={`${className} text-purple-400 shrink-0`} />;
  }
  if (["mp3", "wav", "flac", "aac", "ogg"].includes(ext)) {
    return <FileAudio className={`${className} text-pink-400 shrink-0`} />;
  }
  if (["zip", "rar", "7z", "tar", "gz"].includes(ext)) {
    return <FileArchive className={`${className} text-orange-400 shrink-0`} />;
  }
  if (["ts", "tsx", "js", "jsx", "rs", "py", "html", "css", "json", "c", "cpp", "go", "sql"].includes(ext)) {
    return <FileCode className={`${className} text-emerald-400 shrink-0`} />;
  }
  if (["txt", "md", "pdf", "docx", "doc", "rtf", "xlsx", "pptx"].includes(ext)) {
    return <FileText className={`${className} text-sky-400 shrink-0`} />;
  }
  return <File className={`${className} text-muted-foreground shrink-0`} />;
}

export function FileEntryCard({
  entry,
  onCopyPaths,
  isCopiedPaths,
}: FileEntryCardProps) {
  const [isExpanded, setIsExpanded] = React.useState(false);

  const handleOpenInExplorer = async (path: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("open_in_explorer", { path });
      }
    } catch (err) {
      console.error("Could not open in explorer", err);
    }
  };

  const files: FileItem[] = React.useMemo(() => {
    if (!entry.filesData) {
      return [
        {
          name: entry.content,
          path: entry.content,
          extension: entry.content.split(".").pop() || "",
          size: 0,
          isDirectory: false,
        },
      ];
    }
    try {
      return JSON.parse(entry.filesData);
    } catch {
      return [];
    }
  }, [entry.filesData, entry.content]);

  if (files.length === 0) {
    return (
      <div className="text-xs text-muted-foreground font-mono">
        {entry.content}
      </div>
    );
  }

  const isSingle = files.length === 1;
  const singleFile = files[0];
  const totalSize = files.reduce((acc, f) => acc + f.size, 0);

  if (isSingle) {
    return (
      <div className="space-y-1.5 select-text">
        <div className="flex items-start justify-between gap-3 p-2.5 rounded-lg border bg-muted/20 hover:bg-muted/30 transition-colors">
          <div className="flex items-start gap-3 min-w-0">
            <div className="p-2 rounded-md bg-background/80 border shadow-xs mt-0.5">
              {getFileIcon(singleFile.extension, singleFile.isDirectory, "size-5")}
            </div>
            <div className="min-w-0 space-y-0.5">
              <div className="text-sm font-medium text-foreground break-all leading-tight">
                {singleFile.name}
              </div>
              <div className="text-[11px] text-muted-foreground font-mono break-all line-clamp-1" title={singleFile.path}>
                {singleFile.path}
              </div>
              <div className="flex items-center gap-2 pt-0.5 text-[11px] text-muted-foreground font-mono">
                {singleFile.isDirectory ? (
                  <span className="text-amber-500 font-medium">Directory</span>
                ) : (
                  <span>{formatFileSize(singleFile.size)}</span>
                )}
                {singleFile.extension && !singleFile.isDirectory && (
                  <>
                    <span>·</span>
                    <span className="uppercase text-[10px] bg-muted px-1 rounded border">
                      {singleFile.extension}
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          <div className="flex items-center gap-1 shrink-0">
            <Button
              variant="ghost"
              size="sm"
              onClick={(e) => handleOpenInExplorer(singleFile.path, e)}
              title="Reveal and select in File Explorer"
              className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground shrink-0 gap-1"
            >
              <FolderOpen className="size-3" />
              <span className="text-[11px]">Open</span>
            </Button>

            <Button
              variant="ghost"
              size="sm"
              onClick={onCopyPaths}
              title="Copy path to clipboard"
              className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground shrink-0 gap-1"
            >
              {isCopiedPaths ? (
                <>
                  <Check className="size-3 text-emerald-500" />
                  <span className="text-[11px] text-emerald-500 font-medium">Path Copied</span>
                </>
              ) : (
                <>
                  <Link className="size-3" />
                  <span className="text-[11px]">Copy Path</span>
                </>
              )}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  // Multiple Files
  return (
    <div className="space-y-2 select-text">
      <div className="flex items-center justify-between p-2.5 rounded-lg border bg-muted/20">
        <div className="flex items-center gap-2.5">
          <div className="p-1.5 rounded-md bg-background/80 border shadow-xs">
            <Files className="size-4 text-emerald-500 shrink-0" />
          </div>
          <div>
            <div className="text-xs font-semibold text-foreground">
              {files.length} items copied
            </div>
            <div className="text-[11px] text-muted-foreground font-mono">
              Total size: {formatFileSize(totalSize)}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={onCopyPaths}
            title="Copy all file paths"
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground gap-1"
          >
            {isCopiedPaths ? (
              <>
                <Check className="size-3 text-emerald-500" />
                <span className="text-[11px] text-emerald-500 font-medium">Paths Copied</span>
              </>
            ) : (
              <>
                <Link className="size-3" />
                <span className="text-[11px]">Copy Paths</span>
              </>
            )}
          </Button>

          <Button
            variant="ghost"
            size="sm"
            onClick={(e) => {
              e.stopPropagation();
              setIsExpanded((prev) => !prev);
            }}
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground gap-1"
          >
            {isExpanded ? (
              <>
                <ChevronUp className="size-3" />
                <span className="text-[11px]">Hide</span>
              </>
            ) : (
              <>
                <ChevronDown className="size-3" />
                <span className="text-[11px]">View all</span>
              </>
            )}
          </Button>
        </div>
      </div>

      {isExpanded && (
        <div className="rounded-lg border bg-background/60 p-2 divide-y divide-border/50 max-h-[220px] overflow-y-auto space-y-1">
          {files.map((file, idx) => (
            <div
              key={`${file.path}-${idx}`}
              className="flex items-center justify-between gap-2 py-1.5 px-1 hover:bg-muted/40 rounded transition-colors text-xs"
            >
              <div className="flex items-center gap-2 min-w-0">
                {getFileIcon(file.extension, file.isDirectory, "size-3.5")}
                <span className="font-medium text-foreground truncate" title={file.name}>
                  {file.name}
                </span>
              </div>
              <div className="flex items-center gap-1.5 shrink-0 text-[11px] text-muted-foreground font-mono">
                {file.isDirectory ? (
                  <span className="text-amber-500">Folder</span>
                ) : (
                  <span>{formatFileSize(file.size)}</span>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={(e) => handleOpenInExplorer(file.path, e)}
                  title="Reveal in File Explorer"
                  className="size-6 text-muted-foreground hover:text-foreground"
                >
                  <FolderOpen className="size-3" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
