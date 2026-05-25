import {
  createSignal,
  createEffect,
  Show,
  For,
} from "solid-js";
import { fetchConfig, updateConfig, type ConfigData, type McpServer } from "../lib/api";

interface ConfigPanelProps {
  open: boolean;
  onClose: () => void;
}

export function ConfigPanel(props: ConfigPanelProps) {
  const [config, setConfig] = createSignal<ConfigData | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [saveSuccess, setSaveSuccess] = createSignal(false);

  // Editable form state
  const [targetMode, setTargetMode] = createSignal("browser");
  const [targetModel, setTargetModel] = createSignal("");
  const [systemPrompt, setSystemPrompt] = createSignal("");
  const [workingDir, setWorkingDir] = createSignal("");
  const [allowedTools, setAllowedTools] = createSignal<string[]>([]);
  const [blockedPaths, setBlockedPaths] = createSignal<string[]>([]);
  const [sessionStore, setSessionStore] = createSignal("memory");

  // Tool/path input fields
  const [newTool, setNewTool] = createSignal("");
  const [newPath, setNewPath] = createSignal("");

  // MCP server state
  const [mcpServers, setMcpServers] = createSignal<McpServer[]>([]);
  const [newMcpName, setNewMcpName] = createSignal("");
  const [newMcpCommand, setNewMcpCommand] = createSignal("");
  const [newMcpArgs, setNewMcpArgs] = createSignal("");
  const [newMcpEnv, setNewMcpEnv] = createSignal("");
  const [showAddMcp, setShowAddMcp] = createSignal(false);

  // Load config when panel opens
  createEffect(() => {
    if (props.open) {
      loadConfig();
    }
  });

  async function loadConfig() {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchConfig();
      setConfig(data);
      setTargetMode(data.target.mode);
      setTargetModel(data.target.model);
      setSystemPrompt(data.rules.system_prompt || "");
      setWorkingDir(data.rules.working_dir || "");
      setAllowedTools([...data.rules.allowed_tools]);
      setBlockedPaths([...data.rules.blocked_paths]);
      setSessionStore(data.sessions.store);
      setMcpServers(data.target.mcp?.servers ? [...data.target.mcp.servers] : []);
    } catch (err) {
      setError("Failed to load config");
      console.error("Config load error:", err);
    } finally {
      setLoading(false);
    }
  }

  async function handleSave() {
    setSaving(true);
    setError(null);
    setSaveSuccess(false);
    try {
      const updates: Partial<ConfigData> = {
        target: {
          mode: targetMode(),
          model: targetModel(),
          mcp: {
            servers: mcpServers(),
          },
        },
        rules: {
          system_prompt: systemPrompt(),
          working_dir: workingDir(),
          allowed_tools: allowedTools(),
          blocked_paths: blockedPaths(),
        },
        sessions: {
          store: sessionStore(),
        },
      };
      await updateConfig(updates);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 2000);
    } catch (err) {
      setError("Failed to save config");
      console.error("Config save error:", err);
    } finally {
      setSaving(false);
    }
  }

  function addTool() {
    const tool = newTool().trim();
    if (tool && !allowedTools().includes(tool)) {
      setAllowedTools((prev) => [...prev, tool]);
      setNewTool("");
    }
  }

  function removeTool(tool: string) {
    setAllowedTools((prev) => prev.filter((t) => t !== tool));
  }

  function addPath() {
    const path = newPath().trim();
    if (path && !blockedPaths().includes(path)) {
      setBlockedPaths((prev) => [...prev, path]);
      setNewPath("");
    }
  }

  function removePath(path: string) {
    setBlockedPaths((prev) => prev.filter((p) => p !== path));
  }

  function handleToolKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      addTool();
    }
  }

  function handlePathKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      addPath();
    }
  }

  function parseEnvString(envStr: string): Record<string, string> {
    const env: Record<string, string> = {};
    for (const pair of envStr.split(",")) {
      const trimmed = pair.trim();
      if (!trimmed) continue;
      const eqIdx = trimmed.indexOf("=");
      if (eqIdx === -1) continue;
      const key = trimmed.slice(0, eqIdx).trim();
      const val = trimmed.slice(eqIdx + 1).trim();
      if (key) env[key] = val;
    }
    return env;
  }

  function addMcpServer() {
    const name = newMcpName().trim();
    const command = newMcpCommand().trim();
    if (!name || !command) return;
    if (mcpServers().some((s) => s.name === name)) return;

    const args = newMcpArgs()
      .split(",")
      .map((a) => a.trim())
      .filter(Boolean);
    const env = parseEnvString(newMcpEnv());

    const server: McpServer = { name, command, args };
    if (Object.keys(env).length > 0) server.env = env;

    setMcpServers((prev) => [...prev, server]);
    setNewMcpName("");
    setNewMcpCommand("");
    setNewMcpArgs("");
    setNewMcpEnv("");
    setShowAddMcp(false);
  }

  function removeMcpServer(name: string) {
    setMcpServers((prev) => prev.filter((s) => s.name !== name));
  }

  function handleMcpKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      addMcpServer();
    }
  }

  return (
    <>
      {/* Backdrop */}
      <Show when={props.open}>
        <div
          class="fixed inset-0 bg-black/30 z-40"
          onClick={props.onClose}
        />
      </Show>

      {/* Panel */}
      <div
        class="fixed top-0 right-0 h-full w-[380px] bg-ide-sidebar border-l border-ide-border z-50 flex flex-col shadow-2xl transition-transform duration-200 ease-in-out"
        style={{
          transform: props.open ? "translateX(0)" : "translateX(100%)",
        }}
      >
        {/* Header */}
        <div class="flex items-center justify-between px-4 py-3 border-b border-ide-border shrink-0">
          <h2 class="text-sm font-semibold text-ide-text">Settings</h2>
          <button
            class="text-ide-muted hover:text-ide-text transition-colors p-1"
            onClick={props.onClose}
            title="Close settings"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            >
              <path d="M1 1l12 12M13 1L1 13" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div class="flex-1 overflow-y-auto px-4 py-3 space-y-5">
          <Show when={loading()}>
            <div class="flex items-center gap-2 text-xs text-ide-muted py-8 justify-center">
              <div class="w-3 h-3 bg-ide-accent rounded-full animate-pulse" />
              Loading config...
            </div>
          </Show>

          <Show when={!loading() && error()}>
            <div class="text-xs text-red-400 bg-red-900/20 border border-red-800 rounded p-3">
              {error()}
            </div>
          </Show>

          <Show when={!loading() && config()}>
            {/* ── Target Section ── */}
            <section class="space-y-3">
              <h3 class="text-xs font-semibold text-ide-muted uppercase tracking-wider">
                Target
              </h3>
              <div class="space-y-2">
                <label class="block">
                  <span class="text-xs text-ide-muted mb-1 block">Mode</span>
                  <select
                    class="w-full bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent"
                    value={targetMode()}
                    onChange={(e) => setTargetMode(e.currentTarget.value)}
                  >
                    <option value="browser">browser</option>
                    <option value="api">api</option>
                  </select>
                </label>
                <label class="block">
                  <span class="text-xs text-ide-muted mb-1 block">Model</span>
                  <input
                    type="text"
                    class="w-full bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={targetModel()}
                    onInput={(e) => setTargetModel(e.currentTarget.value)}
                    placeholder="e.g. deepseek-chat"
                  />
                </label>
              </div>
            </section>

            {/* ── Rules Section ── */}
            <section class="space-y-3">
              <h3 class="text-xs font-semibold text-ide-muted uppercase tracking-wider">
                Rules
              </h3>
              <div class="space-y-2">
                <label class="block">
                  <span class="text-xs text-ide-muted mb-1 block">
                    System Prompt
                  </span>
                  <textarea
                    class="w-full bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text resize-none focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    rows={4}
                    value={systemPrompt()}
                    onInput={(e) => setSystemPrompt(e.currentTarget.value)}
                    placeholder="System prompt for the LLM..."
                  />
                </label>
                <label class="block">
                  <span class="text-xs text-ide-muted mb-1 block">
                    Working Directory
                  </span>
                  <input
                    type="text"
                    class="w-full bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={workingDir()}
                    onInput={(e) => setWorkingDir(e.currentTarget.value)}
                    placeholder="/path/to/working/dir"
                  />
                </label>

                {/* Allowed Tools */}
                <div>
                  <span class="text-xs text-ide-muted mb-1 block">
                    Allowed Tools
                  </span>
                  <div class="flex gap-1 mb-1.5">
                    <input
                      type="text"
                      class="flex-1 bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                      value={newTool()}
                      onInput={(e) => setNewTool(e.currentTarget.value)}
                      onKeyDown={handleToolKeyDown}
                      placeholder="Add tool name..."
                    />
                    <button
                      class="px-2 py-1.5 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors"
                      onClick={addTool}
                    >
                      Add
                    </button>
                  </div>
                  <div class="flex flex-wrap gap-1">
                    <For each={allowedTools()}>
                      {(tool) => (
                        <span class="inline-flex items-center gap-1 bg-ide-panel border border-ide-border rounded px-2 py-0.5 text-xs text-ide-text">
                          {tool}
                          <button
                            class="text-ide-muted hover:text-red-400 transition-colors ml-0.5"
                            onClick={() => removeTool(tool)}
                          >
                            x
                          </button>
                        </span>
                      )}
                    </For>
                    <Show when={allowedTools().length === 0}>
                      <span class="text-xs text-ide-muted italic">
                        No restrictions (all tools allowed)
                      </span>
                    </Show>
                  </div>
                </div>

                {/* Blocked Paths */}
                <div>
                  <span class="text-xs text-ide-muted mb-1 block">
                    Blocked Paths
                  </span>
                  <div class="flex gap-1 mb-1.5">
                    <input
                      type="text"
                      class="flex-1 bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                      value={newPath()}
                      onInput={(e) => setNewPath(e.currentTarget.value)}
                      onKeyDown={handlePathKeyDown}
                      placeholder="Add blocked path..."
                    />
                    <button
                      class="px-2 py-1.5 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors"
                      onClick={addPath}
                    >
                      Add
                    </button>
                  </div>
                  <div class="flex flex-wrap gap-1">
                    <For each={blockedPaths()}>
                      {(path) => (
                        <span class="inline-flex items-center gap-1 bg-ide-panel border border-ide-border rounded px-2 py-0.5 text-xs text-ide-text">
                          {path}
                          <button
                            class="text-ide-muted hover:text-red-400 transition-colors ml-0.5"
                            onClick={() => removePath(path)}
                          >
                            x
                          </button>
                        </span>
                      )}
                    </For>
                    <Show when={blockedPaths().length === 0}>
                      <span class="text-xs text-ide-muted italic">
                        No blocked paths
                      </span>
                    </Show>
                  </div>
                </div>
              </div>
            </section>

            {/* ── Sessions Section ── */}
            <section class="space-y-3">
              <h3 class="text-xs font-semibold text-ide-muted uppercase tracking-wider">
                Sessions
              </h3>
              <label class="block">
                <span class="text-xs text-ide-muted mb-1 block">
                  Store Type
                </span>
                <select
                  class="w-full bg-ide-bg border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent"
                  value={sessionStore()}
                  onChange={(e) => setSessionStore(e.currentTarget.value)}
                >
                  <option value="memory">memory</option>
                  <option value="file">file</option>
                </select>
              </label>
            </section>

            {/* ── MCP Servers Section ── */}
            <section class="space-y-3">
              <div class="flex items-center justify-between">
                <h3 class="text-xs font-semibold text-ide-muted uppercase tracking-wider">
                  MCP Servers
                </h3>
                <button
                  class="text-xs text-ide-accent hover:text-blue-400 transition-colors"
                  onClick={() => setShowAddMcp(!showAddMcp())}
                >
                  {showAddMcp() ? "Cancel" : "+ Add Server"}
                </button>
              </div>

              {/* Add Server Form */}
              <Show when={showAddMcp()}>
                <div class="bg-ide-bg border border-ide-border rounded p-3 space-y-2">
                  <input
                    type="text"
                    class="w-full bg-ide-panel border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={newMcpName()}
                    onInput={(e) => setNewMcpName(e.currentTarget.value)}
                    onKeyDown={handleMcpKeyDown}
                    placeholder="Server name (e.g. filesystem)"
                  />
                  <input
                    type="text"
                    class="w-full bg-ide-panel border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={newMcpCommand()}
                    onInput={(e) => setNewMcpCommand(e.currentTarget.value)}
                    onKeyDown={handleMcpKeyDown}
                    placeholder="Command (e.g. npx)"
                  />
                  <input
                    type="text"
                    class="w-full bg-ide-panel border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={newMcpArgs()}
                    onInput={(e) => setNewMcpArgs(e.currentTarget.value)}
                    onKeyDown={handleMcpKeyDown}
                    placeholder="Args (comma-separated, e.g. -y,@modelcontextprotocol/server-filesystem)"
                  />
                  <input
                    type="text"
                    class="w-full bg-ide-panel border border-ide-border rounded px-2 py-1.5 text-xs text-ide-text focus:outline-none focus:border-ide-accent placeholder-ide-muted"
                    value={newMcpEnv()}
                    onInput={(e) => setNewMcpEnv(e.currentTarget.value)}
                    onKeyDown={handleMcpKeyDown}
                    placeholder="Env (key=val pairs, e.g. API_KEY=abc,DEBUG=true)"
                  />
                  <button
                    class="w-full px-3 py-1.5 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    onClick={addMcpServer}
                    disabled={!newMcpName().trim() || !newMcpCommand().trim()}
                  >
                    Add Server
                  </button>
                </div>
              </Show>

              {/* Server List */}
              <Show
                when={mcpServers().length > 0}
                fallback={
                  <span class="text-xs text-ide-muted italic">
                    No MCP servers configured
                  </span>
                }
              >
                <div class="space-y-1.5">
                  <For each={mcpServers()}>
                    {(server) => (
                      <div class="bg-ide-bg border border-ide-border rounded px-3 py-2 flex items-start justify-between gap-2">
                        <div class="min-w-0 flex-1">
                          <div class="flex items-center gap-2">
                            <span class="text-xs font-medium text-ide-text truncate">
                              {server.name}
                            </span>
                          </div>
                          <div class="text-[10px] text-ide-muted font-mono truncate mt-0.5">
                            {server.command}
                            {server.args.length > 0 ? " " + server.args.join(" ") : ""}
                          </div>
                          <Show when={server.env && Object.keys(server.env).length > 0}>
                            <div class="text-[10px] text-ide-muted mt-0.5">
                              env: {Object.entries(server.env!).map(([k, v]) => `${k}=${v}`).join(", ")}
                            </div>
                          </Show>
                        </div>
                        <button
                          class="text-ide-muted hover:text-red-400 transition-colors shrink-0 p-0.5"
                          onClick={() => removeMcpServer(server.name)}
                          title="Remove server"
                        >
                          <svg
                            width="12"
                            height="12"
                            viewBox="0 0 14 14"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                          >
                            <path d="M1 1l12 12M13 1L1 13" />
                          </svg>
                        </button>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </section>
          </Show>
        </div>

        {/* Footer with save button */}
        <Show when={!loading() && config()}>
          <div class="border-t border-ide-border px-4 py-3 shrink-0 flex items-center gap-3">
            <button
              class="px-4 py-1.5 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              onClick={handleSave}
              disabled={saving()}
            >
              {saving() ? "Saving..." : "Save Changes"}
            </button>
            <Show when={saveSuccess()}>
              <span class="text-xs text-green-400">Saved</span>
            </Show>
            <Show when={error() && !loading()}>
              <button
                class="text-xs text-ide-muted hover:text-ide-text transition-colors"
                onClick={loadConfig}
              >
                Retry
              </button>
            </Show>
          </div>
        </Show>
      </div>
    </>
  );
}
