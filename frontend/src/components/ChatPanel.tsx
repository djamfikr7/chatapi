import {
  createSignal,
  createEffect,
  onCleanup,
  For,
  Show,
  type Setter,
} from "solid-js";
import type { ChatMessage, ToolCall, Session } from "../lib/api";
import { fetchTools, type ToolInfo } from "../lib/api";
import { streamChatCompletion } from "../lib/streaming";
import {
  onToken,
  onResponseDone,
  onToolCall,
  onToolResult,
  type WSTokenEvent,
  type WSResponseDoneEvent,
  type WSToolCallEvent,
  type WSToolResultEvent,
} from "../lib/websocket";

interface ChatPanelProps {
  sessionId: string | null;
  messages: ChatMessage[];
  isLoading: boolean;
  setIsLoading: Setter<boolean>;
  onAddMessage: (msg: ChatMessage) => void;
  onSetMessages: (msgs: ChatMessage[]) => void;
  onNewSession: () => void;
  sessions: Session[];
  onSelectSession: (id: string) => void;
}

export function ChatPanel(props: ChatPanelProps) {
  const [input, setInput] = createSignal("");
  const [streamingText, setStreamingText] = createSignal("");
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [pendingToolCalls, setPendingToolCalls] = createSignal<ToolCall[]>([]);
  const [availableTools, setAvailableTools] = createSignal<ToolInfo[]>([]);
  // Track whether the current stream is coming via WebSocket (so SSE callbacks are ignored)
  const [wsStreaming, setWsStreaming] = createSignal(false);
  let messagesRef: HTMLDivElement | undefined;

  // Fetch available tools
  createEffect(() => {
    fetchTools().then(setAvailableTools).catch(() => {});
  });

  // ── WebSocket event subscriptions ──────────────────────────────────────
  // These are active for the lifetime of the component. Events are filtered
  // to only handle ones matching the current active session.

  onCleanup(
    onToken((evt: WSTokenEvent) => {
      if (evt.session_id !== props.sessionId) return;
      // A WS token arrived -- we are in WS streaming mode
      setWsStreaming(true);
      setIsStreaming(true);
      setStreamingText((prev) => prev + evt.content);
    })
  );

  onCleanup(
    onResponseDone((evt: WSResponseDoneEvent) => {
      if (evt.session_id !== props.sessionId) return;
      setWsStreaming(false);
      setIsStreaming(false);
      const fullText = streamingText() || evt.response;
      setStreamingText("");
      if (fullText) {
        const assistantMsg: ChatMessage = {
          role: "assistant",
          content: fullText,
        };
        props.onAddMessage(assistantMsg);
      }
      props.setIsLoading(false);
    })
  );

  onCleanup(
    onToolCall((evt: WSToolCallEvent) => {
      if (evt.session_id !== props.sessionId) return;
      const tc: ToolCall = {
        id: `ws_call_${Date.now()}`,
        type: "function",
        function: {
          name: evt.tool_name,
          arguments: evt.arguments,
        },
      };
      setPendingToolCalls((prev) => [...prev, tc]);
    })
  );

  onCleanup(
    onToolResult((evt: WSToolResultEvent) => {
      if (evt.session_id !== props.sessionId) return;
      // Show tool result as a tool message in the chat
      const toolMsg: ChatMessage = {
        role: "tool",
        content: evt.result,
        name: evt.tool_name,
      };
      props.onAddMessage(toolMsg);
      // Remove matching pending tool call
      setPendingToolCalls((prev) =>
        prev.filter((tc) => tc.function.name !== evt.tool_name)
      );
    })
  );

  // Auto-scroll on new messages
  createEffect(() => {
    // Access messages to track
    const _ = props.messages.length;
    const __ = streamingText();
    if (messagesRef) {
      requestAnimationFrame(() => {
        messagesRef!.scrollTop = messagesRef!.scrollHeight;
      });
    }
  });

  async function handleSend() {
    const text = input().trim();
    if (!text || props.isLoading) return;

    setInput("");
    setStreamingText("");
    setPendingToolCalls([]);
    setWsStreaming(false);

    const userMsg: ChatMessage = { role: "user", content: text };
    props.onAddMessage(userMsg);
    props.setIsLoading(true);
    setIsStreaming(true);

    const messages = [...props.messages, userMsg];

    try {
      await streamChatCompletion(
        {
          model: "deepseek-chat",
          messages,
        },
        {
          onToken(token) {
            // If WS is handling this stream, ignore SSE tokens
            if (wsStreaming()) return;
            setStreamingText((prev) => prev + token);
          },
          onToolCall(toolCall) {
            if (wsStreaming()) return;
            setPendingToolCalls((prev) => [...prev, toolCall]);
          },
          onDone(fullText, toolCalls) {
            // If WS is handling this stream, ignore SSE done
            if (wsStreaming()) return;
            setIsStreaming(false);
            setStreamingText("");

            if (toolCalls.length > 0) {
              const assistantMsg: ChatMessage = {
                role: "assistant",
                content: fullText || undefined,
                tool_calls: toolCalls,
              };
              props.onAddMessage(assistantMsg);
              setPendingToolCalls(toolCalls);
            } else if (fullText) {
              const assistantMsg: ChatMessage = {
                role: "assistant",
                content: fullText,
              };
              props.onAddMessage(assistantMsg);
            }
            props.setIsLoading(false);
          },
          onError(error) {
            console.error("Stream error:", error);
            setIsStreaming(false);
            setStreamingText("");
            const errorMsg: ChatMessage = {
              role: "assistant",
              content: `Error: ${error.message}`,
            };
            props.onAddMessage(errorMsg);
            props.setIsLoading(false);
          },
        }
      );
    } catch (err) {
      console.error("Chat error:", err);
      setIsStreaming(false);
      setStreamingText("");
      props.setIsLoading(false);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function handleApproveTool(toolCall: ToolCall) {
    // Add a tool result message and continue
    const toolMsg: ChatMessage = {
      role: "tool",
      content: `Tool ${toolCall.function.name} approved. Executing with args: ${toolCall.function.arguments}`,
      tool_call_id: toolCall.id,
      name: toolCall.function.name,
    };
    const newMessages = [...props.messages, toolMsg];
    props.onSetMessages(newMessages);
    setPendingToolCalls((prev) => prev.filter((tc) => tc.id !== toolCall.id));

    // Re-send with tool result
    setInput("");
    setStreamingText("");
    setWsStreaming(false);
    props.setIsLoading(true);
    setIsStreaming(true);

    streamChatCompletion(
      {
        model: "deepseek-chat",
        messages: newMessages,
      },
      {
        onToken(token) {
          if (wsStreaming()) return;
          setStreamingText((prev) => prev + token);
        },
        onToolCall(tc) {
          if (wsStreaming()) return;
          setPendingToolCalls((prev) => [...prev, tc]);
        },
        onDone(fullText, toolCalls) {
          if (wsStreaming()) return;
          setIsStreaming(false);
          setStreamingText("");
          if (toolCalls.length > 0) {
            props.onAddMessage({
              role: "assistant",
              content: fullText || undefined,
              tool_calls: toolCalls,
            });
            setPendingToolCalls(toolCalls);
          } else if (fullText) {
            props.onAddMessage({ role: "assistant", content: fullText });
          }
          props.setIsLoading(false);
        },
        onError(error) {
          setIsStreaming(false);
          setStreamingText("");
          props.onAddMessage({ role: "assistant", content: `Error: ${error.message}` });
          props.setIsLoading(false);
        },
      }
    );
  }

  function handleRejectTool(toolCall: ToolCall) {
    const toolMsg: ChatMessage = {
      role: "tool",
      content: `Tool ${toolCall.function.name} was rejected by the user.`,
      tool_call_id: toolCall.id,
      name: toolCall.function.name,
    };
    props.onAddMessage(toolMsg);
    setPendingToolCalls((prev) => prev.filter((tc) => tc.id !== toolCall.id));
  }

  function getRoleColor(role: string): string {
    switch (role) {
      case "user":
        return "text-blue-400";
      case "assistant":
        return "text-green-400";
      case "system":
        return "text-yellow-400";
      case "tool":
        return "text-purple-400";
      default:
        return "text-ide-muted";
    }
  }

  function getRoleLabel(role: string): string {
    switch (role) {
      case "user":
        return "You";
      case "assistant":
        return "Assistant";
      case "system":
        return "System";
      case "tool":
        return "Tool";
      default:
        return role;
    }
  }

  return (
    <div class="flex flex-col h-full">
      {/* Session selector */}
      <div class="flex items-center gap-2 px-3 py-2 border-b border-ide-border shrink-0">
        <select
          class="flex-1 bg-ide-bg border border-ide-border rounded px-2 py-1 text-xs text-ide-text focus:outline-none focus:border-ide-accent"
          value={props.sessionId || ""}
          onChange={(e) => {
            const val = e.currentTarget.value;
            if (val) props.onSelectSession(val);
          }}
        >
          <option value="">No session</option>
          <For each={props.sessions}>
            {(s) => (
              <option value={s.id}>
                {s.id.slice(0, 8)}... ({s.model})
              </option>
            )}
          </For>
        </select>
        <button
          class="px-2 py-1 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors"
          onClick={props.onNewSession}
        >
          New
        </button>
      </div>

      {/* Messages */}
      <div ref={messagesRef} class="flex-1 overflow-y-auto px-3 py-2 space-y-3 chat-message">
        <For each={props.messages}>
          {(msg) => (
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class={`text-xs font-medium ${getRoleColor(msg.role)}`}>
                  {getRoleLabel(msg.role)}
                </span>
                {msg.name && (
                  <span class="text-xs text-ide-muted">({msg.name})</span>
                )}
              </div>
              <div class="text-sm text-ide-text whitespace-pre-wrap break-words pl-2 border-l-2 border-ide-border">
                {msg.content || ""}
                <Show when={msg.tool_calls && msg.tool_calls.length > 0}>
                  <For each={msg.tool_calls}>
                    {(tc) => (
                      <div class="mt-2 p-2 bg-ide-bg rounded border border-ide-border">
                        <div class="flex items-center gap-2 text-xs text-purple-400">
                          <span class="font-medium">Tool Call:</span>
                          <span class="font-mono">{tc.function.name}</span>
                        </div>
                        <pre class="text-xs text-ide-muted mt-1 overflow-x-auto">
                          {tc.function.arguments}
                        </pre>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </div>
          )}
        </For>

        {/* Streaming text */}
        <Show when={isStreaming() && streamingText()}>
          <div class="flex flex-col gap-1">
            <span class="text-xs font-medium text-green-400">Assistant</span>
            <div class="text-sm text-ide-text whitespace-pre-wrap break-words pl-2 border-l-2 border-green-600">
              {streamingText()}
              <span class="inline-block w-1.5 h-3.5 bg-ide-text animate-pulse ml-0.5" />
            </div>
          </div>
        </Show>

        {/* Loading indicator */}
        <Show when={props.isLoading && !streamingText()}>
          <div class="flex items-center gap-2 text-xs text-ide-muted">
            <div class="w-2 h-2 bg-ide-accent rounded-full animate-pulse" />
            Thinking...
          </div>
        </Show>

        {/* Pending tool calls with approve/reject */}
        <Show when={pendingToolCalls().length > 0}>
          <For each={pendingToolCalls()}>
            {(tc) => (
              <div class="p-3 bg-ide-bg rounded border border-yellow-600">
                <div class="flex items-center gap-2 text-sm text-yellow-400 mb-2">
                  <span class="font-medium">Tool Request:</span>
                  <span class="font-mono">{tc.function.name}</span>
                </div>
                <pre class="text-xs text-ide-muted mb-3 overflow-x-auto bg-ide-panel p-2 rounded">
                  {(() => {
                    try {
                      return JSON.stringify(JSON.parse(tc.function.arguments), null, 2);
                    } catch {
                      return tc.function.arguments;
                    }
                  })()}
                </pre>
                <div class="flex gap-2">
                  <button
                    class="px-3 py-1 text-xs bg-green-700 text-white rounded hover:bg-green-600 transition-colors"
                    onClick={() => handleApproveTool(tc)}
                  >
                    Approve
                  </button>
                  <button
                    class="px-3 py-1 text-xs bg-red-700 text-white rounded hover:bg-red-600 transition-colors"
                    onClick={() => handleRejectTool(tc)}
                  >
                    Reject
                  </button>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>

      {/* Input area */}
      <div class="border-t border-ide-border p-3 shrink-0">
        <div class="flex gap-2">
          <textarea
            class="flex-1 bg-ide-bg border border-ide-border rounded px-3 py-2 text-sm text-ide-text resize-none focus:outline-none focus:border-ide-accent placeholder-ide-muted"
            rows={3}
            placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            disabled={props.isLoading}
          />
          <button
            class="px-4 py-2 bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed self-end"
            onClick={handleSend}
            disabled={props.isLoading || !input().trim()}
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
