// ----------
// Image Annotation Types & Constants
// Description: Data models, tool definitions, and presets for image annotation, shapes, cropping, and history.
// ----------

export type ToolType =
  | "select"
  | "pen"
  | "highlighter"
  | "eraser"
  | "rectangle"
  | "circle"
  | "line"
  | "arrow"
  | "crop";

export type ShapeType = "rectangle" | "circle" | "line" | "arrow";

export type FillStyle = "none" | "semi" | "solid";

export type CropAspectRatio = "free" | "1:1" | "16:9" | "4:3" | "3:2";

export interface Point {
  x: number;
  y: number;
}

export interface PenStroke {
  id: string;
  tool: "pen";
  points: Point[];
  color: string;
  width: number;
}

export interface HighlighterStroke {
  id: string;
  tool: "highlighter";
  points: Point[];
  color: string;
  width: number;
}

export interface ShapeAnnotation {
  id: string;
  tool: ShapeType;
  start: Point;
  end: Point;
  color: string;
  width: number;
  fill: FillStyle;
}

export type Annotation = PenStroke | HighlighterStroke | ShapeAnnotation;

export interface CropBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EditorHistoryStep {
  annotations: Annotation[];
  baseImage: string; // Data URL of the image (or cropped image)
  dimensions: { width: number; height: number };
}

export const PEN_COLORS = [
  { name: "Red", value: "#ef4444" },
  { name: "Orange", value: "#f97316" },
  { name: "Yellow", value: "#eab308" },
  { name: "Green", value: "#22c55e" },
  { name: "Cyan", value: "#06b6d4" },
  { name: "Blue", value: "#3b82f6" },
  { name: "Purple", value: "#a855f7" },
  { name: "White", value: "#ffffff" },
  { name: "Black", value: "#09090b" },
];

export const HIGHLIGHTER_COLORS = [
  { name: "Fluorescent Yellow", value: "#facc15" },
  { name: "Fluorescent Green", value: "#4ade80" },
  { name: "Fluorescent Pink", value: "#f472b6" },
  { name: "Fluorescent Cyan", value: "#38bdf8" },
  { name: "Fluorescent Orange", value: "#fb923c" },
];

export const STROKE_WIDTHS = [
  { label: "Thin", value: 2 },
  { label: "Medium", value: 4 },
  { label: "Thick", value: 8 },
  { label: "Heavy", value: 14 },
];

export const HIGHLIGHTER_WIDTHS = [
  { label: "16px", value: 16 },
  { label: "24px", value: 24 },
  { label: "36px", value: 36 },
];
