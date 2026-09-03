// ----------
// Recently Deleted Entry Card
// Description: Compact read-only card for archived clips with restore and permanent-delete actions.
// ----------

import * as React from "react";
import { Files, History, Image as ImageIcon, Loader2, Timer, Trash2 } from "lucide-react";
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
  retention: string | null;
  onRestore: (id: number) => void | Promise<void>;
  onDeleteForever: (id: number, event?: React.MouseEvent) => void;
}

// Mirrors deleted_retention_lifetime_seconds in clipbox-core.
const RETENTION_LIFETIME_SECONDS: Record<string, number> = {
  "1hour": 3_600,
  "1day": 86_400,
  "7days": 7 * 86_400,
  "30days": 30 * 86_400,
};

function formatExpiry(deletedAt: number, retention: string | null): string | null {
  if (!retention || retention === "immediately") return null;
  const lifetime = RETENTION_LIFETIME_SECONDS[retention];
  if (!lifetime) return null;

  const remaining = deletedAt + lifetime - Math.floor(Date.now() / 1000);
  if (remaining <= 0) return "Purging soon";
  const minutes = Math.floor(remaining / 60);
  if (minutes < 1) return "Less than a minute left";
  if (minutes < 60) return `${minutes} min left`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const mins = minutes % 60;
    return mins === 0 ? `${hours} hr left` : `${hours} hr ${mins} min left`;
  }
  const days = Math.floor(hours / 24);
  const hrs = hours % 24;
  if (hrs === 0) return `${days} day${days === 1 ? "" : "s"} left`;
  return `${days}d ${hrs}h left`;
}

export function DeletedEntryCard({ entry, retention, onRestore, onDeleteForever }: DeletedEntryCardProps) {
  const [restoring, setRestoring] = React.useState(false);
  const appName = sourceLabel(entry);
  const snippet =
    entry.entryType === "text" ? stripLeadingEmptyLines(entry.content).slice(0, 280) : null;
  const expiry = formatExpiry(entry.deletedAt, retention);

  const handleRestore = async () => {
    if (restoring) return;
    setRestoring(true);
    try {
      await onRestore(entry.id);
    } finally {
      setRestoring(false);
    }
  };

  return (
    <Card className="transition-all border shadow-sm opacity-90">
      <CardHeader
        className={`flex flex-row items-center justify-between space-y-0 gap-2 p-4 ${
          snippet ? "pb-2" : ""
        }`}
      >
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
          {expiry && (
            <>
              <span className="text-xs text-muted-foreground shrink-0">·</span>
              <span className="inline-flex items-center gap-1 text-xs font-medium text-amber-600 dark:text-amber-400 whitespace-nowrap">
                <Timer className="size-3" />
                {expiry}
              </span>
            </>
          )}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <Button
            variant="outline"
            size="sm"
            onClick={handleRestore}
            disabled={restoring}
            title="Restore to history"
            className="h-7 gap-1.5 text-xs"
          >
            {restoring ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <History className="size-3.5" />
            )}
            <span>{restoring ? "Restoring..." : "Restore"}</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={(e) => onDeleteForever(entry.id, e)}
            disabled={restoring}
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
