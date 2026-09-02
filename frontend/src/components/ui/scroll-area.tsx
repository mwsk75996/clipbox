// ----------
// Scroll Area Component
// Description: Native scrollable area providing cross-platform mouse wheel, touchpad, and touch scrolling with invisible scrollbars.
// ----------

import * as React from "react";
import { cn } from "@/lib/utils";

const ScrollArea = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, children, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("overflow-y-auto no-scrollbar scroll-smooth", className)}
    {...props}
  >
    {children}
  </div>
));
ScrollArea.displayName = "ScrollArea";

export { ScrollArea };
