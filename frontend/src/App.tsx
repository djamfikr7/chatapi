import {
  createSignal,
  createEffect,
  onMount,
  Show,
  For,
} from "solid-js";
import { ChatPanel } from "./components/ChatPanel";
import { FileTree } from "./components/FileTree";
import { MonacoEditor } from "./components/MonacoEditor";
import { Terminal } from "./components/Terminal";
import { SessionList } from "./components/SessionList";
import type { ChatMessage, Session } from "./lib/api";
import { fetchHealth, fetchSessions, createSession, getSession } from "./lib/api";

export interface OpenFile {
  path: string;
  content: string;
  language: string;
  modified: boolean;
}

export default function App() {
  // Health state
  const [health, setHealth] = createSignal<{ status: string; mode: string } | null>(null);
  const [connected, setConnected] = createSignal(false);

  // Session state
  const [sessions, setSessions] = createSignal<Session[]>([]);
  const [activeSessionId, setActiveSessionId] = createSignal<string | null>(null);
  const [messages, setMessages] = createSignal<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);

  // Editor state
  const [openFiles, setOpenFiles] = createSignal<OpenFile[]>([]);
  const [activeFilePath, setActiveFilePath] = createSignal<string | null>(null);

  // Terminal visibility
  const [terminalVisible, setTerminalVisible] = createSignal(true);

  // Left sidebar tab
  const [leftTab, setLeftTab] = createSignal<"files" | "sessions">("files");

  // Panel resize state
  const [leftWidth, setLeftWidth] = createSignal(250);
  const [rightWidth, setRightWidth] = createSignal(380);
  const [terminalHeight, setTerminalHeight] = createSignal(250);

  // Check health on mount
  onMount(async () => {
    try {
      const h = await fetchHealth();
      setHealth(h);
      setConnected(true);
    } catch {
      setConnected(false);
    }

    try {
      const s = await fetchSessions();
      setSessions(s);
    } catch {
      // sessions endpoint may not be available
    }
  });

  // Load session messages when active session changes
  createEffect(async () => {
    const sid = activeSessionId();
    if (!sid) {
      setMessages([]);
      return;
    }
    try {
      const session = await getSession(sid);
      setMessages(session.messages || []);
    } catch {
      setMessages([]);
    }
  });

  // Create a new session
  async function handleNewSession() {
    try {
      const session = await createSession();
      setSessions((prev) => [...prev, session]);
      setActiveSessionId(session.id);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }

  // Select a session
  async function handleSelectSession(id: string) {
    setActiveSessionId(id);
  }

  // Delete a session
  function handleDeleteSession(id: string) {
    setSessions((prev) => prev.filter((s) => s.id !== id));
    if (activeSessionId() === id) {
      setActiveSessionId(null);
      setMessages([]);
    }
  }

  // Open a file in the editor
  function handleOpenFile(path: string, content: string, language: string) {
    const existing = openFiles().find((f) => f.path === path);
    if (existing) {
      setActiveFilePath(path);
      return;
    }
    setOpenFiles((prev) => [...prev, { path, content, language, modified: false }]);
    setActiveFilePath(path);
  }

  // Close a file tab
  function handleCloseFile(path: string) {
    setOpenFiles((prev) => prev.filter((f) => f.path !== path));
    if (activeFilePath() === path) {
      const remaining = openFiles().filter((f) => f.path !== path);
      setActiveFilePath(remaining.length > 0 ? remaining[remaining.length - 1].path : null);
    }
  }

  // Update file content
  function handleFileContentChange(path: string, content: string) {
    setOpenFiles((prev) =>
      prev.map((f) => (f.path === path ? { ...f, content, modified: true } : f))
    );
  }

  // Save file (mark as unmodified)
  function handleSaveFile(path: string) {
    setOpenFiles((prev) =>
      prev.map((f) => (f.path === path ? { ...f, modified: false } : f))
    );
  }

  // Add a message to conversation
  function handleAddMessage(msg: ChatMessage) {
    setMessages((prev) => [...prev, msg]);
  }

  // Update messages (for streaming)
  function handleSetMessages(msgs: ChatMessage[]) {
    setMessages(msgs);
  }

  // Left panel resize
  function onLeftResize(e: MouseEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = leftWidth();
    const onMove = (ev: MouseEvent) => {
      const newWidth = Math.max(180, Math.min(500, startWidth + ev.clientX - startX));
      setLeftWidth(newWidth);
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }

  // Right panel resize
  function onRightResize(e: MouseEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = rightWidth();
    const onMove = (ev: MouseEvent) => {
      const newWidth = Math.max(280, Math.min(600, startWidth - (ev.clientX - startX)));
      setRightWidth(newWidth);
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }

  // Terminal resize
  function onTerminalResize(e: MouseEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const startHeight = terminalHeight();
    const onMove = (ev: MouseEvent) => {
      const newHeight = Math.max(100, Math.min(600, startHeight - (ev.clientY - startY)));
      setTerminalHeight(newHeight);
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";
  }

  // Get language from file extension
  function getLanguage(path: string): string {
    const ext = path.split(".").pop()?.toLowerCase() || "";
    const langMap: Record<string, string> = {
      ts: "typescript",
      tsx: "typescript",
      js: "javascript",
      jsx: "javascript",
      rs: "rust",
      py: "python",
      json: "json",
      md: "markdown",
      html: "html",
      css: "css",
      toml: "toml",
      yaml: "yaml",
      yml: "yaml",
      sh: "shell",
      bash: "shell",
      go: "go",
      c: "c",
      cpp: "cpp",
      h: "c",
      hpp: "cpp",
    };
    return langMap[ext] || "plaintext";
  }

  return (
    <div class="flex flex-col h-screen w-screen overflow-hidden bg-ide-bg text-ide-text select-none">
      {/* Title bar */}
      <div class="flex items-center h-8 bg-ide-sidebar border-b border-ide-border px-3 shrink-0">
        <div class="flex items-center gap-2">
          <div class="w-3 h-3 rounded-full bg-green-500" classList={{ "bg-red-500": !connected(), "bg-green-500": connected() }} />
          <span class="text-xs font-medium">ChatAPI IDE</span>
        </div>
        <div class="flex-1" />
        <Show when={health()}>
          <span class="text-xs text-ide-muted">
            {health()!.mode} | {health()!.status}
          </span>
        </Show>
      </div>

      {/* Main content area */}
      <div class="flex flex-1 overflow-hidden">
        {/* Left sidebar */}
        <div class="flex flex-col bg-ide-sidebar border-r border-ide-border shrink-0 overflow-hidden" style={{ width: `${leftWidth()}px` }}>
          {/* Tab buttons */}
          <div class="flex border-b border-ide-border">
            <button
              class={`flex-1 text-xs py-1.5 px-2 ${leftTab() === "files" ? "bg-ide-panel text-ide-text" : "text-ide-muted hover:text-ide-text"}`}
              onClick={() => setLeftTab("files")}
            >
              Files
            </button>
            <button
              class={`flex-1 text-xs py-1.5 px-2 ${leftTab() === "sessions" ? "bg-ide-panel text-ide-text" : "text-ide-muted hover:text-ide-text"}`}
              onClick={() => setLeftTab("sessions")}
            >
              Sessions
            </button>
          </div>

          {/* Tab content */}
          <div class="flex-1 overflow-auto">
            <Show when={leftTab() === "files"}>
              <FileTree
                onOpenFile={handleOpenFile}
                getLanguage={getLanguage}
              />
            </Show>
            <Show when={leftTab() === "sessions"}>
              <SessionList
                sessions={sessions()}
                activeSessionId={activeSessionId()}
                onSelect={handleSelectSession}
                onDelete={handleDeleteSession}
                onNew={handleNewSession}
              />
            </Show>
          </div>
        </div>

        {/* Left resize handle */}
        <div class="w-1 bg-ide-border hover:bg-ide-accent cursor-col-resize shrink-0" onMouseDown={onLeftResize} />

        {/* Center area (editor + terminal) */}
        <div class="flex flex-col flex-1 overflow-hidden">
          {/* Monaco editor */}
          <div class="flex-1 overflow-hidden" style={{ "min-height": "200px" }}>
            <MonacoEditor
              files={openFiles()}
              activePath={activeFilePath()}
              onSelectTab={setActiveFilePath}
              onCloseTab={handleCloseFile}
              onContentChange={handleFileContentChange}
              onSave={handleSaveFile}
            />
          </div>

          {/* Terminal resize handle */}
          <Show when={terminalVisible()}>
            <div class="h-1 bg-ide-border hover:bg-ide-accent cursor-row-resize shrink-0" onMouseDown={onTerminalResize} />
          </Show>

          {/* Terminal */}
          <Show when={terminalVisible()}>
            <div style={{ height: `${terminalHeight()}px` }} class="shrink-0 overflow-hidden">
              <Terminal />
            </div>
          </Show>
        </div>

        {/* Right resize handle */}
        <div class="w-1 bg-ide-border hover:bg-ide-accent cursor-col-resize shrink-0" onMouseDown={onRightResize} />

        {/* Right sidebar - Chat */}
        <div class="flex flex-col bg-ide-sidebar border-l border-ide-border shrink-0 overflow-hidden" style={{ width: `${rightWidth()}px` }}>
          <ChatPanel
            sessionId={activeSessionId()}
            messages={messages()}
            isLoading={isLoading()}
            setIsLoading={setIsLoading}
            onAddMessage={handleAddMessage}
            onSetMessages={handleSetMessages}
            onNewSession={handleNewSession}
            sessions={sessions()}
            onSelectSession={handleSelectSession}
          />
        </div>
      </div>

      {/* Status bar */}
      <div class="flex items-center h-6 bg-ide-accent text-white text-xs px-3 shrink-0">
        <span>ChatAPI IDE</span>
        <div class="flex-1" />
        <Show when={activeFilePath()}>
          <span class="mr-4">{activeFilePath()}</span>
        </Show>
        <Show when={openFiles().length > 0}>
          <span class="mr-4">
            {openFiles().find((f) => f.path === activeFilePath())?.language || ""}
          </span>
        </Show>
        <span>UTF-8</span>
      </div>
    </div>
  );
}
