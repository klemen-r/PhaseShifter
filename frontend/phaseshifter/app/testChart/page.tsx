"use client";

import { useCallback, useEffect, useRef, useState, useMemo } from "react";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  useTradingData,
  type WSCandle,
  type ClustersData,
  type ClusterItem,
} from "@/lib/websocket";
import type { UTCTimestamp, Time, ISeriesApi } from "lightweight-charts";
import {
  createChart,
  IChartApi,
  CandlestickSeries,
  LineSeries,
  LineType,
  ColorType,
} from "lightweight-charts";
import {
  ClusterRectanglesPrimitive,
  type ClusterRect,
} from "@/lib/chart/ClusterRectanglesPrimitive";

// Generate sample candle data for testing
function generateSampleCandles(count: number): WSCandle[] {
  const now = Date.now();
  const interval = 5 * 60 * 1000; // 5 minutes
  const candles: WSCandle[] = [];
  let price = 21500;

  for (let i = 0; i < count; i++) {
    const change = (Math.random() - 0.5) * 100;
    const high = price + Math.abs(change) + Math.random() * 30;
    const low = price - Math.abs(change) - Math.random() * 30;
    const close = price + change;

    candles.push({
      time: now - (count - i) * interval,
      open: price,
      high,
      low,
      close,
      volume: Math.random() * 10000,
    });

    price = close;
  }

  return candles;
}

// Generate sample cluster data for testing
function generateSampleClusters(currentPrice: number): ClustersData {
  const anchor = currentPrice;
  return {
    ticker: "SAMPLE",
    anchor,
    clusters: [
      {
        side: "bullish",
        low: anchor * 1.01,
        high: anchor * 1.015,
        count: 4,
        unique_scenarios: 3,
      },
      {
        side: "bullish",
        low: anchor * 1.025,
        high: anchor * 1.032,
        count: 6,
        unique_scenarios: 4,
      },
      {
        side: "bearish",
        low: anchor * 0.985,
        high: anchor * 0.99,
        count: 3,
        unique_scenarios: 2,
      },
      {
        side: "bearish",
        low: anchor * 0.965,
        high: anchor * 0.975,
        count: 5,
        unique_scenarios: 3,
      },
    ],
    nodes: [],
    generated_at: new Date().toISOString(),
  };
}

// Convert WS candles to chart format
function toChartCandles(wsCandles: WSCandle[]) {
  return wsCandles.map((c) => ({
    time: Math.floor(c.time / 1000) as UTCTimestamp,
    open: c.open,
    high: c.high,
    low: c.low,
    close: c.close,
  }));
}

export default function TestChartPage() {
  const { open, openMobile, setOpen, setOpenMobile } = useSidebar();

  // Chart settings
  const [showClusters, setShowClusters] = useState(true);
  const [showClusterLabels, setShowClusterLabels] = useState(true);
  const [clusterOpacity, setClusterOpacity] = useState(30);
  const [dataSource, setDataSource] = useState<"sample" | "websocket">(
    "sample",
  );
  const [wsTicker, setWsTicker] = useState("NQ");

  // Sample data state
  const [sampleCandles] = useState(() => generateSampleCandles(200));
  const [sampleClusters, setSampleClusters] = useState<ClustersData | null>(
    null,
  );

  // WebSocket data
  const {
    status: wsStatus,
    subscribedTickers,
    subscribeTicker,
    unsubscribeTicker,
    getCandles,
    getClusters,
    requestClusters,
    connectedSymbols,
  } = useTradingData();

  // Initialize sample clusters based on candle data
  useEffect(() => {
    if (sampleCandles.length > 0) {
      const lastPrice = sampleCandles[sampleCandles.length - 1].close;
      setSampleClusters(generateSampleClusters(lastPrice));
    }
  }, [sampleCandles]);

  // Auto-select first connected symbol when server connects
  useEffect(() => {
    if (connectedSymbols.length > 0 && !connectedSymbols.includes(wsTicker)) {
      setWsTicker(connectedSymbols[0]);
    }
  }, [connectedSymbols, wsTicker]);

  // Get data based on source
  const candles = useMemo(() => {
    if (dataSource === "sample") {
      return sampleCandles;
    }
    return getCandles(wsTicker);
  }, [dataSource, sampleCandles, getCandles, wsTicker]);

  const clusters = useMemo(() => {
    if (dataSource === "sample") {
      return sampleClusters;
    }
    return getClusters(wsTicker);
  }, [dataSource, sampleClusters, getClusters, wsTicker]);

  // Subscribe to ticker when switching to WebSocket mode
  useEffect(() => {
    if (dataSource === "websocket" && wsStatus === "open") {
      if (!subscribedTickers.has(wsTicker)) {
        subscribeTicker(wsTicker);
      }
    }
  }, [dataSource, wsStatus, wsTicker, subscribedTickers, subscribeTicker]);

  const handleChartFocus = useCallback(() => {
    if (open) setOpen(false);
    if (openMobile) setOpenMobile(false);
  }, [open, openMobile, setOpen, setOpenMobile]);

  return (
    <div className="relative flex w-full min-h-screen bg-zinc-50 font-sans dark:bg-black">
      <AppSidebar />
      <CustomTrigger />

      <div className="w-full h-[calc(100vh-32px)] flex gap-4 m-4">
        {/* Main Chart Area */}
        <div className="flex-1 flex flex-col gap-4">
          <div className="flex h-full flex-col rounded-xl border border-zinc-800 bg-zinc-950/40 p-4 backdrop-blur">
            <div className="flex items-center justify-between pb-4">
              <h2 className="text-base font-semibold text-zinc-100">
                Lightweight Charts Test
              </h2>
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="font-mono">
                  {dataSource === "sample" ? "Sample Data" : wsTicker}
                </Badge>
                {clusters && (
                  <Badge variant="secondary" className="text-xs">
                    {clusters.clusters.length} clusters
                  </Badge>
                )}
              </div>
            </div>

            <div className="relative flex-1" onPointerDown={handleChartFocus}>
              <TestChart
                candles={candles}
                clusters={clusters}
                showClusters={showClusters}
                showClusterLabels={showClusterLabels}
                clusterOpacity={clusterOpacity}
              />
            </div>
          </div>
        </div>

        {/* Settings Sidebar */}
        <div className="w-[360px] space-y-4 overflow-y-auto">
          {/* Data Source Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Data Source</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <Button
                  variant={dataSource === "sample" ? "secondary" : "ghost"}
                  size="sm"
                  onClick={() => setDataSource("sample")}
                  className="flex-1"
                >
                  Sample
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
                  <div className="flex gap-2">
                    <Input
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
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full"
                    onClick={() => requestClusters(wsTicker)}
                    disabled={wsStatus !== "open"}
                  >
                    Request Clusters
                  </Button>
                </div>
              )}

              {dataSource === "sample" && (
                <div className="text-xs text-zinc-400">
                  Using {sampleCandles.length} sample candles with generated
                  cluster zones for testing.
                </div>
              )}
            </CardContent>
          </Card>

          {/* Cluster Settings Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Cluster Display</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <Label className="text-sm text-zinc-200">Show Clusters</Label>
                <Switch
                  checked={showClusters}
                  onCheckedChange={setShowClusters}
                />
              </div>

              <div className="flex items-center justify-between">
                <Label className="text-sm text-zinc-200">Show Labels</Label>
                <Switch
                  checked={showClusterLabels}
                  onCheckedChange={setShowClusterLabels}
                  disabled={!showClusters}
                />
              </div>

              <Separator className="bg-zinc-800" />

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm text-zinc-200">Opacity</Label>
                  <span className="text-xs text-zinc-400">
                    {clusterOpacity}%
                  </span>
                </div>
                <Input
                  type="range"
                  min={10}
                  max={80}
                  value={clusterOpacity}
                  onChange={(e) => setClusterOpacity(Number(e.target.value))}
                  disabled={!showClusters}
                />
              </div>
            </CardContent>
          </Card>

          {/* Cluster Info Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Cluster Zones</CardTitle>
            </CardHeader>
            <CardContent>
              {clusters ? (
                <ScrollArea className="h-[300px] pr-2">
                  <ClustersList clusters={clusters} />
                </ScrollArea>
              ) : (
                <div className="py-4 text-center text-zinc-500 text-sm">
                  No cluster data available
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

// Chart component with cluster rectangles
interface TestChartProps {
  candles: WSCandle[];
  clusters: ClustersData | null;
  showClusters: boolean;
  showClusterLabels: boolean;
  clusterOpacity: number;
}

function TestChart({
  candles,
  clusters,
  showClusters,
  showClusterLabels,
  clusterOpacity,
}: TestChartProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const clusterPrimitiveRef = useRef<ClusterRectanglesPrimitive | null>(null);

  // Create chart on mount
  useEffect(() => {
    if (!containerRef.current) return;

    const chart = createChart(containerRef.current, {
      width: containerRef.current.clientWidth,
      height: containerRef.current.clientHeight || 500,
      layout: {
        background: { type: ColorType.Solid, color: "#0a0a0f" },
        textColor: "#e5e7eb",
      },
      grid: {
        vertLines: { color: "#1a1a2e" },
        horzLines: { color: "#1a1a2e" },
      },
      timeScale: {
        borderVisible: false,
        timeVisible: true,
        secondsVisible: false,
      },
      rightPriceScale: {
        borderVisible: false,
      },
      crosshair: {
        mode: 1,
        vertLine: {
          color: "#6366f1",
          labelBackgroundColor: "#6366f1",
        },
        horzLine: {
          color: "#6366f1",
          labelBackgroundColor: "#6366f1",
        },
      },
    });

    chartRef.current = chart;

    // Create candle series
    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: "#22c55e",
      downColor: "#ef4444",
      wickUpColor: "#22c55e",
      wickDownColor: "#ef4444",
      borderUpColor: "#22c55e",
      borderDownColor: "#ef4444",
    });
    candleSeriesRef.current = candleSeries;

    // Create cluster rectangles primitive
    const clusterPrimitive = new ClusterRectanglesPrimitive();
    candleSeries.attachPrimitive(clusterPrimitive);
    clusterPrimitiveRef.current = clusterPrimitive;

    // Handle resize
    const ro = new ResizeObserver((entries) => {
      if (!chartRef.current) return;
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        chartRef.current.applyOptions({ width, height });
      }
    });
    ro.observe(containerRef.current);

    return () => {
      ro.disconnect();
      chart.remove();
      chartRef.current = null;
      candleSeriesRef.current = null;
      clusterPrimitiveRef.current = null;
    };
  }, []);

  // Update candle data
  useEffect(() => {
    if (!candleSeriesRef.current || candles.length === 0) return;

    const chartCandles = toChartCandles(candles);
    candleSeriesRef.current.setData(chartCandles);

    // Fit content to view
    if (chartRef.current) {
      chartRef.current.timeScale().fitContent();
    }
  }, [candles]);

  // Update cluster rectangles
  useEffect(() => {
    if (!clusterPrimitiveRef.current) return;

    if (!showClusters || !clusters?.clusters.length || candles.length === 0) {
      clusterPrimitiveRef.current.clearRectangles();
      return;
    }

    const startTime = Math.floor(candles[0].time / 1000) as Time;
    const opacity = clusterOpacity / 100;

    const rects: ClusterRect[] = clusters.clusters.map((cluster, idx) => {
      const isBullish = cluster.side === "bullish";
      const baseColor = isBullish ? "34, 197, 94" : "239, 68, 68";
      const label = showClusterLabels
        ? `${isBullish ? "▲" : "▼"} ${cluster.count} (${cluster.unique_scenarios} scenarios)`
        : undefined;

      return {
        id: `cluster-${idx}`,
        low: cluster.low,
        high: cluster.high,
        startTime,
        color: `rgba(${baseColor}, ${opacity})`,
        side: cluster.side,
        label,
      };
    });

    clusterPrimitiveRef.current.setRectangles(rects);
  }, [clusters, showClusters, showClusterLabels, clusterOpacity, candles]);

  return (
    <div className="absolute inset-0 h-full w-full min-h-[400px]">
      <div ref={containerRef} className="h-full w-full" />
    </div>
  );
}

// Clusters list component
function ClustersList({ clusters }: { clusters: ClustersData }) {
  const formatPrice = (price: number) =>
    price.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });

  const bullish = clusters.clusters.filter((c) => c.side === "bullish");
  const bearish = clusters.clusters.filter((c) => c.side === "bearish");

  return (
    <div className="space-y-4">
      {clusters.anchor && (
        <div className="text-xs text-zinc-400 pb-2">
          Anchor:{" "}
          <span className="font-mono">{formatPrice(clusters.anchor)}</span>
        </div>
      )}

      {bullish.length > 0 && (
        <div>
          <div className="text-sm font-medium text-emerald-400 mb-2">
            Bullish ({bullish.length})
          </div>
          <div className="space-y-2">
            {bullish.map((cluster, idx) => (
              <ClusterCard
                key={`bull-${idx}`}
                cluster={cluster}
                anchor={clusters.anchor}
              />
            ))}
          </div>
        </div>
      )}

      {bearish.length > 0 && (
        <div>
          <div className="text-sm font-medium text-red-400 mb-2">
            Bearish ({bearish.length})
          </div>
          <div className="space-y-2">
            {bearish.map((cluster, idx) => (
              <ClusterCard
                key={`bear-${idx}`}
                cluster={cluster}
                anchor={clusters.anchor}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function ClusterCard({
  cluster,
  anchor,
}: {
  cluster: ClusterItem;
  anchor: number | null;
}) {
  const formatPrice = (price: number) =>
    price.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });

  const midpoint = (cluster.low + cluster.high) / 2;
  const pctFromAnchor = anchor
    ? (((midpoint - anchor) / anchor) * 100).toFixed(2)
    : null;
  const isBullish = cluster.side === "bullish";

  return (
    <div
      className={`rounded border p-2 text-xs ${
        isBullish
          ? "border-emerald-800/50 bg-emerald-950/20"
          : "border-red-800/50 bg-red-950/20"
      }`}
    >
      <div className="flex items-center justify-between mb-1">
        <span className="font-mono">
          {formatPrice(cluster.low)} - {formatPrice(cluster.high)}
        </span>
        {pctFromAnchor && (
          <span className="text-zinc-400">
            {isBullish ? "+" : ""}
            {pctFromAnchor}%
          </span>
        )}
      </div>
      <div className="text-zinc-500">
        {cluster.count} nodes / {cluster.unique_scenarios} scenarios
      </div>
    </div>
  );
}
