"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

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
import { parseCsvToCandles } from "@/lib/parseCsv";
import { ExtraSettingsDialog } from "@/components/nchartS";
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

const sampleData: Candle[] = [
  { time: "2024-11-18", open: 100, high: 110, low: 95, close: 105 },
  { time: "2024-11-19", open: 105, high: 112, low: 101, close: 108 },
  { time: "2024-11-20", open: 108, high: 115, low: 107, close: 112 },
  { time: "2024-11-21", open: 112, high: 118, low: 110, close: 116 },
  { time: "2024-11-22", open: 116, high: 120, low: 113, close: 114 },
];

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
  const [csvData, setCsvData] = useState<Candle[]>(sampleData);
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

  // WebSocket data source
  const [dataSource, setDataSource] = useState<"csv" | "websocket">("csv");
  const [wsTicker, setWsTicker] = useState("NQ=F");

  // Cluster display settings
  const [clusterOpacity, setClusterOpacity] = useState(25); // 10-80%

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

  const handleChartFocus = useCallback(() => {
    if (open) setOpen(false);
    if (openMobile) setOpenMobile(false);
  }, [open, openMobile, setOpen, setOpenMobile]);

  // Load CSV data
  useEffect(() => {
    let active = true;

    const loadCsv = async () => {
      try {
        const paths = ["/nq_5m.csv"];
        let text: string | null = null;

        for (const path of paths) {
          const res = await fetch(path, { cache: "no-store" });
          if (res.ok) {
            text = await res.text();
            break;
          }
        }

        if (!text) throw new Error("Failed to load CSV data");

        const parsed = parseCsvToCandles(text);
        if (active && parsed.length > 0) {
          setCsvData(parsed);
        }
      } catch (err) {
        console.error(err);
      }
    };

    loadCsv();

    return () => {
      active = false;
    };
  }, []);

  // Subscribe to ticker when switching to WebSocket mode
  useEffect(() => {
    if (dataSource === "websocket" && wsStatus === "open") {
      if (!subscribedTickers.has(wsTicker)) {
        subscribeTicker(wsTicker);
      }
    }
  }, [dataSource, wsStatus, subscribedTickers, subscribeTicker]);

  // Compute chart data based on source
  // Include lastCandle in deps to trigger updates when new candles arrive
  const chartData = useMemo(() => {
    if (dataSource === "csv") {
      return csvData;
    }
    return wsCandiesToChartCandles(getCandles(wsTicker));
  }, [dataSource, csvData, getCandles, lastCandle]);

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
              <div className="text-xs text-zinc-400">
                {dataSource === "csv" ? "CSV File" : `WS: ${wsTicker}`}
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
                clusters={
                  dataSource === "websocket" ? getClusters(wsTicker) : null
                }
                clusterOpacity={clusterOpacity}
              />
            </div>
          </div>
        </div>

        <div className="w-[360px] space-y-4">
          {/* Data Source Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Data Source</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <Button
                  variant={dataSource === "csv" ? "secondary" : "ghost"}
                  size="sm"
                  onClick={() => setDataSource("csv")}
                  className="flex-1"
                >
                  CSV File
                </Button>
                <Button
                  variant={dataSource === "websocket" ? "secondary" : "ghost"}
                  size="sm"
                  onClick={() => setDataSource("websocket")}
                  className="flex-1"
                >
                  WebSocket
                </Button>
              </div>

              {dataSource === "websocket" && (
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
                        onClick={() => subscribeTicker(wsTicker)}
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
              )}
            </CardContent>
          </Card>

          {/* Chart Settings Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="">
              <CardTitle className="text-base">Chart Settings</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <Label htmlFor="phase">Phase Window</Label>
              <Input
                id="phase"
                type="number"
                min={1}
                value={phase}
                onChange={(e) => setPhaseAmount(Number(e.target.value) || 1)}
              />
              <Separator className="bg-zinc-800" />

              <div className="flex items-center justify-between">
                <Label className="text-sm text-zinc-200">Auto-scale</Label>
                <Switch defaultChecked />
              </div>

              <div className="flex items-center justify-between">
                <Label className="text-sm text-zinc-200">
                  Show midpoint line
                </Label>
                <Switch
                  checked={showMidpoint}
                  onCheckedChange={setShowMidpoint}
                />
              </div>

              {/* Cluster Opacity - only shown in WebSocket mode */}
              {dataSource === "websocket" && (
                <>
                  <Separator className="bg-zinc-800" />
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label className="text-sm text-zinc-200">
                        Cluster Opacity
                      </Label>
                      <span className="text-xs text-zinc-400">
                        {clusterOpacity}%
                      </span>
                    </div>
                    <Input
                      type="range"
                      min={10}
                      max={80}
                      value={clusterOpacity}
                      onChange={(e) =>
                        setClusterOpacity(Number(e.target.value))
                      }
                    />
                  </div>
                </>
              )}

              <Separator className="bg-zinc-800" />

              <Button
                variant="secondary"
                className="w-full justify-center"
                onClick={() => setExtraOpen(true)}
              >
                Extra settings
              </Button>
            </CardContent>
          </Card>

          {/* Clusters Display - only when in websocket mode */}
          {dataSource === "websocket" && (
            <div className="max-h-[calc(100vh-777px)]">
              <ClustersDisplay ticker={wsTicker} data={getClusters(wsTicker)} />
            </div>
          )}
        </div>
      </div>
      <ExtraSettingsDialog
        open={extraOpen}
        onOpenChange={setExtraOpen}
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
      // === COLOR: RGB values for cluster fill ===
      // Green (34, 197, 94) for bullish, Red (239, 68, 68) for bearish
      const baseColor = isBullish ? "34, 197, 94" : "239, 68, 68";
      const label = `${isBullish ? "▲" : "▼"} ${cluster.count} (${cluster.unique_scenarios} scenarios)`;

      return {
        id: `cluster-${idx}`,
        low: cluster.low,
        high: cluster.high,
        startTime,
        // === COLOR: Fill with configurable opacity from slider ===
        color: `rgba(${baseColor}, ${opacity})`,
        side: cluster.side,
        label,
      };
    });

    clusterPrimitiveRef.current.setRectangles(rects);
  }, [clusters, data, clusterOpacity]);

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
