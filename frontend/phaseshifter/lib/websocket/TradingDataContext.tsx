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
  type ClustersData,
  type WSMessage,
  parseServerMessage,
  isCandleMessage,
  isClustersMessage,
  isSubscribedMessage,
  isUnsubscribedMessage,
  isErrorMessage,
} from "./types";

const STORAGE_KEY = "phaseshifter_subscriptions";

export interface TickerData {
  candles: WSCandle[];
  clusters: ClustersData | null;
  lastUpdated: number | null;
}

export interface TradingDataContextValue {
  // Subscription management
  subscribedTickers: Set<string>;
  subscribeTicker: (ticker: string) => void;
  unsubscribeTicker: (ticker: string) => void;
  requestClusters: (ticker: string) => void;

  // Data access
  tickerData: Map<string, TickerData>;
  getCandles: (ticker: string) => WSCandle[];
  getClusters: (ticker: string) => ClustersData | null;

  // Latest events
  lastCandle: { ticker: string; candle: WSCandle } | null;
  lastClusters: { ticker: string; data: ClustersData } | null;
  lastError: string | null;

  // Connection state (pass-through from useWebSocket)
  status: ReturnType<typeof useWebSocket>["status"];
  lastPingMs: number | null;

  // Settings
  maxCandlesPerTicker: number;
  setMaxCandlesPerTicker: (n: number) => void;
}

export const TradingDataContext = createContext<TradingDataContextValue | null>(
  null
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
    loadSavedSubscriptions
  );
  // Track which tickers the server has confirmed (kept for future use)
  const [, setConfirmedTickers] = useState<Set<string>>(() => new Set());
  const [tickerData, setTickerData] = useState<Map<string, TickerData>>(
    () => new Map()
  );
  const [lastCandle, setLastCandle] = useState<{
    ticker: string;
    candle: WSCandle;
  } | null>(null);
  const [lastClusters, setLastClusters] = useState<{
    ticker: string;
    data: ClustersData;
  } | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [maxCandlesPerTicker, setMaxCandlesPerTicker] = useState(maxCandles);

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
        send({ type: "subscribe", ticker });
      });
    }
  }, [status, desiredTickers, send]);

  // Subscribe to all WebSocket messages and route by type
  useEffect(() => {
    const unsubscribe = subscribe(
      () => true, // Accept all messages
      (msg: WSMessage) => {
        const parsed = parseServerMessage(msg.data);
        if (!parsed) return;

        if (isCandleMessage(parsed)) {
          const { ticker, data: candle } = parsed;

          setLastCandle({ ticker, candle });

          setTickerData((prev) => {
            const next = new Map(prev);
            const existing = next.get(ticker) ?? {
              candles: [],
              clusters: null,
              lastUpdated: null,
            };

            // Append candle, trim to max
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
            const existing = next.get(ticker) ?? {
              candles: [],
              clusters: null,
              lastUpdated: null,
            };
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
        }
      }
    );

    return unsubscribe;
  }, [subscribe]);

  const subscribeTicker = useCallback(
    (ticker: string) => {
      // Add to desired tickers (persisted)
      setDesiredTickers((prev) => new Set(prev).add(ticker));
      // Send subscribe message if connected
      if (status === "open") {
        send({ type: "subscribe", ticker });
      }
    },
    [send, status]
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
      if (status === "open") {
        send({ type: "unsubscribe", ticker });
      }
    },
    [send, status]
  );

  const requestClusters = useCallback(
    (ticker: string) => {
      send({ type: "get_clusters", ticker });
    },
    [send]
  );

  const getCandles = useCallback(
    (ticker: string): WSCandle[] => {
      return tickerData.get(ticker)?.candles ?? [];
    },
    [tickerData]
  );

  const getClusters = useCallback(
    (ticker: string): ClustersData | null => {
      return tickerData.get(ticker)?.clusters ?? null;
    },
    [tickerData]
  );

  const value: TradingDataContextValue = {
    subscribedTickers: desiredTickers,
    subscribeTicker,
    unsubscribeTicker,
    requestClusters,
    tickerData,
    getCandles,
    getClusters,
    lastCandle,
    lastClusters,
    lastError,
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
