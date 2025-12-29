"use client";

import {
  createContext,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useWebSocket } from "./useWebSocket";
import {
  type WSCandle,
  type WSTick,
  type ClustersData,
  type WSMessage,
  type PhaseUpdateMessage,
  type OpenNodeInfo,
  parseServerMessage,
  isCandleMessage,
  isTickMessage,
  isClustersMessage,
  isSubscribedMessage,
  isUnsubscribedMessage,
  isErrorMessage,
  isAutoClustersEnabledMessage,
  isAutoClustersDisabledMessage,
  isConnectedMessage,
  isBarUpdateMessage,
  isBarClosedMessage,
  isPhaseUpdateMessage,
  isNodeCreatedMessage,
  isHistoryMessage,
} from "./types";

const STORAGE_KEY = "phaseshifter_subscriptions";

// Sierra/Rust symbols - these use the Rust WebSocket server with live Sierra Chart data
const SIERRA_SYMBOLS = ["NQ", "ES", "YM"];

/**
 * Check if a ticker should use the Sierra/Rust server
 * NQ, ES, YM → Rust server (live Sierra Chart data)
 * Everything else → yfinance data (with automatic 1m/5m fallback)
 */
export function isSierraSymbol(ticker: string): boolean {
  const normalized = ticker.toUpperCase().replace(/[^A-Z]/g, "");
  return SIERRA_SYMBOLS.some(
    (s) => normalized === s || normalized.startsWith(s),
  );
}

export interface TickerData {
  candles: WSCandle[];
  clusters: ClustersData | null;
  lastUpdated: number | null;
  lastTick: WSTick | null;
  // Phase engine data
  phase: "bullish" | "bearish" | null;
  anchor: number | null;
  dm: number | null;
  openNodes: OpenNodeInfo[];
}

export type ConnectionStatus = "open" | "closed" | "connecting" | "error";

export interface TradingDataContextValue {
  // Dual-mode detection
  isSierraSymbol: (ticker: string) => boolean;

  // Subscription management
  subscribedTickers: Set<string>;
  subscribeTicker: (ticker: string) => void;
  unsubscribeTicker: (ticker: string) => void;
  requestClusters: (ticker: string) => void;

  // Auto-clusters management
  enableAutoClusters: (ticker: string) => void;
  disableAutoClusters: (ticker: string) => void;
  isAutoClustersEnabled: (ticker: string) => boolean;

  // Data access
  tickerData: Map<string, TickerData>;
  getCandles: (ticker: string) => WSCandle[];
  getClusters: (ticker: string) => ClustersData | null;
  getLastTick: (ticker: string) => WSTick | null;
  getCurrentPrice: (ticker: string) => number | null;

  // Phase engine data access
  getPhase: (ticker: string) => "bullish" | "bearish" | null;
  getAnchor: (ticker: string) => number | null;
  getOpenNodes: (ticker: string) => OpenNodeInfo[];

  // Latest events
  lastCandle: { ticker: string; candle: WSCandle } | null;
  lastTick: { ticker: string; tick: WSTick } | null;
  lastClusters: { ticker: string; data: ClustersData } | null;
  lastPhase: PhaseUpdateMessage | null;
  lastError: string | null;

  // Connection info (Rust server)
  connectedSymbols: string[];
  clientId: number | null;

  // Connection state - Rust server (Sierra symbols)
  status: ReturnType<typeof useWebSocket>["status"];
  lastPingMs: number | null;

  // Connection state - Python server (yfinance symbols)
  pythonStatus: ConnectionStatus;

  // Helper to get appropriate status for a ticker
  getStatusForTicker: (ticker: string) => ConnectionStatus;

  // Settings
  maxCandlesPerTicker: number;
  setMaxCandlesPerTicker: (n: number) => void;
}

export const TradingDataContext = createContext<TradingDataContextValue | null>(
  null,
);

// Helper to load subscriptions from localStorage
function loadSavedSubscriptions(): Set<string> {
  if (typeof window === "undefined") return new Set();
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      const arr = JSON.parse(saved);
      if (Array.isArray(arr)) {
        return new Set(arr.filter((t) => typeof t === "string"));
      }
    }
  } catch {
    // Ignore parse errors
  }
  return new Set();
}

// Helper to save subscriptions to localStorage
function saveSubscriptions(tickers: Set<string>) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(Array.from(tickers)));
  } catch {
    // Ignore storage errors
  }
}

interface TradingDataProviderProps {
  children: ReactNode;
  maxCandles?: number;
  pythonServerUrl?: string;
}

export function TradingDataProvider({
  children,
  maxCandles = 500,
  pythonServerUrl = "ws://localhost:8001",
}: TradingDataProviderProps) {
  const { status, lastPingMs, send, subscribe, connect } = useWebSocket();

  // Python server connection state
  const [pythonSocket, setPythonSocket] = useState<WebSocket | null>(null);
  const [pythonStatus, setPythonStatus] = useState<"open" | "closed" | "connecting" | "error">("closed");
  const pythonReconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Connect to Python server
  const connectPython = useCallback(() => {
    if (pythonSocket?.readyState === WebSocket.OPEN || pythonSocket?.readyState === WebSocket.CONNECTING) {
      return;
    }

    setPythonStatus("connecting");
    const ws = new WebSocket(pythonServerUrl);

    ws.onopen = () => {
      setPythonStatus("open");
      console.log("[Python WS] Connected to", pythonServerUrl);
    };

    ws.onerror = () => {
      setPythonStatus("error");
    };

    ws.onclose = () => {
      setPythonStatus("closed");
      console.log("[Python WS] Disconnected, reconnecting in 5s...");
      pythonReconnectTimer.current = setTimeout(connectPython, 5000);
    };

    ws.onmessage = (event) => {
      handlePythonMessage(event.data);
    };

    setPythonSocket(ws);
  }, [pythonServerUrl]);

  // Send to Python server
  const sendToPython = useCallback((data: string | object) => {
    if (pythonSocket?.readyState === WebSocket.OPEN) {
      const payload = typeof data === "string" ? data : JSON.stringify(data);
      pythonSocket.send(payload);
    }
  }, [pythonSocket]);

  // Ref for maxCandles to use in callbacks
  const maxCandlesRef = useRef(maxCandles);

  // Helper to create default ticker data (defined early for use in callbacks)
  const createDefaultTickerData = useCallback((): TickerData => ({
    candles: [],
    clusters: null,
    lastUpdated: null,
    lastTick: null,
    phase: null,
    anchor: null,
    dm: null,
    openNodes: [],
  }), []);

  // Handle Python server messages (defined early for use in connectPython)
  const handlePythonMessage = useCallback((rawData: string) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(rawData);
    } catch {
      return;
    }
    if (!parsed || typeof parsed !== "object") return;

    const msg = parsed as Record<string, unknown>;

    // Python server message types
    if (msg.type === "candle") {
      const ticker = msg.ticker as string;
      const data = msg.data as {
        time: number;
        open: number;
        high: number;
        low: number;
        close: number;
        volume: number;
      };
      const candle: WSCandle = {
        time: data.time,
        open: data.open,
        high: data.high,
        low: data.low,
        close: data.close,
        volume: data.volume,
      };

      setLastCandle({ ticker, candle });

      setTickerData((prev) => {
        const next = new Map(prev);
        const existing = next.get(ticker) ?? createDefaultTickerData();
        const newCandles = [...existing.candles, candle];
        if (newCandles.length > maxCandlesRef.current) {
          newCandles.shift();
        }
        next.set(ticker, {
          ...existing,
          candles: newCandles,
          lastUpdated: Date.now(),
        });
        return next;
      });
    } else if (msg.type === "clusters") {
      const ticker = msg.ticker as string;
      const data = msg.data as ClustersData;

      setLastClusters({ ticker, data });

      setTickerData((prev) => {
        const next = new Map(prev);
        const existing = next.get(ticker) ?? createDefaultTickerData();
        next.set(ticker, {
          ...existing,
          clusters: data,
          lastUpdated: Date.now(),
        });
        return next;
      });
    } else if (msg.type === "subscribed") {
      const ticker = msg.ticker as string;
      setConfirmedTickers((prev) => new Set(prev).add(ticker));
    } else if (msg.type === "unsubscribed") {
      const ticker = msg.ticker as string;
      setConfirmedTickers((prev) => {
        const next = new Set(prev);
        next.delete(ticker);
        return next;
      });
    } else if (msg.type === "error") {
      setLastError(msg.message as string);
    } else if (msg.type === "auto_clusters_enabled") {
      setAutoClustersEnabled((prev) => new Set(prev).add(msg.ticker as string));
    } else if (msg.type === "auto_clusters_disabled") {
      setAutoClustersEnabled((prev) => {
        const next = new Set(prev);
        next.delete(msg.ticker as string);
        return next;
      });
    }
  }, [createDefaultTickerData]);

  // Track which tickers we want to be subscribed to (persisted)
  const [desiredTickers, setDesiredTickers] = useState<Set<string>>(
    loadSavedSubscriptions,
  );
  // Track which tickers the server has confirmed (kept for future use)
  const [, setConfirmedTickers] = useState<Set<string>>(() => new Set());
  const [tickerData, setTickerData] = useState<Map<string, TickerData>>(
    () => new Map(),
  );
  const [lastCandle, setLastCandle] = useState<{
    ticker: string;
    candle: WSCandle;
  } | null>(null);
  const [lastTick, setLastTick] = useState<{
    ticker: string;
    tick: WSTick;
  } | null>(null);
  const [lastClusters, setLastClusters] = useState<{
    ticker: string;
    data: ClustersData;
  } | null>(null);
  const [lastPhase, setLastPhase] = useState<PhaseUpdateMessage | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [maxCandlesPerTicker, setMaxCandlesPerTicker] = useState(maxCandles);
  const [autoClustersEnabled, setAutoClustersEnabled] = useState<Set<string>>(
    () => new Set(),
  );

  // Rust server connection info
  const [connectedSymbols, setConnectedSymbols] = useState<string[]>([]);
  const [clientId, setClientId] = useState<number | null>(null);

  // Keep maxCandlesRef in sync
  useEffect(() => {
    maxCandlesRef.current = maxCandlesPerTicker;
  }, [maxCandlesPerTicker]);

  // Save desired tickers to localStorage whenever they change
  useEffect(() => {
    saveSubscriptions(desiredTickers);
  }, [desiredTickers]);

  // Connect to Python server on mount
  useEffect(() => {
    connectPython();

    return () => {
      if (pythonReconnectTimer.current) {
        clearTimeout(pythonReconnectTimer.current);
      }
      pythonSocket?.close();
    };
  }, [connectPython]);

  // Re-subscribe to desired tickers when Rust connection opens
  const prevStatusRef = useRef(status);
  useEffect(() => {
    const wasNotOpen = prevStatusRef.current !== "open";
    const isNowOpen = status === "open";
    prevStatusRef.current = status;

    if (wasNotOpen && isNowOpen && desiredTickers.size > 0) {
      // Connection just opened, re-subscribe Sierra symbols to Rust server
      desiredTickers.forEach((ticker) => {
        if (isSierraSymbol(ticker)) {
          send({ type: "subscribe", ticker });
        }
      });
    }
  }, [status, desiredTickers, send]);

  // Re-subscribe to Python server when it connects
  const prevPythonStatusRef = useRef(pythonStatus);
  useEffect(() => {
    const wasNotOpen = prevPythonStatusRef.current !== "open";
    const isNowOpen = pythonStatus === "open";
    prevPythonStatusRef.current = pythonStatus;

    if (wasNotOpen && isNowOpen && desiredTickers.size > 0) {
      // Python connection just opened, re-subscribe non-Sierra symbols
      desiredTickers.forEach((ticker) => {
        if (!isSierraSymbol(ticker)) {
          sendToPython({ type: "subscribe", ticker });
        }
      });
    }
  }, [pythonStatus, desiredTickers, sendToPython]);

  // Subscribe to all WebSocket messages and route by type
  useEffect(() => {
    const unsubscribe = subscribe(
      () => true, // Accept all messages
      (msg: WSMessage) => {
        const parsed = parseServerMessage(msg.data);
        if (!parsed) {
          console.log(`[WS] Unparseable message:`, msg.data);
          return;
        }

        // Log all message types for debugging
        const msgType = (parsed as { type?: string }).type;
        if (msgType && !["tick"].includes(msgType)) {
          // Skip tick spam, but log everything else
          console.log(`[WS] Message type: ${msgType}`);
        }

        // ============================================
        // Rust Server Messages
        // ============================================

        if (isConnectedMessage(parsed)) {
          // Handle connection from Rust server
          console.log(`[WS] connected: client_id=${parsed.client_id}, symbols=${parsed.symbols.join(", ")}`);
          setClientId(parsed.client_id);
          setConnectedSymbols(parsed.symbols);
          // Auto-add connected symbols to desired tickers and request history
          parsed.symbols.forEach((symbol) => {
            setDesiredTickers((prev) => new Set(prev).add(symbol));
            // Request historical bars for 1m timeframe (server expects lowercase)
            console.log(`[WS] Requesting history for ${symbol}`);
            send({ type: "get_history", symbol, timeframe: "1m", limit: 500 });
          });
        } else if (isTickMessage(parsed)) {
          // Rust server tick format: { type: "tick", symbol, price, volume, timestamp, bid, ask }
          const symbol = parsed.symbol;
          const tick: WSTick = {
            price: parsed.price,
            volume: parsed.volume,
            timestamp: parsed.timestamp,
            bid: parsed.bid,
            ask: parsed.ask,
          };

          setLastTick({ ticker: symbol, tick });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();

            // Update the last (current) candle with tick data
            const candles = [...existing.candles];
            if (candles.length > 0) {
              const lastCandle = { ...candles[candles.length - 1] };
              lastCandle.high = Math.max(lastCandle.high, tick.price);
              lastCandle.low = Math.min(lastCandle.low, tick.price);
              lastCandle.close = tick.price;
              lastCandle.volume += tick.volume;
              candles[candles.length - 1] = lastCandle;
            } else {
              // No candles yet, create a synthetic one from the tick
              candles.push({
                time: tick.timestamp,
                open: tick.price,
                high: tick.price,
                low: tick.price,
                close: tick.price,
                volume: tick.volume,
              });
            }

            next.set(symbol, {
              ...existing,
              candles,
              lastTick: tick,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isBarUpdateMessage(parsed)) {
          // Rust server bar_update: in-progress bar changed
          // Only process 1m timeframe for the chart (server sends lowercase "1m")
          console.log(`[WS] bar_update received: symbol=${parsed.symbol}, timeframe=${parsed.timeframe}, time=${parsed.time}`);
          if (parsed.timeframe !== "1m") {
            console.log(`[WS] bar_update SKIPPED: timeframe ${parsed.timeframe} !== "1m"`);
            return;
          }

          const symbol = parsed.symbol;
          const candle: WSCandle = {
            time: parsed.time,
            open: parsed.open,
            high: parsed.high,
            low: parsed.low,
            close: parsed.close,
            volume: parsed.volume,
          };

          // Update lastTick for live price display
          const tick: WSTick = {
            price: parsed.close,
            volume: parsed.volume,
            timestamp: parsed.time,
            bid: parsed.close,
            ask: parsed.close,
          };
          setLastTick({ ticker: symbol, tick });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();
            const candles = [...existing.candles];

            const lastTime =
              candles.length > 0 ? candles[candles.length - 1].time : null;
            const action = lastTime === candle.time ? "UPDATE" : lastTime && lastTime < candle.time ? "PUSH" : candles.length === 0 ? "FIRST" : "SKIP_OLD";
            console.log(
              `[BAR_UPDATE] ${symbol} - rawTime=${candle.time}, lastRawTime=${lastTime}, ` +
              `newDate=${new Date(candle.time).toISOString()}, lastDate=${lastTime ? new Date(lastTime).toISOString() : "none"}, ` +
              `candleCount=${candles.length}, action=${action}`,
            );

            // Update or add the current bar
            if (
              candles.length > 0 &&
              candles[candles.length - 1].time === candle.time
            ) {
              candles[candles.length - 1] = candle;
            } else if (
              candles.length > 0 &&
              candles[candles.length - 1].time < candle.time
            ) {
              candles.push(candle);
              if (candles.length > maxCandlesRef.current) {
                candles.shift();
              }
            } else if (candles.length === 0) {
              candles.push(candle);
            } else {
              console.warn(
                `[BAR_UPDATE] ${symbol} - SKIPPED: lastTime=${lastTime}, candleTime=${candle.time}`,
              );
            }

            next.set(symbol, {
              ...existing,
              candles,
              lastTick: tick,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isBarClosedMessage(parsed)) {
          // Rust server bar_closed: bar completed, new bar started
          // Only process 1m timeframe for the chart (server sends lowercase "1m")
          console.log(`[WS] bar_closed received: symbol=${parsed.symbol}, timeframe=${parsed.timeframe}, time=${parsed.time}, OHLC=[${parsed.open}, ${parsed.high}, ${parsed.low}, ${parsed.close}]`);
          if (parsed.timeframe !== "1m") {
            console.log(`[WS] bar_closed SKIPPED: timeframe ${parsed.timeframe} !== "1m"`);
            return;
          }

          const symbol = parsed.symbol;
          const candle: WSCandle = {
            time: parsed.time,
            open: parsed.open,
            high: parsed.high,
            low: parsed.low,
            close: parsed.close,
            volume: parsed.volume,
          };

          setLastCandle({ ticker: symbol, candle });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();
            const candles = [...existing.candles];

            const lastCandleTime = candles.length > 0 ? candles[candles.length - 1].time : null;
            console.log(
              `[BAR_CLOSED] ${symbol} - New bar time: ${candle.time} (${new Date(candle.time).toISOString()}), ` +
              `Last candle time: ${lastCandleTime} (${lastCandleTime ? new Date(lastCandleTime).toISOString() : 'none'}), ` +
              `Total candles: ${candles.length}`,
            );

            // Find and update or append the closed bar
            const existingIdx = candles.findIndex(
              (c) => c.time === candle.time,
            );
            if (existingIdx >= 0) {
              console.log(
                `[BAR_CLOSED] ${symbol} - Updating existing at index ${existingIdx}`,
              );
              candles[existingIdx] = candle;
            } else {
              // Check if bar time is in the past (should append) or future
              if (lastCandleTime !== null && candle.time < lastCandleTime) {
                console.warn(
                  `[BAR_CLOSED] ${symbol} - SKIPPING: bar time ${candle.time} is OLDER than last candle ${lastCandleTime}`,
                );
              } else {
                console.log(`[BAR_CLOSED] ${symbol} - Appending new bar (index will be ${candles.length})`);
                candles.push(candle);
                if (candles.length > maxCandlesRef.current) {
                  candles.shift();
                }
              }
            }

            next.set(symbol, {
              ...existing,
              candles,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isPhaseUpdateMessage(parsed)) {
          // Rust server phase_update: phase engine state
          const symbol = parsed.symbol;
          setLastPhase(parsed);

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();
            next.set(symbol, {
              ...existing,
              phase: parsed.phase,
              anchor: parsed.anchor,
              dm: parsed.dm,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isNodeCreatedMessage(parsed)) {
          // Rust server node_created: new node was created
          const symbol = parsed.symbol;
          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();
            const newNode: OpenNodeInfo = {
              direction: parsed.direction,
              distance_pct: parsed.distance_pct,
              anchor: parsed.anchor,
              extreme: parsed.extreme,
              created_at: parsed.created_at,
              projected_target:
                parsed.direction === "bullish"
                  ? parsed.anchor * (1 + parsed.distance_pct)
                  : parsed.anchor * (1 - parsed.distance_pct),
            };
            next.set(symbol, {
              ...existing,
              openNodes: [...existing.openNodes, newNode],
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isHistoryMessage(parsed)) {
          // Rust server history: historical bars
          const symbol = parsed.symbol;
          console.log(`[WS] history received: symbol=${symbol}, timeframe=${parsed.timeframe}, bars=${parsed.bars.length}`);
          if (parsed.bars.length > 0) {
            const first = parsed.bars[0];
            const last = parsed.bars[parsed.bars.length - 1];
            console.log(`[WS] history range: first=${new Date(first.time).toISOString()}, last=${new Date(last.time).toISOString()}`);
          }
          const candles: WSCandle[] = parsed.bars.map((bar) => ({
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
          }));

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(symbol) ?? createDefaultTickerData();
            console.log(`[WS] history storing ${candles.length} candles for ${symbol} (had ${existing.candles.length} before)`);
            next.set(symbol, {
              ...existing,
              candles,
              lastUpdated: Date.now(),
            });
            return next;
          });
        }

        // ============================================
        // Legacy Python Server Messages
        // ============================================
        else if (isCandleMessage(parsed)) {
          const { ticker, data: candle } = parsed;

          setLastCandle({ ticker, candle });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(ticker) ?? createDefaultTickerData();

            const newCandles = [...existing.candles, candle];
            if (newCandles.length > maxCandlesRef.current) {
              newCandles.shift();
            }

            next.set(ticker, {
              ...existing,
              candles: newCandles,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isClustersMessage(parsed)) {
          const { ticker, data } = parsed;

          setLastClusters({ ticker, data });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(ticker) ?? createDefaultTickerData();
            next.set(ticker, {
              ...existing,
              clusters: data,
              lastUpdated: Date.now(),
            });
            return next;
          });
        } else if (isSubscribedMessage(parsed)) {
          setConfirmedTickers((prev) => new Set(prev).add(parsed.ticker));
        } else if (isUnsubscribedMessage(parsed)) {
          setConfirmedTickers((prev) => {
            const next = new Set(prev);
            next.delete(parsed.ticker);
            return next;
          });
        } else if (isErrorMessage(parsed)) {
          setLastError(parsed.message);
        } else if (isAutoClustersEnabledMessage(parsed)) {
          setAutoClustersEnabled((prev) => new Set(prev).add(parsed.ticker));
        } else if (isAutoClustersDisabledMessage(parsed)) {
          setAutoClustersEnabled((prev) => {
            const next = new Set(prev);
            next.delete(parsed.ticker);
            return next;
          });
        }
      },
    );

    return unsubscribe;
  }, [subscribe, send]);

  const subscribeTicker = useCallback(
    (ticker: string) => {
      // Add to desired tickers (persisted)
      setDesiredTickers((prev) => new Set(prev).add(ticker));
      // Route to appropriate server based on symbol type
      if (isSierraSymbol(ticker)) {
        // Sierra symbols (NQ, ES) -> Rust server
        if (status === "open") {
          send({ type: "subscribe", ticker });
        }
      } else {
        // Other symbols -> Python server
        if (pythonStatus === "open") {
          sendToPython({ type: "subscribe", ticker });
        }
      }
    },
    [send, status, sendToPython, pythonStatus],
  );

  const unsubscribeTicker = useCallback(
    (ticker: string) => {
      // Remove from desired tickers (persisted)
      setDesiredTickers((prev) => {
        const next = new Set(prev);
        next.delete(ticker);
        return next;
      });
      // Route to appropriate server based on symbol type
      if (isSierraSymbol(ticker)) {
        // Sierra symbols (NQ, ES) -> Rust server
        if (status === "open") {
          send({ type: "unsubscribe", ticker });
        }
      } else {
        // Other symbols -> Python server
        if (pythonStatus === "open") {
          sendToPython({ type: "unsubscribe", ticker });
        }
      }
    },
    [send, status, sendToPython, pythonStatus],
  );

  const requestClusters = useCallback(
    (ticker: string) => {
      // Route to appropriate server based on symbol type
      if (isSierraSymbol(ticker)) {
        send({ type: "get_clusters", ticker });
      } else {
        sendToPython({ type: "get_clusters", ticker });
      }
    },
    [send, sendToPython],
  );

  const enableAutoClusters = useCallback(
    (ticker: string) => {
      // Only supported on Python server for now
      if (!isSierraSymbol(ticker)) {
        sendToPython({ type: "enable_auto_clusters", ticker });
      }
    },
    [sendToPython],
  );

  const disableAutoClusters = useCallback(
    (ticker: string) => {
      // Only supported on Python server for now
      if (!isSierraSymbol(ticker)) {
        sendToPython({ type: "disable_auto_clusters", ticker });
      }
    },
    [sendToPython],
  );

  const isAutoClustersEnabledFn = useCallback(
    (ticker: string): boolean => {
      return autoClustersEnabled.has(ticker);
    },
    [autoClustersEnabled],
  );

  const getCandles = useCallback(
    (ticker: string): WSCandle[] => {
      return tickerData.get(ticker)?.candles ?? [];
    },
    [tickerData],
  );

  const getClusters = useCallback(
    (ticker: string): ClustersData | null => {
      return tickerData.get(ticker)?.clusters ?? null;
    },
    [tickerData],
  );

  const getLastTick = useCallback(
    (ticker: string): WSTick | null => {
      return tickerData.get(ticker)?.lastTick ?? null;
    },
    [tickerData],
  );

  const getCurrentPrice = useCallback(
    (ticker: string): number | null => {
      const data = tickerData.get(ticker);
      if (data?.lastTick) {
        return data.lastTick.price;
      }
      if (data?.candles?.length) {
        return data.candles[data.candles.length - 1].close;
      }
      return null;
    },
    [tickerData],
  );

  const getPhase = useCallback(
    (ticker: string): "bullish" | "bearish" | null => {
      return tickerData.get(ticker)?.phase ?? null;
    },
    [tickerData],
  );

  const getAnchor = useCallback(
    (ticker: string): number | null => {
      return tickerData.get(ticker)?.anchor ?? null;
    },
    [tickerData],
  );

  const getOpenNodes = useCallback(
    (ticker: string): OpenNodeInfo[] => {
      return tickerData.get(ticker)?.openNodes ?? [];
    },
    [tickerData],
  );

  const getStatusForTicker = useCallback(
    (ticker: string): ConnectionStatus => {
      if (isSierraSymbol(ticker)) {
        return status as ConnectionStatus;
      }
      return pythonStatus;
    },
    [status, pythonStatus],
  );

  const value: TradingDataContextValue = {
    isSierraSymbol,
    subscribedTickers: desiredTickers,
    subscribeTicker,
    unsubscribeTicker,
    requestClusters,
    enableAutoClusters,
    disableAutoClusters,
    isAutoClustersEnabled: isAutoClustersEnabledFn,
    tickerData,
    getCandles,
    getClusters,
    getLastTick,
    getCurrentPrice,
    getPhase,
    getAnchor,
    getOpenNodes,
    lastCandle,
    lastTick,
    lastClusters,
    lastPhase,
    lastError,
    connectedSymbols,
    clientId,
    status,
    lastPingMs,
    pythonStatus,
    getStatusForTicker,
    maxCandlesPerTicker,
    setMaxCandlesPerTicker,
  };

  return (
    <TradingDataContext.Provider value={value}>
      {children}
    </TradingDataContext.Provider>
  );
}
