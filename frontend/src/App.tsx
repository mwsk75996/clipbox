// ----------
// Clipboard History Dashboard
// Description: Main application component utilizing shadcn/ui primitives, custom desktop titlebar, date range calendar filtering, and dynamic theme controls without browser artifacts.
// ----------

import * as React from "react";
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
  Trash2,
} from "lucide-react";
import { startOfDay, endOfDay } from "date-fns";
import type { DateRange } from "react-day-picker";

import { Titlebar } from "@/components/titlebar";
import { Button } from "@/components/ui/button";
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
import { Separator } from "@/components/ui/separator";
import { DateRangePicker } from "@/components/date-range-picker";
import { ThemeToggle } from "@/components/theme-toggle";
import { SettingsModal } from "@/components/settings-modal";

export interface ClipboardEntry {
  id: number;
  content: string;
  copiedAt: number;
  sourceApp?: string | null;
  sourceProcess?: string | null;
  windowTitle?: string | null;
  appIcon?: string | null;
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

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

function sourceLabel(entry: ClipboardEntry): string {
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
function stripLeadingEmptyLines(text: string): string {
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
  const [dateRange, setDateRange] = React.useState<DateRange | undefined>();
  const [expandedId, setExpandedId] = React.useState<number | null>(null);
  const [copiedId, setCopiedId] = React.useState<number | null>(null);
  const [isFilterOpen, setIsFilterOpen] = React.useState(false);

  const searchInputRef = React.useRef<HTMLInputElement>(null);

  // Global shortcuts and browser prevention
  React.useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();

      // Ctrl+F or Ctrl+K focuses and selects inside the app search bar
      if ((event.metaKey || event.ctrlKey) && (key === "f" || key === "k")) {
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }

      // Escape key hides window to system tray if no modal dialog is open
      if (event.key === "Escape") {
        const hasOpenDialog = document.querySelector('[role="dialog"]');
        if (!hasOpenDialog) {
          if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
            invoke("hide_window").catch(console.error);
          }
        }
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

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("contextmenu", handleContextMenu);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("contextmenu", handleContextMenu);
    };
  }, []);

  // Restore Always on Top preference on startup
  React.useEffect(() => {
    const saved = localStorage.getItem("clipbox:alwaysOnTop");
    if (saved === "true") {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        invoke("set_always_on_top", { alwaysOnTop: true }).catch((err) =>
          console.warn("Could not restore always on top setting", err)
        );
      }
    }
  }, []);

  // Poll clipboard entries from Tauri IPC
  const fetchEntries = React.useCallback(async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const result = await invoke<ClipboardEntry[]>("list_entries");
        setEntries(result);
      } else {
        setEntries(PREVIEW_ENTRIES);
      }
    } catch {
      setEntries(PREVIEW_ENTRIES);
    }
  }, []);

  React.useEffect(() => {
    fetchEntries();
    const interval = setInterval(fetchEntries, 1000);
    return () => clearInterval(interval);
  }, [fetchEntries]);

  // Real-time clipboard capture listener for instantaneous UI updates
  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    const setupListener = async () => {
      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          unlisten = await listen<ClipboardEntry>("clipboard://new-entry", (event) => {
            setEntries((prev) => {
              if (prev.some((e) => e.id === event.payload.id)) {
                return prev;
              }
              return [event.payload, ...prev];
            });
          });
        }
      } catch (err) {
        console.warn("Could not register clipboard event listener", err);
      }
    };
    setupListener();
    return () => {
      if (unlisten) unlisten();
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

  // Filtered entries by query, app, and min/max date range
  const filteredEntries = React.useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    const fromTime = dateRange?.from
      ? Math.floor(startOfDay(dateRange.from).getTime() / 1000)
      : null;
    const toTime = dateRange?.to
      ? Math.floor(endOfDay(dateRange.to).getTime() / 1000)
      : null;

    return entries.filter((entry) => {
      const matchesSearch =
        !query ||
        [
          entry.content,
          entry.sourceApp,
          entry.sourceProcess,
          entry.windowTitle,
        ].some((val) => val?.toLowerCase().includes(query));

      const matchesApp =
        selectedApp === "all" || sourceLabel(entry) === selectedApp;

      let matchesDate = true;
      if (fromTime !== null) {
        if (toTime !== null) {
          matchesDate = entry.copiedAt >= fromTime && entry.copiedAt <= toTime;
        } else {
          matchesDate = entry.copiedAt >= fromTime;
        }
      }

      return matchesSearch && matchesApp && matchesDate;
    });
  }, [entries, searchQuery, selectedApp, dateRange]);

  // Copy entry text to clipboard
  const handleCopy = async (entry: ClipboardEntry, event?: React.MouseEvent) => {
    event?.stopPropagation();
    try {
      const content = stripLeadingEmptyLines(entry.content);
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("copy_to_clipboard", { text: content });
      } else {
        await navigator.clipboard.writeText(content);
      }
      setCopiedId(entry.id);
      setTimeout(() => setCopiedId(null), 1500);
    } catch (err) {
      console.error("Failed to copy", err);
    }
  };

  // Delete single entry from history
  const handleDelete = async (id: number, event: React.MouseEvent) => {
    event.stopPropagation();
    setEntries((prev) => prev.filter((entry) => entry.id !== id));
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("delete_entry", { id });
      }
    } catch (err) {
      console.error("Failed to delete entry", err);
      fetchEntries();
    }
  };

  const clearFilters = () => {
    setSelectedApp("all");
    setSearchQuery("");
    setDateRange(undefined);
    setIsFilterOpen(false);
  };

  // Handle card expansion while preventing collapse when selecting/marking text
  const handleCardClick = (
    id: number,
    isMultiLine: boolean,
    e: React.MouseEvent
  ) => {
    if (!isMultiLine) return;

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

  const hasActiveFilters = selectedApp !== "all" || Boolean(dateRange?.from);

  return (
    <div className="h-screen w-screen bg-background text-foreground flex flex-col font-sans select-none overflow-hidden transition-colors duration-200">
      {/* Native Desktop Titlebar with Centered Entries Count */}
      <Titlebar entriesCount={filteredEntries.length} />

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
                className="pl-9 pr-14 bg-background h-9"
              />
              <kbd className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded border bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground select-none pointer-events-none">
                Ctrl F
              </kbd>
            </div>

            <DateRangePicker date={dateRange} setDate={setDateRange} />

            <Popover open={isFilterOpen} onOpenChange={setIsFilterOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant={selectedApp !== "all" ? "default" : "outline"}
                  size="sm"
                  className="gap-1.5 shrink-0 h-9"
                >
                  <SlidersHorizontal className="size-4" />
                  Filters
                  {selectedApp !== "all" && (
                    <span className="size-2 rounded-full bg-primary-foreground" />
                  )}
                </Button>
              </PopoverTrigger>
              <PopoverContent align="end" className="w-72 space-y-4">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-sm">Filter History</span>
                  {selectedApp !== "all" && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setSelectedApp("all")}
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

            <ThemeToggle />
            <SettingsModal onClearHistory={() => setEntries([])} />
          </div>
        </header>

        {/* Main Content Area */}
        <main className="flex-1 max-w-4xl w-full mx-auto px-6 pt-5 pb-0 overflow-hidden flex flex-col min-h-0 relative">
          {filteredEntries.length === 0 ? (
            <div className="flex flex-col items-center justify-center my-auto py-16 text-center">
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
            <div className="relative flex-1 min-h-0 flex flex-col overflow-hidden">
              <ScrollArea className="flex-1 h-full pr-1 fade-bottom-mask">
                <div className="space-y-3 pt-1 pb-20 px-1">
                {filteredEntries.map((entry) => {
                  const content = stripLeadingEmptyLines(entry.content);
                  const lines = content.split(/\r?\n/);
                  const isMultiLine = lines.length > 1;
                  const isExpanded = expandedId === entry.id;
                  const appName = sourceLabel(entry);

                  return (
                    <Card
                      key={entry.id}
                      onClick={(e) =>
                        handleCardClick(entry.id, isMultiLine, e)
                      }
                      className={`transition-all border hover:border-primary/40 cursor-pointer shadow-sm ${
                        isExpanded ? "ring-1 ring-primary/20" : ""
                      }`}
                    >
                      <CardHeader className="p-4 pb-2 flex flex-row items-center justify-between space-y-0">
                        <div className="flex items-center gap-2 flex-wrap">
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
                        </div>

                        <div className="flex items-center gap-1 shrink-0">
                          <span className="text-[11px] font-mono text-muted-foreground mr-1">
                            #{entry.id}
                          </span>
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
                        <div
                          onClick={(e) => {
                            if (isExpanded) {
                              e.stopPropagation();
                            }
                          }}
                          className={`text-sm font-dmsans font-normal tracking-normal leading-relaxed whitespace-pre-wrap break-all select-text cursor-text ${
                            !isExpanded && isMultiLine ? "line-clamp-1" : ""
                          }`}
                        >
                          {isExpanded ? content : lines[0]}
                        </div>

                        {isMultiLine && (
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
                                <span>+{lines.length - 1} more lines</span>
                              </>
                            )}
                          </div>
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
    </div>
  );
}
