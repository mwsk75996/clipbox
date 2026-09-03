// ----------
// Settings Modal Component
// Description: Popout dialog providing configuration for launch on startup, database storage location, history retention, hotkeys, privacy filters, excluded apps, and history purge.
// ----------

import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Settings,
  Folder,
  FolderOpen,
  HardDrive,
  Power,
  Shield,
  Keyboard,
  Trash2,
  Check,
  Plus,
  X,
  RotateCcw,
  AlertCircle,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
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

// ----------
// Keybinding Types & Helpers
// Description: Data models and helper functions for recording, formatting, and matching customizable user keyboard shortcuts.
// ----------

export interface KeyBinding {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
  label: string;
}

export interface ShortcutSettings {
  focus_search: KeyBinding;
  nav_down: KeyBinding;
  nav_up: KeyBinding;
  copy_entry: KeyBinding;
  expand_preview: KeyBinding;
  toggle_pin: KeyBinding;
  delete_entry: KeyBinding;
  clear_escape: KeyBinding;
}

export interface PrivacySettings {
  monitoring_paused: boolean;
  ignore_password_managers: boolean;
  excluded_applications: string[];
  duplicate_handling: string;
}

export const DEFAULT_SHORTCUTS: ShortcutSettings = {
  focus_search: { key: "f", ctrl: true, shift: false, alt: false, meta: false, label: "Ctrl + F" },
  nav_down: { key: "ArrowDown", ctrl: false, shift: false, alt: false, meta: false, label: "↓ (Arrow Down)" },
  nav_up: { key: "ArrowUp", ctrl: false, shift: false, alt: false, meta: false, label: "↑ (Arrow Up)" },
  copy_entry: { key: "Enter", ctrl: false, shift: false, alt: false, meta: false, label: "Enter" },
  expand_preview: { key: " ", ctrl: false, shift: false, alt: false, meta: false, label: "Space" },
  toggle_pin: { key: "p", ctrl: false, shift: false, alt: false, meta: false, label: "P" },
  delete_entry: { key: "Delete", ctrl: false, shift: false, alt: false, meta: false, label: "Delete" },
  clear_escape: { key: "Escape", ctrl: false, shift: false, alt: false, meta: false, label: "Escape" },
};

export function formatKeyBinding(e: KeyboardEvent): KeyBinding | null {
  const isModifierOnly = ["Control", "Shift", "Alt", "Meta"].includes(e.key);
  if (isModifierOnly) return null;

  let key = e.key;
  let labelParts: string[] = [];

  if (e.ctrlKey) labelParts.push("Ctrl");
  if (e.altKey) labelParts.push("Alt");
  if (e.shiftKey) labelParts.push("Shift");
  if (e.metaKey) labelParts.push("Win");

  let keyDisplay = key;
  if (key === " ") keyDisplay = "Space";
  else if (key === "ArrowDown") keyDisplay = "↓ (Down)";
  else if (key === "ArrowUp") keyDisplay = "↑ (Up)";
  else if (key === "ArrowLeft") keyDisplay = "← (Left)";
  else if (key === "ArrowRight") keyDisplay = "→ (Right)";
  else if (key.length === 1) keyDisplay = key.toUpperCase();

  labelParts.push(keyDisplay);

  return {
    key,
    ctrl: e.ctrlKey,
    shift: e.shiftKey,
    alt: e.altKey,
    meta: e.metaKey,
    label: labelParts.join(" + "),
  };
}

export function matchesBinding(e: KeyboardEvent, binding?: KeyBinding | null): boolean {
  if (!binding) return false;
  const ctrlMatch = Boolean(binding.ctrl) === Boolean(e.ctrlKey || e.metaKey);
  const shiftMatch = Boolean(binding.shift) === Boolean(e.shiftKey);
  const altMatch = Boolean(binding.alt) === Boolean(e.altKey);
  const keyMatch =
    binding.key.toLowerCase() === e.key.toLowerCase() ||
    (binding.key === " " && e.key === " ") ||
    (binding.key.toLowerCase() === "space" && e.key === " ");
  return ctrlMatch && shiftMatch && altMatch && keyMatch;
}

const SHORTCUT_ACTIONS: { id: keyof ShortcutSettings; label: string; description: string }[] = [
  { id: "focus_search", label: "Focus Search Bar", description: "Jumps focus to the filter/search input" },
  { id: "nav_down", label: "Next Item", description: "Navigate down in the clipboard list" },
  { id: "nav_up", label: "Previous Item", description: "Navigate up in the clipboard list" },
  { id: "copy_entry", label: "Copy Focused Item", description: "Copies the focused entry to clipboard" },
  { id: "expand_preview", label: "Expand / Collapse", description: "Toggles full preview for multiline cards" },
  { id: "toggle_pin", label: "Toggle Pin", description: "Pins or unpins the currently focused entry" },
  { id: "delete_entry", label: "Delete Item", description: "Deletes the currently focused entry" },
  { id: "clear_escape", label: "Clear / Dismiss", description: "Clears active search or unselects card" },
];

interface SettingsModalProps {
  onClearHistory?: () => void;
  shortcuts?: ShortcutSettings;
  onShortcutsChange?: (shortcuts: ShortcutSettings) => void;
  isMonitoringPaused?: boolean;
  onMonitoringPausedChange?: (paused: boolean) => void;
}

export function SettingsModal({
  onClearHistory,
  shortcuts: externalShortcuts,
  onShortcutsChange,
  isMonitoringPaused: externalIsMonitoringPaused,
  onMonitoringPausedChange,
}: SettingsModalProps) {
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

  // Privacy states
  const [monitoringPaused, setMonitoringPaused] = React.useState(externalIsMonitoringPaused ?? false);
  const [ignorePasswordManagers, setIgnorePasswordManagers] = React.useState(true);
  const [excludedApps, setExcludedApps] = React.useState<string[]>([]);
  const [newAppInput, setNewAppInput] = React.useState("");
  const [duplicateHandling, setDuplicateHandling] = React.useState("bump");

  // Shortcuts states
  const [shortcuts, setShortcuts] = React.useState<ShortcutSettings>(externalShortcuts ?? DEFAULT_SHORTCUTS);
  const [recordingAction, setRecordingAction] = React.useState<keyof ShortcutSettings | null>(null);
  const [shortcutConflict, setShortcutConflict] = React.useState<string | null>(null);

  const [retentionLimit, setRetentionLimit] = React.useState(() => {
    return localStorage.getItem("clipbox:retentionLimit") || "500";
  });
  const [dbPath, setDbPath] = React.useState("%APPDATA%\\com.palethea.clipbox\\clipbox.sqlite3");
  const [copiedPath, setCopiedPath] = React.useState(false);
  const [prunedNotice, setPrunedNotice] = React.useState<number | null>(null);
  const [deletedRetention, setDeletedRetention] = React.useState(() => {
    return localStorage.getItem("clipbox:deletedRetention") || "7days";
  });
  const [purgedNotice, setPurgedNotice] = React.useState<number | null>(null);
  const [closeBehavior, setCloseBehavior] = React.useState(() => {
    return localStorage.getItem("clipbox:closeBehavior") || "ask";
  });

  const [confirmClear, setConfirmClear] = React.useState(false);
  const [clearing, setClearing] = React.useState(false);
  const [clearedSuccess, setClearedSuccess] = React.useState(false);

  // Sync external state changes
  React.useEffect(() => {
    if (externalIsMonitoringPaused !== undefined) {
      setMonitoringPaused(externalIsMonitoringPaused);
    }
  }, [externalIsMonitoringPaused]);

  React.useEffect(() => {
    if (externalShortcuts) {
      setShortcuts(externalShortcuts);
    }
  }, [externalShortcuts]);

  // Query actual settings state from backend when modal opens
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

          const realPath = await invoke<string>("get_database_path");
          setDbPath(realPath);

          const realRetention = await invoke<string>("get_retention_limit");
          setRetentionLimit(realRetention);
          localStorage.setItem("clipbox:retentionLimit", realRetention);

          const realDeletedRetention = await invoke<string>("get_deleted_retention");
          setDeletedRetention(realDeletedRetention);
          localStorage.setItem("clipbox:deletedRetention", realDeletedRetention);

          const realCloseBehavior = await invoke<string>("get_close_behavior");
          setCloseBehavior(realCloseBehavior);
          localStorage.setItem("clipbox:closeBehavior", realCloseBehavior);

          const privacy = await invoke<PrivacySettings>("get_privacy_settings");
          setMonitoringPaused(privacy.monitoring_paused);
          setIgnorePasswordManagers(privacy.ignore_password_managers);
          setExcludedApps(privacy.excluded_applications || []);
          setDuplicateHandling(privacy.duplicate_handling || "bump");

          const shortcutData = await invoke<ShortcutSettings>("get_shortcut_settings");
          setShortcuts(shortcutData);
          onShortcutsChange?.(shortcutData);
        }
      } catch (err) {
        console.warn("Could not query settings state", err);
      }
    };
    querySettings();
  }, [open, onShortcutsChange]);

  // Keyboard shortcut recording listener
  React.useEffect(() => {
    if (!recordingAction) return;

    const handleKeyDown = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setRecordingAction(null);
        setShortcutConflict(null);
        return;
      }

      const newBinding = formatKeyBinding(e);
      if (!newBinding) return;

      // Check conflict
      const conflictingAction = SHORTCUT_ACTIONS.find(
        (a) => a.id !== recordingAction && shortcuts[a.id]?.label === newBinding.label
      );

      if (conflictingAction) {
        setShortcutConflict(`"${newBinding.label}" is already used by ${conflictingAction.label}`);
        return;
      }

      setShortcutConflict(null);
      const updated = {
        ...shortcuts,
        [recordingAction]: newBinding,
      };
      setShortcuts(updated);
      setRecordingAction(null);

      try {
        if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
          await invoke("set_shortcut_settings", { settings: updated });
        }
        onShortcutsChange?.(updated);
      } catch (err) {
        console.error("Failed to save shortcut settings", err);
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [recordingAction, shortcuts, onShortcutsChange]);

  const handleResetShortcuts = async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const defaults = await invoke<ShortcutSettings>("reset_shortcut_settings");
        setShortcuts(defaults);
        onShortcutsChange?.(defaults);
      }
    } catch (err) {
      console.error("Failed to reset shortcuts", err);
    }
  };

  const savePrivacySettings = async (partial: Partial<PrivacySettings>) => {
    const next: PrivacySettings = {
      monitoring_paused: monitoringPaused,
      ignore_password_managers: ignorePasswordManagers,
      excluded_applications: excludedApps,
      duplicate_handling: duplicateHandling,
      ...partial,
    };
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_privacy_settings", { settings: next });
      }
    } catch (err) {
      console.error("Failed to save privacy settings", err);
    }
  };

  const handleToggleMonitoringPaused = async (paused: boolean) => {
    setMonitoringPaused(paused);
    onMonitoringPausedChange?.(paused);
    savePrivacySettings({ monitoring_paused: paused });
  };

  const handleToggleIgnorePasswords = (checked: boolean) => {
    setIgnorePasswordManagers(checked);
    savePrivacySettings({ ignore_password_managers: checked });
  };

  const handleDuplicateHandlingChange = (value: string) => {
    setDuplicateHandling(value);
    savePrivacySettings({ duplicate_handling: value });
  };

  const handleAddExcludedApp = () => {
    const trimmed = newAppInput.trim();
    if (!trimmed) return;
    const formatted = trimmed.toLowerCase().endsWith(".exe") ? trimmed : `${trimmed}.exe`;
    if (!excludedApps.some((app) => app.toLowerCase() === formatted.toLowerCase())) {
      const updated = [...excludedApps, formatted];
      setExcludedApps(updated);
      savePrivacySettings({ excluded_applications: updated });
    }
    setNewAppInput("");
  };

  const handleRemoveExcludedApp = (appToRemove: string) => {
    const updated = excludedApps.filter((app) => app.toLowerCase() !== appToRemove.toLowerCase());
    setExcludedApps(updated);
    savePrivacySettings({ excluded_applications: updated });
  };

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

  const handleRetentionLimitChange = async (value: string) => {
    setRetentionLimit(value);
    localStorage.setItem("clipbox:retentionLimit", value);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const pruned = await invoke<number>("set_retention_limit", { limit: value });
        if (pruned > 0) {
          setPrunedNotice(pruned);
          setTimeout(() => setPrunedNotice(null), 3500);
          onClearHistory?.();
        }
      }
    } catch (err) {
      console.error("Failed to update retention limit", err);
    }
  };

  const handleDeletedRetentionChange = async (value: string) => {
    setDeletedRetention(value);
    localStorage.setItem("clipbox:deletedRetention", value);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const purged = await invoke<number>("set_deleted_retention", { retention: value });
        if (purged > 0) {
          setPurgedNotice(purged);
          setTimeout(() => setPurgedNotice(null), 3500);
          onClearHistory?.();
        }
      }
    } catch (err) {
      console.error("Failed to update deleted retention", err);
    }
  };

  const handleCloseBehaviorChange = async (value: string) => {
    setCloseBehavior(value);
    localStorage.setItem("clipbox:closeBehavior", value);
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("set_close_behavior", { behavior: value });
      }
    } catch (err) {
      console.error("Failed to update close behavior", err);
    }
  };

  const handleCopyPath = () => {
    navigator.clipboard.writeText(dbPath);
    setCopiedPath(true);
    setTimeout(() => setCopiedPath(false), 1500);
  };

  const handleOpenDatabaseDirectory = async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("open_database_directory");
      }
    } catch (err) {
      console.error("Failed to open database directory", err);
    }
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
          <DialogTitle className="flex items-center gap-2 text-base font-semibold">
            <Settings className="size-4 text-primary" />
            Clipbox Preferences
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            Configure system behavior, storage limits, custom shortcuts, and privacy exclusions.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 pt-2">
          {/* Section: General */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <Power className="size-3.5" />
              General
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
                    Automatically launch Clipbox when you log into Windows.
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
                    Start minimized in tray
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Launch silently into the system tray without opening the main window.
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
                    htmlFor="top-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Always on top
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Keep the Clipbox window floating above all other application windows.
                  </p>
                </div>
                <Switch
                  id="top-toggle"
                  checked={alwaysOnTop}
                  onCheckedChange={handleToggleAlwaysOnTop}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label className="text-sm font-medium">Close button action</label>
                  <p className="text-xs text-muted-foreground">
                    What happens when you press the titlebar close button.
                  </p>
                </div>
                <Select
                  value={closeBehavior}
                  onValueChange={handleCloseBehaviorChange}
                >
                  <SelectTrigger className="w-[150px] h-8 text-xs shrink-0">
                    <SelectValue placeholder="Select action" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="ask">Ask every time</SelectItem>
                    <SelectItem value="hide">Hide to tray</SelectItem>
                    <SelectItem value="quit">Quit app</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Privacy & Reliability */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <Shield className="size-3.5" />
              Privacy & Reliability
            </h4>

            <div className="space-y-3.5">
              {/* Pause Monitoring */}
              <div className="flex items-center justify-between gap-4 p-3 rounded-lg border bg-muted/20">
                <div className="space-y-0.5">
                  <div className="flex items-center gap-2">
                    <label
                      htmlFor="pause-toggle"
                      className="text-sm font-medium cursor-pointer"
                    >
                      Pause clipboard monitoring
                    </label>
                    {monitoringPaused && (
                      <Badge variant="destructive" className="text-[10px] px-1.5 py-0 h-4">
                        Paused
                      </Badge>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Temporarily suspend capturing new clipboard text, images, and files.
                  </p>
                </div>
                <Switch
                  id="pause-toggle"
                  checked={monitoringPaused}
                  onCheckedChange={handleToggleMonitoringPaused}
                />
              </div>

              {/* Password Manager Filter */}
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label
                    htmlFor="pass-toggle"
                    className="text-sm font-medium cursor-pointer"
                  >
                    Ignore password managers & sensitive tags
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Automatically skip recording copies from 1Password, Bitwarden, KeePassXC, or clipboard items tagged with Windows privacy headers.
                  </p>
                </div>
                <Switch
                  id="pass-toggle"
                  checked={ignorePasswordManagers}
                  onCheckedChange={handleToggleIgnorePasswords}
                />
              </div>

              {/* Duplicate Handling */}
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <label className="text-sm font-medium">
                    Repeated copies handling
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Determine what happens when identical text, images, or files are copied again.
                  </p>
                </div>
                <Select
                  value={duplicateHandling}
                  onValueChange={handleDuplicateHandlingChange}
                >
                  <SelectTrigger className="w-[190px] h-8 text-xs shrink-0">
                    <SelectValue placeholder="Select behavior" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="bump">Move to top (Default)</SelectItem>
                    <SelectItem value="ignore">Ignore duplicate</SelectItem>
                    <SelectItem value="create_new">Record duplicate</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Excluded Applications Manager */}
              <div className="space-y-2 pt-1">
                <div className="space-y-0.5">
                  <label className="text-sm font-medium">
                    Excluded Applications
                  </label>
                  <p className="text-xs text-muted-foreground">
                    Clipboard copies originating from these processes will never be saved.
                  </p>
                </div>

                <div className="flex gap-2">
                  <Input
                    placeholder="e.g. notepad.exe or slack.exe"
                    value={newAppInput}
                    onChange={(e) => setNewAppInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        handleAddExcludedApp();
                      }
                    }}
                    className="h-8 text-xs font-mono"
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={handleAddExcludedApp}
                    className="h-8 text-xs gap-1 shrink-0"
                  >
                    <Plus className="size-3.5" />
                    Add App
                  </Button>
                </div>

                {excludedApps.length > 0 ? (
                  <div className="flex flex-wrap gap-1.5 pt-1">
                    {excludedApps.map((app) => (
                      <Badge
                        key={app}
                        variant="secondary"
                        className="text-xs font-mono pl-2 pr-1 py-0.5 flex items-center gap-1 border"
                      >
                        {app}
                        <button
                          type="button"
                          onClick={() => handleRemoveExcludedApp(app)}
                          className="rounded-full hover:bg-muted p-0.5 text-muted-foreground hover:text-foreground"
                          title={`Remove ${app}`}
                        >
                          <X className="size-3" />
                        </button>
                      </Badge>
                    ))}
                  </div>
                ) : (
                  <p className="text-[11px] text-muted-foreground italic">
                    No applications excluded. Copies from all apps will be recorded.
                  </p>
                )}
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Keyboard Shortcuts */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
                <Keyboard className="size-3.5" />
                Keyboard Shortcuts
              </h4>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={handleResetShortcuts}
                className="h-7 text-xs text-muted-foreground hover:text-foreground gap-1 px-2"
                title="Reset all shortcuts to defaults"
              >
                <RotateCcw className="size-3" />
                Reset Defaults
              </Button>
            </div>

            {shortcutConflict && (
              <div className="flex items-center gap-2 text-xs text-amber-600 dark:text-amber-400 bg-amber-500/10 border border-amber-500/20 p-2.5 rounded-md">
                <AlertCircle className="size-3.5 shrink-0" />
                <span>{shortcutConflict}</span>
              </div>
            )}

            <div className="space-y-2 text-xs">
              {SHORTCUT_ACTIONS.map((action) => {
                const binding = shortcuts[action.id];
                const isRecording = recordingAction === action.id;

                return (
                  <div
                    key={action.id}
                    className="flex items-center justify-between py-1.5 px-2 rounded-md hover:bg-muted/40 transition-colors"
                  >
                    <div className="space-y-0.5">
                      <span className="font-medium text-foreground">{action.label}</span>
                      <p className="text-[11px] text-muted-foreground">{action.description}</p>
                    </div>

                    <button
                      type="button"
                      onClick={() => {
                        setShortcutConflict(null);
                        setRecordingAction(isRecording ? null : action.id);
                      }}
                      className={`min-w-[85px] text-center rounded border px-2.5 py-1 font-mono text-[11px] transition-all cursor-pointer ${
                        isRecording
                          ? "border-primary bg-primary/10 text-primary animate-pulse ring-2 ring-primary/30"
                          : "bg-muted hover:border-primary/50 text-foreground"
                      }`}
                      title="Click to rebind shortcut"
                    >
                      {isRecording ? "Press key..." : binding?.label || "Unset"}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>

          <Separator />

          {/* Section: Storage & Retention */}
          <div className="space-y-4">
            <h4 className="text-xs font-semibold text-muted-foreground tracking-wider uppercase flex items-center gap-2">
              <HardDrive className="size-3.5" />
              Storage & Database
            </h4>

            {/* Retention Limit */}
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-0.5">
                <label className="text-sm font-medium">History Retention Limit</label>
                <p className="text-xs text-muted-foreground">
                  Maximum unpinned entries to retain in local history before pruning.
                </p>
              </div>
              <Select
                value={retentionLimit}
                onValueChange={handleRetentionLimitChange}
              >
                <SelectTrigger className="w-[130px] h-8 text-xs shrink-0">
                  <SelectValue placeholder="Select limit" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="100">100 items</SelectItem>
                  <SelectItem value="250">250 items</SelectItem>
                  <SelectItem value="500">500 items</SelectItem>
                  <SelectItem value="1000">1,000 items</SelectItem>
                  <SelectItem value="5000">5,000 items</SelectItem>
                  <SelectItem value="unlimited">Unlimited</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {prunedNotice !== null && (
              <div className="text-xs text-primary bg-primary/10 border border-primary/20 rounded p-2 flex items-center gap-1.5">
                <Check className="size-3.5" />
                Pruned {prunedNotice} surplus unpinned record{prunedNotice > 1 ? "s" : ""} to match the new limit.
              </div>
            )}

            {/* Deleted Retention */}
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-0.5">
                <label className="text-sm font-medium">Recently Deleted Retention</label>
                <p className="text-xs text-muted-foreground">
                  How long deleted clips stay restorable before permanent purge.
                </p>
              </div>
              <Select
                value={deletedRetention}
                onValueChange={handleDeletedRetentionChange}
              >
                <SelectTrigger className="w-[130px] h-8 text-xs shrink-0">
                  <SelectValue placeholder="Select timespan" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="immediately">Immediately</SelectItem>
                  <SelectItem value="1hour">1 Hour</SelectItem>
                  <SelectItem value="1day">1 Day</SelectItem>
                  <SelectItem value="7days">7 Days</SelectItem>
                  <SelectItem value="30days">30 Days</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {purgedNotice !== null && (
              <div className="text-xs text-primary bg-primary/10 border border-primary/20 rounded p-2 flex items-center gap-1.5">
                <Check className="size-3.5" />
                Purged {purgedNotice} expired archived record{purgedNotice > 1 ? "s" : ""} to match the new timespan.
              </div>
            )}

            {/* Database Path */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Database Location</label>
              <div className="flex gap-2">
                <Input
                  readOnly
                  value={dbPath}
                  className="h-8 text-xs font-mono bg-muted/50 select-all"
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleCopyPath}
                  className="h-8 text-xs gap-1.5 shrink-0"
                >
                  {copiedPath ? (
                    <>
                      <Check className="size-3.5 text-green-500" />
                      Copied
                    </>
                  ) : (
                    <>
                      <Folder className="size-3.5" />
                      Copy Path
                    </>
                  )}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleOpenDatabaseDirectory}
                  className="h-8 text-xs gap-1.5 shrink-0"
                  title="Reveal database directory in Windows File Explorer"
                >
                  <FolderOpen className="size-3.5" />
                  Open Directory
                </Button>
              </div>
            </div>
          </div>

          <Separator />

          {/* Section: Data Cleanup */}
          <div className="pt-1 flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <span className="text-sm font-medium text-destructive">
                Clear Clipboard Data
              </span>
              <p className="text-xs text-muted-foreground">
                Move all history records to Recently Deleted, where they can be restored before permanent purge.
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
