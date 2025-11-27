// lib/parseCsv.ts
import type { UTCTimestamp } from "lightweight-charts";
import type { Candle } from "@/lib/calcDonchianMid";

export function parseCsvToCandles(csv: string): Candle[] {
  const lines = csv.trim().split("\n");
  if (lines.length <= 1) return [];

  // skip header
  const [, ...rows] = lines;

  const candles: Candle[] = rows
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const [timestamp, open, high, low, close, volume] = line.split(",");

      const time = Math.floor(
        new Date(timestamp).getTime() / 1000,
      ) as unknown as UTCTimestamp;

      return {
        time,
        open: parseFloat(open),
        high: parseFloat(high),
        low: parseFloat(low),
        close: parseFloat(close),
        volume: parseFloat(volume),
      };
    });

  return candles;
}
