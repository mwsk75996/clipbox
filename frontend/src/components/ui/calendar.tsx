// ----------
// Calendar Component
// Description: Date selection calendar component built on react-day-picker with custom Tailwind styling for single and range modes.
// ----------

import * as React from "react";
import { DayPicker } from "react-day-picker";
import "react-day-picker/style.css";

import { cn } from "@/lib/utils";

export type CalendarProps = React.ComponentProps<typeof DayPicker>;

function Calendar({
  className,
  showOutsideDays = true,
  ...props
}: CalendarProps) {
  return (
    <div className={cn("p-2 bg-popover text-popover-foreground rounded-md", className)}>
      <DayPicker
        showOutsideDays={showOutsideDays}
        {...props}
      />
    </div>
  );
}
Calendar.displayName = "Calendar";

export { Calendar };
