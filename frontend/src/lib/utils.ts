// ----------
// Class Name Utility
// Description: Merges Tailwind CSS classes with clsx and twMerge to resolve class conflicts cleanly.
// ----------

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
