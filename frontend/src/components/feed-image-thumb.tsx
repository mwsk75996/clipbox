// ----------
// Feed Image Thumbnail
// Description: Card-sized image preview with OCR search-match highlights and click-to-lightbox behavior.
// ----------

import * as React from "react";
import { ZoomIn } from "lucide-react";
import { matchOcrBoxes } from "@/lib/ocr";

interface FeedImageThumbProps {
  imageData: string;
  alt: string;
  boxesJson?: string | null;
  searchQuery: string;
  onPreview: () => void;
}

export function FeedImageThumb({
  imageData,
  alt,
  boxesJson,
  searchQuery,
  onPreview,
}: FeedImageThumbProps) {
  const matchedBoxes = React.useMemo(
    () => matchOcrBoxes(boxesJson, searchQuery),
    [boxesJson, searchQuery]
  );

  return (
    <div
      className="relative group overflow-hidden rounded-md border bg-muted/20 max-h-[300px] flex items-center justify-center p-1.5 cursor-zoom-in transition-all hover:border-primary/40 hover:bg-muted/30"
      onClick={(e) => {
        e.stopPropagation();
        onPreview();
      }}
      title="Click to view full-size image"
    >
      <div className="relative">
        <img
          src={imageData}
          alt={alt}
          className="max-h-[280px] w-auto max-w-full object-contain rounded select-none transition-transform duration-200 group-hover:scale-[1.015]"
          loading="lazy"
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
    </div>
  );
}
