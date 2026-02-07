"use client";
import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { useState, useMemo } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { Check, PlugZap, RotateCw, Send, Wifi, WifiOff } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { motion, AnimatePresence } from "framer-motion";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  useWebSocket,
  useTradingData,
  isSierraSymbol,
  type WSStatus,
  type ConnectionStatus,
} from "@/lib/websocket";
import {
  SubscriptionControls,
  CandleTable,
  ClustersDisplay,
} from "@/components/websocket";

export default function WebSocketDebug() {
  const {
    status: rustStatus,
    messages,
    lastPingMs,
    autoReconnect,
    url,
    connect,
    ping,
    setAutoReconnect,
    clearMessages,
    setDefaultUrl,
  } = useWebSocket();

  const {
    subscribedTickers,
    subscribeTicker,
    unsubscribeTicker,
    requestClusters,
    getCandles,
    getClusters,
    pythonStatus,
  } = useTradingData();

  const [selectedTicker, setSelectedTicker] = useState<string | null>(null);

  // Extract ip and port from current URL for the UI
  const [draftIp, setDraftIp] = useState(() => {
    const match = url.match(/^(wss?:\/\/[^:]+:?)/);
    return match ? match[1] : "ws://localhost:";
  });
  const [draftPort, setDraftPort] = useState(() => {
    const match = url.match(/:(\d+)$/);
    return match ? match[1] : "8000";
  });

  const handleApply = () => {
    const newUrl = `${draftIp}${draftPort}`;
    setDefaultUrl(newUrl);
    connect(newUrl);
  };

  // Auto-select first ticker when subscriptions change
  const tickersArray = Array.from(subscribedTickers);
  const activeTicker = selectedTicker ?? tickersArray[0] ?? null;

  // Check if at least one server is connected for subscription controls
  const canSubscribe = rustStatus === "open" || pythonStatus === "open";

  return (
    <div className="flex w-full min-h-screen bg-zinc-50 font-sans dark:bg-black">
      <AppSidebar />
      <CustomTrigger />

      {/* main container */}
      <div className="w-full h-[calc(100vh-32px)] flex gap-4 m-4">
        {/* left: main content area */}
        <div className="flex-1 flex flex-col gap-4">
          {/* Subscription Controls */}
          <SubscriptionControls
            subscribedTickers={subscribedTickers}
            onSubscribe={subscribeTicker}
            onUnsubscribe={unsubscribeTicker}
            onRequestClusters={requestClusters}
            disabled={!canSubscribe}
          />

          {/* Ticker selector if multiple tickers */}
          {tickersArray.length > 1 && (
            <div className="flex gap-2 items-center">
              <span className="text-xs text-zinc-400">Viewing:</span>
              {tickersArray.map((ticker) => (
                <Button
                  key={ticker}
                  variant={activeTicker === ticker ? "secondary" : "ghost"}
                  size="sm"
                  onClick={() => setSelectedTicker(ticker)}
                  className="font-mono"
                >
                  {ticker}
                  <Badge
                    variant="outline"
                    className={`ml-1 text-[10px] ${
                      isSierraSymbol(ticker)
                        ? "text-blue-400 border-blue-400/30"
                        : "text-purple-400 border-purple-400/30"
                    }`}
                  >
                    {isSierraSymbol(ticker) ? "Rust" : "Py"}
                  </Badge>
                </Button>
              ))}
            </div>
          )}

          {/* Data Display Tabs */}
          <Tabs defaultValue="candles" className="flex-1">
            <TabsList>
              <TabsTrigger value="candles">Live Candles</TabsTrigger>
              <TabsTrigger value="clusters">Clusters</TabsTrigger>
              <TabsTrigger value="raw">Raw Messages</TabsTrigger>
            </TabsList>

            <TabsContent value="candles">
              {activeTicker ? (
                <CandleTable
                  ticker={activeTicker}
                  candles={getCandles(activeTicker)}
                />
              ) : (
                <Card className="border-zinc-800 bg-zinc-950/40">
                  <CardContent className="py-8 text-center text-zinc-500">
                    Subscribe to a ticker to see live candles
                  </CardContent>
                </Card>
              )}
            </TabsContent>

            <TabsContent value="clusters">
              {activeTicker ? (
                <ClustersDisplay
                  ticker={activeTicker}
                  data={getClusters(activeTicker)}
                />
              ) : (
                <Card className="border-zinc-800 bg-zinc-950/40">
                  <CardContent className="py-8 text-center text-zinc-500">
                    Subscribe to a ticker to see cluster projections
                  </CardContent>
                </Card>
              )}
            </TabsContent>

            <TabsContent value="raw">
              <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur h-[500px]">
                <CardHeader className="pb-2">
                  <CardTitle className="text-base">Raw Messages (Rust Server)</CardTitle>
                </CardHeader>
                <CardContent className="h-[calc(100%-56px)]">
                  <ScrollArea className="h-full pr-4">
                    {messages.length === 0 ? (
                      <div className="h-full flex items-center justify-center text-zinc-400">
                        No messages received...
                      </div>
                    ) : (
                      messages.map((msg) => (
                        <Card
                          key={msg.id}
                          className="mb-2 border-zinc-800 bg-zinc-900/40"
                        >
                          <CardContent className="p-3 font-mono text-sm">
                            {msg.raw}
                          </CardContent>
                        </Card>
                      ))
                    )}
                    <ScrollBar />
                  </ScrollArea>
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
        </div>

        {/* right: controls */}
        <div className="w-[360px] space-y-4">
          {/* Rust Server Connection */}
          <ConnectionCard
            title="Rust Server (Sierra)"
            subtitle="NQ, ES, YM - Live Sierra Chart data"
            key={`rust-${draftIp}-${draftPort}`}
            ip={draftIp}
            port={draftPort}
            currentUrl={url}
            status={rustStatus}
            lastPingMs={lastPingMs}
            autoReconnect={autoReconnect}
            setIp={setDraftIp}
            setPort={setDraftPort}
            setAutoReconnect={setAutoReconnect}
            onApply={handleApply}
            onPing={ping}
            onClear={clearMessages}
          />

          {/* Python Server Connection */}
          <Card className="bg-zinc-950/40 border-zinc-800 backdrop-blur">
            <CardHeader className="space-y-1 pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">Python Server (yfinance)</CardTitle>
                <StatusBadge status={pythonStatus} />
              </div>
              <CardDescription className="text-xs text-zinc-400">
                BTC-USD, SPY, QQQ, etc. - yfinance data (60s polling)
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="rounded-md border border-zinc-800 bg-zinc-900/40 p-3 text-xs font-mono text-zinc-200">
                ws://localhost:8001
              </div>
              <div className="text-xs text-zinc-500">
                {pythonStatus === "open" ? (
                  <span className="text-emerald-400">Connected and ready</span>
                ) : pythonStatus === "connecting" ? (
                  <span className="text-amber-400">Connecting...</span>
                ) : (
                  <span className="text-zinc-400">
                    Start server: cd BACKEND/server && python main.py
                  </span>
                )}
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: ConnectionStatus | WSStatus }) {
  const statusMeta = {
    open: { label: "Connected", color: "bg-emerald-500", icon: Wifi },
    connecting: { label: "Connecting", color: "bg-amber-500", icon: PlugZap },
    closed: { label: "Closed", color: "bg-zinc-500", icon: WifiOff },
    error: { label: "Error", color: "bg-red-500", icon: WifiOff },
  } as const;

  const SIcon = statusMeta[status].icon;

  return (
    <Badge variant="secondary" className="gap-2">
      <motion.span
        className={`inline-block h-2 w-2 rounded-full ${statusMeta[status].color}`}
        animate={
          status === "connecting"
            ? { opacity: [0.3, 1, 0.3] }
            : { opacity: 1 }
        }
        transition={{
          duration: 1.2,
          repeat: status === "connecting" ? Infinity : 0,
        }}
      />
      <SIcon className="h-3.5 w-3.5" />
      {statusMeta[status].label}
    </Badge>
  );
}

export function ConnectionCard({
  title = "Connection Details",
  subtitle,
  ip,
  port,
  currentUrl,
  status,
  lastPingMs,
  autoReconnect,
  setIp,
  setPort,
  setAutoReconnect,
  onApply,
  onPing,
  onClear,
}: {
  title?: string;
  subtitle?: string;
  ip: string;
  port: string;
  currentUrl: string;
  status: WSStatus;
  lastPingMs: number | null;
  autoReconnect: boolean;
  setIp(v: string): void;
  setPort(v: string): void;
  setAutoReconnect(v: boolean): void;
  onApply(): void;
  onPing(): void;
  onClear(): void;
}) {
  const [dirty, setDirty] = useState(false);

  const targetUrl = useMemo(() => `${ip}${port}`, [ip, port]);

  return (
    <Card className="w-full bg-zinc-950/40 border-zinc-800 backdrop-blur">
      <CardHeader className="space-y-1">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">{title}</CardTitle>
          <StatusBadge status={status} />
        </div>

        {subtitle && (
          <CardDescription className="text-xs text-zinc-400">
            {subtitle}
          </CardDescription>
        )}

        <CardDescription className="text-xs text-zinc-400">
          Current URL:{" "}
          <span className="font-mono text-zinc-200">{currentUrl}</span>
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="ip">WebSocket host</Label>
          <Input
            id="ip"
            value={ip}
            placeholder="ws://localhost:"
            onChange={(e) => {
              setIp(e.target.value);
              setDirty(true);
            }}
            className="font-mono"
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="port">Port</Label>
          <Input
            id="port"
            value={port}
            placeholder="8000"
            onChange={(e) => {
              setPort(e.target.value.replace(/[^\d]/g, ""));
              setDirty(true);
            }}
            className="font-mono"
          />
        </div>

        <div className="rounded-md border border-zinc-800 bg-zinc-900/40 p-3 text-xs font-mono text-zinc-200">
          Target: {targetUrl}
        </div>

        <Separator className="bg-zinc-800" />

        {/* Actions row */}
        <div className="flex flex-wrap gap-2">
          <Button
            onClick={() => {
              onApply();
              setDirty(false);
            }}
            disabled={!dirty}
            className="gap-2"
          >
            <RotateCw className="h-4 w-4" />
            Apply / Reconnect
          </Button>

          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="outline"
                  onClick={onPing}
                  disabled={status !== "open"}
                  className="gap-2"
                >
                  <Send className="h-4 w-4" />
                  Ping
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                Send a ping and measure round-trip time
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          <Button variant="ghost" onClick={onClear} className="gap-2">
            <RotateCw className="h-4 w-4" />
            Clear log
          </Button>
        </div>

        {/* Ping status */}
        <AnimatePresence mode="popLayout">
          {lastPingMs != null && (
            <motion.div
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 6 }}
              className="flex items-center gap-2 text-sm"
            >
              <Check className="h-4 w-4 text-emerald-500" />
              <span className="text-zinc-300">
                Last ping: <span className="font-mono">{lastPingMs} µs</span>
              </span>
            </motion.div>
          )}
        </AnimatePresence>

        <Separator className="bg-zinc-800" />

        {/* Auto reconnect */}
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <div className="text-sm font-medium">Auto reconnect</div>
            <div className="text-xs text-zinc-400">
              Retry every 1.5 seconds when connection drops
            </div>
          </div>
          <Switch checked={autoReconnect} onCheckedChange={setAutoReconnect} />
        </div>
      </CardContent>
    </Card>
  );
}
