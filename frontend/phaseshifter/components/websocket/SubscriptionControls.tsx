"use client";

import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Plus, X, RefreshCw } from "lucide-react";

interface SubscriptionControlsProps {
  subscribedTickers: Set<string>;
  onSubscribe: (ticker: string) => void;
  onUnsubscribe: (ticker: string) => void;
  onRequestClusters: (ticker: string) => void;
  disabled?: boolean;
}

export function SubscriptionControls({
  subscribedTickers,
  onSubscribe,
  onUnsubscribe,
  onRequestClusters,
  disabled = false,
}: SubscriptionControlsProps) {
  const [tickerInput, setTickerInput] = useState("");

  const handleSubscribe = () => {
    const ticker = tickerInput.trim().toUpperCase();
    if (ticker && !subscribedTickers.has(ticker)) {
      onSubscribe(ticker);
      setTickerInput("");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSubscribe();
    }
  };

  return (
    <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
      <CardHeader className="pb-3">
        <CardTitle className="text-base">Ticker Subscriptions</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex gap-2">
          <Input
            placeholder="Enter ticker (e.g., NQ=F)"
            value={tickerInput}
            onChange={(e) => setTickerInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={disabled}
            className="font-mono"
          />
          <Button
            onClick={handleSubscribe}
            disabled={disabled || !tickerInput.trim()}
            size="sm"
          >
            <Plus className="h-4 w-4" />
          </Button>
        </div>

        <div className="space-y-2">
          <div className="text-xs text-zinc-400">
            Active Subscriptions ({subscribedTickers.size})
          </div>
          <div className="flex flex-wrap gap-2">
            {Array.from(subscribedTickers).map((ticker) => (
              <Badge
                key={ticker}
                variant="secondary"
                className="gap-1 pl-2 pr-1 py-1"
              >
                <span className="font-mono">{ticker}</span>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-4 w-4 p-0 hover:bg-zinc-700"
                  onClick={() => onRequestClusters(ticker)}
                  title="Request clusters"
                >
                  <RefreshCw className="h-3 w-3" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-4 w-4 p-0 hover:bg-red-500/20"
                  onClick={() => onUnsubscribe(ticker)}
                  title="Unsubscribe"
                >
                  <X className="h-3 w-3" />
                </Button>
              </Badge>
            ))}
            {subscribedTickers.size === 0 && (
              <span className="text-xs text-zinc-500">
                No active subscriptions
              </span>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
