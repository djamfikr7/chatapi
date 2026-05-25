import { createSignal, createEffect, onMount, onCleanup, Show, For } from "solid-js";
import type { OpenFile } from "../App";

interface MonacoEditorProps {
  files: OpenFile[];
  activePath: string | null;
  onSelectTab: (path: string) => void;
  onCloseTab: (path: string) => void;
  onContentChange: (path: string, content: string) => void;
  onSave: (path: string) => void;
}

export function MonacoEditor(props: MonacoEditorProps) {
  let containerRef: HTMLDivElement | undefined;
  let editor: any = null;
  const [monaco, setMonaco] = createSignal<any>(null);
  const [ready, setReady] = createSignal(false);

  // Track editor models per file
  const models = new Map<string, any>();
  const viewStates = new Map<string, any>();

  // Load Monaco
  onMount(async () => {
    try {
      const monacoModule = await import("monaco-editor");
      setMonaco(monacoModule);

      // Define dark theme
      monacoModule.editor.defineTheme("chatapi-dark", {
        base: "vs-dark",
        inherit: true,
        rules: [
          { token: "comment", foreground: "6A9955" },
          { token: "keyword", foreground: "569CD6" },
          { token: "string", foreground: "CE9178" },
          { token: "number", foreground: "B5CEA8" },
          { token: "type", foreground: "4EC9B0" },
        ],
        colors: {
          "editor.background": "#1e1e1e",
          "editor.foreground": "#cccccc",
          "editor.lineHighlightBackground": "#2a2d2e",
          "editor.selectionBackground": "#264f78",
          "editor.inactiveSelectionBackground": "#3a3d41",
          "editorLineNumber.foreground": "#858585",
          "editorLineNumber.activeForeground": "#c6c6c6",
          "editorCursor.foreground": "#aeafad",
          "editorWhitespace.foreground": "#3b3b3b",
          "editorIndentGuide.background": "#3b3b3b",
          "editorIndentGuide.activeBackground": "#5b5b5b",
          "editorBracketMatch.background": "#0064001a",
          "editorBracketMatch.border": "#888888",
          "tab.activeBackground": "#1e1e1e",
          "tab.activeForeground": "#ffffff",
          "tab.inactiveBackground": "#2d2d2d",
          "tab.inactiveForeground": "#969696",
          "tab.border": "#252526",
          "editorGroupHeader.tabsBackground": "#252526",
        },
      });

      // Create editor
      if (containerRef) {
        editor = monacoModule.editor.create(containerRef, {
          theme: "chatapi-dark",
          automaticLayout: true,
          fontSize: 14,
          fontFamily: "'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
          lineNumbers: "on",
          minimap: { enabled: true, maxColumn: 80 },
          scrollBeyondLastLine: false,
          renderWhitespace: "selection",
          tabSize: 2,
          insertSpaces: true,
          wordWrap: "off",
          bracketPairColorization: { enabled: true },
          padding: { top: 8 },
          smoothScrolling: true,
          cursorBlinking: "smooth",
          cursorSmoothCaretAnimation: "on",
        });

        // Handle save (Ctrl+S)
        editor.addCommand(monacoModule.KeyMod.CtrlCmd | monacoModule.KeyCode.KeyS, () => {
          const active = props.activePath;
          if (active) {
            props.onSave(active);
          }
        });

        // Handle content changes
        editor.onDidChangeModelContent(() => {
          const active = props.activePath;
          if (active && editor) {
            const model = editor.getModel();
            if (model) {
              props.onContentChange(active, model.getValue());
            }
          }
        });

        setReady(true);
      }
    } catch (err) {
      console.error("Failed to load Monaco:", err);
    }
  });

  onCleanup(() => {
    if (editor) {
      editor.dispose();
      editor = null;
    }
    // Dispose all models
    for (const model of models.values()) {
      model.dispose();
    }
    models.clear();
  });

  // Switch editor content when active tab changes
  createEffect(() => {
    const path = props.activePath;
    if (!editor || !monaco() || !path) return;

    const m = monaco();

    // Save current view state
    const currentModel = editor.getModel();
    if (currentModel) {
      for (const [p, mod] of models.entries()) {
        if (mod === currentModel) {
          viewStates.set(p, editor.saveViewState());
          break;
        }
      }
    }

    // Get or create model for this file
    if (!models.has(path)) {
      const file = props.files.find((f) => f.path === path);
      if (!file) return;
      const uri = m.Uri.parse(`file:///${path}`);
      const model = m.editor.createModel(file.content, file.language, uri);
      models.set(path, model);
    }

    const model = models.get(path)!;

    // Update model content if the file was changed externally
    const file = props.files.find((f) => f.path === path);
    if (file && model.getValue() !== file.content) {
      model.setValue(file.content);
    }

    editor.setModel(model);

    // Restore view state
    if (viewStates.has(path)) {
      editor.restoreViewState(viewStates.get(path));
    }

    editor.focus();
  });

  // Clean up models for closed tabs
  createEffect(() => {
    const openPaths = new Set(props.files.map((f) => f.path));
    for (const [path, model] of models.entries()) {
      if (!openPaths.has(path)) {
        model.dispose();
        models.delete(path);
        viewStates.delete(path);
      }
    }
  });

  function getFileName(path: string): string {
    return path.split("/").pop() || path;
  }

  return (
    <div class="flex flex-col h-full">
      {/* Tab bar */}
      <Show when={props.files.length > 0}>
        <div class="flex bg-ide-sidebar border-b border-ide-border shrink-0 tab-bar">
          <For each={props.files}>
            {(file) => (
              <div
                class={`flex items-center gap-1 px-3 py-1.5 text-xs border-r border-ide-border cursor-pointer shrink-0 max-w-[160px] ${
                  props.activePath === file.path
                    ? "bg-ide-bg text-ide-text"
                    : "bg-ide-sidebar text-ide-muted hover:text-ide-text"
                }`}
                onClick={() => props.onSelectTab(file.path)}
              >
                <Show when={file.modified}>
                  <span class="w-2 h-2 rounded-full bg-orange-400 shrink-0" />
                </Show>
                <span class="truncate">{getFileName(file.path)}</span>
                <button
                  class="ml-auto shrink-0 w-4 h-4 flex items-center justify-center rounded hover:bg-ide-hover text-ide-muted hover:text-ide-text"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onCloseTab(file.path);
                  }}
                >
                  x
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Editor container */}
      <div class="flex-1 relative overflow-hidden">
        <Show
          when={props.files.length > 0}
          fallback={
            <div class="flex items-center justify-center h-full text-ide-muted text-sm">
              <div class="text-center">
                <div class="text-4xl mb-4 opacity-20">&lt;/&gt;</div>
                <div>Open a file from the sidebar to start editing</div>
                <div class="text-xs mt-2 opacity-60">
                  Or use the terminal to explore the project
                </div>
              </div>
            </div>
          }
        >
          <div ref={containerRef} class="absolute inset-0" />
        </Show>
      </div>
    </div>
  );
}
