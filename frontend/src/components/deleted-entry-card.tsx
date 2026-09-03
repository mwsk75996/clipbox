// ----------
// Recently Deleted Entry Card
// Description: Compact read-only card for archived clips with restore and permanent-delete actions.
// ----------

import * as React from "react";
import { Files, History, Image as ImageIcon, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import {
  formatTimestamp,
  sourceLabel,
  stripLeadingEmptyLines,
  type DeletedClipboardEntry,
} from "../App";

interface DeletedEntryCardProps {
  entry: DeletedClipboardEntry;
  onRestore: (id: number) => void;
  onDeleteForever: (id: number, event?: React.MouseEvent) => void;
}

export function DeletedEntryCard({ entry, onRestore, onDeleteForever }: DeletedEntryCardProps) {
  const appName = sourceLabel(entry);
  const snippet =
    entry.entryType === "text" ? stripLeadingEmptyLines(entry.content).slice(0, 280) : null;

  return (
    <Card className="transition-all border shadow-sm opacity-90">
      <CardHeader className="p-4 pb-2 flex flex-row items-center justify-between space-y-0 gap-2">
        <div className="flex items-center gap-2 flex-wrap min-w-0">
          {entry.entryType === "image" ? (
            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 select-none">
              <ImageIcon className="size-2.5" />
              Image
              {entry.imageDimensions ? ` (${entry.imageDimensions})` : ""}
            </span>
          ) : entry.entryType === "file" ? (
            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20 select-none">
              <Files className="size-2.5" />
              Files
            </span>
          ) : null}
          <span className="text-xs font-medium text-foreground truncate">{appName}</span>
          <span className="text-xs text-muted-foreground shrink-0">·</span>
          <CardDescription className="text-xs whitespace-nowrap">
            Deleted {formatTimestamp(entry.deletedAt)}
          </CardDescription>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <Button
            variant="outline"
            size="sm"
            onClick={() => onRestore(entry.id)}
            title="Restore to history"
            className="h-7 gap-1.5 text-xs"
          >
            <History className="size-3.5" />
            <span>Restore</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={(e) => onDeleteForever(entry.id, e)}
            title="Delete forever"
            className="h-7 gap-1.5 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
          >
            <Trash2 className="size-3.5" />
            <span>Delete forever</span>
          </Button>
        </div>
      </CardHeader>
      {snippet && (
        <CardContent className="p-4 pt-1">
          <div className="text-sm font-normal tracking-normal leading-relaxed whitespace-pre-wrap break-words [overflow-wrap:anywhere] select-text line-clamp-3 text-muted-foreground">
            {snippet}
          </div>
        </CardContent>
      )}
    </Card>
  );
}
