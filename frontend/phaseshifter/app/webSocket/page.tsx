"use client";
import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { useEffect, useState, useRef, useCallback, useMemo } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
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
import { motion, AnimatePresence } from "framer-motion";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

type WSStatus = "open" | "connecting" | "closed" | "error";
const RECONNECT_INTERVAL_MS = 1500;

export default function WebSocketDebug() {
  const [messages, setMessages] = useState<string[]>([]);
  const [status, setStatus] = useState<WSStatus>("connecting");

  const [ip, setIp] = useState("ws://localhost:");
  const [port, setPort] = useState("8000");
  const [autoReconnect, setAutoReconnect] = useState(true);

  const [lastPingMs, setLastPingMs] = useState<number | null>(null);
  const pingSentAt = useRef<number | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const autoReconnectRef = useRef(autoReconnect);
  const connectRef = useRef<() => void>(() => {});

  const connect = useCallback(() => {
    if (reconnectTimeout.current) clearTimeout(reconnectTimeout.current);
    wsRef.current?.close();

    setStatus("connecting");
    const url = `${ip}${port}`;
    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => setStatus("open");
    ws.onerror = () => setStatus("error");

    ws.onclose = () => {
      setStatus(autoReconnectRef.current ? "connecting" : "closed");
      if (autoReconnectRef.current) {
        reconnectTimeout.current = setTimeout(() => {
          connectRef.current();
        }, RECONNECT_INTERVAL_MS);
      }
    };

    ws.onmessage = (event) => {
      if (event.data === "pong" && pingSentAt.current) {
        setLastPingMs(Date.now() - pingSentAt.current);
        pingSentAt.current = null;
        return;
      }
      setMessages((prev) => [event.data, ...prev]);
    };
  }, [ip, port]);

  useEffect(() => {
    connectRef.current = connect;
  }, [connect]);

  const handleSetAutoReconnect = useCallback(
    (next: boolean) => {
      autoReconnectRef.current = next;
      setAutoReconnect(next);

      if (!next) {
        if (reconnectTimeout.current) {
          clearTimeout(reconnectTimeout.current);
          reconnectTimeout.current = null;
        }
        if (status === "connecting") {
          setStatus("closed");
        }
        return;
      }

      if (status === "closed") {
        connectRef.current();
      }
    },
    [status],
  );

  useEffect(() => {
    // run once on mount
    connectRef.current();

    return () => {
      if (reconnectTimeout.current) clearTimeout(reconnectTimeout.current);
      wsRef.current?.close();
    };
  }, []);
  const ping = () => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      pingSentAt.current = Date.now();
      wsRef.current.send("ping");
    }
  };

  return (
    <div className="flex w-full min-h-screen bg-zinc-50 font-sans dark:bg-black">
      <AppSidebar />
      <CustomTrigger />

      {/* main container */}
      <div className="w-full h-[calc(100vh-32px)] flex gap-4 m-4">
        {/* left: log */}
        <Card className="flex-1 border-zinc-800 bg-zinc-950/40 backdrop-blur">
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Received Packets</CardTitle>
          </CardHeader>
          <CardContent className="h-[calc(100%-56px)]">
            <ScrollArea className="h-full pr-4">
              {messages.length === 0 ? (
                <div className="h-full flex items-center justify-center text-zinc-400">
                  Not connected to any websocket...
                </div>
              ) : (
                messages.map((msg, i) => (
                  <Card key={i} className="mb-2 border-zinc-800 bg-zinc-900/40">
                    <CardContent className="p-3 font-mono text-sm">
                      {msg}
                    </CardContent>
                  </Card>
                ))
              )}
              <ScrollBar />
            </ScrollArea>
          </CardContent>
        </Card>

        {/* right: controls */}
        <div className="w-[360px]">
          <ConnectionCard
            key={`${ip}-${port}`}
            ip={ip}
            port={port}
            status={status}
            lastPingMs={lastPingMs}
            autoReconnect={autoReconnect}
            setIp={setIp}
            setPort={setPort}
            setAutoReconnect={handleSetAutoReconnect}
            onApply={connect}
            onPing={ping}
            onClear={() => setMessages([])}
          />
        </div>
      </div>
    </div>
  );
}

export function ConnectionCard({
  // live values from parent
  ip,
  port,
  status,
  lastPingMs,
  autoReconnect,

  // setters / actions from parent
  setIp,
  setPort,
  setAutoReconnect,
  onApply,
  onPing,
  onClear,
}: {
  ip: string;
  port: string;
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
  // local draft so typing doesn't reconnect every keystroke
  const [draftIp, setDraftIp] = useState(ip);
  const [draftPort, setDraftPort] = useState(port);
  const [dirty, setDirty] = useState(false);

  const url = useMemo(() => `${draftIp}${draftPort}`, [draftIp, draftPort]);

  const statusMeta = {
    open: { label: "Connected", color: "bg-emerald-500", icon: Wifi },
    connecting: { label: "Connecting", color: "bg-amber-500", icon: PlugZap },
    closed: { label: "Closed", color: "bg-zinc-500", icon: WifiOff },
    error: { label: "Error", color: "bg-red-500", icon: WifiOff },
  } as const;

  const SIcon = statusMeta[status].icon;

  return (
    <Card className="h-full w-full bg-zinc-950/40 border-zinc-800 backdrop-blur">
      <CardHeader className="space-y-1">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">Connection Details</CardTitle>

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
        </div>

        <CardDescription className="text-xs text-zinc-400">
          Current URL:{" "}
          <span className="font-mono text-zinc-200">{`${ip}${port}`}</span>
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="ip">WebSocket host</Label>
          <Input
            id="ip"
            value={draftIp}
            placeholder="ws://localhost:"
            onChange={(e) => {
              setDraftIp(e.target.value);
              setDirty(true);
            }}
            className="font-mono"
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="port">Port</Label>
          <Input
            id="port"
            value={draftPort}
            placeholder="8000"
            onChange={(e) => {
              setDraftPort(e.target.value.replace(/[^\d]/g, ""));
              setDirty(true);
            }}
            className="font-mono"
          />
        </div>

        <div className="rounded-md border border-zinc-800 bg-zinc-900/40 p-3 text-xs font-mono text-zinc-200">
          Target: {url}
        </div>

        <Separator className="bg-zinc-800" />

        {/* Actions row */}
        <div className="flex flex-wrap gap-2">
          <Button
            onClick={() => {
              setIp(draftIp);
              setPort(draftPort);
              onApply();
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
                Last ping: <span className="font-mono">{lastPingMs} ms</span>
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
