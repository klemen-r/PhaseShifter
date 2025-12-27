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

  // Connection state (pass-through from useWebSocket)
  status: ReturnType<typeof useWebSocket>["status"];
  lastPingMs: number | null;

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
}

export function TradingDataProvider({
  children,
  maxCandles = 500,
}: TradingDataProviderProps) {
  const { status, lastPingMs, send, subscribe } = useWebSocket();

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

  const maxCandlesRef = useRef(maxCandlesPerTicker);
  useEffect(() => {
    maxCandlesRef.current = maxCandlesPerTicker;
  }, [maxCandlesPerTicker]);

  // Save desired tickers to localStorage whenever they change
  useEffect(() => {
    saveSubscriptions(desiredTickers);
  }, [desiredTickers]);

  // Re-subscribe to desired tickers when connection opens
  const prevStatusRef = useRef(status);
  useEffect(() => {
    const wasNotOpen = prevStatusRef.current !== "open";
    const isNowOpen = status === "open";
    prevStatusRef.current = status;

    if (wasNotOpen && isNowOpen && desiredTickers.size > 0) {
      // Connection just opened, re-subscribe to all desired tickers
      desiredTickers.forEach((ticker) => {
        // Python server uses 'ticker' field, not 'symbol'
        send({ type: "subscribe", ticker });
      });
    }
  }, [status, desiredTickers, send]);

  // Helper to create default ticker data
  const createDefaultTickerData = (): TickerData => ({
    candles: [],
    clusters: null,
    lastUpdated: null,
    lastTick: null,
    phase: null,
    anchor: null,
    dm: null,
    openNodes: [],
  });

  // Subscribe to all WebSocket messages and route by type
  useEffect(() => {
    const unsubscribe = subscribe(
      () => true, // Accept all messages
      (msg: WSMessage) => {
        const parsed = parseServerMessage(msg.data);
        if (!parsed) return;

        // ============================================
        // Rust Server Messages
        // ============================================

        if (isConnectedMessage(parsed)) {
          // Handle connection from Rust server
          setClientId(parsed.client_id);
          setConnectedSymbols(parsed.symbols);
          // Auto-add connected symbols to desired tickers and request history
          parsed.symbols.forEach((symbol) => {
            setDesiredTickers((prev) => new Set(prev).add(symbol));
            // Request historical bars for M1 timeframe
            send({ type: "get_history", symbol, timeframe: "M1", limit: 500 });
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
          // Only process M1 timeframe for the chart
          if (parsed.timeframe !== "1m") return;

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
            console.log(
              `[BAR_UPDATE] ${symbol} - New: ${new Date(candle.time).toISOString()}, Last: ${lastTime ? new Date(lastTime).toISOString() : "none"}, Action: ${lastTime === candle.time ? "UPDATE" : lastTime && lastTime < candle.time ? "PUSH" : "UNKNOWN"}`,
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
          // Only process M1 timeframe for the chart
          if (parsed.timeframe !== "1m") return;

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

            console.log(
              `[BAR_CLOSED] ${symbol} - Time: ${new Date(candle.time).toISOString()}, Candles before: ${candles.length}`,
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
              console.log(`[BAR_CLOSED] ${symbol} - Appending new bar`);
              candles.push(candle);
              if (candles.length > maxCandlesRef.current) {
                candles.shift();
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
      // Send subscribe message if connected
      // Python server uses 'ticker' field, not 'symbol'
      if (status === "open") {
        send({ type: "subscribe", ticker });
      }
    },
    [send, status],
  );

  const unsubscribeTicker = useCallback(
    (ticker: string) => {
      // Remove from desired tickers (persisted)
      setDesiredTickers((prev) => {
        const next = new Set(prev);
        next.delete(ticker);
        return next;
      });
      // Send unsubscribe message if connected
      // Python server uses 'ticker' field, not 'symbol'
      if (status === "open") {
        send({ type: "unsubscribe", ticker });
      }
    },
    [send, status],
  );

  const requestClusters = useCallback(
    (ticker: string) => {
      send({ type: "get_clusters", ticker });
    },
    [send],
  );

  const enableAutoClusters = useCallback(
    (ticker: string) => {
      send({ type: "enable_auto_clusters", ticker });
    },
    [send],
  );

  const disableAutoClusters = useCallback(
    (ticker: string) => {
      send({ type: "disable_auto_clusters", ticker });
    },
    [send],
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
    maxCandlesPerTicker,
    setMaxCandlesPerTicker,
  };

  return (
    <TradingDataContext.Provider value={value}>
      {children}
    </TradingDataContext.Provider>
  );
}
