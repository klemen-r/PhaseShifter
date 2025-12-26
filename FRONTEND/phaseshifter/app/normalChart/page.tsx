"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Settings } from "lucide-react";

import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { useSidebar } from "@/components/ui/sidebar";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { calculateDonchianMidpoint, type Candle } from "@/lib/calcDonchianMid";
import { ExtraSettingsDialog, type ChartSettingsData, type SavedPreset } from "@/components/nchartS";
import {
  useTradingData,
  type WSCandle,
  type ClustersData,
} from "@/lib/websocket";
import { ClustersDisplay } from "@/components/websocket";
import type { UTCTimestamp, Time, ISeriesApi } from "lightweight-charts";

import {
  createChart,
  IChartApi,
  BarData,
  LineSeries,
  CandlestickSeries,
  LineType,
} from "lightweight-charts";
import {
  ClusterRectanglesPrimitive,
  type ClusterRect,
} from "@/lib/chart/ClusterRectanglesPrimitive";

// Convert WebSocket candles to chart-compatible format
function wsCandiesToChartCandles(wsCandles: WSCandle[]): Candle[] {
  return wsCandles.map((c) => ({
    time: Math.floor(c.time / 1000) as UTCTimestamp,
    open: c.open,
    high: c.high,
    low: c.low,
    close: c.close,
    volume: c.volume,
  }));
}


type LineSettings = {
  lineColor: string;
  bgUpColor: string;
  bgDownColor: string;
  transparency: number;
  candleUpColor: string;
  candleDownColor: string;
  candleBorderUp: string;
  candleBorderDown: string;
};

export default function NormalChartPage() {
  const { open, openMobile, setOpen, setOpenMobile } = useSidebar();
  const [showMidpoint, setShowMidpoint] = useState(true);
  const [phase, setPhaseAmount] = useState(5);
  const [settings, setSettings] = useState<LineSettings>({
    lineColor: "#A06BF3",
    bgUpColor: "#006400",
    bgDownColor: "#8b0000",
    transparency: 96,
    candleUpColor: "#F5F7FA",
    candleDownColor: "#A0B4F3",
    candleBorderUp: "#F5F7FA",
    candleBorderDown: "#A0B4F3",
  });
  const [extraOpen, setExtraOpen] = useState(false);

  // Ticker input (what user types) vs active ticker (what's displayed)
  const [tickerInput, setTickerInput] = useState(() => {
    if (typeof window !== "undefined") {
      return localStorage.getItem("lastTicker") || "NQ=F";
    }
    return "NQ=F";
  });
  const [activeTicker, setActiveTicker] = useState(() => {
    if (typeof window !== "undefined") {
      return localStorage.getItem("lastTicker") || "NQ=F";
    }
    return "NQ=F";
  });

  // Cluster display settings
  const [clusterOpacity, setClusterOpacity] = useState(25); // 10-80%
  const [clusterBullishColor, setClusterBullishColor] = useState("#22c55e");
  const [clusterBearishColor, setClusterBearishColor] = useState("#ef4444");
  const [clusterBorderColor, setClusterBorderColor] = useState("#ffffff");
  const [clusterBorderStyle, setClusterBorderStyle] = useState<"solid" | "dashed" | "none">("dashed");

  // Saved presets
  const [savedPresets, setSavedPresets] = useState<SavedPreset[]>([]);

  // Fetch saved presets from API
  const fetchPresets = useCallback(async () => {
    try {
      const res = await fetch("/api/chart-settings");
      if (res.ok) {
        const data = await res.json();
        setSavedPresets(data);
      }
    } catch (err) {
      console.error("Failed to fetch presets:", err);
    }
  }, []);

  // Apply a preset's settings
  const applyPreset = useCallback((preset: ChartSettingsData) => {
    setPhaseAmount(preset.phaseAmount);
    setShowMidpoint(preset.showMidpoint);
    setClusterOpacity(preset.clusterOpacity);
    setClusterBullishColor(preset.clusterBullishColor);
    setClusterBearishColor(preset.clusterBearishColor);
    setClusterBorderColor(preset.clusterBorderColor);
    setClusterBorderStyle(preset.clusterBorderStyle);
    setSettings({
      lineColor: preset.lineColor,
      bgUpColor: preset.bgUpColor,
      bgDownColor: preset.bgDownColor,
      transparency: preset.transparency,
      candleUpColor: preset.candleUpColor,
      candleDownColor: preset.candleDownColor,
      candleBorderUp: preset.candleBorderUp,
      candleBorderDown: preset.candleBorderDown,
    });
  }, []);

  // Load presets on mount
  useEffect(() => {
    fetchPresets();
  }, [fetchPresets]);

  const {
    status: wsStatus,
    subscribedTickers,
    subscribeTicker,
    unsubscribeTicker,
    getCandles,
    getClusters,
    requestClusters,
    lastCandle,
    enableAutoClusters,
    disableAutoClusters,
    isAutoClustersEnabled,
  } = useTradingData();

  const {
    lineColor,
    bgUpColor,
    bgDownColor,
    transparency,
    candleUpColor,
    candleDownColor,
    candleBorderUp,
    candleBorderDown,
  } = settings;

  // Track if we've auto-requested data on first chart click
  const hasAutoRequested = useRef(false);

  // Auto-subscribe to remembered ticker when WebSocket connects
  useEffect(() => {
    if (wsStatus === "open" && wsTicker && !subscribedTickers.has(wsTicker)) {
      subscribeTicker(wsTicker);
      localStorage.setItem("lastTicker", wsTicker);
    }
  }, [wsStatus, wsTicker, subscribedTickers, subscribeTicker]);

  const handleChartFocus = useCallback(() => {
    if (open) setOpen(false);
    if (openMobile) setOpenMobile(false);

    // Auto-subscribe and request data on first chart interaction
    if (!hasAutoRequested.current && wsStatus === "open") {
      hasAutoRequested.current = true;
      if (!subscribedTickers.has(wsTicker)) {
        subscribeTicker(wsTicker);
        localStorage.setItem("lastTicker", wsTicker);
      }
      requestClusters(wsTicker);
    }
  }, [open, openMobile, setOpen, setOpenMobile, wsStatus, subscribedTickers, wsTicker, subscribeTicker, requestClusters]);

  // Compute chart data from WebSocket
  // Include lastCandle in deps to trigger updates when new candles arrive
  const chartData = useMemo(() => {
    return wsCandiesToChartCandles(getCandles(wsTicker));
  }, [getCandles, wsTicker, lastCandle]);

  return (
    <div className="relative flex w-full min-h-screen bg-zinc-50 font-sans dark:bg-black">
      <AppSidebar />

      <CustomTrigger />

      {/* main */}
      <div className="w-full h-[calc(100vh-32px)] flex gap-4 m-4">
        <div className="flex-1">
          <div className="flex h-full flex-col rounded-xl border border-zinc-800 bg-zinc-950/40 p-4 backdrop-blur">
            <div className="flex items-center justify-between pb-4">
              <h2 className="text-base font-semibold text-zinc-100">
                Price Chart
              </h2>
              <div className="flex items-center gap-2">
                <span className="text-xs text-zinc-400">{wsTicker}</span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => setExtraOpen(true)}
                >
                  <Settings className="h-4 w-4 text-zinc-400" />
                </Button>
              </div>
            </div>

            <div className="relative flex-1" onPointerDown={handleChartFocus}>
              <PriceChart
                data={chartData}
                showMidpoint={showMidpoint}
                phaseAmount={phase}
                lineColor={lineColor}
                bgUpColor={bgUpColor}
                bgDownColor={bgDownColor}
                transparency={transparency}
                candleUpColor={candleUpColor}
                candleDownColor={candleDownColor}
                candleBorderUp={candleBorderUp}
                candleBorderDown={candleBorderDown}
                clusters={getClusters(wsTicker)}
                clusterOpacity={clusterOpacity}
                clusterBullishColor={clusterBullishColor}
                clusterBearishColor={clusterBearishColor}
                clusterBorderColor={clusterBorderColor}
                clusterBorderStyle={clusterBorderStyle}
              />
            </div>
          </div>
        </div>

        <div className="w-[360px] space-y-4">
          {/* Connection Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Connection</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-3">
                <Label htmlFor="wsTicker">Ticker</Label>
                <div className="flex gap-2">
                  <Input
                    id="wsTicker"
                    value={wsTicker}
                    onChange={(e) =>
                      setWsTicker(e.target.value.toUpperCase())
                    }
                    placeholder="NQ=F"
                    className="font-mono"
                  />
                  {subscribedTickers.has(wsTicker) ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => unsubscribeTicker(wsTicker)}
                    >
                      Unsub
                    </Button>
                  ) : (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => {
                        subscribeTicker(wsTicker);
                        localStorage.setItem("lastTicker", wsTicker);
                      }}
                      disabled={wsStatus !== "open"}
                    >
                      Sub
                    </Button>
                  )}
                </div>
                <div className="space-y-1">
                  <div className="flex items-center gap-2 text-xs">
                    <Badge
                      variant="outline"
                      className={
                        wsStatus === "open"
                          ? "text-emerald-400 border-emerald-400/30"
                          : wsStatus === "connecting"
                            ? "text-amber-400 border-amber-400/30"
                            : "text-zinc-400 border-zinc-400/30"
                      }
                    >
                      {wsStatus}
                    </Badge>
                    <span className="text-zinc-400">
                      {getCandles(wsTicker).length} candles
                    </span>
                  </div>
                  {wsStatus !== "open" && (
                    <div className="text-xs text-amber-400">
                      Server not connected. Start the server with: python
                      main.py
                    </div>
                  )}
                  {wsStatus === "open" &&
                    getCandles(wsTicker).length === 0 &&
                    subscribedTickers.has(wsTicker) && (
                      <div className="text-xs text-zinc-500">
                        Waiting for candles... (server polls every 60s)
                      </div>
                    )}
                </div>
                <Separator className="bg-zinc-800" />
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={() => requestClusters(wsTicker)}
                  disabled={wsStatus !== "open"}
                >
                  Get Clusters
                </Button>

                {/* Auto-fetch toggle - only show if we have clusters */}
                {getClusters(wsTicker) && (
                  <div className="flex items-center justify-between pt-2">
                    <Label className="text-sm text-zinc-200">
                      Auto-refresh (5 min)
                    </Label>
                    <Switch
                      checked={isAutoClustersEnabled(wsTicker)}
                      onCheckedChange={(checked) => {
                        if (checked) {
                          enableAutoClusters(wsTicker);
                        } else {
                          disableAutoClusters(wsTicker);
                        }
                      }}
                      disabled={wsStatus !== "open"}
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>

          {/* Clusters Display */}
          <div className="max-h-[calc(100vh-420px)]">
            <ClustersDisplay ticker={wsTicker} data={getClusters(wsTicker)} />
          </div>
        </div>
      </div>
      <ExtraSettingsDialog
        open={extraOpen}
        onOpenChange={setExtraOpen}
        // Chart behavior
        phaseAmount={phase}
        setPhaseAmount={setPhaseAmount}
        showMidpoint={showMidpoint}
        setShowMidpoint={setShowMidpoint}
        // Cluster colors
        clusterOpacity={clusterOpacity}
        setClusterOpacity={setClusterOpacity}
        clusterBullishColor={clusterBullishColor}
        setClusterBullishColor={setClusterBullishColor}
        clusterBearishColor={clusterBearishColor}
        setClusterBearishColor={setClusterBearishColor}
        clusterBorderColor={clusterBorderColor}
        setClusterBorderColor={setClusterBorderColor}
        clusterBorderStyle={clusterBorderStyle}
        setClusterBorderStyle={setClusterBorderStyle}
        // Midpoint colors
        lineColor={lineColor}
        setLineColor={(val) =>
          setSettings((prev) => ({ ...prev, lineColor: val }))
        }
        bgUpColor={bgUpColor}
        setBgUpColor={(val) =>
          setSettings((prev) => ({ ...prev, bgUpColor: val }))
        }
        bgDownColor={bgDownColor}
        setBgDownColor={(val) =>
          setSettings((prev) => ({ ...prev, bgDownColor: val }))
        }
        transparency={transparency}
        setTransparency={(val) =>
          setSettings((prev) => ({ ...prev, transparency: val }))
        }
        // Candle colors
        candleUpColor={candleUpColor}
        setCandleUpColor={(val) =>
          setSettings((prev) => ({ ...prev, candleUpColor: val }))
        }
        candleDownColor={candleDownColor}
        setCandleDownColor={(val) =>
          setSettings((prev) => ({ ...prev, candleDownColor: val }))
        }
        candleBorderUp={candleBorderUp}
        setCandleBorderUp={(val) =>
          setSettings((prev) => ({ ...prev, candleBorderUp: val }))
        }
        candleBorderDown={candleBorderDown}
        setCandleBorderDown={(val) =>
          setSettings((prev) => ({ ...prev, candleBorderDown: val }))
        }
        // Presets
        savedPresets={savedPresets}
        onRefreshPresets={fetchPresets}
        onApplyPreset={applyPreset}
      />
    </div>
  );
}

type Props = {
  data: Candle[];
  showMidpoint: boolean;
  phaseAmount: number;
  lineColor: string;
  bgUpColor: string;
  bgDownColor: string;
  transparency: number; // 0 opaque, 100 invisible
  candleUpColor: string;
  candleDownColor: string;
  candleBorderUp: string;
  candleBorderDown: string;
  clusters?: ClustersData | null;
  clusterOpacity?: number; // 10-80%, controls cluster rectangle transparency
  clusterBullishColor?: string;
  clusterBearishColor?: string;
  clusterBorderColor?: string;
  clusterBorderStyle?: "solid" | "dashed" | "none";
};

const colorWithAlpha = (hex: string, alpha: number) => {
  // supports #rrggbb or #rrggbbaa
  const normalized = hex.startsWith("#") ? hex.slice(1) : hex;
  const hasAlpha = normalized.length === 8;
  const r = parseInt(normalized.slice(0, 2), 16);
  const g = parseInt(normalized.slice(2, 4), 16);
  const b = parseInt(normalized.slice(4, 6), 16);
  const a = hasAlpha ? parseInt(normalized.slice(6, 8), 16) / 255 : alpha;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
};

export function PriceChart({
  data,
  showMidpoint,
  phaseAmount,
  lineColor,
  bgUpColor,
  bgDownColor,
  transparency,
  candleUpColor,
  candleDownColor,
  candleBorderUp,
  candleBorderDown,
  clusters,
  clusterOpacity = 25,
  clusterBullishColor = "#22c55e",
  clusterBearishColor = "#ef4444",
  clusterBorderColor = "#ffffff",
  clusterBorderStyle = "dashed",
}: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const midpointSeriesRef = useRef<ReturnType<IChartApi["addSeries"]> | null>(
    null,
  );
  const clusterPrimitiveRef = useRef<ClusterRectanglesPrimitive | null>(null);
  const bgColorRef = useRef<string | null>(null);
  const [currentBg, setCurrentBg] = useState<string | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const chartHeight = containerRef.current.clientHeight || 400;
    const chart = createChart(containerRef.current, {
      width: containerRef.current.clientWidth,
      height: chartHeight,
      layout: {
        background: { color: "#050816" },
        textColor: "#e5e7eb",
      },
      grid: {
        vertLines: { color: "#111827" },
        horzLines: { color: "#111827" },
      },
      timeScale: { borderVisible: false },
      rightPriceScale: { borderVisible: false },
    });

    chartRef.current = chart;

    // --- candles ---
    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: candleUpColor,
      downColor: candleDownColor,
      wickUpColor: candleUpColor,
      wickDownColor: candleDownColor,
      borderUpColor: candleBorderUp,
      borderDownColor: candleBorderDown,
    });
    candleSeriesRef.current = candleSeries;

    // --- midpoint line: (high + low) / 2 for each bar ---
    const midpointSeries = chart.addSeries(LineSeries, {
      color: lineColor,
      lineWidth: 2,
      lineType: LineType.WithSteps,
    });
    midpointSeriesRef.current = midpointSeries;

    // --- cluster rectangles primitive ---
    const clusterPrimitive = new ClusterRectanglesPrimitive();
    candleSeries.attachPrimitive(clusterPrimitive);
    clusterPrimitiveRef.current = clusterPrimitive;

    // responsive resize
    const ro = new ResizeObserver((entries) => {
      if (!chartRef.current) return;
      for (const entry of entries) {
        if (entry.target === containerRef.current) {
          const { width, height } = entry.contentRect;
          chartRef.current.applyOptions({ width, height });
        }
      }
    });

    ro.observe(containerRef.current);

    return () => {
      ro.disconnect();
      chart.remove();
      candleSeriesRef.current = null;
      midpointSeriesRef.current = null;
      clusterPrimitiveRef.current = null;
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    midpointSeriesRef.current?.applyOptions({
      color: lineColor,
      lineWidth: 2,
      lineType: LineType.WithSteps,
    });
  }, [lineColor]);

  useEffect(() => {
    if (!candleSeriesRef.current) return;
    candleSeriesRef.current.applyOptions({
      upColor: candleUpColor,
      downColor: candleDownColor,
      wickUpColor: candleUpColor,
      wickDownColor: candleDownColor,
      borderUpColor: candleBorderUp,
      borderDownColor: candleBorderDown,
    });
  }, [candleUpColor, candleDownColor, candleBorderUp, candleBorderDown]);

  useEffect(() => {
    if (!candleSeriesRef.current || !midpointSeriesRef.current) return;

    candleSeriesRef.current.setData(data as BarData[]);

    const safePhase = Number.isFinite(phaseAmount)
      ? Math.max(1, Math.floor(phaseAmount))
      : 1;

    const midpointData = calculateDonchianMidpoint({
      phaseAmount: safePhase,
      data,
    });

    midpointSeriesRef.current.setData(midpointData);

    const alpha = Math.max(0, Math.min(100, transparency));
    const opacity = (100 - alpha) / 100;
    const up = colorWithAlpha(bgUpColor, opacity);
    const down = colorWithAlpha(bgDownColor, opacity);

    if (midpointData.length >= 1) {
      const prevIndex = midpointData.length - 2;
      const currIndex = midpointData.length - 1;
      const prev = prevIndex >= 0 ? midpointData[prevIndex].value : null;
      const curr = midpointData[currIndex]?.value ?? null;
      let nextBg = bgColorRef.current ?? up;
      if (prev != null && curr != null) {
        if (curr > prev) nextBg = up;
        else if (curr < prev) nextBg = down;
      }
      if (nextBg !== bgColorRef.current) {
        bgColorRef.current = nextBg;
        setCurrentBg(nextBg);
      }
    }
  }, [data, phaseAmount, bgUpColor, bgDownColor, transparency]);

  useEffect(() => {
    midpointSeriesRef.current?.applyOptions({ visible: showMidpoint });
  }, [showMidpoint]);

  // Draw cluster rectangles
  useEffect(() => {
    if (!clusterPrimitiveRef.current) return;

    if (!clusters?.clusters?.length || data.length === 0) {
      clusterPrimitiveRef.current.clearRectangles();
      return;
    }

    // Helper to convert hex to RGB string
    const hexToRgb = (hex: string): string => {
      const normalized = hex.startsWith("#") ? hex.slice(1) : hex;
      const r = parseInt(normalized.slice(0, 2), 16);
      const g = parseInt(normalized.slice(2, 4), 16);
      const b = parseInt(normalized.slice(4, 6), 16);
      return `${r}, ${g}, ${b}`;
    };

    // Use the first candle time as the start time for rectangles
    const firstCandle = data[0];
    const startTime = (
      typeof firstCandle.time === "number"
        ? firstCandle.time
        : Math.floor(new Date(firstCandle.time as string).getTime() / 1000)
    ) as Time;

    // Convert clusterOpacity (10-80%) to decimal (0.1-0.8)
    const opacity = clusterOpacity / 100;

    const rects: ClusterRect[] = clusters.clusters.map((cluster, idx) => {
      const isBullish = cluster.side === "bullish";
      const baseColor = isBullish
        ? hexToRgb(clusterBullishColor)
        : hexToRgb(clusterBearishColor);
      const label = `${isBullish ? "▲" : "▼"} ${cluster.count} (${cluster.unique_scenarios} scenarios)`;

      return {
        id: `cluster-${idx}`,
        low: cluster.low,
        high: cluster.high,
        startTime,
        color: `rgba(${baseColor}, ${opacity})`,
        side: cluster.side,
        label,
        borderColor: clusterBorderColor,
        borderStyle: clusterBorderStyle,
      };
    });

    clusterPrimitiveRef.current.setRectangles(rects);
  }, [clusters, data, clusterOpacity, clusterBullishColor, clusterBearishColor, clusterBorderColor, clusterBorderStyle]);

  return (
    <div className="absolute inset-0 h-full w-full min-h-[480px]">
      <div ref={containerRef} className="relative h-full w-full" />
      {currentBg && (
        <div
          className="pointer-events-none absolute inset-0 transition-colors duration-300"
          style={{ backgroundColor: currentBg }}
        />
      )}
    </div>
  );
}
