"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Settings } from "lucide-react";
import { toast } from "sonner";

import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { useSidebar } from "@/components/ui/sidebar";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { calculateDonchianMidpoint, type Candle } from "@/lib/calcDonchianMid";
import {
  ExtraSettingsDialog,
  type ChartSettingsData,
  type SavedPreset,
} from "@/components/nchartS";
import {
  useTradingData,
  isSierraSymbol,
  type WSCandle,
  type ClustersData,
} from "@/lib/websocket";
import { ClustersDisplay } from "@/components/websocket";
import type {
  UTCTimestamp,
  Time,
  ISeriesApi,
  TickMarkFormatter,
} from "lightweight-charts";

import {
  createChart,
  IChartApi,
  BarData,
  LineSeries,
  CandlestickSeries,
  LineType,
  TickMarkType,
} from "lightweight-charts";

// Format timestamp to US Eastern time (CME exchange time)
// TradingView uses UTC internally, so we use a custom formatter to display Eastern time
function formatToEasternTime(
  timestamp: number,
  tickMarkType: TickMarkType,
): string {
  // timestamp is in seconds (UTC)
  const date = new Date(timestamp * 1000);

  // Format in America/New_York timezone
  const options: Intl.DateTimeFormatOptions = { timeZone: "America/New_York" };

  switch (tickMarkType) {
    case TickMarkType.Year:
      return date.toLocaleString("en-US", { ...options, year: "numeric" });
    case TickMarkType.Month:
      return date.toLocaleString("en-US", { ...options, month: "short" });
    case TickMarkType.DayOfMonth:
      return date.toLocaleString("en-US", { ...options, day: "numeric" });
    case TickMarkType.Time:
      return date.toLocaleString("en-US", {
        ...options,
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      });
    case TickMarkType.TimeWithSeconds:
      return date.toLocaleString("en-US", {
        ...options,
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });
    default:
      return date.toLocaleString("en-US", {
        ...options,
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      });
  }
}

// Custom tick mark formatter for Eastern time
const easternTimeFormatter: TickMarkFormatter = (
  time,
  tickMarkType,
  _locale,
) => {
  const timestamp =
    typeof time === "number" ? time : new Date(time as string).getTime() / 1000;
  return formatToEasternTime(timestamp, tickMarkType);
};
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
  const [phase, setPhaseAmount] = useState(240);
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

  // Ticker with localStorage persistence
  const [wsTicker, setWsTicker] = useState(() => {
    if (typeof window !== "undefined") {
      return localStorage.getItem("phaseshifter_ticker") || "NQ";
    }
    return "NQ";
  });

  // Save ticker to localStorage when it changes
  useEffect(() => {
    if (typeof window !== "undefined") {
      localStorage.setItem("phaseshifter_ticker", wsTicker);
    }
  }, [wsTicker]);

  // Determine if current ticker uses Sierra/Rust server
  const isSierra = useMemo(() => isSierraSymbol(wsTicker), [wsTicker]);

  // Cluster display settings
  const [clusterOpacity, setClusterOpacity] = useState(67); // 0-100%
  const [clusterBullishColor, setClusterBullishColor] = useState("#22c55e");
  const [clusterBearishColor, setClusterBearishColor] = useState("#ef4444");
  const [clusterBorderColor, setClusterBorderColor] = useState("#ffffff");
  const [clusterBorderStyle, setClusterBorderStyle] = useState<
    "solid" | "dashed" | "none"
  >("none");
  const [clusterAlerts, setClusterAlerts] = useState(false); // off by default

  // Pushcat webhook URL with localStorage persistence
  const [pushcatWebhookUrl, setPushcatWebhookUrl] = useState(() => {
    if (typeof window !== "undefined") {
      return localStorage.getItem("phaseshifter_pushcat_webhook") || "";
    }
    return "";
  });

  // Save webhook URL to localStorage when it changes
  useEffect(() => {
    if (typeof window !== "undefined") {
      localStorage.setItem("phaseshifter_pushcat_webhook", pushcatWebhookUrl);
    }
  }, [pushcatWebhookUrl]);

  // Preset management
  const [savedPresets, setSavedPresets] = useState<SavedPreset[]>([]);

  // Fetch presets from API
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

  // Apply preset callback
  const applyPreset = useCallback((preset: ChartSettingsData) => {
    setPhaseAmount(preset.phaseAmount);
    setShowMidpoint(preset.showMidpoint);
    setClusterOpacity(preset.clusterOpacity);
    setClusterBullishColor(preset.clusterBullishColor);
    setClusterBearishColor(preset.clusterBearishColor);
    setClusterBorderColor(preset.clusterBorderColor);
    setClusterBorderStyle(preset.clusterBorderStyle);
    if (preset.clusterAlerts !== undefined) {
      setClusterAlerts(preset.clusterAlerts);
    }
    if (preset.pushcatWebhookUrl !== undefined) {
      setPushcatWebhookUrl(preset.pushcatWebhookUrl);
    }
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
    pythonStatus,
    getStatusForTicker,
    subscribedTickers,
    subscribeTicker,
    unsubscribeTicker,
    getClusters,
    requestClusters,
    enableAutoClusters,
    disableAutoClusters,
    isAutoClustersEnabled,
    connectedSymbols,
    tickerData,
  } = useTradingData();

  // Get the appropriate connection status for current ticker
  const connectionStatus = getStatusForTicker(wsTicker);

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

  // Auto-select first connected symbol when server FIRST connects (not on every ticker change)
  const hasAutoSelectedRef = useRef(false);
  useEffect(() => {
    if (connectedSymbols.length > 0 && !hasAutoSelectedRef.current) {
      // Only auto-select if current ticker is empty or the default "NQ" on first load
      if (!wsTicker || wsTicker === "NQ") {
        setWsTicker(connectedSymbols[0]);
      }
      hasAutoSelectedRef.current = true;
    }
  }, [connectedSymbols, wsTicker]);

  // Get candles for current ticker - extract from tickerData for reactivity
  const tickerInfo = tickerData.get(wsTicker);
  const wsCandles = tickerInfo?.candles ?? [];
  const lastTickPrice = tickerInfo?.lastTick?.price ?? null;

  // Compute chart data from WebSocket candles
  const chartData = useMemo(() => {
    return wsCandiesToChartCandles(wsCandles);
  }, [wsCandles]);

  // Get current price for display - use lastTick price directly
  const currentPrice =
    lastTickPrice ??
    (wsCandles.length > 0 ? wsCandles[wsCandles.length - 1]?.close : null);

  // Get clusters once for both chart and alerts
  const clusters = useMemo(
    () => getClusters(wsTicker),
    [getClusters, wsTicker],
  );

  // Cluster zone alerts - track which clusters price is currently inside
  const inClusterRef = useRef<Set<string>>(new Set());
  const lastAlertTimeRef = useRef<number>(0);
  const initializedRef = useRef<boolean>(false);
  const prevTickerForAlertsRef = useRef<string>("");
  const prevClustersRef = useRef<typeof clusters>(null);

  // Cluster zone alert - check when price changes
  useEffect(() => {
    if (
      !clusterAlerts ||
      currentPrice === null ||
      !clusters?.clusters?.length
    ) {
      return;
    }

    // Reset on ticker change
    if (prevTickerForAlertsRef.current !== wsTicker) {
      prevTickerForAlertsRef.current = wsTicker;
      inClusterRef.current.clear();
      initializedRef.current = false;
    }

    // Reset on clusters change (phase flip created new clusters)
    if (prevClustersRef.current !== clusters) {
      prevClustersRef.current = clusters;
      inClusterRef.current.clear();
      initializedRef.current = false;
    }

    // Single pass: check all clusters, build set of current keys
    const nowInKeys = new Set<string>();
    let enteredCluster: {
      key: string;
      side: string;
      low: number;
      high: number;
    } | null = null;

    for (const cluster of clusters.clusters) {
      const key = `${cluster.side}:${cluster.low.toFixed(2)}-${cluster.high.toFixed(2)}`;

      if (currentPrice >= cluster.low && currentPrice <= cluster.high) {
        nowInKeys.add(key);

        // Check if new entry (not first run, not already tracked)
        if (
          initializedRef.current &&
          !inClusterRef.current.has(key) &&
          !enteredCluster
        ) {
          enteredCluster = {
            key,
            side: cluster.side,
            low: cluster.low,
            high: cluster.high,
          };
        }
      }
    }

    // First run: record state, no alerts
    if (!initializedRef.current) {
      initializedRef.current = true;
      inClusterRef.current = nowInKeys;
      return;
    }

    // Update tracking state
    inClusterRef.current = nowInKeys;

    // Alert on entry (debounced)
    if (enteredCluster) {
      const now = Date.now();
      if (now - lastAlertTimeRef.current >= 1000) {
        lastAlertTimeRef.current = now;
        const side = enteredCluster.side === "bullish" ? "Bullish" : "Bearish";

        // Play sound
        const audio = new Audio("/notification.mp3");
        audio.volume = 1;
        audio.play().catch(() => {}); // Ignore if audio fails

        toast.info(`${wsTicker} entered ${side} zone`, {
          description: `Price ${currentPrice.toFixed(2)} in cluster ${enteredCluster.low.toFixed(2)} - ${enteredCluster.high.toFixed(2)}`,
          duration: 5000,
        });

        // Send Pushcat notification if webhook URL is configured
        if (pushcatWebhookUrl) {
          const title = `${wsTicker} ${side} Zone`;
          const body = `Price ${currentPrice.toFixed(2)} entered cluster ${enteredCluster.low.toFixed(2)} - ${enteredCluster.high.toFixed(2)}`;
          fetch(pushcatWebhookUrl, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ title, body }),
          }).catch(() => {}); // Ignore errors silently
        }
      }
    }
  }, [clusterAlerts, currentPrice, clusters, wsTicker, pushcatWebhookUrl]);

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
              <div className="flex items-center gap-3">
                {currentPrice !== null && (
                  <span className="text-lg font-mono font-semibold text-emerald-400">
                    ${currentPrice.toFixed(2)}
                  </span>
                )}
                <span className="text-xs text-zinc-400">
                  {wsTicker} ({isSierra ? "Sierra" : "yfinance"})
                </span>
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
                ticker={wsTicker}
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
                clusters={clusters}
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
          {/* Data Source Card */}
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
              <CardTitle className="text-base">Data Source</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-3">
                <Label htmlFor="wsTicker">Ticker</Label>
                <div className="flex gap-2">
                  <Input
                    id="wsTicker"
                    value={wsTicker}
                    onChange={(e) => setWsTicker(e.target.value.toUpperCase())}
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
                      disabled={connectionStatus !== "open"}
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
                        connectionStatus === "open"
                          ? "text-emerald-400 border-emerald-400/30"
                          : connectionStatus === "connecting"
                            ? "text-amber-400 border-amber-400/30"
                            : "text-zinc-400 border-zinc-400/30"
                      }
                    >
                      {connectionStatus}
                    </Badge>
                    <Badge
                      variant="outline"
                      className={
                        isSierra
                          ? "text-blue-400 border-blue-400/30"
                          : "text-purple-400 border-purple-400/30"
                      }
                    >
                      {isSierra ? "Sierra" : "yfinance"}
                    </Badge>
                    <span className="text-zinc-400">
                      {wsCandles.length} candles
                    </span>
                    {currentPrice !== null && (
                      <span className="text-emerald-400 font-mono">
                        ${currentPrice.toFixed(2)}
                      </span>
                    )}
                  </div>
                  {connectionStatus !== "open" && (
                    <div className="text-xs text-amber-400">
                      {isSierra
                        ? "Start Rust server: cd BACKEND/phaseshifter-server && cargo run --release -- --symbols NQ,ES"
                        : "Start Python server: cd BACKEND/server && python main.py"}
                    </div>
                  )}
                  {connectionStatus === "open" &&
                    wsCandles.length === 0 &&
                    subscribedTickers.has(wsTicker) && (
                      <div className="text-xs text-zinc-500">
                        {isSierra
                          ? "Waiting for Sierra Chart data..."
                          : "Waiting for yfinance data (updates every 60s)..."}
                      </div>
                    )}
                </div>
                <Separator className="bg-zinc-800" />
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={() => requestClusters(wsTicker)}
                  disabled={connectionStatus !== "open"}
                >
                  Get Clusters
                </Button>

                {/* Auto-fetch toggle - only show if we have clusters */}
                {clusters && (
                  <div className="flex items-center justify-between pt-2">
                    <Label className="text-sm text-zinc-200">
                      Auto-refresh (on bar close)
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
                      disabled={connectionStatus !== "open"}
                    />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>

          {/* Clusters Display */}
          <div className="flex-1 overflow-auto">
            <ClustersDisplay ticker={wsTicker} data={clusters} />
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
        // Cluster settings
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
        clusterAlerts={clusterAlerts}
        setClusterAlerts={setClusterAlerts}
        // Notifications
        pushcatWebhookUrl={pushcatWebhookUrl}
        setPushcatWebhookUrl={setPushcatWebhookUrl}
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
        // Save/Load presets
        savedPresets={savedPresets}
        onRefreshPresets={fetchPresets}
        onApplyPreset={applyPreset}
      />
    </div>
  );
}

type Props = {
  data: Candle[];
  ticker: string; // Used to detect ticker changes for auto-centering
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
  ticker,
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
  const prevTickerRef = useRef<string>(ticker);
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
      timeScale: {
        borderVisible: false,
        rightOffset: 100, // Empty space on right for new candles
        timeVisible: true, // Show time (hours:minutes) on the scale
        tickMarkFormatter: easternTimeFormatter, // Display in US Eastern time
      },
      rightPriceScale: {
        borderVisible: false,
        autoScale: true, // Auto-fit price range
      },
      localization: {
        // Format crosshair tooltip time in Eastern time
        timeFormatter: (time: number) => {
          const date = new Date(time * 1000);
          return (
            date.toLocaleString("en-US", {
              timeZone: "America/New_York",
              month: "short",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
              hour12: false,
            }) + " ET"
          );
        },
      },
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
    if (
      !candleSeriesRef.current ||
      !midpointSeriesRef.current ||
      !chartRef.current
    )
      return;

    if (data.length === 0) return;

    // Sort data by time and remove duplicates (required by TradingView charts)
    const getTimeValue = (time: string | number): number => {
      return typeof time === "number" ? time : new Date(time).getTime() / 1000;
    };

    const sortedData = [...data]
      .sort((a, b) => getTimeValue(a.time) - getTimeValue(b.time))
      .filter((candle, index, arr) => {
        if (index === 0) return true;
        return getTimeValue(candle.time) > getTimeValue(arr[index - 1].time);
      });

    if (sortedData.length === 0) return;

    // Always use setData for simplicity and consistency (matches working implementation)
    candleSeriesRef.current.setData(sortedData as BarData[]);

    const safePhase = Number.isFinite(phaseAmount)
      ? Math.max(1, Math.floor(phaseAmount))
      : 1;

    const midpointData = calculateDonchianMidpoint({
      phaseAmount: safePhase,
      data: sortedData,
    });

    // For midpoint, always use setData since it's calculated data
    // (TradingView doesn't support update for derived series as efficiently)
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

  // Auto-center chart when ticker changes - adjust both time and price scales
  useEffect(() => {
    if (prevTickerRef.current !== ticker) {
      prevTickerRef.current = ticker;
      const timer = setTimeout(() => {
        if (chartRef.current && candleSeriesRef.current) {
          // Reset price scale to fit new ticker's price range
          chartRef.current.priceScale("right").applyOptions({
            autoScale: true,
          });
          // Set right offset for empty space on right side
          chartRef.current.timeScale().applyOptions({
            rightOffset: 100,
          });
          // Fit all content (shows all candles + right offset)
          chartRef.current.timeScale().fitContent();
        }
      }, 200);
      return () => clearTimeout(timer);
    }
  }, [ticker]);

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

    // Helper to convert hex color to rgba with opacity
    const hexToRgba = (hex: string, alpha: number): string => {
      const normalized = hex.startsWith("#") ? hex.slice(1) : hex;
      const r = parseInt(normalized.slice(0, 2), 16);
      const g = parseInt(normalized.slice(2, 4), 16);
      const b = parseInt(normalized.slice(4, 6), 16);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    };

    const rects: ClusterRect[] = clusters.clusters.map((cluster, idx) => {
      const isBullish = cluster.side === "bullish";
      const baseColor = isBullish ? clusterBullishColor : clusterBearishColor;
      const label = `${isBullish ? "▲" : "▼"} ${cluster.count} (${cluster.unique_scenarios} scenarios)`;

      return {
        id: `cluster-${idx}`,
        low: cluster.low,
        high: cluster.high,
        startTime,
        color: hexToRgba(baseColor, opacity),
        side: cluster.side,
        label,
        borderColor:
          clusterBorderStyle !== "none" ? clusterBorderColor : undefined,
        borderStyle: clusterBorderStyle,
      };
    });

    clusterPrimitiveRef.current.setRectangles(rects);
  }, [
    clusters,
    data,
    clusterOpacity,
    clusterBullishColor,
    clusterBearishColor,
    clusterBorderColor,
    clusterBorderStyle,
  ]);

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
