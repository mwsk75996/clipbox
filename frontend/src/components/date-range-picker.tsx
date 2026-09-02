// ----------
// Date Range Picker Component
// Description: Interactive popover combining a calendar and presets to filter clipboard entries between min and max dates.
// ----------

import * as React from "react";
import { format, subDays, startOfDay, endOfDay, startOfMonth } from "date-fns";
import { Calendar as CalendarIcon, X } from "lucide-react";
import type { DateRange } from "react-day-picker";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";

interface DateRangePickerProps {
  date: DateRange | undefined;
  setDate: (date: DateRange | undefined) => void;
  className?: string;
}

export function DateRangePicker({
  date,
  setDate,
  className,
}: DateRangePickerProps) {
  const [isOpen, setIsOpen] = React.useState(false);

  const handlePreset = (preset: "today" | "week" | "month" | "thisMonth") => {
    const now = new Date();
    if (preset === "today") {
      setDate({ from: startOfDay(now), to: endOfDay(now) });
    } else if (preset === "week") {
      setDate({ from: startOfDay(subDays(now, 6)), to: endOfDay(now) });
    } else if (preset === "month") {
      setDate({ from: startOfDay(subDays(now, 29)), to: endOfDay(now) });
    } else if (preset === "thisMonth") {
      setDate({ from: startOfMonth(now), to: endOfDay(now) });
    }
    setIsOpen(false);
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    setDate(undefined);
  };

  let label = "All dates";
  if (date?.from) {
    if (date.to) {
      label = `${format(date.from, "LLL dd, y")} – ${format(date.to, "LLL dd, y")}`;
    } else {
      label = `${format(date.from, "LLL dd, y")} – ...`;
    }
  }

  return (
    <div className={cn("grid gap-2", className)}>
      <Popover open={isOpen} onOpenChange={setIsOpen}>
        <PopoverTrigger asChild>
          <Button
            variant={date?.from ? "default" : "outline"}
            size="sm"
            className={cn(
              "justify-start text-left font-normal h-9 gap-2",
              !date?.from && "text-muted-foreground"
            )}
          >
            <CalendarIcon className="size-4 shrink-0" />
            <span className="truncate max-w-[200px] text-xs font-medium">
              {label}
            </span>
            {date?.from && (
              <span
                role="button"
                tabIndex={0}
                onClick={handleClear}
                className="ml-auto rounded-full hover:bg-primary-foreground/20 p-0.5"
                title="Clear date filter"
              >
                <X className="size-3" />
              </span>
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <div className="p-3 pb-2 flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold text-foreground">
                Filter by date range
              </span>
              {date?.from && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setDate(undefined)}
                  className="h-6 px-2 text-[11px] text-muted-foreground hover:text-foreground"
                >
                  Clear
                </Button>
              )}
            </div>

            <div className="flex flex-wrap gap-1">
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[11px] px-2"
                onClick={() => handlePreset("today")}
              >
                Today
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[11px] px-2"
                onClick={() => handlePreset("week")}
              >
                Past 7 days
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[11px] px-2"
                onClick={() => handlePreset("month")}
              >
                Past 30 days
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-[11px] px-2"
                onClick={() => handlePreset("thisMonth")}
              >
                This month
              </Button>
            </div>
          </div>

          <Separator />

          <Calendar
            mode="range"
            defaultMonth={date?.from || new Date()}
            selected={date}
            onSelect={setDate}
            numberOfMonths={1}
          />
        </PopoverContent>
      </Popover>
    </div>
  );
}
