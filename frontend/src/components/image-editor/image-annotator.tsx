// ----------
// Image Annotator & Cropping Suite Component
// Description: Full-featured image annotation and cropping workspace supporting ballpoint pen, semi-transparent highlighter, shapes (rectangle, circle, line, arrow), eraser, 8-point interactive cropping, undo/redo history, and flattened image export (copy, save as, save as new clip).
// ----------

import * as React from "react";
import {
  Pen,
  Highlighter,
  Eraser,
  Square,
  Circle,
  Minus,
  MoveRight,
  Crop,
  Undo2,
  Redo2,
  RotateCcw,
  Check,
  Copy,
  Download,
  BookmarkPlus,
  X,
  Palette,
  CheckCheck,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  ToolType,
  FillStyle,
  CropAspectRatio,
  Point,
  Annotation,
  PenStroke,
  HighlighterStroke,
  ShapeAnnotation,
  CropBox,
  EditorHistoryStep,
  PEN_COLORS,
  HIGHLIGHTER_COLORS,
  STROKE_WIDTHS,
  HIGHLIGHTER_WIDTHS,
} from "./types";

interface ImageAnnotatorProps {
  initialDataUrl: string;
  sourceEntryId?: number;
  initialDimensions?: string | null;
  onClose: () => void;
  onSavedNewClip?: () => void;
}

export function ImageAnnotator({
  initialDataUrl,
  sourceEntryId,
  initialDimensions,
  onClose,
  onSavedNewClip,
}: ImageAnnotatorProps) {
  // Base image state (can be changed by cropping)
  const [baseImage, setBaseImage] = React.useState<string>(initialDataUrl);
  const [dimensions, setDimensions] = React.useState<{ width: number; height: number }>(() => {
    if (initialDimensions) {
      const parts = initialDimensions.split("x");
      if (parts.length === 2) {
        const w = parseInt(parts[0], 10);
        const h = parseInt(parts[1], 10);
        if (!isNaN(w) && !isNaN(h) && w > 0 && h > 0) {
          return { width: w, height: h };
        }
      }
    }
    return { width: 800, height: 600 };
  });

  // Active Tool & Style settings
  const [activeTool, setActiveTool] = React.useState<ToolType>("pen");
  const [penColor, setPenColor] = React.useState<string>("#ef4444");
  const [highlighterColor, setHighlighterColor] = React.useState<string>("#facc15");
  const [strokeWidth, setStrokeWidth] = React.useState<number>(4);
  const [highlighterWidth, setHighlighterWidth] = React.useState<number>(24);
  const [fillStyle, setFillStyle] = React.useState<FillStyle>("none");
  const [cropRatio, setCropRatio] = React.useState<CropAspectRatio>("free");
  const [isShapeMenuOpen, setIsShapeMenuOpen] = React.useState<boolean>(false);

  // Annotations & History
  const [annotations, setAnnotations] = React.useState<Annotation[]>([]);
  const [undoStack, setUndoStack] = React.useState<EditorHistoryStep[]>([]);
  const [redoStack, setRedoStack] = React.useState<EditorHistoryStep[]>([]);

  // Crop Box state
  const [cropBox, setCropBox] = React.useState<CropBox | null>(null);
  const [activeDragHandle, setActiveDragHandle] = React.useState<string | null>(null);
  const [hoverCropHandle, setHoverCropHandle] = React.useState<string | null>(null);
  const dragStartPointRef = React.useRef<Point | null>(null);
  const cropBoxStartRef = React.useRef<CropBox | null>(null);

  // Drawing state
  const [isDrawing, setIsDrawing] = React.useState(false);
  const currentStrokePointsRef = React.useRef<Point[]>([]);
  const shapeStartRef = React.useRef<Point | null>(null);
  const shapeCurrentRef = React.useRef<Point | null>(null);
  const lastRawPointRef = React.useRef<Point | null>(null);
  const isShiftPressedRef = React.useRef<boolean>(false);

  // Export action states
  const [copied, setCopied] = React.useState(false);
  const [savedClip, setSavedClip] = React.useState(false);
  const [savingFile, setSavingFile] = React.useState(false);
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);

  // Canvas Refs
  const containerRef = React.useRef<HTMLDivElement>(null);
  const canvasRef = React.useRef<HTMLCanvasElement>(null);
  const imgElementRef = React.useRef<HTMLImageElement | null>(null);

  // Load initial image to determine dimensions
  React.useEffect(() => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = baseImage;
    img.onload = () => {
      imgElementRef.current = img;
      setDimensions({ width: img.naturalWidth, height: img.naturalHeight });
    };
  }, [baseImage]);

  // Push current state to undo history
  const recordHistory = React.useCallback(() => {
    setUndoStack((prev) => [
      ...prev,
      {
        annotations: [...annotations],
        baseImage,
        dimensions,
      },
    ]);
    setRedoStack([]);
  }, [annotations, baseImage, dimensions]);

  // Undo action
  const handleUndo = React.useCallback(() => {
    if (undoStack.length === 0) return;
    const previous = undoStack[undoStack.length - 1];
    setUndoStack((prev) => prev.slice(0, prev.length - 1));
    setRedoStack((prev) => [
      ...prev,
      {
        annotations: [...annotations],
        baseImage,
        dimensions,
      },
    ]);

    setAnnotations(previous.annotations);
    setBaseImage(previous.baseImage);
    setDimensions(previous.dimensions);
  }, [undoStack, annotations, baseImage, dimensions]);

  // Redo action
  const handleRedo = React.useCallback(() => {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    setRedoStack((prev) => prev.slice(0, prev.length - 1));
    setUndoStack((prev) => [
      ...prev,
      {
        annotations: [...annotations],
        baseImage,
        dimensions,
      },
    ]);

    setAnnotations(next.annotations);
    setBaseImage(next.baseImage);
    setDimensions(next.dimensions);
  }, [redoStack, annotations, baseImage, dimensions]);

  // Reset all edits back to initial
  const handleReset = () => {
    if (annotations.length === 0 && baseImage === initialDataUrl) return;
    recordHistory();
    setAnnotations([]);
    setBaseImage(initialDataUrl);
    setCropBox(null);
  };

  // Helper to constrain shape to 1:1 square/circle or 45-degree line/arrow when Shift is held
  const constrainShapePoint = React.useCallback(
    (start: Point, pt: Point, isShift: boolean): Point => {
      if (!isShift) return pt;
      const dx = pt.x - start.x;
      const dy = pt.y - start.y;

      if (activeTool === "rectangle" || activeTool === "circle") {
        const side = Math.max(Math.abs(dx), Math.abs(dy));
        const signX = dx >= 0 ? 1 : -1;
        const signY = dy >= 0 ? 1 : -1;
        return {
          x: start.x + side * signX,
          y: start.y + side * signY,
        };
      } else if (activeTool === "line" || activeTool === "arrow") {
        const angle = Math.atan2(dy, dx);
        const snappedAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4);
        const len = Math.hypot(dx, dy);
        return {
          x: start.x + len * Math.cos(snappedAngle),
          y: start.y + len * Math.sin(snappedAngle),
        };
      }
      return pt;
    },
    [activeTool]
  );



  // Initializing Crop Box when switching to Crop tool
  const handleSelectCropTool = () => {
    setActiveTool("crop");
    const marginW = Math.round(dimensions.width * 0.08);
    const marginH = Math.round(dimensions.height * 0.08);
    setCropBox({
      x: marginW,
      y: marginH,
      width: dimensions.width - marginW * 2,
      height: dimensions.height - marginH * 2,
    });
  };

  // Apply Crop
  const handleApplyCrop = () => {
    if (!cropBox || cropBox.width < 10 || cropBox.height < 10) return;

    recordHistory();

    const cropCanvas = document.createElement("canvas");
    cropCanvas.width = Math.round(cropBox.width);
    cropCanvas.height = Math.round(cropBox.height);
    const ctx = cropCanvas.getContext("2d");
    if (!ctx || !imgElementRef.current) return;

    // Draw cropped region of the image
    ctx.drawImage(
      imgElementRef.current,
      cropBox.x,
      cropBox.y,
      cropBox.width,
      cropBox.height,
      0,
      0,
      cropBox.width,
      cropBox.height
    );

    const croppedUrl = cropCanvas.toDataURL("image/png");

    // Adjust existing annotations to new coordinates
    const adjustedAnnotations = annotations
      .map((ann) => {
        if (ann.tool === "pen" || ann.tool === "highlighter") {
          const shiftedPoints = ann.points
            .map((p) => ({
              x: p.x - cropBox.x,
              y: p.y - cropBox.y,
            }))
            .filter(
              (p) =>
                p.x >= -50 &&
                p.x <= cropBox.width + 50 &&
                p.y >= -50 &&
                p.y <= cropBox.height + 50
            );
          if (shiftedPoints.length < 2) return null;
          return { ...ann, points: shiftedPoints };
        } else {
          return {
            ...ann,
            start: {
              x: ann.start.x - cropBox.x,
              y: ann.start.y - cropBox.y,
            },
            end: {
              x: ann.end.x - cropBox.x,
              y: ann.end.y - cropBox.y,
            },
          };
        }
      })
      .filter((a): a is Annotation => a !== null);

    setBaseImage(croppedUrl);
    setDimensions({ width: Math.round(cropBox.width), height: Math.round(cropBox.height) });
    setAnnotations(adjustedAnnotations);
    setCropBox(null);
    setActiveTool("pen");
    setStatusMessage("Image cropped");
    setTimeout(() => setStatusMessage(null), 2000);
  };

  // Cancel Crop
  const handleCancelCrop = () => {
    setCropBox(null);
    setActiveTool("pen");
  };

  // Convert pointer event coordinates into native image coordinates
  const getCanvasCoords = (
    e: React.PointerEvent<HTMLCanvasElement> | React.MouseEvent<HTMLCanvasElement>
  ): Point => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  };

  // Determine dynamic cursor style based on active tool and crop handle hovering/dragging
  const getCanvasCursor = (): string => {
    if (activeTool === "crop") {
      const handle = activeDragHandle || hoverCropHandle;
      if (handle === "nw" || handle === "se") return "nwse-resize";
      if (handle === "ne" || handle === "sw") return "nesw-resize";
      if (handle === "n" || handle === "s") return "ns-resize";
      if (handle === "w" || handle === "e") return "ew-resize";
      if (handle === "move") return "move";
      return "default";
    }
    if (activeTool === "eraser") return "cell";
    return "crosshair";
  };

  // Render loop: draws base image, annotations, in-progress shapes/strokes, and crop overlay
  const renderCanvas = React.useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // 1. Draw base image
    if (imgElementRef.current && imgElementRef.current.complete) {
      ctx.drawImage(imgElementRef.current, 0, 0, canvas.width, canvas.height);
    }

    // 2. Draw committed annotations
    for (const ann of annotations) {
      drawAnnotation(ctx, ann);
    }

    // 3. Draw active in-progress stroke / shape
    if (isDrawing) {
      if (activeTool === "pen") {
        drawPenStroke(ctx, {
          id: "temp",
          tool: "pen",
          points: currentStrokePointsRef.current,
          color: penColor,
          width: strokeWidth,
        });
      } else if (activeTool === "highlighter") {
        drawHighlighterStroke(ctx, {
          id: "temp",
          tool: "highlighter",
          points: currentStrokePointsRef.current,
          color: highlighterColor,
          width: highlighterWidth,
        });
      } else if (
        (activeTool === "rectangle" ||
          activeTool === "circle" ||
          activeTool === "line" ||
          activeTool === "arrow") &&
        shapeStartRef.current &&
        shapeCurrentRef.current
      ) {
        drawShape(ctx, {
          id: "temp",
          tool: activeTool,
          start: shapeStartRef.current,
          end: shapeCurrentRef.current,
          color: penColor,
          width: strokeWidth,
          fill: fillStyle,
        });
      }
    }

    // 4. Draw crop overlay if in crop mode
    if (activeTool === "crop" && cropBox) {
      drawCropOverlay(ctx, canvas.width, canvas.height, cropBox);
    }
  }, [
    annotations,
    isDrawing,
    activeTool,
    penColor,
    highlighterColor,
    strokeWidth,
    highlighterWidth,
    fillStyle,
    cropBox,
  ]);

  React.useEffect(() => {
    renderCanvas();
  }, [renderCanvas, dimensions, baseImage]);

  // Keyboard shortcuts (Ctrl+Z, Ctrl+Shift+Z, Shift 1:1 constrain, tool switches)
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).tagName === "INPUT") return;

      if (e.key === "Shift") {
        isShiftPressedRef.current = true;
        if (
          isDrawing &&
          shapeStartRef.current &&
          lastRawPointRef.current &&
          (activeTool === "rectangle" ||
            activeTool === "circle" ||
            activeTool === "line" ||
            activeTool === "arrow")
        ) {
          shapeCurrentRef.current = constrainShapePoint(
            shapeStartRef.current,
            lastRawPointRef.current,
            true
          );
          renderCanvas();
        }
      }

      if ((e.ctrlKey || e.metaKey) && e.key === "z") {
        e.preventDefault();
        if (e.shiftKey) {
          handleRedo();
        } else {
          handleUndo();
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key === "y") {
        e.preventDefault();
        handleRedo();
      } else if (e.key === "Escape") {
        if (activeTool === "crop") {
          setCropBox(null);
          setActiveTool("pen");
        } else {
          onClose();
        }
      } else if (e.key === "p" || e.key === "P") {
        setActiveTool("pen");
      } else if (e.key === "h" || e.key === "H") {
        setActiveTool("highlighter");
      } else if (e.key === "e" || e.key === "E") {
        setActiveTool("eraser");
      } else if (e.key === "c" || e.key === "C") {
        handleSelectCropTool();
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Shift") {
        isShiftPressedRef.current = false;
        if (
          isDrawing &&
          shapeStartRef.current &&
          lastRawPointRef.current &&
          (activeTool === "rectangle" ||
            activeTool === "circle" ||
            activeTool === "line" ||
            activeTool === "arrow")
        ) {
          shapeCurrentRef.current = lastRawPointRef.current;
          renderCanvas();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [handleUndo, handleRedo, activeTool, onClose, isDrawing, constrainShapePoint, renderCanvas]);

  // Drawing helpers
  function drawAnnotation(ctx: CanvasRenderingContext2D, ann: Annotation) {
    if (ann.tool === "pen") {
      drawPenStroke(ctx, ann);
    } else if (ann.tool === "highlighter") {
      drawHighlighterStroke(ctx, ann);
    } else {
      drawShape(ctx, ann);
    }
  }

  function drawPenStroke(ctx: CanvasRenderingContext2D, stroke: PenStroke) {
    if (stroke.points.length === 0) return;
    ctx.save();
    ctx.strokeStyle = stroke.color;
    ctx.lineWidth = stroke.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (stroke.points.length === 1) {
      ctx.fillStyle = stroke.color;
      ctx.beginPath();
      ctx.arc(stroke.points[0].x, stroke.points[0].y, stroke.width / 2, 0, Math.PI * 2);
      ctx.fill();
    } else {
      ctx.beginPath();
      ctx.moveTo(stroke.points[0].x, stroke.points[0].y);
      for (let i = 1; i < stroke.points.length - 1; i++) {
        const xc = (stroke.points[i].x + stroke.points[i + 1].x) / 2;
        const yc = (stroke.points[i].y + stroke.points[i + 1].y) / 2;
        ctx.quadraticCurveTo(stroke.points[i].x, stroke.points[i].y, xc, yc);
      }
      ctx.lineTo(
        stroke.points[stroke.points.length - 1].x,
        stroke.points[stroke.points.length - 1].y
      );
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawHighlighterStroke(
    ctx: CanvasRenderingContext2D,
    stroke: HighlighterStroke
  ) {
    if (stroke.points.length === 0) return;
    ctx.save();
    ctx.globalAlpha = 0.38;
    ctx.strokeStyle = stroke.color;
    ctx.lineWidth = stroke.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    ctx.beginPath();
    ctx.moveTo(stroke.points[0].x, stroke.points[0].y);
    for (let i = 1; i < stroke.points.length - 1; i++) {
      const xc = (stroke.points[i].x + stroke.points[i + 1].x) / 2;
      const yc = (stroke.points[i].y + stroke.points[i + 1].y) / 2;
      ctx.quadraticCurveTo(stroke.points[i].x, stroke.points[i].y, xc, yc);
    }
    ctx.lineTo(
      stroke.points[stroke.points.length - 1].x,
      stroke.points[stroke.points.length - 1].y
    );
    ctx.stroke();
    ctx.restore();
  }

  function drawShape(ctx: CanvasRenderingContext2D, shape: ShapeAnnotation) {
    ctx.save();
    ctx.strokeStyle = shape.color;
    ctx.lineWidth = shape.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    const { start, end, tool, fill } = shape;

    if (tool === "rectangle") {
      const x = Math.min(start.x, end.x);
      const y = Math.min(start.y, end.y);
      const w = Math.abs(end.x - start.x);
      const h = Math.abs(end.y - start.y);

      if (fill === "solid") {
        ctx.fillStyle = shape.color;
        ctx.fillRect(x, y, w, h);
      } else if (fill === "semi") {
        ctx.save();
        ctx.globalAlpha = 0.25;
        ctx.fillStyle = shape.color;
        ctx.fillRect(x, y, w, h);
        ctx.restore();
      }
      ctx.strokeRect(x, y, w, h);
    } else if (tool === "circle") {
      const rx = Math.abs(end.x - start.x) / 2;
      const ry = Math.abs(end.y - start.y) / 2;
      const cx = Math.min(start.x, end.x) + rx;
      const cy = Math.min(start.y, end.y) + ry;

      ctx.beginPath();
      ctx.ellipse(cx, cy, Math.max(1, rx), Math.max(1, ry), 0, 0, Math.PI * 2);
      if (fill === "solid") {
        ctx.fillStyle = shape.color;
        ctx.fill();
      } else if (fill === "semi") {
        ctx.save();
        ctx.globalAlpha = 0.25;
        ctx.fillStyle = shape.color;
        ctx.fill();
        ctx.restore();
      }
      ctx.stroke();
    } else if (tool === "line") {
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
    } else if (tool === "arrow") {
      const angle = Math.atan2(end.y - start.y, end.x - start.x);
      const dist = Math.hypot(end.x - start.x, end.y - start.y);
      const headLen = Math.max(16, shape.width * 3.8);
      const effectiveHeadLen = Math.min(headLen, dist * 0.9);
      const arrowAngle = Math.PI / 6; // 30 degrees

      // Stop the shaft line inside the base of the arrowhead so its rounded cap never pokes out past the tip
      const shaftEndDist = Math.max(0, dist - effectiveHeadLen * 0.65);
      const shaftEndX = start.x + shaftEndDist * Math.cos(angle);
      const shaftEndY = start.y + shaftEndDist * Math.sin(angle);

      // Draw line shaft
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(shaftEndX, shaftEndY);
      ctx.stroke();

      // Draw sharp arrowhead triangle
      ctx.fillStyle = shape.color;
      ctx.beginPath();
      ctx.moveTo(end.x, end.y);
      ctx.lineTo(
        end.x - effectiveHeadLen * Math.cos(angle - arrowAngle),
        end.y - effectiveHeadLen * Math.sin(angle - arrowAngle)
      );
      ctx.lineTo(
        end.x - effectiveHeadLen * Math.cos(angle + arrowAngle),
        end.y - effectiveHeadLen * Math.sin(angle + arrowAngle)
      );
      ctx.closePath();
      ctx.fill();
    }

    ctx.restore();
  }

  function drawCropOverlay(
    ctx: CanvasRenderingContext2D,
    canvasW: number,
    canvasH: number,
    box: CropBox
  ) {
    ctx.save();
    // 1. Darken outer region
    ctx.fillStyle = "rgba(0, 0, 0, 0.65)";
    ctx.beginPath();
    ctx.rect(0, 0, canvasW, canvasH);
    ctx.rect(box.x, box.y, box.width, box.height);
    ctx.fill("evenodd");

    // 2. Crop box border & rule-of-thirds grid
    ctx.strokeStyle = "#38bdf8";
    ctx.lineWidth = 2;
    ctx.strokeRect(box.x, box.y, box.width, box.height);

    ctx.strokeStyle = "rgba(255, 255, 255, 0.35)";
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    // Horizontal grid lines
    ctx.beginPath();
    ctx.moveTo(box.x, box.y + box.height / 3);
    ctx.lineTo(box.x + box.width, box.y + box.height / 3);
    ctx.moveTo(box.x, box.y + (box.height * 2) / 3);
    ctx.lineTo(box.x + box.width, box.y + (box.height * 2) / 3);
    // Vertical grid lines
    ctx.moveTo(box.x + box.width / 3, box.y);
    ctx.lineTo(box.x + box.width / 3, box.y + box.height);
    ctx.moveTo(box.x + (box.width * 2) / 3, box.y);
    ctx.lineTo(box.x + (box.width * 2) / 3, box.y + box.height);
    ctx.stroke();
    ctx.setLineDash([]);

    // 3. Draw 8 corner & edge handles with drop shadow
    const handleRadius = 6;
    const handles = [
      { x: box.x, y: box.y }, // nw
      { x: box.x + box.width / 2, y: box.y }, // n
      { x: box.x + box.width, y: box.y }, // ne
      { x: box.x + box.width, y: box.y + box.height / 2 }, // e
      { x: box.x + box.width, y: box.y + box.height }, // se
      { x: box.x + box.width / 2, y: box.y + box.height }, // s
      { x: box.x, y: box.y + box.height }, // sw
      { x: box.x, y: box.y + box.height / 2 }, // w
    ];

    ctx.save();
    ctx.shadowColor = "rgba(0, 0, 0, 0.5)";
    ctx.shadowBlur = 4;
    ctx.fillStyle = "#ffffff";
    ctx.strokeStyle = "#0284c7";
    ctx.lineWidth = 2.5;

    for (const h of handles) {
      ctx.beginPath();
      ctx.arc(h.x, h.y, handleRadius, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.restore();

    ctx.restore();
  }

  // Enhanced crop handle & full edge hit-testing
  function getCropHandleAt(pt: Point, box: CropBox): string | null {
    // 1. Corner handles first (with generous 22px radius)
    const cornerR = 22;
    if (Math.hypot(pt.x - box.x, pt.y - box.y) <= cornerR) return "nw";
    if (Math.hypot(pt.x - (box.x + box.width), pt.y - box.y) <= cornerR) return "ne";
    if (Math.hypot(pt.x - (box.x + box.width), pt.y - (box.y + box.height)) <= cornerR) return "se";
    if (Math.hypot(pt.x - box.x, pt.y - (box.y + box.height)) <= cornerR) return "sw";

    // 2. Full Edges with generous tolerance (18px)
    const edgeTolerance = 18;
    const isWithinX = pt.x >= box.x - edgeTolerance && pt.x <= box.x + box.width + edgeTolerance;
    const isWithinY = pt.y >= box.y - edgeTolerance && pt.y <= box.y + box.height + edgeTolerance;

    if (isWithinX) {
      if (Math.abs(pt.y - box.y) <= edgeTolerance) return "n";
      if (Math.abs(pt.y - (box.y + box.height)) <= edgeTolerance) return "s";
    }

    if (isWithinY) {
      if (Math.abs(pt.x - box.x) <= edgeTolerance) return "w";
      if (Math.abs(pt.x - (box.x + box.width)) <= edgeTolerance) return "e";
    }

    return null;
  }

  // Handle pointer down (drawing start, shape start, eraser click, crop handle drag)
  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const pt = getCanvasCoords(e);

    if (activeTool === "crop" && cropBox) {
      const handle = getCropHandleAt(pt, cropBox);
      if (handle) {
        setActiveDragHandle(handle);
        dragStartPointRef.current = pt;
        cropBoxStartRef.current = { ...cropBox };
        try {
          (e.currentTarget as HTMLCanvasElement).setPointerCapture(e.pointerId);
        } catch {}
      } else if (
        pt.x >= cropBox.x &&
        pt.x <= cropBox.x + cropBox.width &&
        pt.y >= cropBox.y &&
        pt.y <= cropBox.y + cropBox.height
      ) {
        setActiveDragHandle("move");
        dragStartPointRef.current = pt;
        cropBoxStartRef.current = { ...cropBox };
        try {
          (e.currentTarget as HTMLCanvasElement).setPointerCapture(e.pointerId);
        } catch {}
      }
      return;
    }

    if (activeTool === "eraser") {
      eraseAnnotationAt(pt);
      return;
    }

    setIsDrawing(true);
    try {
      (e.currentTarget as HTMLCanvasElement).setPointerCapture(e.pointerId);
    } catch {}

    if (activeTool === "pen" || activeTool === "highlighter") {
      currentStrokePointsRef.current = [pt];
    } else {
      shapeStartRef.current = pt;
      shapeCurrentRef.current = pt;
      lastRawPointRef.current = pt;
      isShiftPressedRef.current = e.shiftKey;
    }
  };

  // Handle pointer move
  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const pt = getCanvasCoords(e);

    if (activeTool === "crop" && cropBox) {
      if (activeDragHandle && dragStartPointRef.current && cropBoxStartRef.current) {
        const dx = pt.x - dragStartPointRef.current.x;
        const dy = pt.y - dragStartPointRef.current.y;
        const start = cropBoxStartRef.current;

        let newBox = { ...start };

        if (activeDragHandle === "move") {
          newBox.x = Math.max(0, Math.min(dimensions.width - start.width, start.x + dx));
          newBox.y = Math.max(0, Math.min(dimensions.height - start.height, start.y + dy));
        } else {
          if (activeDragHandle.includes("w")) {
            const maxLeft = start.x + start.width - 20;
            newBox.x = Math.max(0, Math.min(maxLeft, start.x + dx));
            newBox.width = start.width - (newBox.x - start.x);
          }
          if (activeDragHandle.includes("e")) {
            newBox.width = Math.max(20, Math.min(dimensions.width - start.x, start.width + dx));
          }
          if (activeDragHandle.includes("n")) {
            const maxTop = start.y + start.height - 20;
            newBox.y = Math.max(0, Math.min(maxTop, start.y + dy));
            newBox.height = start.height - (newBox.y - start.y);
          }
          if (activeDragHandle.includes("s")) {
            newBox.height = Math.max(20, Math.min(dimensions.height - start.y, start.height + dy));
          }

          // Apply aspect ratio constraints if enabled
          if (cropRatio !== "free") {
            let ratio = 1;
            if (cropRatio === "16:9") ratio = 16 / 9;
            else if (cropRatio === "4:3") ratio = 4 / 3;
            else if (cropRatio === "3:2") ratio = 3 / 2;

            newBox.height = newBox.width / ratio;
            if (newBox.y + newBox.height > dimensions.height) {
              newBox.height = dimensions.height - newBox.y;
              newBox.width = newBox.height * ratio;
            }
          }
        }

        setCropBox(newBox);
        renderCanvas();
        return;
      } else {
        // Hovering in crop mode: update cursor based on handle
        const handle = getCropHandleAt(pt, cropBox);
        if (handle) {
          setHoverCropHandle(handle);
        } else if (
          pt.x >= cropBox.x &&
          pt.x <= cropBox.x + cropBox.width &&
          pt.y >= cropBox.y &&
          pt.y <= cropBox.y + cropBox.height
        ) {
          setHoverCropHandle("move");
        } else {
          setHoverCropHandle(null);
        }
      }
      return;
    }

    if (!isDrawing) return;

    if (activeTool === "pen" || activeTool === "highlighter") {
      currentStrokePointsRef.current.push(pt);
      renderCanvas();
    } else if (
      activeTool === "rectangle" ||
      activeTool === "circle" ||
      activeTool === "line" ||
      activeTool === "arrow"
    ) {
      lastRawPointRef.current = pt;
      const isShift = e.shiftKey || isShiftPressedRef.current;
      if (shapeStartRef.current) {
        shapeCurrentRef.current = constrainShapePoint(shapeStartRef.current, pt, isShift);
      } else {
        shapeCurrentRef.current = pt;
      }
      renderCanvas();
    } else if (activeTool === "eraser") {
      eraseAnnotationAt(pt);
    }
  };

  // Handle pointer up / end
  const handlePointerUp = (e?: React.PointerEvent<HTMLCanvasElement>) => {
    if (e && (e.currentTarget as HTMLCanvasElement)?.hasPointerCapture?.(e.pointerId)) {
      try {
        (e.currentTarget as HTMLCanvasElement).releasePointerCapture(e.pointerId);
      } catch {}
    }

    if (activeTool === "crop") {
      setActiveDragHandle(null);
      dragStartPointRef.current = null;
      cropBoxStartRef.current = null;
      return;
    }

    if (!isDrawing) return;
    setIsDrawing(false);

    recordHistory();

    const isShift = (e ? e.shiftKey : false) || isShiftPressedRef.current;

    if (activeTool === "pen" && currentStrokePointsRef.current.length > 0) {
      const newStroke: PenStroke = {
        id: `pen-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`,
        tool: "pen",
        points: [...currentStrokePointsRef.current],
        color: penColor,
        width: strokeWidth,
      };
      setAnnotations((prev) => [...prev, newStroke]);
      currentStrokePointsRef.current = [];
    } else if (activeTool === "highlighter" && currentStrokePointsRef.current.length > 0) {
      const newStroke: HighlighterStroke = {
        id: `hl-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`,
        tool: "highlighter",
        points: [...currentStrokePointsRef.current],
        color: highlighterColor,
        width: highlighterWidth,
      };
      setAnnotations((prev) => [...prev, newStroke]);
      currentStrokePointsRef.current = [];
    } else if (
      (activeTool === "rectangle" ||
        activeTool === "circle" ||
        activeTool === "line" ||
        activeTool === "arrow") &&
      shapeStartRef.current &&
      shapeCurrentRef.current
    ) {
      if (lastRawPointRef.current && isShift) {
        shapeCurrentRef.current = constrainShapePoint(
          shapeStartRef.current,
          lastRawPointRef.current,
          true
        );
      }
      const dx = Math.abs(shapeCurrentRef.current.x - shapeStartRef.current.x);
      const dy = Math.abs(shapeCurrentRef.current.y - shapeStartRef.current.y);
      if (dx > 2 || dy > 2) {
        const newShape: ShapeAnnotation = {
          id: `shape-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`,
          tool: activeTool,
          start: shapeStartRef.current,
          end: shapeCurrentRef.current,
          color: penColor,
          width: strokeWidth,
          fill: fillStyle,
        };
        setAnnotations((prev) => [...prev, newShape]);
      }
      shapeStartRef.current = null;
      shapeCurrentRef.current = null;
      lastRawPointRef.current = null;
    }

    renderCanvas();
  };

  // Eraser hit detection: deletes any annotation near the point
  function eraseAnnotationAt(pt: Point) {
    const threshold = 16;
    const remaining = annotations.filter((ann) => {
      if (ann.tool === "pen" || ann.tool === "highlighter") {
        return !ann.points.some(
          (p) => Math.hypot(p.x - pt.x, p.y - pt.y) <= threshold + ann.width / 2
        );
      } else {
        const minX = Math.min(ann.start.x, ann.end.x) - threshold;
        const maxX = Math.max(ann.start.x, ann.end.x) + threshold;
        const minY = Math.min(ann.start.y, ann.end.y) - threshold;
        const maxY = Math.max(ann.start.y, ann.end.y) + threshold;

        if (pt.x >= minX && pt.x <= maxX && pt.y >= minY && pt.y <= maxY) {
          return false; // Hit
        }
        return true;
      }
    });

    if (remaining.length !== annotations.length) {
      recordHistory();
      setAnnotations(remaining);
    }
  }



  // Generate flattened PNG Data URL with all annotations rendered
  const getFlattenedDataUrl = (): string => {
    const exportCanvas = document.createElement("canvas");
    exportCanvas.width = dimensions.width;
    exportCanvas.height = dimensions.height;
    const ctx = exportCanvas.getContext("2d");
    if (!ctx) return baseImage;

    // 1. Draw base image
    if (imgElementRef.current) {
      ctx.drawImage(imgElementRef.current, 0, 0, dimensions.width, dimensions.height);
    }

    // 2. Draw annotations
    for (const ann of annotations) {
      drawAnnotation(ctx, ann);
    }

    return exportCanvas.toDataURL("image/png");
  };

  // Export 1: Copy Edited Image to clipboard
  const handleCopy = async () => {
    const flattened = getFlattenedDataUrl();
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("copy_image_to_clipboard", { dataUrl: flattened });
      }
      setCopied(true);
      setStatusMessage("Copied to clipboard!");
      setTimeout(() => {
        setCopied(false);
        setStatusMessage(null);
      }, 2000);
    } catch (err) {
      console.error("Failed to copy edited image", err);
    }
  };

  // Export 2: Save As File
  const handleSaveAs = async () => {
    setSavingFile(true);
    setStatusMessage(null);
    const flattened = getFlattenedDataUrl();
    const defaultFilename = `clipbox-edited-${Date.now()}.png`;

    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        const savedPath = await invoke<string | null>("save_image_to_file", {
          dataUrl: flattened,
          defaultFilename,
        });
        if (savedPath) {
          const filenameOnly = savedPath.split(/[/\\]/).pop() || savedPath;
          setStatusMessage(`Saved as ${filenameOnly}`);
          setTimeout(() => setStatusMessage(null), 3000);
        }
      } else {
        const link = document.createElement("a");
        link.href = flattened;
        link.download = defaultFilename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        setStatusMessage("Saved!");
        setTimeout(() => setStatusMessage(null), 3000);
      }
    } catch (err) {
      console.error("Failed to save image file", err);
    } finally {
      setSavingFile(false);
    }
  };

  // Export 3: Save as New Clip in Clipbox
  const handleSaveAsNewClip = async () => {
    const flattened = getFlattenedDataUrl();
    const dimStr = `${dimensions.width}x${dimensions.height}`;

    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("save_edited_image_entry", {
          dataUrl: flattened,
          dimensions: dimStr,
          sourceEntryId: sourceEntryId ?? null,
        });
      }
      setSavedClip(true);
      setStatusMessage("Saved as new clip!");
      if (onSavedNewClip) onSavedNewClip();
      setTimeout(() => {
        setSavedClip(false);
        setStatusMessage(null);
      }, 2500);
    } catch (err) {
      console.error("Failed to save as new clip", err);
    }
  };

  return (
    <div className="relative flex flex-col h-full w-full bg-background select-none overflow-hidden animate-in fade-in-0 duration-200">
      {/* Top Editor Command Bar */}
      <div
        data-window-chrome
        onMouseDown={(e) => {
          if (
            e.button === 0 &&
            !(e.target as HTMLElement).closest("button, input, [role='dialog'], [role='menu']") &&
            "__TAURI_INTERNALS__" in window
          ) {
            invoke("begin_window_drag").catch(console.warn);
          }
        }}
        className="h-14 w-full bg-card border-b px-4 flex items-center justify-between gap-3 shrink-0 shadow-sm select-none flex-nowrap"
      >
        {/* Left: Contextual Tool Controls */}
        {activeTool === "crop" ? (
          /* Dedicated Crop Controls Bar */
          <div className="flex items-center gap-2 shrink-0">
            <div
              className="size-8 rounded-md bg-sky-500/15 text-sky-400 border border-sky-500/30 flex items-center justify-center select-none"
              title="Crop Mode"
            >
              <Crop className="size-4" />
            </div>

            <div className="flex items-center gap-0.5 bg-muted/60 p-0.5 rounded-lg border">
              {(["free", "1:1", "16:9", "4:3"] as CropAspectRatio[]).map((ratio) => (
                <Button
                  key={ratio}
                  variant={cropRatio === ratio ? "secondary" : "ghost"}
                  size="sm"
                  className="h-7 px-2.5 text-xs font-medium"
                  onClick={() => {
                    setCropRatio(ratio);
                    if (cropBox && ratio !== "free") {
                      let r = 1;
                      if (ratio === "16:9") r = 16 / 9;
                      else if (ratio === "4:3") r = 4 / 3;
                      const newH = Math.min(dimensions.height, cropBox.width / r);
                      const newW = newH * r;
                      setCropBox({
                        x: Math.max(0, Math.min(dimensions.width - newW, cropBox.x)),
                        y: Math.max(0, Math.min(dimensions.height - newH, cropBox.y)),
                        width: newW,
                        height: newH,
                      });
                    }
                  }}
                >
                  {ratio === "free" ? "Free" : ratio}
                </Button>
              ))}
            </div>

            <Button
              variant="default"
              size="icon"
              onClick={handleApplyCrop}
              className="size-8 bg-emerald-600 hover:bg-emerald-700 text-white shadow-xs"
              title="Apply Crop"
            >
              <Check className="size-4" />
            </Button>

            <Button
              variant="outline"
              size="icon"
              onClick={handleCancelCrop}
              className="size-8"
              title="Cancel Crop"
            >
              <X className="size-4" />
            </Button>
          </div>
        ) : (
          /* Segmented Drawing Tools & Color Palette */
          <div className="flex items-center gap-2 shrink-0">
            <div className="flex items-center bg-muted/50 p-0.5 rounded-lg border gap-0.5">
              <Button
                variant={activeTool === "pen" ? "secondary" : "ghost"}
                size="icon"
                onClick={() => setActiveTool("pen")}
                className="size-8"
                title="Ballpoint Pen (P)"
              >
                <Pen className="size-4 text-primary" />
              </Button>

              <Button
                variant={activeTool === "highlighter" ? "secondary" : "ghost"}
                size="icon"
                onClick={() => setActiveTool("highlighter")}
                className="size-8"
                title="Highlighter (H)"
              >
                <Highlighter className="size-4 text-amber-500" />
              </Button>

              <Button
                variant={activeTool === "eraser" ? "secondary" : "ghost"}
                size="icon"
                onClick={() => setActiveTool("eraser")}
                className="size-8"
                title="Eraser (E)"
              >
                <Eraser className="size-4 text-rose-500" />
              </Button>

              {/* Shapes Popover */}
              <Popover open={isShapeMenuOpen} onOpenChange={setIsShapeMenuOpen}>
                <PopoverTrigger asChild>
                  <Button
                    variant={
                      activeTool === "rectangle" ||
                      activeTool === "circle" ||
                      activeTool === "line" ||
                      activeTool === "arrow"
                        ? "secondary"
                        : "ghost"
                    }
                    size="icon"
                    className="size-8"
                    title="Shapes (Rectangle, Circle, Arrow, Line)"
                  >
                    {activeTool === "circle" ? (
                      <Circle className="size-4" />
                    ) : activeTool === "line" ? (
                      <Minus className="size-4" />
                    ) : activeTool === "arrow" ? (
                      <MoveRight className="size-4" />
                    ) : (
                      <Square className="size-4" />
                    )}
                  </Button>
                </PopoverTrigger>
                <PopoverContent align="start" className="w-48 p-2 space-y-1">
                  <div className="text-[11px] font-semibold text-muted-foreground px-2 py-1 uppercase tracking-wider">
                    Select Shape
                  </div>
                  <Button
                    variant={activeTool === "rectangle" ? "secondary" : "ghost"}
                    size="sm"
                    className="w-full justify-start gap-2 h-8 text-xs"
                    onClick={() => {
                      setActiveTool("rectangle");
                      setIsShapeMenuOpen(false);
                    }}
                  >
                    <Square className="size-3.5" />
                    <span>Rectangle</span>
                  </Button>
                  <Button
                    variant={activeTool === "circle" ? "secondary" : "ghost"}
                    size="sm"
                    className="w-full justify-start gap-2 h-8 text-xs"
                    onClick={() => {
                      setActiveTool("circle");
                      setIsShapeMenuOpen(false);
                    }}
                  >
                    <Circle className="size-3.5" />
                    <span>Circle / Ellipse</span>
                  </Button>
                  <Button
                    variant={activeTool === "arrow" ? "secondary" : "ghost"}
                    size="sm"
                    className="w-full justify-start gap-2 h-8 text-xs"
                    onClick={() => {
                      setActiveTool("arrow");
                      setIsShapeMenuOpen(false);
                    }}
                  >
                    <MoveRight className="size-3.5" />
                    <span>Arrow</span>
                  </Button>
                  <Button
                    variant={activeTool === "line" ? "secondary" : "ghost"}
                    size="sm"
                    className="w-full justify-start gap-2 h-8 text-xs"
                    onClick={() => {
                      setActiveTool("line");
                      setIsShapeMenuOpen(false);
                    }}
                  >
                    <Minus className="size-3.5" />
                    <span>Line</span>
                  </Button>

                  <div className="border-t pt-1 mt-1">
                    <div className="text-[11px] font-semibold text-muted-foreground px-2 py-1 uppercase tracking-wider">
                      Fill Style
                    </div>
                    <div className="grid grid-cols-3 gap-1 px-1">
                      <Button
                        variant={fillStyle === "none" ? "secondary" : "ghost"}
                        size="sm"
                        className="h-7 text-[11px]"
                        onClick={() => setFillStyle("none")}
                      >
                        Outline
                      </Button>
                      <Button
                        variant={fillStyle === "semi" ? "secondary" : "ghost"}
                        size="sm"
                        className="h-7 text-[11px]"
                        onClick={() => setFillStyle("semi")}
                      >
                        Semi
                      </Button>
                      <Button
                        variant={fillStyle === "solid" ? "secondary" : "ghost"}
                        size="sm"
                        className="h-7 text-[11px]"
                        onClick={() => setFillStyle("solid")}
                      >
                        Solid
                      </Button>
                    </div>
                  </div>
                </PopoverContent>
              </Popover>

              {/* Crop Tool Toggle */}
              <Button
                variant="ghost"
                size="icon"
                onClick={handleSelectCropTool}
                className="size-8 text-sky-400 hover:text-sky-300"
                title="Crop Image (C)"
              >
                <Crop className="size-4" />
              </Button>
            </div>

            {/* Color & Width Popover */}
            {activeTool !== "eraser" && (
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-8 px-2.5 gap-1.5 text-xs font-medium border"
                    title="Color & Thickness"
                  >
                    <div
                      className="size-4 rounded-full border border-black/20 shadow-sm"
                      style={{
                        backgroundColor:
                          activeTool === "highlighter" ? highlighterColor : penColor,
                      }}
                    />
                    <Palette className="size-3.5 text-muted-foreground" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent align="start" className="w-72 p-3.5 space-y-3.5">
                  <div>
                    <div className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                      Colors
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {(activeTool === "highlighter" ? HIGHLIGHTER_COLORS : PEN_COLORS).map(
                        (c) => (
                          <button
                            key={c.value}
                            type="button"
                            onClick={() => {
                              if (activeTool === "highlighter") {
                                setHighlighterColor(c.value);
                              } else {
                                setPenColor(c.value);
                              }
                            }}
                            className={`size-7 rounded-full border transition-transform flex items-center justify-center ${
                              (activeTool === "highlighter" ? highlighterColor : penColor) ===
                              c.value
                                ? "scale-110 ring-2 ring-primary ring-offset-1 ring-offset-background"
                                : "hover:scale-105"
                            }`}
                            style={{ backgroundColor: c.value }}
                            title={c.name}
                          >
                            {(activeTool === "highlighter" ? highlighterColor : penColor) ===
                              c.value && (
                              <Check
                                className={`size-3.5 ${
                                  c.value === "#ffffff" || c.value === "#facc15"
                                    ? "text-black"
                                    : "text-white"
                                }`}
                              />
                            )}
                          </button>
                        )
                      )}
                    </div>
                  </div>

                  <div className="border-t pt-2.5">
                    <div className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                      Thickness
                    </div>
                    {activeTool === "highlighter" ? (
                      <div className="grid grid-cols-3 gap-1.5">
                        {HIGHLIGHTER_WIDTHS.map((w) => (
                          <Button
                            key={w.value}
                            variant={highlighterWidth === w.value ? "secondary" : "ghost"}
                            size="sm"
                            className="h-7 text-xs font-medium"
                            onClick={() => setHighlighterWidth(w.value)}
                          >
                            {w.label}
                          </Button>
                        ))}
                      </div>
                    ) : (
                      <div className="grid grid-cols-4 gap-1.5">
                        {STROKE_WIDTHS.map((w) => (
                          <Button
                            key={w.value}
                            variant={strokeWidth === w.value ? "secondary" : "ghost"}
                            size="sm"
                            className="h-7 text-xs font-medium"
                            onClick={() => setStrokeWidth(w.value)}
                          >
                            {w.label}
                          </Button>
                        ))}
                      </div>
                    )}
                  </div>
                </PopoverContent>
              </Popover>
            )}
          </div>
        )}

        {/* Right: History & Export Actions */}
        <div className="flex items-center gap-1.5 shrink-0">

          <Button
            variant="ghost"
            size="icon"
            onClick={handleUndo}
            disabled={undoStack.length === 0}
            className="size-8"
            title="Undo (Ctrl+Z)"
          >
            <Undo2 className="size-3.5" />
          </Button>

          <Button
            variant="ghost"
            size="icon"
            onClick={handleRedo}
            disabled={redoStack.length === 0}
            className="size-8"
            title="Redo (Ctrl+Shift+Z)"
          >
            <Redo2 className="size-3.5" />
          </Button>

          <Button
            variant="ghost"
            size="icon"
            onClick={handleReset}
            disabled={annotations.length === 0 && baseImage === initialDataUrl}
            className="size-8 text-muted-foreground hover:text-destructive"
            title="Reset All Edits"
          >
            <RotateCcw className="size-3.5" />
          </Button>

          <div className="h-4 w-px bg-border mx-1" />

          {/* Export: Save As New Clip */}
          <Button
            variant={savedClip ? "default" : "outline"}
            size="sm"
            onClick={handleSaveAsNewClip}
            className="h-8 px-2.5 gap-1.5 text-xs font-medium"
            title="Save annotated image as a new Clipbox history entry"
          >
            {savedClip ? (
              <CheckCheck className="size-3.5 text-emerald-400" />
            ) : (
              <BookmarkPlus className="size-3.5 text-primary" />
            )}
            <span className="hidden sm:inline">New Clip</span>
          </Button>

          {/* Export: Save File to Disk */}
          <Button
            variant="outline"
            size="icon"
            onClick={handleSaveAs}
            disabled={savingFile}
            className="size-8"
            title="Save as PNG file"
          >
            <Download className="size-4" />
          </Button>

          {/* Export: Copy to Clipboard */}
          <Button
            variant="default"
            size="sm"
            onClick={handleCopy}
            className="h-8 px-3 gap-1.5 text-xs font-medium shadow-sm"
            title="Copy flattened image to clipboard"
          >
            {copied ? (
              <>
                <Check className="size-3.5 text-white" />
                <span>Copied!</span>
              </>
            ) : (
              <>
                <Copy className="size-3.5" />
                <span>Copy</span>
              </>
            )}
          </Button>

          {/* Exit Editor */}
          <Button
            variant="ghost"
            size="icon"
            onClick={onClose}
            title="Exit Editor (Esc)"
            className="size-8 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground ml-1"
          >
            <X className="size-4" />
          </Button>
        </div>
      </div>

      {/* Main Canvas Workspace Stage */}
      <div
        ref={containerRef}
        className="flex-1 w-full overflow-hidden flex items-center justify-center p-4 relative bg-black/75 cursor-crosshair"
      >
        <div
          className="relative max-w-full max-h-full flex items-center justify-center rounded-lg shadow-2xl overflow-hidden border border-border/40"
          style={{
            backgroundImage: `
              linear-gradient(45deg, rgba(255,255,255,0.03) 25%, transparent 25%),
              linear-gradient(-45deg, rgba(255,255,255,0.03) 25%, transparent 25%),
              linear-gradient(45deg, transparent 75%, rgba(255,255,255,0.03) 75%),
              linear-gradient(-45deg, transparent 75%, rgba(255,255,255,0.03) 75%)
            `,
            backgroundSize: "20px 20px",
            backgroundPosition: "0 0, 0 10px, 10px -10px, -10px 0px",
          }}
        >
          <canvas
            ref={canvasRef}
            width={dimensions.width}
            height={dimensions.height}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
            onPointerLeave={() => {
              if (!activeDragHandle && !isDrawing) {
                setHoverCropHandle(null);
              }
            }}
            style={{ cursor: getCanvasCursor() }}
            className="max-w-[calc(90vw)] max-h-[calc(78vh)] object-contain select-none touch-none"
          />
        </div>
      </div>

      {/* Bottom Status Footer */}
      <div className="h-8 w-full bg-card/90 border-t px-4 flex items-center justify-between text-[11px] text-muted-foreground shrink-0 select-none">
        <div className="flex items-center gap-2">
          <span>{dimensions.width} × {dimensions.height} px</span>
          <span>•</span>
          <span>{annotations.length} annotation{annotations.length === 1 ? "" : "s"}</span>
        </div>
        <div className="flex items-center gap-3">
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">P</kbd> Pen</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">H</kbd> Highlighter</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">E</kbd> Eraser</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">C</kbd> Crop</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">Shift</kbd> 1:1 Square/Circle</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">Ctrl+Z</kbd> Undo</span>
          <span><kbd className="px-1 py-0.5 bg-muted rounded border text-[10px] font-mono">Esc</kbd> Exit</span>
        </div>
      </div>

      {/* Floating Status Toast Notification */}
      {statusMessage && (
        <div className="absolute bottom-12 left-1/2 -translate-x-1/2 z-50 pointer-events-none animate-in fade-in-0 slide-in-from-bottom-2 duration-200">
          <div className="bg-popover/95 backdrop-blur-md text-popover-foreground border shadow-xl rounded-full px-4 py-2 flex items-center gap-2 text-xs font-medium">
            <Check className="size-4 text-emerald-400 shrink-0" />
            <span>{statusMessage}</span>
          </div>
        </div>
      )}
    </div>
  );
}
