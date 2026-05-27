import { createSignal, createEffect, onCleanup, Show, For, onMount } from "solid-js";
import {
  submitTask,
  listTasks,
  getTask,
  cancelTask,
  fetchCapabilities,
  type AgentTask,
  type AgentStep,
} from "../lib/agent";

interface AgentPanelProps {
  onAgentEvent?: (event: unknown) => void;
}

export function AgentPanel(props: AgentPanelProps) {
  const [tasks, setTasks] = createSignal<AgentTask[]>([]);
  const [selectedTask, setSelectedTask] = createSignal<AgentTask | null>(null);
  const [input, setInput] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [capabilities, setCapabilities] = createSignal<string[]>([]);
  const [autoRefresh, setAutoRefresh] = createSignal(true);
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  onMount(async () => {
    const [t, c] = await Promise.all([listTasks(), fetchCapabilities()]);
    setTasks(t);
    setCapabilities(c);
  });

  // Auto-refresh task list every 3 seconds
  createEffect(() => {
    if (refreshInterval) clearInterval(refreshInterval);
    if (autoRefresh()) {
      refreshInterval = setInterval(async () => {
        const t = await listTasks();
        setTasks(t);
        // Also refresh selected task if it's active
        const sel = selectedTask();
        if (sel && (sel.status === "Planning" || sel.status === "InProgress")) {
          const updated = await getTask(sel.id);
          if (updated) setSelectedTask(updated);
        }
      }, 3000);
    }
  });

  onCleanup(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const desc = input().trim();
    if (!desc || submitting()) return;

    setSubmitting(true);
    try {
      const { task_id } = await submitTask(desc);
      setInput("");
      // Refresh task list and select the new task
      const t = await listTasks();
      setTasks(t);
      const task = t.find((x) => x.id === task_id);
      if (task) setSelectedTask(task);
    } catch (err) {
      console.error("Failed to submit task:", err);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCancel(taskId: string) {
    await cancelTask(taskId);
    const t = await listTasks();
    setTasks(t);
    const sel = selectedTask();
    if (sel?.id === taskId) {
      const updated = await getTask(taskId);
      if (updated) setSelectedTask(updated);
    }
  }

  async function handleSelectTask(taskId: string) {
    const task = await getTask(taskId);
    if (task) setSelectedTask(task);
  }

  function statusColor(status: string): string {
    switch (status) {
      case "Completed": return "text-green-400";
      case "InProgress": return "text-yellow-400";
      case "Planning": return "text-blue-400";
      case "Failed": return "text-red-400";
      case "Cancelled": return "text-gray-500";
      default: return "text-ide-muted";
    }
  }

  function statusDot(status: string): string {
    switch (status) {
      case "Completed": return "bg-green-400";
      case "InProgress": return "bg-yellow-400 animate-pulse";
      case "Planning": return "bg-blue-400 animate-pulse";
      case "Failed": return "bg-red-400";
      case "Cancelled": return "bg-gray-500";
      default: return "bg-gray-600";
    }
  }

  function stepIcon(role: string): string {
    switch (role) {
      case "Coding": return "{ }";
      case "Architecture": return "A";
      case "Testing": return "T";
      case "Debugging": return "D";
      case "GitHub": return "G";
      case "Wiki": return "W";
      default: return "?";
    }
  }

  return (
    <div class="flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div class="px-3 py-2 border-b border-ide-border shrink-0">
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-xs font-semibold text-ide-text uppercase tracking-wider">Agent Tasks</h3>
          <div class="flex items-center gap-2">
            <Show when={capabilities().length > 0}>
              <span class="text-[10px] text-ide-muted">
                {capabilities().length} agents
              </span>
            </Show>
            <button
              class="text-[10px] px-1.5 py-0.5 rounded border border-ide-border hover:bg-ide-panel text-ide-muted"
              onClick={() => setAutoRefresh((v) => !v)}
              title={autoRefresh() ? "Pause auto-refresh" : "Resume auto-refresh"}
            >
              {autoRefresh() ? "Live" : "Paused"}
            </button>
          </div>
        </div>

        {/* Task input */}
        <form onSubmit={handleSubmit} class="flex gap-1">
          <input
            type="text"
            class="flex-1 bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text placeholder-ide-muted focus:border-ide-accent focus:outline-none"
            placeholder="Describe a task for the agent team..."
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            disabled={submitting()}
          />
          <button
            type="submit"
            class="bg-ide-accent hover:bg-blue-600 text-white px-3 py-1.5 rounded text-xs font-medium disabled:opacity-50 transition-colors"
            disabled={submitting() || !input().trim()}
          >
            {submitting() ? "..." : "Run"}
          </button>
        </form>
      </div>

      {/* Content: task list + detail */}
      <div class="flex flex-1 overflow-hidden">
        {/* Task list */}
        <div class="w-1/3 border-r border-ide-border overflow-auto shrink-0">
          <Show when={tasks().length === 0}>
            <div class="p-3 text-xs text-ide-muted text-center">
              No tasks yet. Submit one above.
            </div>
          </Show>
          <For each={tasks()}>
            {(task) => (
              <button
                class="w-full text-left px-3 py-2 border-b border-ide-border hover:bg-ide-panel transition-colors"
                classList={{ "bg-ide-panel": selectedTask()?.id === task.id }}
                onClick={() => handleSelectTask(task.id)}
              >
                <div class="flex items-center gap-2 mb-1">
                  <span class={`w-2 h-2 rounded-full shrink-0 ${statusDot(task.status)}`} />
                  <span class="text-xs font-medium text-ide-text truncate flex-1">
                    {task.description.slice(0, 40)}{task.description.length > 40 ? "..." : ""}
                  </span>
                </div>
                <div class="flex items-center gap-2 text-[10px] text-ide-muted">
                  <span class={statusColor(task.status)}>{task.status}</span>
                  <span>{task.steps.length} steps</span>
                </div>
              </button>
            )}
          </For>
        </div>

        {/* Task detail */}
        <div class="flex-1 overflow-auto">
          <Show when={selectedTask()} fallback={
            <div class="p-4 text-xs text-ide-muted text-center">
              Select a task to view details
            </div>
          }>
            {(task) => (
              <div class="p-3">
                {/* Task header */}
                <div class="flex items-start justify-between mb-3">
                  <div class="flex-1">
                    <h4 class="text-sm font-medium text-ide-text mb-1">
                      {task().description}
                    </h4>
                    <div class="flex items-center gap-3 text-[10px] text-ide-muted">
                      <span class={statusColor(task().status)}>{task().status}</span>
                      <span>ID: {task().id.slice(0, 8)}</span>
                      <span>{new Date(task().created_at).toLocaleTimeString()}</span>
                    </div>
                  </div>
                  <Show when={task().status === "InProgress" || task().status === "Planning"}>
                    <button
                      class="text-[10px] px-2 py-1 rounded border border-red-800 text-red-400 hover:bg-red-900/30 transition-colors"
                      onClick={() => handleCancel(task().id)}
                    >
                      Cancel
                    </button>
                  </Show>
                </div>

                {/* Steps */}
                <div class="space-y-2">
                  <For each={task().steps}>
                    {(step, idx) => (
                      <div class="border border-ide-border rounded p-2 bg-ide-bg">
                        <div class="flex items-center gap-2 mb-1">
                          <span class="w-5 h-5 rounded bg-ide-panel flex items-center justify-center text-[10px] font-mono text-ide-muted shrink-0">
                            {stepIcon(step.assigned_to)}
                          </span>
                          <span class="text-xs text-ide-text flex-1">{step.description}</span>
                          <span class={`w-2 h-2 rounded-full shrink-0 ${statusDot(step.status)}`} />
                        </div>
                        <div class="flex items-center gap-2 text-[10px] text-ide-muted ml-7">
                          <span>{step.assigned_to}</span>
                          <span class={statusColor(step.status)}>{step.status}</span>
                        </div>
                        <Show when={step.result}>
                          <pre class="mt-2 ml-7 text-[11px] text-green-300 bg-ide-panel rounded p-2 overflow-auto max-h-32 whitespace-pre-wrap">
                            {step.result}
                          </pre>
                        </Show>
                        <Show when={step.error}>
                          <pre class="mt-2 ml-7 text-[11px] text-red-300 bg-ide-panel rounded p-2 overflow-auto max-h-32 whitespace-pre-wrap">
                            {step.error}
                          </pre>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>

                {/* Capabilities */}
                <Show when={capabilities().length > 0}>
                  <div class="mt-4 pt-3 border-t border-ide-border">
                    <h5 class="text-[10px] font-semibold text-ide-muted uppercase tracking-wider mb-2">
                      Available Agents
                    </h5>
                    <div class="flex flex-wrap gap-1">
                      <For each={capabilities()}>
                        {(cap) => (
                          <span class="text-[10px] px-2 py-0.5 rounded-full bg-ide-panel text-ide-muted border border-ide-border">
                            {cap}
                          </span>
                        )}
                      </For>
                    </div>
                  </div>
                </Show>
              </div>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}
