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
  color: string;
  side: "bullish" | "bearish";
  label?: string;
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
      }) => void
    ) => void;
  }) {
    // Early exit if no data
    if (this._rects.length === 0) return;

    const series = this._series;
    if (!series) return;

    // Check if chart and timeScale are available
    const chart = series.chart;
    if (!chart) return;

    const timeScale = chart.timeScale();
    if (!timeScale) return;

    // Check if series has priceToCoordinate method
    const seriesApi = series as unknown as SeriesApi;
    if (typeof seriesApi.priceToCoordinate !== "function") return;

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
          const y = topY;
          const width = rightCoord - startX;
          const height = bottomY - topY;

          // Skip if dimensions are invalid
          if (width <= 0 || height === 0) continue;

          // Draw filled rectangle with transparency
          ctx.fillStyle = rect.color;
          ctx.fillRect(x, y, width, Math.abs(height));

          // Draw border
          ctx.strokeStyle = rect.side === "bullish" ? "#22c55e" : "#ef4444";
          ctx.lineWidth = 1;
          ctx.strokeRect(x, y, width, Math.abs(height));

          // Draw label if present
          if (rect.label) {
            ctx.font = "11px sans-serif";
            ctx.fillStyle = rect.side === "bullish" ? "#22c55e" : "#ef4444";
            ctx.textAlign = "left";
            ctx.textBaseline = "top";
            ctx.fillText(rect.label, x + 4, y + 4);
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

export class ClusterRectanglesPrimitive implements ISeriesPrimitive<Time> {
  private _paneView = new ClusterRectanglesPaneView();
  private _rects: ClusterRect[] = [];
  private _series: SeriesAttachedParameter<Time> | null = null;
  private _requestUpdate?: () => void;

  attached(param: SeriesAttachedParameter<Time>): void {
    this._series = param;
    this._requestUpdate = param.requestUpdate;
    this._paneView.update(this._rects, this._series);
  }

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

  setRectangles(rects: ClusterRect[]): void {
    this._rects = rects;
    this._paneView.update(this._rects, this._series);
  }

  clearRectangles(): void {
    this._rects = [];
    this._paneView.update(this._rects, this._series);
  }
}
