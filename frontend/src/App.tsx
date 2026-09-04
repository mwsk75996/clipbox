// ----------
// Clipboard History Dashboard
// Description: Main application component utilizing shadcn/ui primitives, custom desktop titlebar, date range calendar filtering, and dynamic theme controls without browser artifacts.
// ----------

import * as React from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Search,
  SlidersHorizontal,
  Copy,
  Check,
  RotateCcw,
  ChevronDown,
  ChevronUp,
  ChevronLeft,
  ChevronRight,
  Trash2,
  Pin,
  Image as ImageIcon,
  Camera,
  ZoomIn,
  Files,
  PauseCircle,
  ExternalLink,
  TriangleAlert,
  Download,
  Loader2,
  X,
} from "lucide-react";
import { startOfDay, endOfDay } from "date-fns";
import type { DateRange } from "react-day-picker";

import { Titlebar } from "@/components/titlebar";
import { ImageLightbox } from "@/components/image-lightbox";
import { FileEntryCard } from "@/components/file-entry-card";
import { DeletedEntryCard } from "@/components/deleted-entry-card";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { DateRangePicker } from "@/components/date-range-picker";
import { ThemeToggle } from "@/components/theme-toggle";
import {
  SettingsModal,
  DEFAULT_SHORTCUTS,
  matchesBinding,
  type ShortcutSettings,
} from "@/components/settings-modal";

export interface ClipboardEntry {
  id: number;
  content: string;
  copiedAt: number;
  sourceApp?: string | null;
  sourceProcess?: string | null;
  windowTitle?: string | null;
  appIcon?: string | null;
  isPinned?: boolean;
  entryType?: string;
  imageData?: string | null;
  imageDimensions?: string | null;
  filesData?: string | null;
  sourceUrl?: string | null;
  ocrText?: string | null;
}

export interface DeletedClipboardEntry {
  id: number;
  content: string;
  copiedAt: number;
  sourceApp?: string | null;
  sourceProcess?: string | null;
  windowTitle?: string | null;
  appIcon?: string | null;
  isPinned?: boolean;
  entryType?: string;
  imageData?: string | null;
  imageDimensions?: string | null;
  filesData?: string | null;
  sourceUrl?: string | null;
  deletedAt: number;
}

// Minimum time between full feed reloads triggered by window focus.
// Real-time clipboard events keep the visible feed updated, so a focus that
// closely follows any fetch skips the reload instead of rerendering everything.
const FOCUS_REFETCH_MIN_INTERVAL_MS = 30_000;

// Display labels for the deleted_retention setting values.
const TRASH_RETENTION_LABELS: Record<string, string> = {
  immediately: "Immediately",
  "1hour": "1 hour",
  "1day": "1 day",
  "7days": "7 days",
  "30days": "30 days",
};

// Minimum time the per-card "Restoring..." indication stays visible, so fast
// restores don't flash and slow ones feel deliberate instead of glitchy.
const RESTORE_MIN_INDICATION_MS = 900;

// Maximum stacked global toasts; oldest dismisses first beyond this.
const MAX_STACKED_TOASTS = 3;

interface AppToast {
  id: number;
  message: string;
  kind: "success" | "error";
}

type UpdateStatus =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "downloading"; downloaded: number; total: number }
  | { phase: "ready"; path: string };

// Numeric dotted-version comparison ignoring a leading "v".
// Returns negative/zero/positive; malformed parts count as 0 (never newer).
function compareAppVersions(a: string, b: string): number {
  const parse = (v: string) => v.replace(/^v/i, "").split(".").map((n) => Number(n) || 0);
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

const PREVIEW_ENTRIES: ClipboardEntry[] = [
  {
    id: 3,
    content:
      "https://github.com/mwsk75996/clipbox\nSecond line from the copied page\nA third line for the history preview\nA fourth line to test the expanded view\nFifth line of copied content",
    copiedAt: Math.floor(Date.now() / 1000) - 42,
    sourceApp: "Brave",
    sourceProcess: "brave.exe",
    windowTitle: "Clipbox · GitHub",
    sourceUrl: "https://github.com/mwsk75996/clipbox",
  },
  {
    id: 2,
    content: "Everything you copy, kept close at hand.",
    copiedAt: Math.floor(Date.now() / 1000) - 380,
    sourceApp: "Code",
    sourceProcess: "Code.exe",
    windowTitle: "Clipbox — Visual Studio Code",
  },
  {
    id: 1,
    content: "A small note worth keeping around.",
    copiedAt: Math.floor(Date.now() / 1000) - 3600,
    sourceApp: "Notepad",
    sourceProcess: "notepad.exe",
    windowTitle: "Notes.txt",
  },
];

export function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

function formatSourceUrl(url: string): string {
  try {
    const parsed = new URL(url);
    return parsed.hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

function isWebUrl(text?: string | null): boolean {
  if (!text) return false;
  const trimmed = text.trim();
  return (
    (trimmed.startsWith("http://") || trimmed.startsWith("https://")) &&
    !trimmed.includes("\n") &&
    !trimmed.includes(" ") &&
    trimmed.length > 8
  );
}

export function sourceLabel(entry: ClipboardEntry): string {
  return (
    entry.sourceApp ||
    entry.sourceProcess ||
    entry.windowTitle ||
    "Unknown source"
  );
}

// ----------
// Leading Empty Line Sanitizer
// Description: Trims leading empty lines so copied text and display blocks start cleanly with content.
// ----------
export function stripLeadingEmptyLines(text: string): string {
  const lines = text.split(/\r?\n/);
  while (lines.length > 1 && lines[0].trim() === "") {
    lines.shift();
  }
  return lines.join("\n");
}

export default function App() {
  const [entries, setEntries] = React.useState<ClipboardEntry[]>([]);
  const [searchQuery, setSearchQuery] = React.useState("");
  const [selectedApp, setSelectedApp] = React.useState<string>("all");
  const [selectedType, setSelectedType] = React.useState<"all" | "text" | "image" | "file">("all");
  const [dateRange, setDateRange] = React.useState<DateRange | undefined>();
  const [expandedId, setExpandedId] = React.useState<number | null>(null);
  const [copiedId, setCopiedId] = React.useState<number | null>(null);
  const [copiedPathId, setCopiedPathId] = React.useState<number | null>(null);
  const [focusedIndex, setFocusedIndex] = React.useState<number | null>(null);
  const [isFilterOpen, setIsFilterOpen] = React.useState(false);
  const [previewEntry, setPreviewEntry] = React.useState<ClipboardEntry | null>(null);
  const [showDeleted, setShowDeleted] = React.useState(false);
  const [deletedEntries, setDeletedEntries] = React.useState<DeletedClipboardEntry[]>([]);
  const [entriesPerPage, setEntriesPerPage] = React.useState("25");
  const [page, setPage] = React.useState(1);
  const [trashRetention, setTrashRetention] = React.useState<string | null>(null);
  const [closePromptOpen, setClosePromptOpen] = React.useState(false);
  const [closeRemember, setCloseRemember] = React.useState(false);
  const [appVersion, setAppVersion] = React.useState<string | null>(null);
  const [updateStatus, setUpdateStatus] = React.useState<UpdateStatus>({ phase: "idle" });
  const [toasts, setToasts] = React.useState<AppToast[]>([]);

  // Keyboard shortcuts and privacy monitoring state
  const [shortcuts, setShortcuts] = React.useState<ShortcutSettings>(DEFAULT_SHORTCUTS);
  const [isMonitoringPaused, setIsMonitoringPaused] = React.useState<boolean>(false);

  const searchInputRef = React.useRef<HTMLInputElement>(null);
  const feedScrollRef = React.useRef<HTMLDivElement | null>(null);
  const inFlightDeletionsRef = React.useRef<Set<number>>(new Set());
  const lastFetchTimeRef = React.useRef<number>(0);
  const toastIdRef = React.useRef<number>(0);
  const updateBusyRef = React.useRef(false);
  const installFiredRef = React.useRef(false);
  const autoCheckedRef = React.useRef(false);

  // Stacked transient confirmation pills (each auto-dismisses independently).
  const showToast = React.useCallback((message: string, kind: "success" | "error" = "success") => {
    toastIdRef.current += 1;
    const id = toastIdRef.current;
    setToasts((prev) => [...prev.slice(-(MAX_STACKED_TOASTS - 1)), { id, message, kind }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((toast) => toast.id !== id));
    }, 3000);
  }, []);
  const lastMousePosRef = React.useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const lastKeyboardNavTimeRef = React.useRef<number>(0);

  // Restore Always on Top, Shortcuts, and Monitoring state on startup
  React.useEffect(() => {
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      const saved = localStorage.getItem("clipbox:alwaysOnTop");
      if (saved === "true") {
        invoke("set_always_on_top", { alwaysOnTop: true }).catch((err) =>
          console.warn("Could not restore always on top setting", err)
        );
      }
      invoke<ShortcutSettings>("get_shortcut_settings")
        .then(setShortcuts)
        .catch(console.error);
      invoke<boolean>("is_monitoring_paused")
        .then(setIsMonitoringPaused)
        .catch(console.error);
    }
  }, []);

  const handleResumeMonitoring = async () => {
    setIsMonitoringPaused(false);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_monitoring_paused", { paused: false });
      }
    } catch (err) {
      console.error("Failed to resume monitoring", err);
    }
  };

  const handleOpenUrl = async (url: string) => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("open_url", { url });
      } else {
        window.open(url, "_blank");
      }
    } catch (err) {
      console.error("Failed to open source URL:", err);
    }
  };

  // Fetch clipboard entries from Tauri IPC with in-flight deletion protection
  const fetchEntries = React.useCallback(async () => {
    lastFetchTimeRef.current = Date.now();
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const result = await invoke<ClipboardEntry[]>("list_entries");
        setEntries(result.filter((e) => !inFlightDeletionsRef.current.has(e.id)));
      } else {
        setEntries(PREVIEW_ENTRIES);
      }
    } catch (err) {
      console.error("Failed to fetch entries:", err);
      setEntries(PREVIEW_ENTRIES);
    }
  }, []);

  // Initial load and re-sync on window focus
  React.useEffect(() => {
    fetchEntries();
    const handleFocus = () => {
      if (Date.now() - lastFetchTimeRef.current < FOCUS_REFETCH_MIN_INTERVAL_MS) {
        return;
      }
      fetchEntries();
    };
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [fetchEntries]);

  // Real-time clipboard capture and status event listeners
  React.useEffect(() => {
    let unlistenNew: (() => void) | undefined;
    let unlistenBump: (() => void) | undefined;
    let unlistenPause: (() => void) | undefined;
    let unlistenClose: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;

    const setupListener = async () => {
      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          unlistenNew = await listen<ClipboardEntry>("clipboard://new-entry", (event) => {
            if (inFlightDeletionsRef.current.has(event.payload.id)) {
              return;
            }
            setEntries((prev) => {
              if (prev.some((e) => e.id === event.payload.id)) {
                return prev;
              }
              return [event.payload, ...prev];
            });
          });

          unlistenBump = await listen<ClipboardEntry>("clipboard://entry-bumped", (event) => {
            if (inFlightDeletionsRef.current.has(event.payload.id)) {
              return;
            }
            setEntries((prev) => {
              const filtered = prev.filter((e) => e.id !== event.payload.id);
              return [event.payload, ...filtered];
            });
          });

          unlistenPause = await listen<boolean>("clipboard://monitoring-paused-changed", (event) => {
            setIsMonitoringPaused(event.payload);
          });

          unlistenClose = await listen("window://close-requested", () => {
            setCloseRemember(false);
            setClosePromptOpen(true);
          });

          unlistenProgress = await listen<{ downloaded: number; total: number }>(
            "update://download-progress",
            (event) => {
              // Late-arriving events must not clobber a finished state.
              setUpdateStatus((prev) =>
                prev.phase === "idle" ||
                prev.phase === "checking" ||
                prev.phase === "downloading"
                  ? {
                      phase: "downloading",
                      downloaded: event.payload.downloaded,
                      total: event.payload.total,
                    }
                  : prev
              );
            }
          );
        }
      } catch (err) {
        console.warn("Could not register clipboard event listeners", err);
      }
    };
    setupListener();
    return () => {
      if (unlistenNew) unlistenNew();
      if (unlistenBump) unlistenBump();
      if (unlistenPause) unlistenPause();
      if (unlistenClose) unlistenClose();
      if (unlistenProgress) unlistenProgress();
    };
  }, []);

  // Unique source apps list for filtering
  const availableApps = React.useMemo(() => {
    const apps = new Set<string>();
    for (const entry of entries) {
      apps.add(sourceLabel(entry));
    }
    return Array.from(apps).sort();
  }, [entries]);

  // Filtered entries by query, app, content type, and min/max date range
  const filteredEntries = React.useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    const fromTime = dateRange?.from
      ? Math.floor(startOfDay(dateRange.from).getTime() / 1000)
      : null;
    const toTime = dateRange?.to
      ? Math.floor(endOfDay(dateRange.to).getTime() / 1000)
      : null;

    return entries.filter((entry) => {
      const effectiveUrl =
        entry.sourceUrl || (isWebUrl(entry.content) ? entry.content.trim() : null);

      const matchesSearch =
        !query ||
        [
          entry.content,
          entry.sourceApp,
          entry.sourceProcess,
          entry.windowTitle,
          entry.sourceUrl,
          effectiveUrl,
          entry.ocrText,
        ].some((val) => val?.toLowerCase().includes(query));

      const matchesApp =
        selectedApp === "all" || sourceLabel(entry) === selectedApp;

      const matchesType =
        selectedType === "all" ||
        (selectedType === "image"
          ? entry.entryType === "image"
          : selectedType === "file"
            ? entry.entryType === "file"
            : entry.entryType !== "image" && entry.entryType !== "file");

      let matchesDate = true;
      if (fromTime !== null) {
        if (toTime !== null) {
          matchesDate = entry.copiedAt >= fromTime && entry.copiedAt <= toTime;
        } else {
          matchesDate = entry.copiedAt >= fromTime;
        }
      }

      return matchesSearch && matchesApp && matchesDate && matchesType;
    }).sort((a, b) => {
      if (Boolean(a.isPinned) === Boolean(b.isPinned)) {
        return b.copiedAt - a.copiedAt || b.id - a.id;
      }
      return a.isPinned ? -1 : 1;
    });
  }, [entries, searchQuery, selectedApp, selectedType, dateRange]);

  // Feed pagination: slice the filtered feed into pages. The page is clamped
  // inline so a shrinking list never renders an empty page for a frame.
  const perPage = Math.max(1, parseInt(entriesPerPage, 10) || 25);
  const totalPages = Math.max(1, Math.ceil(filteredEntries.length / perPage));
  const safePage = Math.min(Math.max(page, 1), totalPages);
  const paginatedEntries = React.useMemo(() => {
    const start = (safePage - 1) * perPage;
    return filteredEntries.slice(start, start + perPage);
  }, [filteredEntries, safePage, perPage]);

  // Toggle pinned status of an entry
  const handleTogglePin = async (id: number, event?: React.MouseEvent) => {
    event?.stopPropagation();
    setEntries((prev) =>
      prev.map((entry) =>
        entry.id === id ? { ...entry, isPinned: !entry.isPinned } : entry
      )
    );
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("toggle_pinned", { id });
      }
    } catch (err) {
      console.error("Failed to toggle pin", err);
      fetchEntries();
    }
  };

  // Copy entry text, image, or files to clipboard
  const handleCopy = async (entry: ClipboardEntry, event?: React.MouseEvent) => {
    event?.stopPropagation();
    try {
      if (entry.entryType === "image" && entry.imageData) {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          await invoke("copy_image_to_clipboard", { dataUrl: entry.imageData });
        } else {
          const res = await fetch(entry.imageData);
          const blob = await res.blob();
          await navigator.clipboard.write([
            new ClipboardItem({ [blob.type]: blob }),
          ]);
        }
      } else if (entry.entryType === "file") {
        let paths: string[] = [];
        try {
          if (entry.filesData) {
            const parsed = JSON.parse(entry.filesData);
            paths = parsed.map((item: { path: string }) => item.path);
          } else {
            paths = [entry.content];
          }
        } catch {
          paths = [entry.content];
        }

        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          await invoke("copy_files_to_clipboard", { paths });
        } else {
          await navigator.clipboard.writeText(paths.join("\n"));
        }
      } else {
        const content = stripLeadingEmptyLines(entry.content);
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          await invoke("copy_to_clipboard", { text: content });
        } else {
          await navigator.clipboard.writeText(content);
        }
      }
      setCopiedId(entry.id);
      setTimeout(() => setCopiedId(null), 1500);
    } catch (err) {
      console.error("Failed to copy", err);
    }
  };

  const handleCopyPaths = async (entry: ClipboardEntry, event: React.MouseEvent) => {
    event.stopPropagation();
    let paths: string[] = [];
    try {
      if (entry.filesData) {
        const parsed = JSON.parse(entry.filesData);
        paths = parsed.map((item: { path: string }) => item.path);
      } else {
        paths = [entry.content];
      }
    } catch {
      paths = [entry.content];
    }
    const text = paths.join("\n");
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      await invoke("copy_to_clipboard", { text });
    } else {
      await navigator.clipboard.writeText(text);
    }
    setCopiedPathId(entry.id);
    setTimeout(() => setCopiedPathId(null), 1500);
  };

  // Delete single entry from history
  const handleDelete = async (id: number, event?: React.MouseEvent) => {
    event?.stopPropagation();
    inFlightDeletionsRef.current.add(id);
    setEntries((prev) => prev.filter((entry) => entry.id !== id));
    // Re-sync after success: the backend resequences higher ids on every
    // delete, so displayed ids above the deleted one are stale afterwards.
    // A second delete using a stale id would archive the wrong record
    // (or nothing, leaving a ghost that only a later refetch reveals).
    let synced = false;
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const outcome = await invoke<string>("delete_entry", { id });
        if (outcome === "archived") {
          showToast("Moved to Trash");
        } else if (outcome === "deleted") {
          showToast("Deleted permanently");
        }
        synced = outcome === "archived" || outcome === "deleted";
      }
    } catch (err) {
      console.error("Failed to delete entry", err);
      showToast("Delete failed", "error");
      fetchEntries();
    } finally {
      inFlightDeletionsRef.current.delete(id);
    }
    if (synced) {
      fetchEntries();
    }
  };

  const fetchDeletedEntries = React.useCallback(async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        setDeletedEntries(await invoke<DeletedClipboardEntry[]>("list_deleted_entries"));
      } else {
        setDeletedEntries([]);
      }
    } catch (err) {
      console.error("Failed to fetch deleted entries:", err);
    }
  }, []);

  const fetchTrashRetention = React.useCallback(async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        setTrashRetention(await invoke<string>("get_deleted_retention"));
      } else {
        setTrashRetention(null);
      }
    } catch (err) {
      console.error("Failed to fetch deleted retention:", err);
      setTrashRetention(null);
    }
  }, []);

  const handleRestoreDeleted = async (id: number) => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        // Race the restore against a minimum indication window so the
        // per-card "Restoring..." state always reads as deliberate.
        const [restoredId] = await Promise.all([
          invoke<number | null>("restore_deleted_entry", { id }),
          new Promise<void>((resolve) => {
            window.setTimeout(resolve, RESTORE_MIN_INDICATION_MS);
          }),
        ]);
        if (restoredId !== null) {
          showToast("Restored to history");
        }
      }
      // Awaited so the card holds "Restoring..." until the lists actually
      // commit; otherwise the spinner stops first and the row lingers while
      // the (potentially image-heavy) refetch is still in flight.
      await Promise.all([fetchEntries(), fetchDeletedEntries()]);
    } catch (err) {
      console.error("Failed to restore entry", err);
      showToast("Restore failed", "error");
    }
  };

  const handleDeleteForever = async (id: number, event?: React.MouseEvent) => {
    event?.stopPropagation();
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("delete_deleted_entry", { id });
        showToast("Permanently deleted");
      }
      setDeletedEntries((prev) => prev.filter((entry) => entry.id !== id));
    } catch (err) {
      console.error("Failed to permanently delete entry", err);
      showToast("Delete failed", "error");
      fetchDeletedEntries();
    }
  };

  const scrollFeedToTop = () => {
    // Project ScrollArea is a plain overflow div (not Radix): scroll it
    // directly. "instant" overrides the scroll-smooth class so page turns
    // snap instead of gliding over the swapped content.
    feedScrollRef.current?.scrollTo({ top: 0, behavior: "instant" });
  };

  const clearFilters = () => {
    setSelectedApp("all");
    setSelectedType("all");
    setSearchQuery("");
    setDateRange(undefined);
    setIsFilterOpen(false);
    setPage(1);
    scrollFeedToTop();
  };

  const enterDeletedView = () => {
    clearFilters();
    setFocusedIndex(null);
    setShowDeleted(true);
    fetchDeletedEntries();
    fetchTrashRetention();
  };

  // Titlebar close (and Alt+F4 in "ask" mode via the backend event): follow
  // the stored close policy, prompting when no choice was remembered.
  const requestClose = React.useCallback(async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const behavior = await invoke<string>("get_close_behavior");
        if (behavior === "hide") {
          await invoke("hide_window");
          return;
        }
        if (behavior === "quit") {
          await invoke("exit_app");
          return;
        }
      }
    } catch (err) {
      console.warn("Could not read close behavior, prompting instead", err);
    }
    setCloseRemember(false);
    setClosePromptOpen(true);
  }, []);

  const handleCloseChoice = async (choice: "hide" | "quit") => {
    setClosePromptOpen(false);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        if (closeRemember) {
          await invoke("set_close_behavior", { behavior: choice });
        }
        await invoke(choice === "hide" ? "hide_window" : "exit_app");
      }
    } catch (err) {
      console.error("Failed to apply close choice", err);
    }
  };

  // Check GitHub Releases for a newer setup asset. Silent unless manual or
  // an update is found; offline failures simply leave the idle state.
  const checkForUpdates = React.useCallback(
    async (manual: boolean) => {
      if (updateBusyRef.current || !appVersion) return;
      updateBusyRef.current = true;
      if (manual) {
        setUpdateStatus({ phase: "checking" });
      }
      try {
        const response = await fetch(
          "https://api.github.com/repos/mwsk75996/clipbox/releases/latest"
        );
        if (!response.ok) throw new Error(`release lookup failed: ${response.status}`);
        const latest = (await response.json()) as {
          tag_name?: string;
          assets?: { name: string; browser_download_url: string }[];
        };
        const tag = latest.tag_name ?? "";
        const asset = (latest.assets ?? []).find(
          (a) => a.name.toLowerCase().endsWith(".exe") && /x64|setup/i.test(a.name)
        );
        if (
          asset &&
          compareAppVersions(tag, appVersion) > 0 &&
          typeof window !== "undefined" &&
          "__TAURI_INTERNALS__" in window
        ) {
          setUpdateStatus({ phase: "downloading", downloaded: 0, total: 0 });
          const path = await invoke<string>("download_update", {
            url: asset.browser_download_url,
            filename: asset.name,
          });
          setUpdateStatus({ phase: "ready", path });
          return;
        }
        if (manual) {
          showToast("You're up to date");
        }
      } catch (err) {
        console.warn("Update check failed", err);
        if (manual) {
          showToast("Update check failed", "error");
        }
      } finally {
        updateBusyRef.current = false;
        setUpdateStatus((prev) => (prev.phase === "checking" ? { phase: "idle" } : prev));
      }
    },
    [appVersion, showToast]
  );

  const installUpdate = React.useCallback(() => {
    if (updateStatus.phase !== "ready" || installFiredRef.current) return;
    installFiredRef.current = true;
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      // Resolves never on success (the app exits); failure restores the button.
      invoke("install_update", { path: updateStatus.path }).catch((err) => {
        console.error("Failed to install update", err);
        showToast("Update install failed", "error");
        installFiredRef.current = false;
        setUpdateStatus({ phase: "ready", path: updateStatus.path });
      });
    } else {
      installFiredRef.current = false;
      setUpdateStatus({ phase: "idle" });
    }
  }, [updateStatus, showToast]);

  // Read the packaged version once; auto-check shortly after startup.
  React.useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          const version = await invoke<string>("get_app_version");
          if (!cancelled) setAppVersion(version);
        }
      } catch (err) {
        console.warn("Could not read app version", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Load the persisted entries-per-page preference once.
  React.useEffect(() => {
    (async () => {
      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          setEntriesPerPage(await invoke<string>("get_entries_per_page"));
        }
      } catch (err) {
        console.warn("Could not read entries per page", err);
      }
    })();
  }, []);

  React.useEffect(() => {
    if (!appVersion || autoCheckedRef.current) return;
    autoCheckedRef.current = true;
    const timer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, 2000);
    return () => window.clearTimeout(timer);
  }, [appVersion, checkForUpdates]);

  // Reset keyboard focus and page when search or filters change
  React.useEffect(() => {
    setFocusedIndex(null);
    setPage(1);
  }, [searchQuery, selectedApp, selectedType, dateRange]);

  // Auto-scroll focused card into view smoothly
  React.useEffect(() => {
    if (focusedIndex !== null) {
      const cardEl = document.querySelector(`[data-card-index="${focusedIndex}"]`);
      if (cardEl) {
        cardEl.scrollIntoView({ block: "nearest", behavior: "smooth" });
      }
    }
  }, [focusedIndex]);

  const hasActiveFilters =
    selectedApp !== "all" || selectedType !== "all" || Boolean(dateRange?.from);

  // Global shortcuts, keyboard navigation, and browser prevention
  React.useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const activeEl = document.activeElement;
      const isInputFocused =
        activeEl?.tagName === "INPUT" ||
        activeEl?.tagName === "TEXTAREA" ||
        (activeEl as HTMLElement)?.isContentEditable;

      const hasOpenDialog = Boolean(document.querySelector('[role="dialog"]'));

      // Configurable Focus Search Bar shortcut
      if (matchesBinding(event, shortcuts.focus_search)) {
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        setFocusedIndex(null);
        return;
      }

      // If user is currently typing in an input
      if (isInputFocused) {
        if (event.key === "Escape") {
          event.preventDefault();
          searchInputRef.current?.blur();
          setFocusedIndex(null);
        }
        return;
      }

      // Ignore when modal dialog or lightbox is active
      if (hasOpenDialog || previewEntry !== null) {
        return;
      }

      // Configurable Clear / Escape shortcut
      if (matchesBinding(event, shortcuts.clear_escape)) {
        if (searchQuery || hasActiveFilters) {
          clearFilters();
          setFocusedIndex(null);
        } else if (focusedIndex !== null) {
          setFocusedIndex(null);
        } else if (showDeleted) {
          setShowDeleted(false);
        } else if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          invoke("hide_window").catch(console.error);
        }
        return;
      }

      // Configurable Nav Down shortcut (active feed only)
      if (matchesBinding(event, shortcuts.nav_down) && !showDeleted) {
        event.preventDefault();
        lastKeyboardNavTimeRef.current = Date.now();
        if (paginatedEntries.length === 0) return;
        setFocusedIndex((prev) => {
          if (prev === null) return 0;
          return Math.min(prev + 1, paginatedEntries.length - 1);
        });
        return;
      }

      // Configurable Nav Up shortcut (active feed only, clamps to the list)
      if (matchesBinding(event, shortcuts.nav_up) && !showDeleted) {
        event.preventDefault();
        lastKeyboardNavTimeRef.current = Date.now();
        if (paginatedEntries.length === 0) return;
        setFocusedIndex((prev) => {
          if (prev === null) return paginatedEntries.length - 1;
          return Math.max(prev - 1, 0);
        });
        return;
      }

      // Page navigation with arrow keys (active feed only, never while typing)
      if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && !showDeleted) {
        event.preventDefault();
        setFocusedIndex(null);
        setPage((prev) =>
          Math.min(Math.max(prev + (event.key === "ArrowRight" ? 1 : -1), 1), totalPages)
        );
        scrollFeedToTop();
        return;
      }

      // Configurable Copy focused entry shortcut (active feed only)
      if (matchesBinding(event, shortcuts.copy_entry) && !showDeleted && focusedIndex !== null && paginatedEntries[focusedIndex]) {
        event.preventDefault();
        const targetEntry = paginatedEntries[focusedIndex];
        handleCopy(targetEntry);
        return;
      }

      // Configurable Expand / collapse preview shortcut (active feed only)
      if (matchesBinding(event, shortcuts.expand_preview) && !showDeleted && focusedIndex !== null && paginatedEntries[focusedIndex]) {
        event.preventDefault();
        const targetEntry = paginatedEntries[focusedIndex];
        const content = stripLeadingEmptyLines(targetEntry.content);
        const lines = content.split(/\r?\n/);
        const isExpandable = lines.length > 1 || content.length > 90;
        if (isExpandable) {
          setExpandedId((prev) => (prev === targetEntry.id ? null : targetEntry.id));
        }
        return;
      }

      // Configurable Delete focused entry shortcut (active feed only)
      if (matchesBinding(event, shortcuts.delete_entry) && !showDeleted && focusedIndex !== null && paginatedEntries[focusedIndex]) {
        event.preventDefault();
        const targetEntry = paginatedEntries[focusedIndex];
        handleDelete(targetEntry.id);
        if (focusedIndex >= paginatedEntries.length - 1) {
          setFocusedIndex(Math.max(0, paginatedEntries.length - 2));
        }
        return;
      }

      // Configurable Toggle Pin shortcut (active feed only)
      if (matchesBinding(event, shortcuts.toggle_pin) && !showDeleted && focusedIndex !== null && paginatedEntries[focusedIndex]) {
        event.preventDefault();
        const targetEntry = paginatedEntries[focusedIndex];
        handleTogglePin(targetEntry.id);
        return;
      }

      // Block browser default shortcuts (find, refresh, print, view source, devtools, history, etc.)
      if (
        event.key === "F5" ||
        event.key === "F3" ||
        event.key === "F12" ||
        ((event.ctrlKey || event.metaKey) &&
          ["r", "p", "u", "g", "h", "j", "s", "o", "n", "d", "b", "l", "w"].includes(key)) ||
        ((event.ctrlKey || event.metaKey) && event.shiftKey && ["i", "j", "r", "c"].includes(key))
      ) {
        event.preventDefault();
      }
    };

    // Block browser right-click context menu
    const handleContextMenu = (event: MouseEvent) => {
      event.preventDefault();
    };

    // Moving or clicking within app content clears the keyboard navigation highlight line.
    // Window chrome is excluded so native window actions never trigger a feed rerender first.
    const isWindowChromeEvent = (event: Event) =>
      event.target instanceof Element &&
      event.target.closest("[data-window-chrome]") !== null;

    const handleMouseMove = (event: MouseEvent) => {
      if (focusedIndex === null || isWindowChromeEvent(event)) {
        return;
      }

      if (
        event.clientX === lastMousePosRef.current.x &&
        event.clientY === lastMousePosRef.current.y
      ) {
        return;
      }
      lastMousePosRef.current = { x: event.clientX, y: event.clientY };

      if (Date.now() - lastKeyboardNavTimeRef.current < 150) {
        return;
      }

      setFocusedIndex(null);
    };

    const handlePointerDown = (event: PointerEvent) => {
      if (focusedIndex !== null && !isWindowChromeEvent(event)) {
        setFocusedIndex(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("contextmenu", handleContextMenu);
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("pointerdown", handlePointerDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("contextmenu", handleContextMenu);
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [filteredEntries, paginatedEntries, totalPages, focusedIndex, previewEntry, searchQuery, hasActiveFilters, shortcuts, showDeleted]);

  // Handle card expansion while preventing collapse when selecting/marking text
  const handleCardClick = (
    id: number,
    isExpandable: boolean,
    e: React.MouseEvent
  ) => {
    if (!isExpandable) return;

    // Ignore clicks on buttons
    if ((e.target as HTMLElement).closest("button")) return;

    // If user marked/selected text with mouse, do not toggle
    const selection = window.getSelection();
    if (
      selection &&
      !selection.isCollapsed &&
      selection.toString().trim().length > 0
    ) {
      return;
    }

    setExpandedId((prev) => (prev === id ? null : id));
  };

  return (
    <div className="h-screen w-screen bg-background text-foreground flex flex-col font-sans select-none overflow-hidden transition-colors duration-200">
      {/* Native Desktop Titlebar with Centered Entries Count */}
      <Titlebar entriesCount={filteredEntries.length} onCloseRequest={requestClose} />

      {/* Main App Container */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Single-Line Compact Toolbar */}
        <header className="border-b bg-card/60 backdrop-blur px-6 py-2.5 shrink-0">
          <div className="max-w-4xl mx-auto flex items-center gap-2">
            <div className="relative flex-1 min-w-[200px]">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground pointer-events-none" />
              <Input
                ref={searchInputRef}
                type="search"
                placeholder="Search history..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "ArrowDown" && !showDeleted && paginatedEntries.length > 0) {
                    e.preventDefault();
                    searchInputRef.current?.blur();
                    setFocusedIndex(0);
                  }
                }}
                className="pl-9 pr-14 bg-background h-9"
              />
              <kbd className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded border bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground select-none pointer-events-none">
                {shortcuts.focus_search?.label || "Ctrl F"}
              </kbd>
            </div>

            <DateRangePicker date={dateRange} setDate={setDateRange} />

            <Popover open={isFilterOpen} onOpenChange={setIsFilterOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant={selectedApp !== "all" || selectedType !== "all" ? "default" : "outline"}
                  size="sm"
                  className={`gap-1.5 shrink-0 h-9 ${
                    selectedApp !== "all" || selectedType !== "all" ? "border border-transparent" : ""
                  }`}
                >
                  <SlidersHorizontal className="size-4" />
                  Filters
                  {(selectedApp !== "all" || selectedType !== "all") && (
                    <span className="size-2 rounded-full bg-primary-foreground" />
                  )}
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-72 space-y-4">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-sm">Filter History</span>
                  {(selectedApp !== "all" || selectedType !== "all") && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setSelectedApp("all");
                        setSelectedType("all");
                      }}
                      className="h-8 text-xs text-muted-foreground hover:text-foreground"
                    >
                      <RotateCcw className="size-3 mr-1" />
                      Reset
                    </Button>
                  )}
                </div>

                <Separator />

                <div className="space-y-2">
                  <label className="text-xs font-medium text-muted-foreground">
                    Content Type
                  </label>
                  <Select
                    value={selectedType}
                    onValueChange={(val) =>
                      setSelectedType(val as "all" | "text" | "image" | "file")
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="All types" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All types</SelectItem>
                      <SelectItem value="text">Text only</SelectItem>
                      <SelectItem value="image">Images only</SelectItem>
                      <SelectItem value="file">Files only</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <label className="text-xs font-medium text-muted-foreground">
                    Source Application
                  </label>
                  <Select value={selectedApp} onValueChange={setSelectedApp}>
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="All applications" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All applications</SelectItem>
                      {availableApps.map((app) => (
                        <SelectItem key={app} value={app}>
                          {app}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </PopoverContent>
            </Popover>

            <Button
              variant={showDeleted ? "default" : "outline"}
              size="sm"
              onClick={() => {
                if (showDeleted) {
                  setShowDeleted(false);
                } else {
                  enterDeletedView();
                }
              }}
              // Transparent border in the active state keeps the box identical
              // to the outlined inactive state (outline adds a 1px border).
              className={`gap-1.5 shrink-0 h-9 ${showDeleted ? "border border-transparent" : ""}`}
              title="View recently deleted clips"
            >
              <Trash2 className="size-4" />
              Trash
            </Button>

            {isMonitoringPaused && (
              <Button
                variant="outline"
                size="sm"
                onClick={handleResumeMonitoring}
                className="gap-1.5 shrink-0 h-9 text-amber-600 dark:text-amber-400 border-amber-500/30 bg-amber-500/10 hover:bg-amber-500/20 text-xs font-medium animate-pulse"
                title="Clipboard capture is currently paused. Click to resume."
              >
                <PauseCircle className="size-4" />
                Paused
              </Button>
            )}

            <ThemeToggle />
            <SettingsModal
              onClearHistory={() => {
                inFlightDeletionsRef.current.clear();
                setEntries([]);
                setPage(1);
                fetchDeletedEntries();
              }}
              shortcuts={shortcuts}
              onShortcutsChange={setShortcuts}
              isMonitoringPaused={isMonitoringPaused}
              onMonitoringPausedChange={setIsMonitoringPaused}
              onEntriesPerPageChange={(value) => {
                setEntriesPerPage(value);
                setPage(1);
                setFocusedIndex(null);
                scrollFeedToTop();
              }}
            />
          </div>
        </header>

        {/* Main Content Area */}
        <main className="flex-1 w-full overflow-hidden flex flex-col min-h-0 relative">
          {showDeleted ? (
            <div className="relative flex-1 w-full min-h-0 flex flex-col overflow-hidden">
              <ScrollArea className="flex-1 w-full h-full">
                <div className="max-w-4xl w-full mx-auto px-6 pt-5 pb-20 space-y-3">
                  <div className="flex items-center justify-between gap-4">
                    <div>
                      <h3 className="text-sm font-semibold flex items-center gap-1.5">
                        <Trash2 className="size-4" />
                        Recently Deleted
                        {trashRetention === "immediately" ? (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                            <TriangleAlert className="size-3" />
                            Instant deletion on
                          </span>
                        ) : trashRetention ? (
                          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-muted text-muted-foreground border">
                            Auto-purge: {TRASH_RETENTION_LABELS[trashRetention] ?? trashRetention}
                          </span>
                        ) : null}
                      </h3>
                      <p className="text-xs text-muted-foreground mt-0.5">
                        Restore clips before they are permanently purged.
                      </p>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setShowDeleted(false)}
                      className="gap-1.5 h-8 text-xs shrink-0"
                    >
                      <RotateCcw className="size-3.5" />
                      Back to history
                    </Button>
                  </div>
                  {deletedEntries.length === 0 ? (
                    <div className="flex flex-col items-center justify-center py-16 text-center">
                      {trashRetention === "immediately" ? (
                        <>
                          <span className="size-12 rounded-full bg-amber-500/10 border border-amber-500/20 flex items-center justify-center">
                            <TriangleAlert className="size-6 text-amber-500" />
                          </span>
                          <h4 className="text-sm font-semibold mt-4">Instant deletion is on</h4>
                          <p className="text-xs text-muted-foreground max-w-xs mt-1">
                            Deleted clips are purged immediately and can&apos;t be restored.
                            Change this in Settings, under Storage &amp; Database.
                          </p>
                        </>
                      ) : (
                        <>
                          <span className="size-12 rounded-full bg-muted border flex items-center justify-center">
                            <Trash2 className="size-6 text-muted-foreground" />
                          </span>
                          <h4 className="text-sm font-semibold mt-4">Trash is empty</h4>
                          <p className="text-xs text-muted-foreground max-w-xs mt-1">
                            {trashRetention && TRASH_RETENTION_LABELS[trashRetention]
                              ? `Deleted clips stay here for ${TRASH_RETENTION_LABELS[trashRetention].toLowerCase()} before permanent purge.`
                              : "Deleted clips stay here until the retention timespan passes."}
                          </p>
                        </>
                      )}
                    </div>
                  ) : (
                    deletedEntries.map((entry) => (
                      <DeletedEntryCard
                        key={entry.id}
                        entry={entry}
                        retention={trashRetention}
                        onRestore={handleRestoreDeleted}
                        onDeleteForever={handleDeleteForever}
                      />
                    ))
                  )}
                </div>
              </ScrollArea>
            </div>
          ) : filteredEntries.length === 0 ? (
            <div className="max-w-4xl w-full mx-auto px-6 flex flex-col items-center justify-center my-auto py-16 text-center">
              <p className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
                Your Clipboard
              </p>
              <h3 className="text-lg font-bold tracking-tight mt-0.5">History</h3>
              <p className="text-sm text-muted-foreground max-w-sm mt-1">
                {searchQuery || hasActiveFilters
                  ? "No clipboard entries match your filter criteria. Try clearing search or filters."
                  : "Everything you copy, kept close at hand. Copy text from any other application and it will appear here."}
              </p>
              {(searchQuery || hasActiveFilters) && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={clearFilters}
                  className="mt-4"
                >
                  Clear all filters
                </Button>
              )}
            </div>
          ) : (
            <div className="relative flex-1 w-full min-h-0 flex flex-col overflow-hidden">
              <ScrollArea ref={feedScrollRef} className="flex-1 w-full h-full">
                <div className="max-w-4xl w-full mx-auto px-6 pt-5 pb-20 space-y-3">
                  {paginatedEntries.map((entry, index) => {
                  const content = stripLeadingEmptyLines(entry.content);
                  const lines = content.split(/\r?\n/);
                  const isExpandable = lines.length > 1 || content.length > 90;
                  const isExpanded = expandedId === entry.id;
                  const isFocused = focusedIndex === index;
                  const appName = sourceLabel(entry);

                  return (
                    <Card
                      key={entry.id}
                      data-card-index={index}
                      onClick={(e) => {
                        setFocusedIndex(null);
                        handleCardClick(entry.id, isExpandable, e);
                      }}
                      className={`transition-all border hover:border-primary/40 cursor-pointer shadow-sm ${
                        isFocused
                          ? "ring-2 ring-primary border-primary shadow-md"
                          : ""
                      } ${
                        entry.isPinned
                          ? "border-primary/40 bg-primary/[0.03] dark:bg-primary/[0.05] ring-1 ring-primary/20"
                          : ""
                      } ${isExpanded ? "ring-1 ring-primary/30" : ""}`}
                    >
                      <CardHeader className="p-4 pb-2 flex flex-row items-center justify-between space-y-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          {entry.isPinned && (
                            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-primary/10 text-primary border border-primary/20 select-none">
                              <Pin className="size-2.5 fill-primary" />
                              Pinned
                            </span>
                          )}
                          {entry.entryType === "file" ? (
                            <>
                              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20 select-none">
                                <Files className="size-2.5" />
                                Files
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
                              {entry.windowTitle &&
                                entry.windowTitle.toLowerCase() !==
                                  appName.toLowerCase() && (
                                  <>
                                    <span className="text-xs text-muted-foreground">·</span>
                                    <span className="text-xs text-muted-foreground truncate max-w-[240px]">
                                      {entry.windowTitle}
                                    </span>
                                  </>
                                )}
                              <span className="text-xs text-muted-foreground">·</span>
                              <CardDescription className="text-xs">
                                {formatTimestamp(entry.copiedAt)}
                              </CardDescription>
                            </>
                          ) : entry.entryType === "image" ? (
                            <>
                              {appName.toLowerCase().includes("screen capture") ||
                              appName.toLowerCase().includes("screenshot") ? (
                                <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 select-none">
                                  <Camera className="size-2.5" />
                                  Screen Capture
                                </span>
                              ) : (
                                <>
                                  <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 select-none">
                                    <ImageIcon className="size-2.5" />
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
                                </>
                              )}
                              <span className="text-xs text-muted-foreground">·</span>
                              <CardDescription className="text-xs">
                                {formatTimestamp(entry.copiedAt)}
                              </CardDescription>
                            </>
                          ) : (
                            <>
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
                              {entry.windowTitle &&
                                entry.windowTitle.toLowerCase() !==
                                  appName.toLowerCase() && (
                                  <>
                                    <span className="text-xs text-muted-foreground">·</span>
                                    <span className="text-xs text-muted-foreground truncate max-w-[240px]">
                                      {entry.windowTitle}
                                    </span>
                                  </>
                                )}
                              <span className="text-xs text-muted-foreground">·</span>
                              <CardDescription className="text-xs">
                                {formatTimestamp(entry.copiedAt)}
                              </CardDescription>
                            </>
                          )}
                          {(() => {
                            const effectiveUrl =
                              entry.sourceUrl ||
                              (isWebUrl(entry.content) ? entry.content.trim() : null);
                            if (!effectiveUrl) return null;
                            return (
                              <>
                                <span className="text-xs text-muted-foreground">·</span>
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleOpenUrl(effectiveUrl);
                                  }}
                                  title={`Open original URL: ${effectiveUrl}`}
                                  className="inline-flex items-center gap-1 text-xs text-primary/80 hover:text-primary hover:underline max-w-[200px] truncate transition-colors cursor-pointer group"
                                >
                                  <ExternalLink className="size-3 shrink-0 opacity-70 group-hover:opacity-100" />
                                  <span className="truncate">{formatSourceUrl(effectiveUrl)}</span>
                                </button>
                              </>
                            );
                          })()}
                        </div>

                        <div className="flex items-center gap-1 shrink-0">
                          <span className="text-[11px] font-mono text-muted-foreground mr-1">
                            #{entry.id}
                          </span>
                          <Button
                            variant="ghost"
                            size="icon"
                            className={`size-7 transition-colors ${
                              entry.isPinned
                                ? "text-primary hover:text-primary/80 bg-primary/10 hover:bg-primary/20"
                                : "text-muted-foreground hover:text-foreground"
                            }`}
                            title={entry.isPinned ? "Unpin from top" : "Pin to top"}
                            onClick={(e) => handleTogglePin(entry.id, e)}
                          >
                            <Pin className={`size-3.5 ${entry.isPinned ? "fill-primary" : ""}`} />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-7"
                            title="Copy to clipboard"
                            onClick={(e) => handleCopy(entry, e)}
                          >
                            {copiedId === entry.id ? (
                              <Check className="size-3.5 text-green-600 dark:text-green-400" />
                            ) : (
                              <Copy className="size-3.5 text-muted-foreground" />
                            )}
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-7 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                            title="Delete entry"
                            onClick={(e) => handleDelete(entry.id, e)}
                          >
                            <Trash2 className="size-3.5" />
                          </Button>
                        </div>
                      </CardHeader>

                      <CardContent className="p-4 pt-1">
                        {entry.entryType === "file" ? (
                          <FileEntryCard
                            entry={entry}
                            onCopyPaths={(e) => handleCopyPaths(entry, e)}
                            isCopiedPaths={copiedPathId === entry.id}
                          />
                        ) : entry.entryType === "image" && entry.imageData ? (
                          <div>
                            <div
                              className="relative group overflow-hidden rounded-md border bg-muted/20 max-h-[300px] flex items-center justify-center p-1.5 cursor-zoom-in transition-all hover:border-primary/40 hover:bg-muted/30"
                              onClick={(e) => {
                                e.stopPropagation();
                                setPreviewEntry(entry);
                              }}
                              title="Click to view full-size image"
                            >
                              <img
                                src={entry.imageData}
                                alt={entry.content}
                                className="max-h-[280px] w-auto max-w-full object-contain rounded select-none transition-transform duration-200 group-hover:scale-[1.015]"
                                loading="lazy"
                              />
                              <div className="absolute inset-0 bg-black/0 group-hover:bg-black/25 transition-colors flex items-center justify-center opacity-0 group-hover:opacity-100 pointer-events-none">
                                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-black/75 text-white text-xs font-medium backdrop-blur-sm shadow-md">
                                  <ZoomIn className="size-3.5" />
                                  Preview
                                </span>
                              </div>
                            </div>
                            {entry.imageDimensions && (
                              <div className="flex items-center justify-between mt-2 text-[11px] text-muted-foreground font-mono">
                                <div className="flex items-center gap-1.5">
                                  <ImageIcon className="size-3" />
                                  <span>{entry.imageDimensions}</span>
                                </div>
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setPreviewEntry(entry);
                                  }}
                                  className="text-[11px] text-muted-foreground hover:text-foreground cursor-pointer transition-colors"
                                >
                                  Click to expand
                                </button>
                              </div>
                            )}
                          </div>
                        ) : (
                          <>
                            <div
                              onClick={(e) => {
                                if (isExpanded) {
                                  e.stopPropagation();
                                }
                              }}
                              className={`text-sm font-dmsans font-normal tracking-normal leading-relaxed whitespace-pre-wrap break-words [overflow-wrap:anywhere] select-text cursor-text ${
                                !isExpanded && isExpandable ? "line-clamp-1" : ""
                              }`}
                            >
                              {isExpanded ? content : (lines.length > 1 ? lines[0] : content)}
                            </div>

                            {isExpandable && (
                              <div
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setExpandedId(isExpanded ? null : entry.id);
                                }}
                                className="flex items-center gap-1 mt-2 text-xs text-muted-foreground hover:text-foreground cursor-pointer select-none"
                              >
                                {isExpanded ? (
                                  <>
                                    <ChevronUp className="size-3" />
                                    <span>Collapse</span>
                                  </>
                                ) : (
                                  <>
                                    <ChevronDown className="size-3" />
                                    <span>
                                      {lines.length > 1
                                        ? `+${lines.length - 1} more lines`
                                        : "Show more"}
                                    </span>
                                  </>
                                )}
                              </div>
                            )}
                          </>
                        )}
                      </CardContent>
                    </Card>
                  );
                })}
              </div>
            </ScrollArea>
          </div>
        )}
        </main>
      </div>

      {toasts.length > 0 &&
        createPortal(
          <div className="fixed bottom-16 left-1/2 -translate-x-1/2 z-[100] pointer-events-none flex flex-col items-center gap-2">
            {toasts.map((toast) => (
              <div
                key={toast.id}
                className="animate-in fade-in-0 slide-in-from-bottom-2 duration-200"
              >
                <div className="bg-popover/95 backdrop-blur-md text-popover-foreground border shadow-xl rounded-full px-4 py-2 flex items-center gap-2 text-xs font-medium whitespace-nowrap">
                  {toast.kind === "success" ? (
                    <Check className="size-4 text-emerald-400 shrink-0" />
                  ) : (
                    <X className="size-4 text-red-400 shrink-0" />
                  )}
                  <span>{toast.message}</span>
                </div>
              </div>
            ))}
          </div>,
          document.body
        )}

      {/* Bottom status footer */}
      <footer className="border-t bg-card/60 backdrop-blur px-6 py-1.5 shrink-0">
        <div className="max-w-4xl mx-auto grid grid-cols-[1fr_auto_1fr] items-center gap-2 text-[11px] text-muted-foreground">
          <span className="select-none justify-self-start">{appVersion ? `v${appVersion}` : ""}</span>
          {!showDeleted && (
            <div className="flex items-center gap-1.5 justify-self-center">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  setFocusedIndex(null);
                  setPage((prev) => Math.max(prev - 1, 1));
                  scrollFeedToTop();
                }}
                disabled={safePage <= 1}
                className="size-7 rounded-md"
                title="Previous page (Arrow Left)"
              >
                <ChevronLeft className="size-3.5" />
              </Button>
              <span
                className="size-7 rounded-full border bg-muted flex items-center justify-center text-xs leading-none font-medium text-foreground select-none"
                title={`Page ${safePage} of ${totalPages}`}
              >
                {safePage}
              </span>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  setFocusedIndex(null);
                  setPage((prev) => Math.min(prev + 1, totalPages));
                  scrollFeedToTop();
                }}
                disabled={safePage >= totalPages}
                className="size-7 rounded-md"
                title="Next page (Arrow Right)"
              >
                <ChevronRight className="size-3.5" />
              </Button>
            </div>
          )}
          <div className="justify-self-end">
          {appVersion && updateStatus.phase === "ready" ? (
            <Button
              size="sm"
              onClick={installUpdate}
              className="h-7 gap-1.5 text-xs"
              title="Install the downloaded update and restart"
            >
              <RotateCcw className="size-3.5" />
              <span>Restart to update</span>
            </Button>
          ) : appVersion ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void checkForUpdates(true)}
              disabled={updateStatus.phase !== "idle"}
              className="h-7 gap-1.5 text-xs"
              title="Check for updates"
            >
              {updateStatus.phase === "downloading" ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Download className="size-3.5" />
              )}
              <span>
                {updateStatus.phase === "checking"
                  ? "Checking..."
                  : updateStatus.phase === "downloading"
                    ? updateStatus.total > 0
                      ? `Downloading ${Math.round(
                          (updateStatus.downloaded / updateStatus.total) * 100
                        )}%`
                      : "Downloading..."
                    : "Check for updates"}
              </span>
            </Button>
          ) : null}
          </div>
        </div>
      </footer>

      <ImageLightbox
        entry={previewEntry}
        isOpen={previewEntry !== null}
        onClose={() => setPreviewEntry(null)}
        formatTimestamp={formatTimestamp}
      />

      {/* Close behavior prompt (titlebar X, or Alt+F4 in "ask" mode) */}
      <Dialog open={closePromptOpen} onOpenChange={setClosePromptOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Close Clipbox?</DialogTitle>
            <DialogDescription>
              Quit the app entirely, or hide it to the tray and keep monitoring your clipboard?
            </DialogDescription>
          </DialogHeader>
          <label
            htmlFor="close-remember"
            className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer select-none"
          >
            <Switch
              id="close-remember"
              checked={closeRemember}
              onCheckedChange={setCloseRemember}
            />
            Remember my choice
          </label>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button variant="outline" onClick={() => handleCloseChoice("hide")}>
              Hide to tray
            </Button>
            <Button variant="destructive" onClick={() => handleCloseChoice("quit")}>
              Quit Clipbox
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
