// ----------
// Settings Modal Component
// Description: Popout dialog providing configuration for launch on startup, database storage location, history retention, hotkeys, privacy filters, and history purge.
// ----------

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Settings,
  Folder,
  HardDrive,
  Power,
  Shield,
  Keyboard,
  Trash2,
  Check,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface SettingsModalProps {
  onClearHistory?: () => void;
}

export function SettingsModal({ onClearHistory }: SettingsModalProps) {
  const [open, setOpen] = React.useState(false);
  const [launchOnStartup, setLaunchOnStartup] = React.useState(() => {
    const saved = localStorage.getItem("clipbox:launchOnStartup");
    return saved !== null ? saved === "true" : true;
  });
  const [startMinimized, setStartMinimized] = React.useState(() => {
    return localStorage.getItem("clipbox:startMinimized") === "true";
  });
  const [alwaysOnTop, setAlwaysOnTop] = React.useState(() => {
    return localStorage.getItem("clipbox:alwaysOnTop") === "true";
  });
  const [ignorePasswordManagers, setIgnorePasswordManagers] = React.useState(true);
  const [retentionLimit, setRetentionLimit] = React.useState("500");
  const [copiedPath, setCopiedPath] = React.useState(false);

  const [confirmClear, setConfirmClear] = React.useState(false);
  const [clearing, setClearing] = React.useState(false);
  const [clearedSuccess, setClearedSuccess] = React.useState(false);

  // Query actual window always-on-top, autostart, and start-minimized state when settings opens
  React.useEffect(() => {
    if (!open) return;
    const querySettings = async () => {
      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          const isTop = await invoke<boolean>("is_always_on_top");
          setAlwaysOnTop(isTop);
          localStorage.setItem("clipbox:alwaysOnTop", String(isTop));

          const isAutostart = await invoke<boolean>("is_autostart_enabled");
          setLaunchOnStartup(isAutostart);
          localStorage.setItem("clipbox:launchOnStartup", String(isAutostart));

          const isMin = await invoke<boolean>("is_start_minimized");
          setStartMinimized(isMin);
          localStorage.setItem("clipbox:startMinimized", String(isMin));
        }
      } catch (err) {
        console.warn("Could not query settings state", err);
      }
    };
    querySettings();
  }, [open]);

  const handleToggleLaunchOnStartup = async (checked: boolean) => {
    setLaunchOnStartup(checked);
    localStorage.setItem("clipbox:launchOnStartup", String(checked));
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_autostart", { enabled: checked });
      }
    } catch (err) {
      console.error("Failed to update launch on startup setting", err);
    }
  };

  const handleToggleStartMinimized = async (checked: boolean) => {
    setStartMinimized(checked);
    localStorage.setItem("clipbox:startMinimized", String(checked));
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_start_minimized", { enabled: checked });
      }
    } catch (err) {
      console.error("Failed to update start minimized setting", err);
    }
  };

  const handleToggleAlwaysOnTop = async (checked: boolean) => {
    setAlwaysOnTop(checked);
    localStorage.setItem("clipbox:alwaysOnTop", String(checked));
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_always_on_top", { alwaysOnTop: checked });
      }
    } catch (err) {
      console.error("Failed to update always on top setting", err);
    }
  };

  const dbPath = "%APPDATA%\\com.palethea.clipbox\\clipbox.sqlite3";

  const handleCopyPath = () => {
    navigator.clipboard.writeText(dbPath);
    setCopiedPath(true);
    setTimeout(() => setCopiedPath(false), 1500);
  };

  const handleClearHistory = async () => {
    setClearing(true);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("clear_entries");
      }
      onClearHistory?.();
      setClearedSuccess(true);
      setConfirmClear(false);
      setTimeout(() => setClearedSuccess(false), 2000);
    } catch (err) {
      console.error("Failed to clear history", err);
    } finally {
      setClearing(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          className="size-9 shrink-0"
          title="Settings"
        >
          <Settings className="size-4 text-foreground" />
          <span className="sr-only">Settings</span>
        </Button>
      </DialogTrigger>

      <DialogContent className="max-w-xl max-h-[85vh] overflow-y-auto no-scrollbar p-6">
        <DialogHeader className="pb-2">
          <DialogTitle className="text-xl flex items-center gap-2">
            <Settings className="size-5" />
            Settings
          </DialogTitle>
          <DialogDescription>
            Configure Clipbox startup behavior, storage, and clipboard capture preferences.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 pt-2 text-sm">
          {/* Section: Startup & Behavior */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <Power className="size-3.5" />
              Startup & Window Behavior
            </h4>

            <div className="space-y-3">
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label
                    htmlFor="startup-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Launch on system startup
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Automatically launch Clipbox in the background when your computer boots.
                  </p>
                </div>
                <Switch
                  id="startup-toggle"
                  checked={launchOnStartup}
                  onCheckedChange={handleToggleLaunchOnStartup}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label
                    htmlFor="minimized-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Start minimized to system tray
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Run quietly in the notification area without popping up the main window.
                  </p>
                </div>
                <Switch
                  id="minimized-toggle"
                  checked={startMinimized}
                  onCheckedChange={handleToggleStartMinimized}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label
                    htmlFor="ontop-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Keep window always on top
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Prevent Clipbox from being hidden beneath other active applications.
                  </p>
                </div>
                <Switch
                  id="ontop-toggle"
                  checked={alwaysOnTop}
                  onCheckedChange={handleToggleAlwaysOnTop}
                />
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Storage & Database */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <HardDrive className="size-3.5" />
              Database & Storage Location
            </h4>

            <div className="space-y-3">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground">
                  Local SQLite Database Path
                </label>
                <div className="flex items-center gap-2">
                  <Input
                    readOnly
                    value={dbPath}
                    className="font-mono text-xs bg-muted/50 h-8"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 px-3 gap-1.5 shrink-0 text-xs"
                    onClick={handleCopyPath}
                  >
                    {copiedPath ? (
                      <>
                        <Check className="size-3 text-green-500" />
                        Copied
                      </>
                    ) : (
                      <>
                        <Folder className="size-3" />
                        Copy Path
                      </>
                    )}
                  </Button>
                </div>
                <p className="text-[11px] text-muted-foreground">
                  Clipbox keeps 100% of your data offline on your local disk.
                </p>
              </div>

              <div className="flex items-center justify-between gap-4 pt-1">
                <div className="space-y-0.5">
                  <span className="text-sm font-medium">History Retention Limit</span>
                  <p className="text-xs text-muted-foreground">
                    Maximum number of recent copies stored before recycling oldest entries.
                  </p>
                </div>
                <Select value={retentionLimit} onValueChange={setRetentionLimit}>
                  <SelectTrigger className="w-36 h-8 text-xs">
                    <SelectValue placeholder="Retention" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="100">100 entries</SelectItem>
                    <SelectItem value="500">500 entries</SelectItem>
                    <SelectItem value="1000">1,000 entries</SelectItem>
                    <SelectItem value="5000">5,000 entries</SelectItem>
                    <SelectItem value="unlimited">Unlimited</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Privacy & Security */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <Shield className="size-3.5" />
              Privacy & Security
            </h4>

            <div className="space-y-3">
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label
                    htmlFor="pass-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Ignore password managers
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Automatically skip recording copies originating from 1Password, Bitwarden, KeePass, or LastPass.
                  </p>
                </div>
                <Switch
                  id="pass-toggle"
                  checked={ignorePasswordManagers}
                  onCheckedChange={setIgnorePasswordManagers}
                />
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Keyboard Shortcuts */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <Keyboard className="size-3.5" />
              Keyboard Shortcuts
            </h4>

            <div className="space-y-2 text-xs">
              <div className="flex items-center justify-between py-1">
                <span className="text-muted-foreground">Focus Search Bar</span>
                <kbd className="rounded border bg-muted px-2 py-0.5 font-mono text-[10px]">
                  Ctrl + F
                </kbd>
              </div>
              <div className="flex items-center justify-between py-1">
                <span className="text-muted-foreground">Alternate Search Hotkey</span>
                <kbd className="rounded border bg-muted px-2 py-0.5 font-mono text-[10px]">
                  Ctrl + K
                </kbd>
              </div>
            </div>
          </div>

          <Separator />

          {/* Data Cleanup */}
          <div className="pt-1 flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <span className="text-sm font-medium text-destructive">
                Clear Clipboard Data
              </span>
              <p className="text-xs text-muted-foreground">
                Permanently purge all stored history records from the local database.
              </p>
            </div>

            {clearedSuccess ? (
              <div className="flex items-center gap-1.5 text-xs text-green-600 dark:text-green-400 font-medium px-3 py-1.5 rounded bg-green-500/10 border border-green-500/20">
                <Check className="size-3.5" />
                History Cleared!
              </div>
            ) : confirmClear ? (
              <div className="flex items-center gap-1.5">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setConfirmClear(false)}
                  disabled={clearing}
                  className="text-xs h-8 px-2"
                >
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={handleClearHistory}
                  disabled={clearing}
                  className="text-xs h-8 gap-1.5 px-3"
                >
                  {clearing ? (
                    "Clearing..."
                  ) : (
                    <>
                      <Trash2 className="size-3.5" />
                      Confirm Clear
                    </>
                  )}
                </Button>
              </div>
            ) : (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setConfirmClear(true)}
                className="text-destructive border-destructive/30 hover:bg-destructive/10 text-xs h-8 gap-1.5"
              >
                <Trash2 className="size-3.5" />
                Purge History
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
