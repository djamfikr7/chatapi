import { For, Show } from "solid-js";
import type { Session } from "../lib/api";

interface SessionListProps {
  sessions: Session[];
  activeSessionId: string | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
}

export function SessionList(props: SessionListProps) {
  function formatTime(timestamp: number): string {
    const date = new Date(timestamp * 1000);
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (minutes < 1) return "just now";
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    return `${days}d ago`;
  }

  return (
    <div class="flex flex-col h-full">
      {/* New session button */}
      <div class="p-2 border-b border-ide-border">
        <button
          class="w-full px-3 py-1.5 text-xs bg-ide-accent text-white rounded hover:bg-blue-600 transition-colors"
          onClick={props.onNew}
        >
          + New Session
        </button>
      </div>

      {/* Session list */}
      <div class="flex-1 overflow-y-auto">
        <Show
          when={props.sessions.length > 0}
          fallback={
            <div class="flex items-center justify-center h-20 text-xs text-ide-muted">
              No sessions yet
            </div>
          }
        >
          <For each={props.sessions}>
            {(session) => (
              <div
                class={`flex items-center gap-2 px-3 py-2 cursor-pointer border-b border-ide-border ${
                  props.activeSessionId === session.id
                    ? "bg-ide-active"
                    : "hover:bg-ide-hover"
                }`}
                onClick={() => props.onSelect(session.id)}
              >
                <div class="flex-1 min-w-0">
                  <div class="text-xs font-medium text-ide-text truncate">
                    {session.id.slice(0, 12)}...
                  </div>
                  <div class="text-[10px] text-ide-muted flex items-center gap-2 mt-0.5">
                    <span>{session.model}</span>
                    <span>{formatTime(session.created_at)}</span>
                  </div>
                </div>
                <button
                  class="w-5 h-5 flex items-center justify-center rounded text-ide-muted hover:text-red-400 hover:bg-ide-hover shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onDelete(session.id);
                  }}
                  title="Delete session"
                >
                  x
                </button>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}
