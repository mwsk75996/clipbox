// ----------
// OCR Word Boxes
// Description: Shared helpers for parsed image text-match rectangles (fractions of the bitmap) and query matching.
// ----------

export interface OcrBox {
  t: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export const isOcrBox = (value: unknown): value is OcrBox => {
  if (typeof value !== "object" || value === null) return false;
  const box = value as Record<string, unknown>;
  return (
    typeof box.t === "string" &&
    typeof box.x === "number" &&
    typeof box.y === "number" &&
    typeof box.w === "number" &&
    typeof box.h === "number" &&
    [box.x, box.y, box.w, box.h].every((n) => Number.isFinite(n))
  );
};

export function parseOcrBoxes(json: string | null | undefined): OcrBox[] {
  if (!json) return [];
  try {
    const parsed: unknown = JSON.parse(json);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isOcrBox);
  } catch {
    return [];
  }
}

/// Boxes whose text contains any whitespace-separated query token,
/// mirroring substring feed search per word.
export function matchOcrBoxes(json: string | null | undefined, query: string): OcrBox[] {
  const tokens = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return [];
  return parseOcrBoxes(json).filter((box) =>
    tokens.some((token) => box.t.toLowerCase().includes(token))
  );
}
