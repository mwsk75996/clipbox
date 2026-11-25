// ----------
// Bounded Lazy Image Cache
// Description: On-demand LRU cache for full-resolution clipboard image data URLs
// loaded via get_entry_image, capping renderer memory usage while deduplicating
// concurrent fetches across feed thumbnails and the lightbox.
// ----------

import { invoke } from "@tauri-apps/api/core";

const MAX_CACHED_IMAGES = 30;
const MAX_CACHED_BYTES = 48 * 1024 * 1024;

interface CacheEntry {
  dataUrl: string;
  bytes: number;
}

const imageCache = new Map<string, CacheEntry>();
const inFlightRequests = new Map<string, Promise<string | null>>();
let cachedBytes = 0;

function cacheKey(id: number, copiedAt: number): string {
  return `${id}:${copiedAt}`;
}

function touch(key: string, entry: CacheEntry) {
  imageCache.delete(key);
  imageCache.set(key, entry);
}

function evictIfNeeded(incomingBytes: number) {
  while (
    imageCache.size > 0 &&
    (imageCache.size >= MAX_CACHED_IMAGES ||
      cachedBytes + incomingBytes > MAX_CACHED_BYTES)
  ) {
    const oldestKey = imageCache.keys().next().value;
    if (oldestKey === undefined) break;
    const removed = imageCache.get(oldestKey);
    if (removed) {
      cachedBytes -= removed.bytes;
    }
    imageCache.delete(oldestKey);
  }
}

export function peekEntryImage(id: number, copiedAt: number): string | null {
  const key = cacheKey(id, copiedAt);
  const hit = imageCache.get(key);
  if (!hit) return null;
  touch(key, hit);
  return hit.dataUrl;
}

export function loadEntryImage(
  id: number,
  copiedAt: number,
  inlineDataUrl?: string | null
): Promise<string | null> {
  if (inlineDataUrl) {
    return Promise.resolve(inlineDataUrl);
  }

  const cached = peekEntryImage(id, copiedAt);
  if (cached) {
    return Promise.resolve(cached);
  }

  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return Promise.resolve(null);
  }

  const key = cacheKey(id, copiedAt);
  const existing = inFlightRequests.get(key);
  if (existing) {
    return existing;
  }

  const request = invoke<string | null>("get_entry_image", { id })
    .then((dataUrl) => {
      if (dataUrl) {
        const bytes = dataUrl.length;
        // Skip caching single giant images larger than the budget so they
        // don't evict every other thumbnail; they still render cleanly.
        if (bytes <= MAX_CACHED_BYTES) {
          evictIfNeeded(bytes);
          imageCache.set(key, { dataUrl, bytes });
          cachedBytes += bytes;
        }
      }
      return dataUrl ?? null;
    })
    .catch((err) => {
      console.error(`Failed to load image for entry #${id}:`, err);
      return null;
    })
    .finally(() => {
      inFlightRequests.delete(key);
    });

  inFlightRequests.set(key, request);
  return request;
}

export function clearEntryImageCache() {
  imageCache.clear();
  inFlightRequests.clear();
  cachedBytes = 0;
}
