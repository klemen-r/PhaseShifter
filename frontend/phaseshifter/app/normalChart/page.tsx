"use client";
import { useCallback, useEffect, useRef, useState } from "react";

import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { useSidebar } from "@/components/ui/sidebar";
import { Input } from "@/components/ui/input";
import { calculateDonchianMidpoint, type Candle } from "@/lib/calcDonchianMid";
import { parseCsvToCandles } from "@/lib/parseCsv";
import { ExtraSettingsDialog } from "@/components/nchartS";

import {
  createChart,
  IChartApi,
  BarData,
  LineSeries,
  CandlestickSeries,
  LineType,
} from "lightweight-charts";

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
  const [chartData, setChartData] = useState<Candle[]>(sampleData);
  const [settings, setSettings] = useState<LineSettings>({
    lineColor: "#800080",
    bgUpColor: "#006400",
    bgDownColor: "#8b0000",
    transparency: 96,
    candleUpColor: "#22c55e",
    candleDownColor: "#ef4444",
    candleBorderUp: "#22c55e",
    candleBorderDown: "#ef4444",
  });
  const [extraOpen, setExtraOpen] = useState(false);

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
          setChartData(parsed);
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
              <div className="text-xs text-zinc-400">Sample feed</div>
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
              />
            </div>
          </div>
        </div>

        <div className="w-[360px] space-y-4">
          <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
            <CardHeader className="pb-3">
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
}: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ReturnType<IChartApi["addSeries"]> | null>(
    null,
  );
  const midpointSeriesRef = useRef<ReturnType<IChartApi["addSeries"]> | null>(
    null,
  );
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
      lineType: LineType.Step,
    });
    midpointSeriesRef.current = midpointSeries;

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
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    midpointSeriesRef.current?.applyOptions({
      color: lineColor,
      lineWidth: 2,
      lineType: LineType.Step,
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
