/**
 * ClusterRectanglesPrimitive - Custom LightweightCharts primitive for rendering cluster zones
 *
 * This primitive draws semi-transparent rectangular zones on the chart representing
 * price clusters (bullish targets above current price, bearish targets below).
 *
 * COLOR SCHEME:
 * - Bullish clusters: Green (#22c55e / rgb(34, 197, 94))
 * - Bearish clusters: Red (#ef4444 / rgb(239, 68, 68))
 *
 * VISUAL ELEMENTS:
 * 1. Filled rectangle with configurable opacity (from rect.color)
 * 2. Left border accent (2px solid) for visual emphasis
 * 3. Top/bottom dashed lines (50% opacity) extending to chart edge
 * 4. Optional label badge with count information
 */

import type {
  ISeriesPrimitive,
  SeriesAttachedParameter,
  Time,
  IPrimitivePaneRenderer,
  IPrimitivePaneView,
  PrimitiveHoveredItem,
  Coordinate,
} from "lightweight-charts";

export interface ClusterRect {
  id: string;
  low: number;
  high: number;
  startTime: Time;
  /** Fill color with opacity, e.g. "rgba(34, 197, 94, 0.25)" */
  color: string;
  side: "bullish" | "bearish";
  label?: string;
  /** Custom border color (optional, defaults to side-based color) */
  borderColor?: string;
  /** Border style: solid, dashed, or none */
  borderStyle?: "solid" | "dashed" | "none";
}

interface SeriesApi {
  priceToCoordinate: (price: number) => Coordinate | null;
}

class ClusterRectanglesPaneRenderer implements IPrimitivePaneRenderer {
  private _rects: ClusterRect[] = [];
  private _series: SeriesAttachedParameter<Time> | null = null;

  update(rects: ClusterRect[], series: SeriesAttachedParameter<Time> | null) {
    this._rects = rects;
    this._series = series;
  }

  draw(target: {
    useMediaCoordinateSpace: (
      callback: (scope: {
        context: CanvasRenderingContext2D;
        mediaSize: { width: number; height: number };
      }) => void,
    ) => void;
  }) {
    // Early exit if no data
    if (this._rects.length === 0) return;

    const param = this._series;
    if (!param) return;

    // Check if chart and timeScale are available
    const chart = param.chart;
    if (!chart) return;

    const timeScale = chart.timeScale();
    if (!timeScale) return;

    // Get the actual series API which has priceToCoordinate
    const seriesApi = param.series as unknown as SeriesApi;
    if (!seriesApi || typeof seriesApi.priceToCoordinate !== "function") return;

    target.useMediaCoordinateSpace((scope) => {
      const ctx = scope.context;
      const rightCoord = scope.mediaSize.width;

      // Save context state to avoid affecting other chart drawings
      ctx.save();

      try {
        for (const rect of this._rects) {
          // Get x coordinate for start time
          const startX = timeScale.timeToCoordinate(rect.startTime);
          if (startX === null) continue;

          // Get y coordinates for price range
          const topY = seriesApi.priceToCoordinate(rect.high);
          const bottomY = seriesApi.priceToCoordinate(rect.low);
          if (topY === null || bottomY === null) continue;

          const x = startX;
          const y = Math.min(topY, bottomY);
          const width = rightCoord - startX;
          const height = Math.abs(bottomY - topY);

          // Skip if dimensions are invalid
          if (width <= 0 || height === 0) continue;

          const borderRadius = Math.min(4, height / 2, width / 2);
          const isBullish = rect.side === "bullish";

          // === COLOR: Border color - use custom or fall back to side-based ===
          const defaultBorderColor = isBullish ? "#22c55e" : "#ef4444";
          const borderColor = rect.borderColor ?? defaultBorderColor;
          const borderStyle = rect.borderStyle ?? "dashed";

          // === FILL: Semi-transparent rectangle background ===
          // Color comes from rect.color which includes opacity (e.g. "rgba(34, 197, 94, 0.25)")
          // Opacity is controlled by the clusterOpacity setting in the parent component
          ctx.beginPath();
          ctx.roundRect(x, y, width, height, [
            borderRadius,
            0,
            0,
            borderRadius,
          ]);
          ctx.fillStyle = rect.color;
          ctx.fill();

          // Skip border drawing if style is "none"
          if (borderStyle !== "none") {
            // === LEFT BORDER: Colored accent line (2px) ===
            // Thicker border on left edge to highlight cluster zone boundary
            ctx.beginPath();
            ctx.moveTo(x + borderRadius, y);
            ctx.lineTo(x, y + borderRadius);
            ctx.lineTo(x, y + height - borderRadius);
            ctx.lineTo(x + borderRadius, y + height);
            ctx.strokeStyle = borderColor;
            ctx.lineWidth = 2;
            ctx.stroke();

            // === TOP/BOTTOM BORDERS: Horizontal lines ===
            // Style based on borderStyle setting (solid or dashed)
            ctx.beginPath();
            if (borderStyle === "dashed") {
              ctx.setLineDash([4, 4]);
            } else {
              ctx.setLineDash([]);
            }
            ctx.moveTo(x + borderRadius, y);
            ctx.lineTo(x + width, y);
            ctx.moveTo(x + borderRadius, y + height);
            ctx.lineTo(x + width, y + height);
            ctx.strokeStyle = borderColor;
            ctx.lineWidth = 1;
            ctx.globalAlpha = 0.5;
            ctx.stroke();
            ctx.setLineDash([]);
            ctx.globalAlpha = 1;
          }

          // === LABEL: Badge with cluster info ===
          if (rect.label) {
            const padding = 6;
            ctx.font =
              "bold 11px -apple-system, BlinkMacSystemFont, sans-serif";
            const textMetrics = ctx.measureText(rect.label);
            const textHeight = 14;
            const labelBgWidth = textMetrics.width + padding * 2;
            const labelBgHeight = textHeight + padding;

            // === LABEL BACKGROUND: Solid color at 90% opacity ===
            // Green rgba(34, 197, 94, 0.9) for bullish
            // Red rgba(239, 68, 68, 0.9) for bearish
            ctx.fillStyle = isBullish
              ? "rgba(34, 197, 94, 0.9)"
              : "rgba(239, 68, 68, 0.9)";
            ctx.beginPath();
            ctx.roundRect(x + 4, y + 4, labelBgWidth, labelBgHeight, 3);
            ctx.fill();

            // === LABEL TEXT: White text on colored background ===
            ctx.fillStyle = "#ffffff";
            ctx.textAlign = "left";
            ctx.textBaseline = "top";
            ctx.fillText(rect.label, x + 4 + padding, y + 4 + padding / 2);
          }
        }
      } finally {
        // Always restore context state
        ctx.restore();
      }
    });
  }
}

class ClusterRectanglesPaneView implements IPrimitivePaneView {
  private _renderer = new ClusterRectanglesPaneRenderer();
  private _rects: ClusterRect[] = [];
  private _series: SeriesAttachedParameter<Time> | null = null;

  update(rects: ClusterRect[], series: SeriesAttachedParameter<Time> | null) {
    this._rects = rects;
    this._series = series;
    this._renderer.update(rects, series);
  }

  renderer(): IPrimitivePaneRenderer {
    return this._renderer;
  }

  zOrder(): "bottom" | "normal" | "top" {
    return "bottom";
  }
}

/**
 * ClusterRectanglesPrimitive - Attachable primitive for LightweightCharts series
 *
 * Usage:
 *   const primitive = new ClusterRectanglesPrimitive();
 *   candleSeries.attachPrimitive(primitive);
 *   primitive.setRectangles(clusterRects);
 */
export class ClusterRectanglesPrimitive implements ISeriesPrimitive<Time> {
  private _paneView = new ClusterRectanglesPaneView();
  private _rects: ClusterRect[] = [];
  private _series: SeriesAttachedParameter<Time> | null = null;
  private _requestUpdate?: () => void;

  /** Called by LightweightCharts when primitive is attached to a series */
  attached(param: SeriesAttachedParameter<Time>): void {
    this._series = param;
    this._requestUpdate = param.requestUpdate;
    this._paneView.update(this._rects, this._series);
  }

  /** Called by LightweightCharts when primitive is detached */
  detached(): void {
    this._series = null;
    this._requestUpdate = undefined;
  }

  paneViews(): readonly IPrimitivePaneView[] {
    return [this._paneView];
  }

  updateAllViews(): void {
    this._paneView.update(this._rects, this._series);
  }

  hitTest(): PrimitiveHoveredItem | null {
    return null;
  }

  /** Update the cluster rectangles to display */
  setRectangles(rects: ClusterRect[]): void {
    this._rects = rects;
    this._paneView.update(this._rects, this._series);
    this._requestUpdate?.();
  }

  /** Clear all cluster rectangles */
  clearRectangles(): void {
    this._rects = [];
    this._paneView.update(this._rects, this._series);
    this._requestUpdate?.();
  }
}
