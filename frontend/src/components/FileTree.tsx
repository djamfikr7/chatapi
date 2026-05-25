import { createSignal, createEffect, For, Show, onMount } from "solid-js";

interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  extension?: string;
}

interface FileTreeProps {
  onOpenFile: (path: string, content: string, language: string) => void;
  getLanguage: (path: string) => string;
}

const FILE_ICONS: Record<string, string> = {
  ts: "TS", tsx: "TX", js: "JS", jsx: "JX", rs: "RS", py: "PY",
  json: "{}", md: "M ", html: "<>", css: "# ", toml: "T ",
  yaml: "Y ", yml: "Y ", sh: "$ ", go: "GO", c: "C ", cpp: "C+",
  h: "H ", hpp: "H+", txt: "  ", gitignore: "G ", lock: "L ",
};

function getFileIcon(name: string, isDir: boolean): string {
  if (isDir) return "D ";
  const ext = name.split(".").pop()?.toLowerCase() || "";
  return FILE_ICONS[ext] || "  ";
}

function getFileIconColor(name: string, isDir: boolean): string {
  if (isDir) return "text-blue-400";
  const ext = name.split(".").pop()?.toLowerCase() || "";
  switch (ext) {
    case "ts": case "tsx": return "text-blue-400";
    case "js": case "jsx": return "text-yellow-400";
    case "rs": return "text-orange-400";
    case "py": return "text-green-400";
    case "json": return "text-yellow-300";
    case "md": return "text-blue-300";
    case "html": return "text-red-400";
    case "css": return "text-purple-400";
    case "toml": case "yaml": case "yml": return "text-green-300";
    case "sh": return "text-green-500";
    default: return "text-ide-muted";
  }
}

export function FileTree(props: FileTreeProps) {
  const [files, setFiles] = createSignal<FileEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["."]));
  const [dirContents, setDirContents] = createSignal<Record<string, FileEntry[]>>({});

  onMount(async () => {
    await loadDirectory(".");
  });

  async function loadDirectory(dirPath: string) {
    setLoading(true);
    try {
      const res = await fetch(`/files?path=${encodeURIComponent(dirPath)}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const entries: FileEntry[] = data.entries || [];
      setDirContents((prev) => ({ ...prev, [dirPath]: entries }));
      if (dirPath === ".") setFiles(entries);
      setExpanded((prev) => new Set([...prev, dirPath]));
    } catch (err) {
      console.error("Failed to load directory:", err);
      if (dirPath === ".") {
        // Fallback to hardcoded project files
        setFiles(getFallbackFiles());
      }
    } finally {
      setLoading(false);
    }
  }

  function getFallbackFiles(): FileEntry[] {
    return [
      { name: "Cargo.toml", path: "Cargo.toml", isDir: false },
      { name: "specs.md", path: "specs.md", isDir: false },
      { name: "gateway", path: "gateway", isDir: true },
      { name: "shared", path: "shared", isDir: true },
      { name: "rules", path: "rules", isDir: true },
      { name: "sessions", path: "sessions", isDir: true },
      { name: "tools", path: "tools", isDir: true },
      { name: "targets", path: "targets", isDir: true },
      { name: "frontend", path: "frontend", isDir: true },
    ];
  }

  function toggleDir(path: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
    // Load directory contents if not already loaded
    if (!dirContents()[path]) {
      loadDirectory(path);
    }
  }

  async function handleFileClick(entry: FileEntry) {
    if (entry.isDir) {
      toggleDir(entry.path);
      return;
    }

    try {
      const res = await fetch(`/files/read?path=${encodeURIComponent(entry.path)}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const language = props.getLanguage(entry.path);
      props.onOpenFile(entry.path, data.content || "", language);
    } catch {
      const language = props.getLanguage(entry.path);
      props.onOpenFile(entry.path, `// Failed to load: ${entry.path}`, language);
    }
  }

  function renderTree(entries: FileEntry[], depth: number = 0) {
    const sorted = [...entries].sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });

    return (
      <For each={sorted}>
        {(entry) => (
          <div>
            <div
              class="flex items-center gap-1 py-0.5 px-2 hover:bg-ide-hover cursor-pointer text-xs group"
              style={{ "padding-left": `${depth * 16 + 8}px` }}
              onClick={() => handleFileClick(entry)}
            >
              <span class="w-4 text-center shrink-0">
                {entry.isDir ? (
                  expanded().has(entry.path) ? (
                    <span class="text-ide-muted">&#x25BC;</span>
                  ) : (
                    <span class="text-ide-muted">&#x25B6;</span>
                  )
                ) : (
                  <span class={`${getFileIconColor(entry.name, entry.isDir)} font-mono text-[10px]`}>
                    {getFileIcon(entry.name, entry.isDir)}
                  </span>
                )}
              </span>
              <span
                class={`truncate ${entry.isDir ? "text-blue-300" : "text-ide-text"} group-hover:text-white`}
              >
                {entry.name}
              </span>
            </div>
            <Show when={entry.isDir && expanded().has(entry.path)}>
              <Show
                when={dirContents()[entry.path]}
                fallback={
                  <div class="text-xs text-ide-muted pl-8 py-0.5">
                    Loading...
                  </div>
                }
              >
                {renderTree(dirContents()[entry.path]!, depth + 1)}
              </Show>
            </Show>
          </div>
        )}
      </For>
    );
  }

  return (
    <div class="h-full overflow-y-auto py-1">
      <Show
        when={!loading()}
        fallback={
          <div class="flex items-center justify-center h-20 text-xs text-ide-muted">
            Loading files...
          </div>
        }
      >
        {renderTree(files())}
      </Show>
    </div>
  );
}
