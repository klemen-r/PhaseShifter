"use client";

import {
  createContext,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  DEFAULT_CONNECTION_ID,
  DEFAULT_RECONNECT_INTERVAL_MS,
  type MessageFilter,
  type Subscription,
  type WebSocketContextValue,
  type WSConnection,
  type WSMessage,
  type WSStatus,
} from "./types";

export const WebSocketContext = createContext<WebSocketContextValue | null>(
  null,
);

interface WebSocketProviderProps {
  children: ReactNode;
  defaultUrl?: string;
  autoConnect?: boolean;
  reconnectInterval?: number;
}

let messageIdCounter = 0;
const generateMessageId = () => `msg_${Date.now()}_${++messageIdCounter}`;

export function WebSocketProvider({
  children,
  defaultUrl = "ws://localhost:8000",
  autoConnect = true,
  reconnectInterval = DEFAULT_RECONNECT_INTERVAL_MS,
}: WebSocketProviderProps) {
  const [connections, setConnections] = useState<Map<string, WSConnection>>(
    () => new Map(),
  );
  const [messages, setMessages] = useState<WSMessage[]>([]);
  const [defaultUrlState, setDefaultUrlState] = useState(defaultUrl);

  // Refs for WebSocket instances and reconnect timers
  const socketsRef = useRef<Map<string, WebSocket>>(new Map());
  const reconnectTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );
  const autoReconnectRef = useRef<Map<string, boolean>>(new Map());
  const subscriptionsRef = useRef<Set<Subscription>>(new Set());
  const pingSentAtRef = useRef<Map<string, number>>(new Map());
  const connectRef = useRef<(url: string, id?: string) => void>(() => {});

  // Helper to update a specific connection's state
  const updateConnection = useCallback(
    (id: string, updates: Partial<WSConnection>) => {
      setConnections((prev) => {
        const next = new Map(prev);
        const existing = next.get(id);
        if (existing) {
          next.set(id, { ...existing, ...updates });
        }
        return next;
      });
    },
    [],
  );

  // Notify subscribers of a new message
  const notifySubscribers = useCallback((msg: WSMessage) => {
    subscriptionsRef.current.forEach(({ filter, callback }) => {
      if (filter(msg)) {
        callback(msg);
      }
    });
  }, []);

  // Connect to a WebSocket
  const connect = useCallback(
    (url: string, id: string = DEFAULT_CONNECTION_ID) => {
      // Clear any pending reconnect
      const existingTimer = reconnectTimersRef.current.get(id);
      if (existingTimer) {
        clearTimeout(existingTimer);
        reconnectTimersRef.current.delete(id);
      }

      // Close existing socket if any
      const existingSocket = socketsRef.current.get(id);
      if (existingSocket) {
        existingSocket.close();
        socketsRef.current.delete(id);
      }

      // Initialize connection state
      const connectionState: WSConnection = {
        id,
        url,
        status: "connecting",
        lastPingMs: null,
        autoReconnect: autoReconnectRef.current.get(id) ?? true,
      };

      setConnections((prev) => {
        const next = new Map(prev);
        next.set(id, connectionState);
        return next;
      });

      // Create new WebSocket
      const ws = new WebSocket(url);
      socketsRef.current.set(id, ws);

      ws.onopen = () => {
        updateConnection(id, { status: "open" });
      };

      ws.onerror = () => {
        updateConnection(id, { status: "error" });
      };

      ws.onclose = () => {
        socketsRef.current.delete(id);
        const shouldReconnect = autoReconnectRef.current.get(id) ?? true;

        updateConnection(id, {
          status: shouldReconnect ? "connecting" : "closed",
        });

        if (shouldReconnect) {
          const timer = setTimeout(() => {
            // Get the current URL from connections state
            const currentUrl =
              id === DEFAULT_CONNECTION_ID ? defaultUrlState : url;
            connectRef.current(currentUrl, id);
          }, reconnectInterval);
          reconnectTimersRef.current.set(id, timer);
        }
      };

      ws.onmessage = (event) => {
        const rawData =
          typeof event.data === "string" ? event.data : String(event.data);

        // Check for pong response
        let parsed: unknown;
        try {
          parsed = JSON.parse(rawData);
        } catch {
          parsed = null;
        }

        const parsedObj =
          parsed && typeof parsed === "object"
            ? (parsed as { msg?: string; type?: string })
            : null;

        const isPong =
          rawData === "pong" ||
          parsedObj?.msg === "pong" ||
          parsedObj?.type === "pong";

        if (isPong) {
          const sentAt = pingSentAtRef.current.get(id);
          if (sentAt !== undefined) {
            const delta = (performance.now() - sentAt) * 1000;
            updateConnection(id, {
              lastPingMs: Math.max(0, Math.round(delta)),
            });
            pingSentAtRef.current.delete(id);
          }
        }

        // Create message object
        const msg: WSMessage = {
          id: generateMessageId(),
          data: parsed ?? rawData,
          raw: rawData,
          timestamp: Date.now(),
          connectionId: id,
        };

        // Add to messages (newest first)
        setMessages((prev) => [msg, ...prev]);

        // Notify subscribers
        notifySubscribers(msg);
      };
    },
    [defaultUrlState, reconnectInterval, updateConnection, notifySubscribers],
  );

  // Keep connectRef in sync with connect
  useEffect(() => {
    connectRef.current = connect;
  }, [connect]);

  // Disconnect from a WebSocket
  const disconnect = useCallback(
    (id: string = DEFAULT_CONNECTION_ID) => {
      // Disable auto-reconnect for this connection
      autoReconnectRef.current.set(id, false);

      // Clear reconnect timer
      const timer = reconnectTimersRef.current.get(id);
      if (timer) {
        clearTimeout(timer);
        reconnectTimersRef.current.delete(id);
      }

      // Close socket
      const socket = socketsRef.current.get(id);
      if (socket) {
        socket.close();
        socketsRef.current.delete(id);
      }

      updateConnection(id, { status: "closed", autoReconnect: false });
    },
    [updateConnection],
  );

  // Send data through a WebSocket
  const send = useCallback(
    (data: string | object, connectionId: string = DEFAULT_CONNECTION_ID) => {
      const socket = socketsRef.current.get(connectionId);
      if (socket?.readyState === WebSocket.OPEN) {
        const payload = typeof data === "string" ? data : JSON.stringify(data);
        socket.send(payload);
      }
    },
    [],
  );

  // Ping a connection
  const ping = useCallback((connectionId: string = DEFAULT_CONNECTION_ID) => {
    const socket = socketsRef.current.get(connectionId);
    if (socket?.readyState === WebSocket.OPEN) {
      pingSentAtRef.current.set(connectionId, performance.now());
      socket.send(JSON.stringify({ type: "ping" }));
    }
  }, []);

  // Set auto-reconnect for a connection
  const setAutoReconnect = useCallback(
    (enabled: boolean, connectionId: string = DEFAULT_CONNECTION_ID) => {
      autoReconnectRef.current.set(connectionId, enabled);
      updateConnection(connectionId, { autoReconnect: enabled });

      if (!enabled) {
        // Clear any pending reconnect timer
        const timer = reconnectTimersRef.current.get(connectionId);
        if (timer) {
          clearTimeout(timer);
          reconnectTimersRef.current.delete(connectionId);
        }

        // If currently in "connecting" state due to reconnect, change to "closed"
        const conn = connections.get(connectionId);
        if (
          conn?.status === "connecting" &&
          !socketsRef.current.has(connectionId)
        ) {
          updateConnection(connectionId, { status: "closed" });
        }
      } else {
        // If closed, start reconnecting
        const conn = connections.get(connectionId);
        if (conn?.status === "closed") {
          connect(conn.url, connectionId);
        }
      }
    },
    [connections, connect, updateConnection],
  );

  // Subscribe to messages with a filter
  const subscribe = useCallback(
    (filter: MessageFilter, callback: (msg: WSMessage) => void) => {
      const subscription: Subscription = { filter, callback };
      subscriptionsRef.current.add(subscription);

      // Return unsubscribe function
      return () => {
        subscriptionsRef.current.delete(subscription);
      };
    },
    [],
  );

  // Clear messages
  const clearMessages = useCallback((connectionId?: string) => {
    if (connectionId) {
      setMessages((prev) =>
        prev.filter((m) => m.connectionId !== connectionId),
      );
    } else {
      setMessages([]);
    }
  }, []);

  // Set default URL
  const setDefaultUrl = useCallback((url: string) => {
    setDefaultUrlState(url);
  }, []);

  // Auto-connect on mount
  useEffect(() => {
    if (autoConnect) {
      autoReconnectRef.current.set(DEFAULT_CONNECTION_ID, true);
      // Use queueMicrotask to avoid synchronous setState in effect
      queueMicrotask(() => {
        connectRef.current(defaultUrl, DEFAULT_CONNECTION_ID);
      });
    }

    // Capture refs for cleanup
    const timers = reconnectTimersRef.current;
    const sockets = socketsRef.current;

    // Cleanup on unmount
    return () => {
      timers.forEach((timer) => clearTimeout(timer));
      timers.clear();
      sockets.forEach((socket) => socket.close());
      sockets.clear();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Compute default connection status
  const defaultConnection = connections.get(DEFAULT_CONNECTION_ID);
  const defaultStatus: WSStatus = defaultConnection?.status ?? "closed";

  const value: WebSocketContextValue = {
    connections,
    connect,
    disconnect,
    send,
    ping,
    setAutoReconnect,
    messages,
    subscribe,
    clearMessages,
    defaultStatus,
    defaultUrl: defaultUrlState,
    setDefaultUrl,
  };

  return (
    <WebSocketContext.Provider value={value}>
      {children}
    </WebSocketContext.Provider>
  );
}
