"use client";

import { useContext, useEffect, useMemo, useRef, useState } from "react";
import { WebSocketContext } from "./WebSocketContext";
import {
  DEFAULT_CONNECTION_ID,
  type MessageFilter,
  type WSMessage,
  type WSStatus,
} from "./types";

export interface UseWebSocketOptions {
  connectionId?: string;
  filter?: MessageFilter;
}

export interface UseWebSocketReturn {
  // Connection state
  status: WSStatus;
  lastPingMs: number | null;
  autoReconnect: boolean;
  url: string;

  // All connections (for managing multiple)
  connections: Map<
    string,
    {
      id: string;
      url: string;
      status: WSStatus;
      lastPingMs: number | null;
      autoReconnect: boolean;
    }
  >;

  // Messages (filtered if filter provided, otherwise all for this connection)
  messages: WSMessage[];

  // Actions
  connect: (url: string, id?: string) => void;
  disconnect: (id?: string) => void;
  send: (data: string | object) => void;
  ping: () => void;
  setAutoReconnect: (enabled: boolean) => void;
  clearMessages: () => void;

  // Subscription for real-time filtered messages
  subscribe: (
    filter: MessageFilter,
    callback: (msg: WSMessage) => void,
  ) => () => void;

  // Default URL management
  setDefaultUrl: (url: string) => void;
}

export function useWebSocket(
  options: UseWebSocketOptions = {},
): UseWebSocketReturn {
  const context = useContext(WebSocketContext);

  if (!context) {
    throw new Error("useWebSocket must be used within a WebSocketProvider");
  }

  const { connectionId = DEFAULT_CONNECTION_ID, filter } = options;

  // Track filtered messages via subscription - use ref to track subscription state
  const [filteredMessages, setFilteredMessages] = useState<WSMessage[]>([]);
  const isInitializedRef = useRef(false);

  // Subscribe to filtered messages
  useEffect(() => {
    if (!filter) {
      isInitializedRef.current = false;
      return;
    }

    // Initialize with existing messages that match the filter (only on first mount or filter change)
    if (!isInitializedRef.current) {
      const existingMatches = context.messages.filter(
        (msg) => msg.connectionId === connectionId && filter(msg),
      );
      // Use a microtask to avoid synchronous setState in effect
      queueMicrotask(() => {
        setFilteredMessages(existingMatches);
      });
      isInitializedRef.current = true;
    }

    // Subscribe to new messages
    const unsubscribe = context.subscribe(
      (msg) => msg.connectionId === connectionId && filter(msg),
      (msg) => {
        setFilteredMessages((prev) => [msg, ...prev]);
      },
    );

    return () => {
      unsubscribe();
      isInitializedRef.current = false;
    };
  }, [context, connectionId, filter]);

  // Get connection state
  const connection = context.connections.get(connectionId);
  const status: WSStatus = connection?.status ?? "closed";
  const lastPingMs = connection?.lastPingMs ?? null;
  const autoReconnect = connection?.autoReconnect ?? true;
  const url = connection?.url ?? context.defaultUrl;

  // Messages for this connection (filtered or all)
  const messages = useMemo(() => {
    if (filter) {
      return filteredMessages;
    }
    console.log(
      context.messages.filter((msg) => msg.connectionId === connectionId),
    );
    return context.messages.filter((msg) => msg.connectionId === connectionId);
  }, [filter, filteredMessages, context.messages, connectionId]);

  // Bound actions for this connection
  const send = useMemo(
    () => (data: string | object) => context.send(data, connectionId),
    [context, connectionId],
  );

  const ping = useMemo(
    () => () => context.ping(connectionId),
    [context, connectionId],
  );

  const setAutoReconnectBound = useMemo(
    () => (enabled: boolean) => context.setAutoReconnect(enabled, connectionId),
    [context, connectionId],
  );

  const clearMessages = useMemo(
    () => () => {
      context.clearMessages(connectionId);
      if (filter) {
        setFilteredMessages([]);
      }
    },
    [context, connectionId, filter],
  );

  return {
    status,
    lastPingMs,
    autoReconnect,
    url,
    connections: context.connections,
    messages,
    connect: context.connect,
    disconnect: context.disconnect,
    send,
    ping,
    setAutoReconnect: setAutoReconnectBound,
    clearMessages,
    subscribe: context.subscribe,
    setDefaultUrl: context.setDefaultUrl,
  };
}
