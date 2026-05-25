import { onMount, onCleanup } from "solid-js";
import { Terminal as XTerminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

export function Terminal() {
  let containerRef: HTMLDivElement | undefined;
  let terminal: XTerminal | undefined;
  let fitAddon: FitAddon | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let ws: WebSocket | undefined;
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

  function connectTerminal() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//${window.location.host}/ws/terminal`;

    ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      terminal?.writeln("\x1b[1;32mConnected to shell.\x1b[0m");
    };

    ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        const decoder = new TextDecoder();
        terminal?.write(decoder.decode(event.data));
      } else {
        terminal?.write(event.data);
      }
    };

    ws.onclose = () => {
      terminal?.writeln("\r\n\x1b[1;31mDisconnected. Reconnecting...\x1b[0m");
      reconnectTimer = setTimeout(connectTerminal, 2000);
    };

    ws.onerror = () => {
      ws?.close();
    };
  }

  onMount(() => {
    if (!containerRef) return;

    terminal = new XTerminal({
      theme: {
        background: "#1e1e1e",
        foreground: "#cccccc",
        cursor: "#aeafad",
        cursorAccent: "#1e1e1e",
        selectionBackground: "#264f78",
        black: "#1e1e1e",
        red: "#f44747",
        green: "#6A9955",
        yellow: "#D7BA7D",
        blue: "#569CD6",
        magenta: "#C586C0",
        cyan: "#4EC9B0",
        white: "#cccccc",
        brightBlack: "#808080",
        brightRed: "#f44747",
        brightGreen: "#6A9955",
        brightYellow: "#D7BA7D",
        brightBlue: "#569CD6",
        brightMagenta: "#C586C0",
        brightCyan: "#4EC9B0",
        brightWhite: "#ffffff",
      },
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
      fontSize: 13,
      cursorBlink: true,
      cursorStyle: "bar",
      scrollback: 5000,
    });

    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(containerRef);

    // Initial fit
    setTimeout(() => fitAddon?.fit(), 100);

    // Resize observer for dynamic fitting
    resizeObserver = new ResizeObserver(() => {
      fitAddon?.fit();
    });
    resizeObserver.observe(containerRef);

    // Welcome message
    terminal.writeln("\x1b[1;36mChatAPI Terminal\x1b[0m");
    terminal.writeln("\x1b[90mConnecting to shell...\x1b[0m");

    // Connect to WebSocket terminal
    connectTerminal();

    // Forward terminal input to WebSocket
    terminal.onData((data) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(data);
      }
    });

    // Forward terminal resize to WebSocket
    terminal.onResize(({ cols, rows }) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "resize", cols, rows }));
      }
    });
  });

  onCleanup(() => {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    resizeObserver?.disconnect();
    ws?.close();
    terminal?.dispose();
  });

  return (
    <div ref={containerRef} class="h-full w-full bg-[#1e1e1e]" />
  );
}
