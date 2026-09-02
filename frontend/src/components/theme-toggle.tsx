// ----------
// Theme Toggle Component
// Description: Interactive control allowing the user to select between Light, Dark, and Automatic System appearance.
// ----------

import * as React from "react";
import { Moon, Sun, Monitor } from "lucide-react";

import { useTheme } from "@/components/theme-provider";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";

export function ThemeToggle() {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const [open, setOpen] = React.useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          className="size-9 shrink-0 relative"
          title={`Appearance: ${theme} (currently ${resolvedTheme})`}
        >
          {theme === "system" ? (
            <Monitor className="size-4 text-foreground" />
          ) : theme === "dark" ? (
            <Moon className="size-4 text-foreground" />
          ) : (
            <Sun className="size-4 text-foreground" />
          )}
          <span className="sr-only">Toggle theme</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-40 p-1.5">
        <div className="flex flex-col gap-1">
          <button
            type="button"
            onClick={() => {
              setTheme("light");
              setOpen(false);
            }}
            className={`flex items-center gap-2 w-full px-2.5 py-1.5 text-xs rounded-sm font-medium transition-colors ${
              theme === "light"
                ? "bg-accent text-accent-foreground font-semibold"
                : "hover:bg-muted text-muted-foreground hover:text-foreground"
            }`}
          >
            <Sun className="size-3.5" />
            <span>Light</span>
          </button>
          <button
            type="button"
            onClick={() => {
              setTheme("dark");
              setOpen(false);
            }}
            className={`flex items-center gap-2 w-full px-2.5 py-1.5 text-xs rounded-sm font-medium transition-colors ${
              theme === "dark"
                ? "bg-accent text-accent-foreground font-semibold"
                : "hover:bg-muted text-muted-foreground hover:text-foreground"
            }`}
          >
            <Moon className="size-3.5" />
            <span>Dark</span>
          </button>
          <button
            type="button"
            onClick={() => {
              setTheme("system");
              setOpen(false);
            }}
            className={`flex items-center gap-2 w-full px-2.5 py-1.5 text-xs rounded-sm font-medium transition-colors ${
              theme === "system"
                ? "bg-accent text-accent-foreground font-semibold"
                : "hover:bg-muted text-muted-foreground hover:text-foreground"
            }`}
          >
            <Monitor className="size-3.5" />
            <span>System</span>
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
