export type WSStatus = "open" | "connecting" | "closed" | "error";

export interface WSMessage {
  id: string;
  data: unknown;
  raw: string;
  timestamp: number;
  connectionId: string;
}

export interface WSConnection {
  id: string;
  url: string;
  status: WSStatus;
  lastPingMs: number | null;
  autoReconnect: boolean;
}

export type MessageFilter = (msg: WSMessage) => boolean;

export interface Subscription {
  filter: MessageFilter;
  callback: (msg: WSMessage) => void;
}

export interface WebSocketContextValue {
  // Connection management
  connections: Map<string, WSConnection>;
  connect: (url: string, id?: string) => void;
  disconnect: (id?: string) => void;
  send: (data: string | object, connectionId?: string) => void;
  ping: (connectionId?: string) => void;
  setAutoReconnect: (enabled: boolean, connectionId?: string) => void;

  // Message access
  messages: WSMessage[];
  subscribe: (
    filter: MessageFilter,
    callback: (msg: WSMessage) => void
  ) => () => void;
  clearMessages: (connectionId?: string) => void;

  // Default connection helpers
  defaultStatus: WSStatus;
  defaultUrl: string;
  setDefaultUrl: (url: string) => void;
}

export const DEFAULT_CONNECTION_ID = "default";
export const DEFAULT_RECONNECT_INTERVAL_MS = 1500;

// ============================================
// Server Message Types
// ============================================

export interface WSCandle {
  time: number; // Unix timestamp in milliseconds
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface ClusterItem {
  side: "bullish" | "bearish";
  low: number;
  high: number;
  count: number;
  unique_scenarios: number;
}

export interface NodeItem {
  value: number;
  side: "bullish" | "bearish";
  interval: string;
  phase_window: number;
  depth_days: number;
  pct_from_anchor: number | null;
}

export interface ClustersData {
  ticker: string;
  anchor: number | null;
  clusters: ClusterItem[];
  nodes: NodeItem[];
  generated_at: string;
}

// Server → Client messages
export interface CandleMessage {
  type: "candle";
  ticker: string;
  data: WSCandle;
}

export interface ClustersMessage {
  type: "clusters";
  ticker: string;
  data: ClustersData;
}

export interface SubscribedMessage {
  type: "subscribed";
  ticker: string;
}

export interface UnsubscribedMessage {
  type: "unsubscribed";
  ticker: string;
}

export interface ErrorMessage {
  type: "error";
  message: string;
}

export interface PongMessage {
  type: "pong";
}

export interface AutoClustersEnabledMessage {
  type: "auto_clusters_enabled";
  ticker: string;
}

export interface AutoClustersDisabledMessage {
  type: "auto_clusters_disabled";
  ticker: string;
}

export type ServerMessage =
  | CandleMessage
  | ClustersMessage
  | SubscribedMessage
  | UnsubscribedMessage
  | ErrorMessage
  | PongMessage
  | AutoClustersEnabledMessage
  | AutoClustersDisabledMessage;

// Type guards
export function isCandleMessage(msg: unknown): msg is CandleMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "candle"
  );
}

export function isClustersMessage(msg: unknown): msg is ClustersMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "clusters"
  );
}

export function isSubscribedMessage(msg: unknown): msg is SubscribedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "subscribed"
  );
}

export function isUnsubscribedMessage(msg: unknown): msg is UnsubscribedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "unsubscribed"
  );
}

export function isErrorMessage(msg: unknown): msg is ErrorMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "error"
  );
}

export function isAutoClustersEnabledMessage(
  msg: unknown
): msg is AutoClustersEnabledMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "auto_clusters_enabled"
  );
}

export function isAutoClustersDisabledMessage(
  msg: unknown
): msg is AutoClustersDisabledMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "auto_clusters_disabled"
  );
}

export function parseServerMessage(data: unknown): ServerMessage | null {
  if (typeof data !== "object" || data === null || !("type" in data)) {
    return null;
  }
  const type = (data as { type: unknown }).type;
  if (
    type === "candle" ||
    type === "clusters" ||
    type === "subscribed" ||
    type === "unsubscribed" ||
    type === "error" ||
    type === "pong" ||
    type === "auto_clusters_enabled" ||
    type === "auto_clusters_disabled"
  ) {
    return data as ServerMessage;
  }
  return null;
}
