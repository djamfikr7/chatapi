import { createSignal, createEffect, For, Show, onMount } from "solid-js";
import { fetchTools, type ToolInfo } from "../lib/api";

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
  ts: "TS",
  tsx: "TX",
  js: "JS",
  jsx: "JX",
  rs: "RS",
  py: "PY",
  json: "{}",
  md: "M ",
  html: "<>",
  css: "# ",
  toml: "T ",
  yaml: "Y ",
  yml: "Y ",
  sh: "$ ",
  go: "GO",
  c: "C ",
  cpp: "C+",
  h: "H ",
  hpp: "H+",
  txt: "  ",
  gitignore: "G ",
  lock: "L ",
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
    case "ts":
    case "tsx":
      return "text-blue-400";
    case "js":
    case "jsx":
      return "text-yellow-400";
    case "rs":
      return "text-orange-400";
    case "py":
      return "text-green-400";
    case "json":
      return "text-yellow-300";
    case "md":
      return "text-blue-300";
    case "html":
      return "text-red-400";
    case "css":
      return "text-purple-400";
    case "toml":
    case "yaml":
    case "yml":
      return "text-green-300";
    case "sh":
      return "text-green-500";
    default:
      return "text-ide-muted";
  }
}

export function FileTree(props: FileTreeProps) {
  const [files, setFiles] = createSignal<FileEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["."]));
  const [dirContents, setDirContents] = createSignal<Record<string, FileEntry[]>>({});

  // Load root directory on mount
  onMount(async () => {
    await loadDirectory(".");
  });

  async function loadDirectory(dirPath: string) {
    setLoading(true);
    try {
      // Use the tools endpoint to get file listing
      // The gateway exposes tools; we simulate a directory listing by using
      // well-known project files
      const rootFiles = getProjectFiles();
      setFiles(rootFiles);
      setDirContents((prev) => ({ ...prev, [dirPath]: rootFiles }));
      setExpanded((prev) => new Set([...prev, dirPath]));
    } catch (err) {
      console.error("Failed to load directory:", err);
    } finally {
      setLoading(false);
    }
  }

  function getProjectFiles(): FileEntry[] {
    // Provide a sensible default file tree based on the ChatAPI project structure
    return [
      { name: "Cargo.toml", path: "Cargo.toml", isDir: false, extension: "toml" },
      { name: "Cargo.lock", path: "Cargo.lock", isDir: false, extension: "lock" },
      { name: "specs.md", path: "specs.md", isDir: false, extension: "md" },
      { name: "gateway", path: "gateway", isDir: true },
      { name: "shared", path: "shared", isDir: true },
      { name: "rules", path: "rules", isDir: true },
      { name: "sessions", path: "sessions", isDir: true },
      { name: "tools", path: "tools", isDir: true },
      { name: "targets", path: "targets", isDir: true },
      { name: "tests", path: "tests", isDir: true },
      { name: "docs", path: "docs", isDir: true },
      { name: ".knowledge", path: ".knowledge", isDir: true },
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
  }

  async function handleFileClick(entry: FileEntry) {
    if (entry.isDir) {
      toggleDir(entry.path);
      return;
    }

    // Try to fetch file content via the tools API
    try {
      const res = await fetch("/v1/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "deepseek-chat",
          messages: [
            {
              role: "user",
              content: `Read the file at path: ${entry.path}. Return ONLY the raw file content, no explanations.`,
            },
          ],
          stream: false,
        }),
      });
      const data = await res.json();
      const content = data.choices?.[0]?.message?.content || `// File: ${entry.path}\n// Content could not be loaded`;
      const language = props.getLanguage(entry.path);
      props.onOpenFile(entry.path, content, language);
    } catch {
      // Fallback: open with placeholder content
      const language = props.getLanguage(entry.path);
      props.onOpenFile(entry.path, `// File: ${entry.path}\n// Failed to load content`, language);
    }
  }

  function renderTree(entries: FileEntry[], depth: number = 0) {
    // Sort: directories first, then files alphabetically
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
