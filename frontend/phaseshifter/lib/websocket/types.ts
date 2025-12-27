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
    callback: (msg: WSMessage) => void,
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
// Server Message Types (Rust phaseshifter-server)
// ============================================

export interface WSCandle {
  time: number; // Unix timestamp in milliseconds
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface WSTick {
  price: number;
  volume: number;
  timestamp: number; // Unix timestamp in milliseconds
  bid?: number;
  ask?: number;
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

export interface OpenNodeInfo {
  direction: string;
  distance_pct: number;
  anchor: number;
  extreme: number;
  created_at: number;
  projected_target: number;
}

export interface HistoryBar {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

// Server → Client messages (Rust server format)
export interface ConnectedMessage {
  type: "connected";
  client_id: number;
  symbols: string[];
}

export interface TickMessage {
  type: "tick";
  symbol: string;
  price: number;
  volume: number;
  timestamp: number;
  bid: number;
  ask: number;
}

export interface BarUpdateMessage {
  type: "bar_update";
  symbol: string;
  timeframe: string;
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface BarClosedMessage {
  type: "bar_closed";
  symbol: string;
  timeframe: string;
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface PhaseUpdateMessage {
  type: "phase_update";
  symbol: string;
  timeframe: string;
  time: number;
  phase: "bullish" | "bearish";
  anchor: number;
  dm: number;
}

export interface NodeCreatedMessage {
  type: "node_created";
  symbol: string;
  timeframe: string;
  direction: string;
  distance_pct: number;
  anchor: number;
  extreme: number;
  created_at: number;
}

export interface NodeClosedMessage {
  type: "node_closed";
  symbol: string;
  timeframe: string;
  direction: string;
  distance_pct: number;
  target: number;
  closed_at: number;
}

export interface OpenNodesMessage {
  type: "open_nodes";
  symbol: string;
  timeframe: string;
  nodes: OpenNodeInfo[];
}

export interface HistoryMessage {
  type: "history";
  symbol: string;
  timeframe: string;
  bars: HistoryBar[];
}

export interface ErrorMessage {
  type: "error";
  message: string;
}

export interface PongMessage {
  type: "pong";
  timestamp: number;
}

// Legacy message types (for backwards compatibility with Python server)
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

export interface AutoClustersEnabledMessage {
  type: "auto_clusters_enabled";
  ticker: string;
}

export interface AutoClustersDisabledMessage {
  type: "auto_clusters_disabled";
  ticker: string;
}

export type ServerMessage =
  // Rust server messages
  | ConnectedMessage
  | TickMessage
  | BarUpdateMessage
  | BarClosedMessage
  | PhaseUpdateMessage
  | NodeCreatedMessage
  | NodeClosedMessage
  | OpenNodesMessage
  | HistoryMessage
  | ErrorMessage
  | PongMessage
  // Legacy Python server messages
  | CandleMessage
  | ClustersMessage
  | SubscribedMessage
  | UnsubscribedMessage
  | AutoClustersEnabledMessage
  | AutoClustersDisabledMessage;

// Type guards
export function isConnectedMessage(msg: unknown): msg is ConnectedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "connected"
  );
}

export function isTickMessage(msg: unknown): msg is TickMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "tick"
  );
}

export function isBarUpdateMessage(msg: unknown): msg is BarUpdateMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "bar_update"
  );
}

export function isBarClosedMessage(msg: unknown): msg is BarClosedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "bar_closed"
  );
}

export function isPhaseUpdateMessage(msg: unknown): msg is PhaseUpdateMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "phase_update"
  );
}

export function isNodeCreatedMessage(msg: unknown): msg is NodeCreatedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "node_created"
  );
}

export function isNodeClosedMessage(msg: unknown): msg is NodeClosedMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "node_closed"
  );
}

export function isOpenNodesMessage(msg: unknown): msg is OpenNodesMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "open_nodes"
  );
}

export function isHistoryMessage(msg: unknown): msg is HistoryMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "history"
  );
}

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

export function isUnsubscribedMessage(
  msg: unknown,
): msg is UnsubscribedMessage {
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

export function isPongMessage(msg: unknown): msg is PongMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "pong"
  );
}

export function isAutoClustersEnabledMessage(
  msg: unknown,
): msg is AutoClustersEnabledMessage {
  return (
    typeof msg === "object" &&
    msg !== null &&
    "type" in msg &&
    (msg as { type: unknown }).type === "auto_clusters_enabled"
  );
}

export function isAutoClustersDisabledMessage(
  msg: unknown,
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
  const validTypes = [
    // Rust server messages
    "connected",
    "tick",
    "bar_update",
    "bar_closed",
    "phase_update",
    "node_created",
    "node_closed",
    "open_nodes",
    "history",
    "error",
    "pong",
    // Legacy Python server messages
    "candle",
    "clusters",
    "subscribed",
    "unsubscribed",
    "auto_clusters_enabled",
    "auto_clusters_disabled",
  ];
  if (validTypes.includes(type as string)) {
    return data as ServerMessage;
  }
  return null;
}
