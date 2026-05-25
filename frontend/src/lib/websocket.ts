/**
 * WebSocket connection manager for real-time gateway events.
 *
 * Singleton pattern — one shared connection across all components.
 * Auto-reconnects on disconnect with exponential backoff.
 */

import { createSignal } from "solid-js";

// ── Event types from the gateway ──────────────────────────────────────────

export interface WSTokenEvent {
  type: "token";
  session_id: string;
  content: string;
}

export interface WSResponseDoneEvent {
  type: "response_done";
  session_id: string;
  response: string;
}

export interface WSToolCallEvent {
  type: "tool_call";
  session_id: string;
  tool_name: string;
  arguments: string;
}

export interface WSToolResultEvent {
  type: "tool_result";
  session_id: string;
  tool_name: string;
  result: string;
  is_error: boolean;
}

export interface WSSessionEvent {
  type: "session_event";
  session_id: string;
  action: string;
}

export type WSEvent =
  | WSTokenEvent
  | WSResponseDoneEvent
  | WSToolCallEvent
  | WSToolResultEvent
  | WSSessionEvent;

// ── Listener types ────────────────────────────────────────────────────────

export type TokenListener = (event: WSTokenEvent) => void;
export type ResponseDoneListener = (event: WSResponseDoneEvent) => void;
export type ToolCallListener = (event: WSToolCallEvent) => void;
export type ToolResultListener = (event: WSToolResultEvent) => void;
export type SessionEventListener = (event: WSSessionEvent) => void;

// ── Connection state ──────────────────────────────────────────────────────

export type ConnectionState = "connecting" | "connected" | "disconnected";

const [connectionState, setConnectionState] =
  createSignal<ConnectionState>("disconnected");

export { connectionState };

// ── Internal state ────────────────────────────────────────────────────────

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectAttempts = 0;
let intentionalClose = false;

const MAX_RECONNECT_DELAY = 30_000; // 30 seconds
const BASE_RECONNECT_DELAY = 1_000; // 1 second

const tokenListeners = new Set<TokenListener>();
const responseDoneListeners = new Set<ResponseDoneListener>();
const toolCallListeners = new Set<ToolCallListener>();
const toolResultListeners = new Set<ToolResultListener>();
const sessionEventListeners = new Set<SessionEventListener>();

// ── Listener registration ─────────────────────────────────────────────────

export function onToken(listener: TokenListener): () => void {
  tokenListeners.add(listener);
  return () => tokenListeners.delete(listener);
}

export function onResponseDone(listener: ResponseDoneListener): () => void {
  responseDoneListeners.add(listener);
  return () => responseDoneListeners.delete(listener);
}

export function onToolCall(listener: ToolCallListener): () => void {
  toolCallListeners.add(listener);
  return () => toolCallListeners.delete(listener);
}

export function onToolResult(listener: ToolResultListener): () => void {
  toolResultListeners.add(listener);
  return () => toolResultListeners.delete(listener);
}

export function onSessionEvent(listener: SessionEventListener): () => void {
  sessionEventListeners.add(listener);
  return () => sessionEventListeners.delete(listener);
}

// ── Dispatching events ────────────────────────────────────────────────────

function dispatchEvent(event: WSEvent) {
  switch (event.type) {
    case "token":
      tokenListeners.forEach((l) => l(event));
      break;
    case "response_done":
      responseDoneListeners.forEach((l) => l(event));
      break;
    case "tool_call":
      toolCallListeners.forEach((l) => l(event));
      break;
    case "tool_result":
      toolResultListeners.forEach((l) => l(event));
      break;
    case "session_event":
      sessionEventListeners.forEach((l) => l(event));
      break;
  }
}

// ── Connection management ─────────────────────────────────────────────────

function getWsUrl(): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws`;
}

function scheduleReconnect() {
  if (intentionalClose) return;

  const delay = Math.min(
    BASE_RECONNECT_DELAY * Math.pow(2, reconnectAttempts),
    MAX_RECONNECT_DELAY
  );
  reconnectAttempts++;

  console.log(
    `[WS] Reconnecting in ${delay}ms (attempt ${reconnectAttempts})...`
  );
  setConnectionState("disconnected");

  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, delay);
}

function connect() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return;
  }

  intentionalClose = false;
  setConnectionState("connecting");

  const url = getWsUrl();
  console.log(`[WS] Connecting to ${url}...`);

  try {
    ws = new WebSocket(url);
  } catch (err) {
    console.error("[WS] Failed to create WebSocket:", err);
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    console.log("[WS] Connected");
    reconnectAttempts = 0;
    setConnectionState("connected");
  };

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data) as WSEvent;
      dispatchEvent(data);
    } catch (err) {
      console.warn("[WS] Failed to parse message:", event.data, err);
    }
  };

  ws.onerror = (event) => {
    console.error("[WS] Error:", event);
  };

  ws.onclose = (event) => {
    console.log(`[WS] Disconnected (code=${event.code}, reason=${event.reason})`);
    ws = null;
    if (!intentionalClose) {
      scheduleReconnect();
    } else {
      setConnectionState("disconnected");
    }
  };
}

/**
 * Initialize the WebSocket connection. Call once on app startup.
 * Returns a cleanup function that closes the connection.
 */
export function initWebSocket(): () => void {
  connect();

  return () => {
    intentionalClose = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.close();
      ws = null;
    }
    setConnectionState("disconnected");
  };
}

/**
 * Send a JSON message over the WebSocket (if connected).
 */
export function sendWSMessage(data: unknown): boolean {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(data));
    return true;
  }
  return false;
}
