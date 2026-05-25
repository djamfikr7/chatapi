const API_BASE = "";

export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
  name?: string;
}

export interface ToolCall {
  id: string;
  type: "function";
  function: {
    name: string;
    arguments: string;
  };
}

export interface ChatCompletionRequest {
  model: string;
  messages: ChatMessage[];
  stream?: boolean;
  tools?: ToolDefinition[];
}

export interface ToolDefinition {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

export interface ChatCompletionResponse {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: {
    index: number;
    message: ChatMessage;
    finish_reason: string;
  }[];
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

export interface Session {
  id: string;
  model: string;
  created_at: number;
  updated_at?: number;
  messages?: ChatMessage[];
  metadata?: Record<string, unknown>;
}

export interface ToolInfo {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

export interface HealthStatus {
  status: string;
  mode: string;
  tool_count: number;
}

export interface McpServer {
  name: string;
  command: string;
  args: string[];
}

export interface ConfigData {
  target: {
    mode: string;
    model: string;
    mcp?: {
      servers: McpServer[];
    };
  };
  rules: {
    system_prompt?: string;
    working_dir?: string;
    allowed_tools: string[];
    blocked_paths: string[];
  };
  sessions: {
    store: string;
  };
}

// Health check
export async function fetchHealth(): Promise<HealthStatus> {
  const res = await fetch(`${API_BASE}/health`);
  return res.json();
}

// Models
export async function fetchModels(): Promise<{ data: { id: string }[] }> {
  const res = await fetch(`${API_BASE}/v1/models`);
  return res.json();
}

// Sessions
export async function fetchSessions(): Promise<Session[]> {
  const res = await fetch(`${API_BASE}/sessions`);
  const data = await res.json();
  return data.sessions || [];
}

export async function createSession(model: string = "deepseek-chat"): Promise<Session> {
  const res = await fetch(`${API_BASE}/sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ model }),
  });
  return res.json();
}

export async function getSession(id: string): Promise<Session & { messages: ChatMessage[] }> {
  const res = await fetch(`${API_BASE}/sessions/${id}`);
  return res.json();
}

export async function deleteSession(id: string): Promise<void> {
  await fetch(`${API_BASE}/sessions/${id}`, { method: "DELETE" });
}

// Tools
export async function fetchTools(): Promise<ToolInfo[]> {
  const res = await fetch(`${API_BASE}/tools`);
  const data = await res.json();
  return data.tools || [];
}

// Config
export async function fetchConfig(): Promise<ConfigData> {
  const res = await fetch(`${API_BASE}/config`);
  return res.json();
}

export async function updateConfig(updates: Partial<ConfigData>): Promise<void> {
  await fetch(`${API_BASE}/config`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates),
  });
}

// Non-streaming chat completion
export async function sendChatCompletion(
  request: ChatCompletionRequest
): Promise<ChatCompletionResponse> {
  const res = await fetch(`${API_BASE}/v1/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ...request, stream: false }),
  });
  return res.json();
}

// Execute a tool locally (via the gateway's run_command tool through chat)
export async function executeTool(
  toolName: string,
  args: Record<string, unknown>
): Promise<string> {
  const res = await fetch(`${API_BASE}/v1/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model: "deepseek-chat",
      messages: [
        {
          role: "user",
          content: `Execute tool: ${toolName} with args: ${JSON.stringify(args)}`,
        },
      ],
      stream: false,
    }),
  });
  const data: ChatCompletionResponse = await res.json();
  return data.choices?.[0]?.message?.content || "No response";
}
