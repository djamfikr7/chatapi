import { onMount, onCleanup } from "solid-js";

interface TerminalProps {
  workingDirectory?: string;
}

export function Terminal(props: TerminalProps) {
  let terminalRef: HTMLDivElement | undefined;
  let ws: WebSocket | null = null;

  onMount(async () => {
    if (!terminalRef) return;

    const { Terminal: XTerm } = await import("@xterm/xterm");
    const { FitAddon } = await import("@xterm/addon-fit");
    await import("@xterm/xterm/css/xterm.css");

    const term = new XTerm({
      theme: {
        background: "#1e1e2e",
        foreground: "#cdd6f4",
        cursor: "#f5e0dc",
        selectionBackground: "#45475a",
        black: "#45475a",
        red: "#f38ba8",
        green: "#a6e3a1",
        yellow: "#f9e2af",
        blue: "#89b4fa",
        magenta: "#f5c2e7",
        cyan: "#94e2d5",
        white: "#bac2de",
      },
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      fontSize: 13,
      cursorBlink: true,
      scrollback: 10000,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(terminalRef);
    fitAddon.fit();

    // Connect to WebSocket terminal
    const wsUrl = `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws/terminal`;
    ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      term.write("\x1b[32mConnected to terminal\x1b[0m\r\n");
    };

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(event.data));
      } else {
        term.write(event.data);
      }
    };

    ws.onclose = () => {
      term.write("\r\n\x1b[31mTerminal disconnected\x1b[0m\r\n");
    };

    ws.onerror = () => {
      term.write("\r\n\x1b[31mTerminal connection error\x1b[0m\r\n");
    };

    // Send keystrokes to the server
    term.onData((data) => {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(data);
      }
    });

    // Handle resize
    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: "resize",
          cols: term.cols,
          rows: term.rows,
        }));
      }
    });
    resizeObserver.observe(terminalRef);

    onCleanup(() => {
      resizeObserver.disconnect();
      ws?.close();
      term.dispose();
    });
  });

  return (
    <div
      ref={terminalRef}
      class="h-full w-full bg-ide-bg p-1"
    />
  );
}
