import type { ChatCompletionRequest, ChatMessage, ToolCall } from "./api";

export interface StreamChunk {
  id: string;
  object: string;
  choices: {
    index: number;
    delta: {
      role?: string;
      content?: string;
      tool_calls?: {
        index: number;
        id?: string;
        type?: string;
        function?: {
          name?: string;
          arguments?: string;
        };
      }[];
    };
    finish_reason: string | null;
  }[];
}

export interface StreamCallbacks {
  onToken: (token: string) => void;
  onToolCall: (toolCall: ToolCall) => void;
  onDone: (fullText: string, toolCalls: ToolCall[]) => void;
  onError: (error: Error) => void;
}

/**
 * Parse an SSE data line into a StreamChunk.
 * Returns null for [DONE] or unparseable lines.
 */
function parseSSELine(line: string): StreamChunk | "[DONE]" | null {
  const trimmed = line.trim();
  if (!trimmed || !trimmed.startsWith("data: ")) {
    return null;
  }
  const data = trimmed.slice(6);
  if (data === "[DONE]") {
    return "[DONE]";
  }
  try {
    return JSON.parse(data) as StreamChunk;
  } catch {
    return null;
  }
}

/**
 * Stream a chat completion request via SSE using fetch + ReadableStream.
 * This approach is more reliable than EventSource for POST requests.
 */
export async function streamChatCompletion(
  request: ChatCompletionRequest,
  callbacks: StreamCallbacks
): Promise<void> {
  let fullText = "";
  const toolCallsMap = new Map<number, ToolCall>();

  try {
    const response = await fetch("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...request, stream: true }),
    });

    if (!response.ok) {
      const errorText = await response.text();
      callbacks.onError(new Error(`HTTP ${response.status}: ${errorText}`));
      return;
    }

    if (!response.body) {
      callbacks.onError(new Error("No response body"));
      return;
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Process complete SSE messages (separated by double newlines)
      const messages = buffer.split("\n\n");
      buffer = messages.pop() || "";

      for (const msg of messages) {
        const lines = msg.split("\n");
        for (const line of lines) {
          const parsed = parseSSELine(line);
          if (parsed === "[DONE]") {
            // Convert tool calls map to array sorted by index
            const toolCalls = Array.from(toolCallsMap.entries())
              .sort(([a], [b]) => a - b)
              .map(([, tc]) => tc);
            callbacks.onDone(fullText, toolCalls);
            return;
          }
          if (parsed && parsed.choices) {
            for (const choice of parsed.choices) {
              // Handle content delta
              if (choice.delta.content) {
                fullText += choice.delta.content;
                callbacks.onToken(choice.delta.content);
              }

              // Handle tool call deltas
              if (choice.delta.tool_calls) {
                for (const tcDelta of choice.delta.tool_calls) {
                  const idx = tcDelta.index;
                  if (!toolCallsMap.has(idx)) {
                    toolCallsMap.set(idx, {
                      id: tcDelta.id || `call_${idx}`,
                      type: "function",
                      function: {
                        name: tcDelta.function?.name || "",
                        arguments: tcDelta.function?.arguments || "",
                      },
                    });
                  } else {
                    const existing = toolCallsMap.get(idx)!;
                    if (tcDelta.id) existing.id = tcDelta.id;
                    if (tcDelta.type) existing.type = tcDelta.type as "function";
                    if (tcDelta.function?.name) {
                      existing.function.name += tcDelta.function.name;
                    }
                    if (tcDelta.function?.arguments) {
                      existing.function.arguments += tcDelta.function.arguments;
                    }
                  }
                }
              }

              // Handle finish reason
              if (choice.finish_reason === "stop") {
                callbacks.onDone(fullText, []);
                return;
              }
              if (choice.finish_reason === "tool_calls") {
                const toolCalls = Array.from(toolCallsMap.entries())
                  .sort(([a], [b]) => a - b)
                  .map(([, tc]) => tc);
                callbacks.onDone(fullText, toolCalls);
                return;
              }
            }
          }
        }
      }
    }

    // If we reach here without [DONE], finalize
    const toolCalls = Array.from(toolCallsMap.entries())
      .sort(([a], [b]) => a - b)
      .map(([, tc]) => tc);
    callbacks.onDone(fullText, toolCalls);
  } catch (err) {
    callbacks.onError(err instanceof Error ? err : new Error(String(err)));
  }
}

/**
 * Build the full messages array for a chat request from conversation history.
 */
export function buildMessages(
  history: ChatMessage[],
  systemPrompt?: string
): ChatMessage[] {
  const messages: ChatMessage[] = [];
  if (systemPrompt) {
    messages.push({
      role: "system",
      content: systemPrompt,
    });
  }
  messages.push(...history);
  return messages;
}
