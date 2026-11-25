// ----------
// Feed Image Thumbnail
// Description: Card-sized image preview with viewport-gated lazy loading from
// SQLite, OCR search-match highlights, and click-to-lightbox behavior.
// ----------

import * as React from "react";
import { Image as ImageIcon, Loader2, ZoomIn } from "lucide-react";
import { matchOcrBoxes } from "@/lib/ocr";
import { loadEntryImage, peekEntryImage } from "@/lib/image-cache";

interface FeedImageThumbProps {
  entryId: number;
  copiedAt: number;
  imageData?: string | null;
  alt: string;
  boxesJson?: string | null;
  searchQuery: string;
  showHighlights: boolean;
  onPreview: () => void;
}

export function FeedImageThumb({
  entryId,
  copiedAt,
  imageData,
  alt,
  boxesJson,
  searchQuery,
  showHighlights,
  onPreview,
}: FeedImageThumbProps) {
  const containerRef = React.useRef<HTMLDivElement>(null);
  const [src, setSrc] = React.useState<string | null>(
    () => imageData ?? peekEntryImage(entryId, copiedAt)
  );
  const [loading, setLoading] = React.useState<boolean>(
    () => !(imageData ?? peekEntryImage(entryId, copiedAt))
  );
  const [failed, setFailed] = React.useState(false);

  // Sync immediately when inline data or cached entry changes.
  React.useEffect(() => {
    const immediate = imageData ?? peekEntryImage(entryId, copiedAt);
    if (immediate) {
      setSrc(immediate);
      setLoading(false);
      setFailed(false);
      return;
    }

    setSrc(null);
    setLoading(true);
    setFailed(false);

    let cancelled = false;
    let started = false;

    const triggerLoad = () => {
      if (started || cancelled) return;
      started = true;
      loadEntryImage(entryId, copiedAt, imageData).then((resolved) => {
        if (cancelled) return;
        if (resolved) {
          setSrc(resolved);
          setFailed(false);
        } else {
          setFailed(true);
        }
        setLoading(false);
      });
    };

    const el = containerRef.current;
    if (el && typeof IntersectionObserver !== "undefined") {
      const observer = new IntersectionObserver(
        (entries) => {
          if (entries.some((entry) => entry.isIntersecting)) {
            observer.disconnect();
            triggerLoad();
          }
        },
        { rootMargin: "200px 0px" }
      );
      observer.observe(el);
      return () => {
        cancelled = true;
        observer.disconnect();
      };
    }

    triggerLoad();
    return () => {
      cancelled = true;
    };
  }, [entryId, copiedAt, imageData]);

  const matchedBoxes = React.useMemo(
    () => (showHighlights ? matchOcrBoxes(boxesJson, searchQuery) : []),
    [boxesJson, searchQuery, showHighlights]
  );

  return (
    <div
      ref={containerRef}
      className="relative group overflow-hidden rounded-md border bg-muted/20 min-h-[120px] max-h-[300px] flex items-center justify-center p-1.5 cursor-zoom-in transition-all hover:border-primary/40 hover:bg-muted/30"
      onClick={(e) => {
        e.stopPropagation();
        onPreview();
      }}
      title="Click to view full-size image"
    >
      {src ? (
        <>
          <div className="relative">
            <img
              src={src}
              alt={alt}
              className="max-h-[280px] w-auto max-w-full object-contain rounded select-none transition-transform duration-200 group-hover:scale-[1.015]"
              loading="lazy"
              decoding="async"
            />
            {matchedBoxes.length > 0 && (
              <div className="absolute inset-0 pointer-events-none">
                {matchedBoxes.map((box, index) => (
                  <div
                    key={index}
                    className="absolute bg-yellow-400/30 border border-yellow-400/70 rounded-[1px]"
                    style={{
                      left: `${box.x * 100}%`,
                      top: `${box.y * 100}%`,
                      width: `${box.w * 100}%`,
                      height: `${box.h * 100}%`,
                    }}
                  />
                ))}
              </div>
            )}
          </div>
          <div className="absolute inset-0 bg-black/0 group-hover:bg-black/25 transition-colors flex items-center justify-center opacity-0 group-hover:opacity-100 pointer-events-none">
            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-black/75 text-white text-xs font-medium backdrop-blur-sm shadow-md">
              <ZoomIn className="size-3.5" />
              Preview
            </span>
          </div>
        </>
      ) : loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground py-8">
          <Loader2 className="size-4 animate-spin" />
          <span>Loading image...</span>
        </div>
      ) : failed ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground py-8">
          <ImageIcon className="size-4 opacity-60" />
          <span>Image unavailable</span>
        </div>
      ) : null}
    </div>
  );
}
