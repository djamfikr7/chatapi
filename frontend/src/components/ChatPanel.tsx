import {
  createSignal,
  createEffect,
  onCleanup,
  For,
  Show,
  type Setter,
} from "solid-js";
import type { ChatMessage, ToolCall, Session } from "../lib/api";
import { fetchTools, executeTool, type ToolInfo } from "../lib/api";
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

/** Stored tool result associated with a tool call. */
interface ToolResultEntry {
  result: string;
  isError: boolean;
  toolName: string;
  /** Parsed args from the matching tool call (if available). */
  args?: Record<string, unknown>;
}

export function ChatPanel(props: ChatPanelProps) {
  const [input, setInput] = createSignal("");
  const [streamingText, setStreamingText] = createSignal("");
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [pendingToolCalls, setPendingToolCalls] = createSignal<ToolCall[]>([]);
  const [availableTools, setAvailableTools] = createSignal<ToolInfo[]>([]);
  const [wsStreaming, setWsStreaming] = createSignal(false);

  // Track tool results keyed by "toolName:idx" or tool_call_id
  const [toolResults, setToolResults] = createSignal<Record<string, ToolResultEntry>>({});
  // Collapsed state for tool call cards
  const [collapsedTools, setCollapsedTools] = createSignal<Record<string, boolean>>({});
  // Track executing tool calls (for button loading state)
  const [executingTools, setExecutingTools] = createSignal<Record<string, boolean>>({});

  let messagesRef: HTMLDivElement | undefined;

  // Fetch available tools
  createEffect(() => {
    fetchTools().then(setAvailableTools).catch(() => {});
  });

  // ── WebSocket event subscriptions ──────────────────────────────────────

  onCleanup(
    onToken((evt: WSTokenEvent) => {
      if (evt.session_id !== props.sessionId) return;
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
        props.onAddMessage({
          role: "assistant",
          content: fullText,
        });
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

      // Try to parse args from the matching pending tool call
      let args: Record<string, unknown> | undefined;
      const pending = pendingToolCalls();
      const matchIdx = pending.findIndex((tc) => tc.function.name === evt.tool_name);
      if (matchIdx >= 0) {
        try {
          args = JSON.parse(pending[matchIdx].function.arguments);
        } catch {
          // not valid JSON, skip
        }
      }

      // Store the result keyed by tool name + timestamp
      const key = `${evt.tool_name}_${Date.now()}`;
      setToolResults((prev) => ({
        ...prev,
        [key]: {
          result: evt.result,
          isError: evt.is_error,
          toolName: evt.tool_name,
          args,
        },
      }));

      // Also add as a tool message in the chat
      props.onAddMessage({
        role: "tool",
        content: evt.result,
        name: evt.tool_name,
      });

      // Remove matching pending tool call
      setPendingToolCalls((prev) => {
        const idx = prev.findIndex((tc) => tc.function.name === evt.tool_name);
        if (idx >= 0) {
          const next = [...prev];
          next.splice(idx, 1);
          return next;
        }
        return prev;
      });
    })
  );

  // Auto-scroll on new messages
  createEffect(() => {
    const _ = props.messages.length;
    const __ = streamingText();
    if (messagesRef) {
      requestAnimationFrame(() => {
        messagesRef!.scrollTop = messagesRef!.scrollHeight;
      });
    }
  });

  // ── Helpers ────────────────────────────────────────────────────────────

  function parseArgs(raw: string): Record<string, unknown> {
    try {
      return JSON.parse(raw);
    } catch {
      return {};
    }
  }

  function formatArgs(raw: string): string {
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }

  function toggleCollapse(key: string) {
    setCollapsedTools((prev) => ({ ...prev, [key]: !prev[key] }));
  }

  function isCollapsed(key: string): boolean {
    return collapsedTools()[key] ?? false;
  }

  /**
   * Extract old_text / new_text from tool call arguments for edit_file / write_file.
   */
  function getEditContent(args: Record<string, unknown>): {
    oldText: string;
    newText: string;
    path: string;
  } | null {
    const path = (args.path as string) || "";
    const oldText = args.old_text as string | undefined;
    const newText = args.new_text as string | undefined;
    const content = args.content as string | undefined;

    if (oldText !== undefined && newText !== undefined) {
      return { oldText, newText, path };
    }
    if (content !== undefined) {
      // write_file: no old text, just new content
      return { oldText: "", newText: content, path };
    }
    return null;
  }

  /**
   * Parse a tool result string that may contain a Diff structure.
   * The gateway formats diffs as:
   *   Diff for <path>:
   *   --- old
   *   <old content>
   *   +++ new
   *   <new content>
   */
  function parseDiffFromResult(result: string): {
    old: string;
    new: string;
    path: string;
  } | null {
    const diffMatch = result.match(
      /^Diff for (.+?):\n--- old\n([\s\S]*?)\n\+\+\+ new\n([\s\S]*)$/
    );
    if (diffMatch) {
      return {
        path: diffMatch[1],
        old: diffMatch[2],
        new: diffMatch[3],
      };
    }
    return null;
  }

  // ── Tool execution ─────────────────────────────────────────────────────

  async function handleApproveTool(toolCall: ToolCall) {
    const tcKey = toolCall.id;
    setExecutingTools((prev) => ({ ...prev, [tcKey]: true }));

    try {
      const args = parseArgs(toolCall.function.arguments);
      const response = await executeTool(toolCall.function.name, args);

      // Store result
      const resultKey = `${toolCall.id}_result`;
      setToolResults((prev) => ({
        ...prev,
        [resultKey]: {
          result: response.result,
          isError: response.is_error,
          toolName: toolCall.function.name,
          args,
        },
      }));

      // Add tool result message for the LLM conversation
      const toolMsg: ChatMessage = {
        role: "tool",
        content: response.result,
        tool_call_id: toolCall.id,
        name: toolCall.function.name,
      };
      const newMessages = [...props.messages, toolMsg];
      props.onSetMessages(newMessages);

      // Remove from pending
      setPendingToolCalls((prev) => prev.filter((tc) => tc.id !== toolCall.id));

      // Re-send to LLM with tool result
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
            props.onAddMessage({
              role: "assistant",
              content: `Error: ${error.message}`,
            });
            props.setIsLoading(false);
          },
        }
      );
    } catch (err) {
      console.error("Tool execution error:", err);
      // Show error as tool result
      const resultKey = `${toolCall.id}_result`;
      setToolResults((prev) => ({
        ...prev,
        [resultKey]: {
          result: `Execution failed: ${err instanceof Error ? err.message : String(err)}`,
          isError: true,
          toolName: toolCall.function.name,
          args: parseArgs(toolCall.function.arguments),
        },
      }));
      setPendingToolCalls((prev) => prev.filter((tc) => tc.id !== toolCall.id));
    } finally {
      setExecutingTools((prev) => {
        const next = { ...prev };
        delete next[tcKey];
        return next;
      });
    }
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

  // ── Send message ───────────────────────────────────────────────────────

  async function handleSend() {
    const text = input().trim();
    if (!text || props.isLoading) return;

    setInput("");
    setStreamingText("");
    setPendingToolCalls([]);
    setWsStreaming(false);
    setToolResults({});

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
            if (wsStreaming()) return;
            setStreamingText((prev) => prev + token);
          },
          onToolCall(toolCall) {
            if (wsStreaming()) return;
            setPendingToolCalls((prev) => [...prev, toolCall]);
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
            console.error("Stream error:", error);
            setIsStreaming(false);
            setStreamingText("");
            props.onAddMessage({
              role: "assistant",
              content: `Error: ${error.message}`,
            });
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

  // ── Role display ───────────────────────────────────────────────────────

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

  /** Get an icon for the tool type */
  function getToolIcon(toolName: string): string {
    switch (toolName) {
      case "edit_file":
      case "write_file":
      case "apply_patch":
        return "M";
      case "read_file":
        return "R";
      case "run_command":
      case "get_diagnostics":
        return "$";
      case "list_dir":
        return "/";
      default:
        return "T";
    }
  }

  /** Get a color class for the tool type */
  function getToolColor(toolName: string): string {
    switch (toolName) {
      case "edit_file":
      case "write_file":
      case "apply_patch":
        return "bg-blue-900/40 border-blue-700";
      case "read_file":
        return "bg-green-900/40 border-green-700";
      case "run_command":
      case "get_diagnostics":
        return "bg-amber-900/40 border-amber-700";
      case "list_dir":
        return "bg-teal-900/40 border-teal-700";
      default:
        return "bg-ide-panel border-ide-border";
    }
  }

  // ── Tool result renderers ──────────────────────────────────────────────

  function renderDiffView(oldText: string, newText: string, path: string) {
    const oldLines = oldText.split("\n");
    const newLines = newText.split("\n");
    const maxLen = Math.max(oldLines.length, newLines.length);

    // Simple line-by-line diff
    const diffLines: { type: "same" | "removed" | "added"; old: string; new: string; lineNum: number }[] = [];
    for (let i = 0; i < maxLen; i++) {
      const oldLine = i < oldLines.length ? oldLines[i] : undefined;
      const newLine = i < newLines.length ? newLines[i] : undefined;

      if (oldLine === newLine) {
        diffLines.push({ type: "same", old: oldLine ?? "", new: newLine ?? "", lineNum: i + 1 });
      } else if (oldLine !== undefined && newLine !== undefined) {
        diffLines.push({ type: "removed", old: oldLine, new: "", lineNum: i + 1 });
        diffLines.push({ type: "added", old: "", new: newLine, lineNum: i + 1 });
      } else if (oldLine !== undefined) {
        diffLines.push({ type: "removed", old: oldLine, new: "", lineNum: i + 1 });
      } else {
        diffLines.push({ type: "added", old: "", new: newLine ?? "", lineNum: i + 1 });
      }
    }

    return (
      <div class="mt-1 rounded overflow-hidden border border-ide-border">
        {path && (
          <div class="px-2 py-1 bg-ide-panel text-xs text-ide-muted font-mono border-b border-ide-border">
            {path}
          </div>
        )}
        <div class="overflow-x-auto max-h-64 overflow-y-auto">
          <table class="w-full text-xs font-mono">
            <tbody>
              <For each={diffLines}>
                {(line) => (
                  <tr
                    classList={{
                      "bg-red-950/30": line.type === "removed",
                      "bg-green-950/30": line.type === "added",
                      "bg-transparent": line.type === "same",
                    }}
                  >
                    <td class="px-2 py-0.5 text-right text-ide-muted select-none w-8 border-r border-ide-border">
                      {line.type !== "added" ? line.lineNum : ""}
                    </td>
                    <td class="px-2 py-0.5 text-right text-ide-muted select-none w-8 border-r border-ide-border">
                      {line.type !== "removed" ? line.lineNum : ""}
                    </td>
                    <td class="px-2 py-0.5 whitespace-pre">
                      <span
                        classList={{
                          "text-red-400": line.type === "removed",
                          "text-green-400": line.type === "added",
                          "text-ide-text": line.type === "same",
                        }}
                      >
                        {line.type === "removed"
                          ? `- ${line.old}`
                          : line.type === "added"
                          ? `+ ${line.new}`
                          : `  ${line.old}`}
                      </span>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </div>
    );
  }

  function renderToolResult(entry: ToolResultEntry) {
    const toolName = entry.toolName;

    // Check if result contains a diff structure (from edit_file)
    const parsedDiff = parseDiffFromResult(entry.result);

    // edit_file: show diff view
    if (toolName === "edit_file") {
      if (parsedDiff) {
        return renderDiffView(parsedDiff.old, parsedDiff.new, parsedDiff.path);
      }
      // Fall back to args-based diff if result doesn't contain diff
      if (entry.args) {
        const edit = getEditContent(entry.args);
        if (edit) {
          return renderDiffView(edit.oldText, edit.newText, edit.path);
        }
      }
    }

    // write_file: show content preview
    if (toolName === "write_file" && entry.args) {
      const content = entry.args.content as string;
      if (content) {
        return (
          <div class="mt-1 rounded border border-ide-border overflow-hidden">
            <div class="px-2 py-1 bg-ide-panel text-xs text-ide-muted font-mono border-b border-ide-border">
              {(entry.args.path as string) || "file"} (new content)
            </div>
            <pre class="text-xs p-2 bg-ide-bg overflow-x-auto max-h-40 overflow-y-auto text-green-300">
              {content}
            </pre>
          </div>
        );
      }
    }

    // read_file: show file content
    if (toolName === "read_file") {
      return (
        <div class="mt-1 rounded border border-ide-border overflow-hidden">
          <div class="px-2 py-1 bg-ide-panel text-xs text-ide-muted font-mono border-b border-ide-border">
            {(entry.args?.path as string) || "file"} content
          </div>
          <pre class="text-xs p-2 bg-ide-bg overflow-x-auto max-h-48 overflow-y-auto text-ide-text">
            {entry.result}
          </pre>
        </div>
      );
    }

    // run_command: show output in code block
    if (toolName === "run_command" || toolName === "get_diagnostics") {
      return (
        <div class="mt-1 rounded border border-ide-border overflow-hidden">
          <Show when={toolName === "run_command" && entry.args?.command}>
            <div class="px-2 py-1 bg-ide-panel text-xs text-amber-400 font-mono border-b border-ide-border flex items-center gap-1">
              <span class="text-ide-muted">$</span> {entry.args!.command as string}
            </div>
          </Show>
          <pre
            class="text-xs p-2 bg-ide-bg overflow-x-auto max-h-48 overflow-y-auto font-mono"
            classList={{
              "text-ide-text": !entry.isError,
              "text-red-400": entry.isError,
            }}
          >
            {entry.result || "(no output)"}
          </pre>
        </div>
      );
    }

    // apply_patch: show result
    if (toolName === "apply_patch") {
      return (
        <pre class="text-xs mt-1 p-2 bg-ide-bg rounded border border-ide-border overflow-x-auto max-h-40 overflow-y-auto text-ide-text">
          {entry.result}
        </pre>
      );
    }

    // list_dir: show in a formatted list
    if (toolName === "list_dir") {
      return (
        <pre class="text-xs mt-1 p-2 bg-ide-bg rounded border border-ide-border overflow-x-auto max-h-40 overflow-y-auto font-mono text-ide-text">
          {entry.result}
        </pre>
      );
    }

    // Default: plain text result
    return (
      <pre
        class="text-xs mt-1 p-2 bg-ide-bg rounded border border-ide-border overflow-x-auto max-h-40 overflow-y-auto"
        classList={{
          "text-ide-text": !entry.isError,
          "text-red-400": entry.isError,
        }}
      >
        {entry.result}
      </pre>
    );
  }

  // ── Render a tool call card (for messages with tool_calls) ─────────────

  function renderToolCallCard(tc: ToolCall, msgIndex: number, tcIndex: number) {
    const collapseKey = `msg_${msgIndex}_tc_${tcIndex}`;
    const collapsed = isCollapsed(collapseKey);
    const toolName = tc.function.name;
    const args = parseArgs(tc.function.arguments);

    return (
      <div
        class={`mt-2 rounded border overflow-hidden ${getToolColor(toolName)}`}
      >
        {/* Tool call header */}
        <button
          class="w-full flex items-center gap-2 px-2 py-1.5 text-xs hover:bg-white/5 transition-colors cursor-pointer"
          onClick={() => toggleCollapse(collapseKey)}
        >
          <span class="w-5 h-5 rounded bg-ide-bg flex items-center justify-center text-[10px] font-bold font-mono text-purple-400">
            {getToolIcon(toolName)}
          </span>
          <span class="font-mono font-medium text-purple-300">{toolName}</span>
          <Show when={args.path}>
            <span class="text-ide-muted truncate flex-1 text-left">
              {args.path as string}
            </span>
          </Show>
          <Show when={args.command}>
            <span class="text-ide-muted truncate flex-1 text-left font-mono">
              {(args.command as string).slice(0, 50)}
            </span>
          </Show>
          <svg
            class="w-3 h-3 text-ide-muted transition-transform"
            classList={{ "rotate-180": !collapsed }}
            viewBox="0 0 12 12"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M3 4.5L6 7.5L9 4.5" />
          </svg>
        </button>

        {/* Tool call body (collapsible) */}
        <Show when={!collapsed}>
          <div class="px-2 pb-2 border-t border-white/5">
            {/* Arguments */}
            <div class="mt-1.5">
              <span class="text-[10px] text-ide-muted uppercase tracking-wider">
                Arguments
              </span>
              <pre class="text-xs text-ide-muted mt-0.5 overflow-x-auto bg-ide-bg/50 rounded p-1.5">
                {formatArgs(tc.function.arguments)}
              </pre>
            </div>
          </div>
        </Show>
      </div>
    );
  }

  // ── Render a completed tool result card (for tool-role messages) ───────

  function renderToolResultMessage(msg: ChatMessage, msgIndex: number) {
    const toolName = msg.name || "tool";
    const collapseKey = `tool_result_${msgIndex}`;
    const collapsed = isCollapsed(collapseKey);

    // Try to find matching tool call args from previous assistant message
    let matchingArgs: Record<string, unknown> | undefined;
    for (let i = msgIndex - 1; i >= 0; i--) {
      const prev = props.messages[i];
      if (prev.role === "assistant" && prev.tool_calls) {
        const match = prev.tool_calls.find(
          (tc) => tc.function.name === toolName
        );
        if (match) {
          matchingArgs = parseArgs(match.function.arguments);
          break;
        }
      }
      // Stop searching if we hit a user message
      if (prev.role === "user") break;
    }

    const entry: ToolResultEntry = {
      result: msg.content || "",
      isError: msg.content?.startsWith("Error:") || msg.content?.startsWith("Tool error:") || false,
      toolName,
      args: matchingArgs,
    };

    // Check if this is a rejection message
    const isRejected = msg.content?.includes("rejected by the user");

    return (
      <div
        class={`mt-1.5 rounded border overflow-hidden ${
          isRejected
            ? "bg-red-900/20 border-red-800"
            : entry.isError
            ? "bg-red-900/20 border-red-800"
            : getToolColor(toolName)
        }`}
      >
        {/* Result header */}
        <button
          class="w-full flex items-center gap-2 px-2 py-1.5 text-xs hover:bg-white/5 transition-colors cursor-pointer"
          onClick={() => toggleCollapse(collapseKey)}
        >
          <Show when={isRejected}>
            <span class="text-red-400 font-medium">Rejected</span>
          </Show>
          <Show when={!isRejected && entry.isError}>
            <svg class="w-3.5 h-3.5 text-red-400" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 10.5a.75.75 0 110-1.5.75.75 0 010 1.5zM8.75 4.75v4a.75.75 0 01-1.5 0v-4a.75.75 0 011.5 0z" />
            </svg>
            <span class="text-red-400 font-medium">Error</span>
          </Show>
          <Show when={!isRejected && !entry.isError}>
            <svg class="w-3.5 h-3.5 text-green-400" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm3.78 5.22a.75.75 0 010 1.06l-4.25 4.25a.75.75 0 01-1.06 0L4.22 9.28a.75.75 0 011.06-1.06L7 9.94l3.72-3.72a.75.75 0 011.06 0z" />
            </svg>
            <span class="text-green-400 font-medium">Done</span>
          </Show>
          <span class="font-mono text-ide-muted">{toolName}</span>
          <Show when={matchingArgs?.path}>
            <span class="text-ide-muted truncate flex-1 text-left">
              {matchingArgs!.path as string}
            </span>
          </Show>
          <svg
            class="w-3 h-3 text-ide-muted transition-transform"
            classList={{ "rotate-180": !collapsed }}
            viewBox="0 0 12 12"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M3 4.5L6 7.5L9 4.5" />
          </svg>
        </button>

        {/* Result body (collapsible) */}
        <Show when={!collapsed}>
          <div class="px-2 pb-2 border-t border-white/5">
            <Show when={!isRejected}>
              {renderToolResult(entry)}
            </Show>
            <Show when={isRejected}>
              <p class="text-xs text-red-300 mt-1">
                Tool execution was rejected by the user.
              </p>
            </Show>
          </div>
        </Show>
      </div>
    );
  }

  // ── Main render ────────────────────────────────────────────────────────

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
          {(msg, msgIndex) => (
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <span class={`text-xs font-medium ${getRoleColor(msg.role)}`}>
                  {getRoleLabel(msg.role)}
                </span>
                {msg.name && (
                  <span class="text-xs text-ide-muted">({msg.name})</span>
                )}
                {msg.tool_call_id && (
                  <span class="text-[10px] text-ide-muted font-mono">
                    id: {msg.tool_call_id.slice(0, 12)}...
                  </span>
                )}
              </div>
              <div class="text-sm text-ide-text whitespace-pre-wrap break-words pl-2 border-l-2 border-ide-border">
                {/* Regular content (skip for tool messages that are just results) */}
                <Show when={msg.role !== "tool" || !msg.name}>
                  {msg.content || ""}
                </Show>

                {/* Tool calls in assistant messages */}
                <Show when={msg.tool_calls && msg.tool_calls.length > 0}>
                  <For each={msg.tool_calls}>
                    {(tc, tcIndex) => renderToolCallCard(tc, msgIndex(), tcIndex())}
                  </For>
                </Show>

                {/* Tool result messages */}
                <Show when={msg.role === "tool" && msg.name}>
                  {renderToolResultMessage(msg, msgIndex())}
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
            {(tc) => {
              const isExecuting = () => executingTools()[tc.id] ?? false;
              return (
                <div class="p-3 bg-amber-950/30 rounded border border-amber-700">
                  <div class="flex items-center gap-2 text-sm text-amber-400 mb-2">
                    <span class="w-5 h-5 rounded bg-ide-bg flex items-center justify-center text-[10px] font-bold font-mono text-amber-400">
                      {getToolIcon(tc.function.name)}
                    </span>
                    <span class="font-medium">Tool Request:</span>
                    <span class="font-mono">{tc.function.name}</span>
                  </div>
                  <pre class="text-xs text-ide-muted mb-3 overflow-x-auto bg-ide-bg/50 p-2 rounded border border-ide-border">
                    {formatArgs(tc.function.arguments)}
                  </pre>
                  <div class="flex gap-2">
                    <button
                      class="px-3 py-1.5 text-xs bg-green-700 text-white rounded hover:bg-green-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5"
                      onClick={() => handleApproveTool(tc)}
                      disabled={isExecuting()}
                    >
                      <Show when={isExecuting()}>
                        <div class="w-3 h-3 border border-white/50 border-t-white rounded-full animate-spin" />
                      </Show>
                      {isExecuting() ? "Executing..." : "Approve & Execute"}
                    </button>
                    <button
                      class="px-3 py-1.5 text-xs bg-red-700 text-white rounded hover:bg-red-600 transition-colors disabled:opacity-50"
                      onClick={() => handleRejectTool(tc)}
                      disabled={isExecuting()}
                    >
                      Reject
                    </button>
                  </div>
                </div>
              );
            }}
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
