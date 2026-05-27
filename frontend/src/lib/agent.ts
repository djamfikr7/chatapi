const API_BASE = "";

export interface AgentTask {
  id: string;
  description: string;
  status: "Planning" | "InProgress" | "Completed" | "Failed" | "Cancelled";
  steps: AgentStep[];
  created_at: string;
  updated_at: string;
}

export interface AgentStep {
  id: string;
  description: string;
  assigned_to: string;
  status: "Planning" | "InProgress" | "Completed" | "Failed" | "Cancelled";
  result?: string;
  error?: string;
}

// POST /agents/tasks
export async function submitTask(description: string): Promise<{ task_id: string }> {
  const res = await fetch(`${API_BASE}/agents/tasks`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ description }),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Submit failed (${res.status}): ${err}`);
  }
  return res.json();
}

// GET /agents/tasks
export async function listTasks(): Promise<AgentTask[]> {
  const res = await fetch(`${API_BASE}/agents/tasks`);
  if (!res.ok) return [];
  const data = await res.json();
  return data.tasks || [];
}

// GET /agents/tasks/:id
export async function getTask(taskId: string): Promise<AgentTask | null> {
  const res = await fetch(`${API_BASE}/agents/tasks/${taskId}`);
  if (!res.ok) return null;
  return res.json();
}

// POST /agents/tasks/:id/cancel
export async function cancelTask(taskId: string): Promise<void> {
  await fetch(`${API_BASE}/agents/tasks/${taskId}/cancel`, { method: "POST" });
}

// GET /agents/capabilities
export async function fetchCapabilities(): Promise<string[]> {
  const res = await fetch(`${API_BASE}/agents/capabilities`);
  if (!res.ok) return [];
  const data = await res.json();
  return data.agents || [];
}
