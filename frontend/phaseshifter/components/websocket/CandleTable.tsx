"use client";

import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import type { WSCandle } from "@/lib/websocket";

interface CandleTableProps {
  ticker: string;
  candles: WSCandle[];
  maxDisplay?: number;
}

export function CandleTable({
  ticker,
  candles,
  maxDisplay = 50,
}: CandleTableProps) {
  const displayCandles = useMemo(() => {
    return candles.slice(-maxDisplay).reverse();
  }, [candles, maxDisplay]);

  const formatTime = (timestamp: number) => {
    return new Date(timestamp).toLocaleTimeString();
  };

  const formatPrice = (price: number) => {
    return price.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  return (
    <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">
            Live Candles
            <Badge variant="outline" className="ml-2 font-mono">
              {ticker}
            </Badge>
          </CardTitle>
          <span className="text-xs text-zinc-400">{candles.length} total</span>
        </div>
      </CardHeader>
      <CardContent>
        <ScrollArea className="h-[300px]">
          <table className="w-full text-sm font-mono">
            <thead className="sticky top-0 bg-zinc-950">
              <tr className="text-xs text-zinc-400 border-b border-zinc-800">
                <th className="text-left py-2 px-1">Time</th>
                <th className="text-right py-2 px-1">Open</th>
                <th className="text-right py-2 px-1">High</th>
                <th className="text-right py-2 px-1">Low</th>
                <th className="text-right py-2 px-1">Close</th>
                <th className="text-right py-2 px-1">Vol</th>
              </tr>
            </thead>
            <tbody>
              {displayCandles.map((candle, idx) => {
                const isUp = candle.close >= candle.open;
                return (
                  <tr
                    key={`${candle.time}-${idx}`}
                    className="border-b border-zinc-800/50 hover:bg-zinc-800/30"
                  >
                    <td className="py-1.5 px-1 text-zinc-400">
                      {formatTime(candle.time)}
                    </td>
                    <td className="text-right py-1.5 px-1">
                      {formatPrice(candle.open)}
                    </td>
                    <td className="text-right py-1.5 px-1 text-emerald-400">
                      {formatPrice(candle.high)}
                    </td>
                    <td className="text-right py-1.5 px-1 text-red-400">
                      {formatPrice(candle.low)}
                    </td>
                    <td
                      className={`text-right py-1.5 px-1 ${
                        isUp ? "text-emerald-400" : "text-red-400"
                      }`}
                    >
                      {formatPrice(candle.close)}
                    </td>
                    <td className="text-right py-1.5 px-1 text-zinc-500">
                      {candle.volume.toLocaleString()}
                    </td>
                  </tr>
                );
              })}
              {displayCandles.length === 0 && (
                <tr>
                  <td colSpan={6} className="py-8 text-center text-zinc-500">
                    No candles received yet
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}
