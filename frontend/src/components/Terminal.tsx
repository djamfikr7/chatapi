import { onMount, onCleanup } from "solid-js";
import { Terminal as XTerminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

export function Terminal() {
  let containerRef: HTMLDivElement | undefined;
  let terminal: XTerminal | undefined;
  let fitAddon: FitAddon | undefined;
  let resizeObserver: ResizeObserver | undefined;

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
    terminal.writeln("\x1b[90mConnected to gateway on localhost:8090\x1b[0m");
    terminal.writeln("\x1b[90mType commands to execute via the gateway.\x1b[0m");
    terminal.writeln("");

    // Command buffer
    let commandBuffer = "";
    let historyIndex = -1;
    const commandHistory: string[] = [];

    function writePrompt() {
      terminal!.write("\x1b[1;32m$\x1b[0m ");
    }

    writePrompt();

    terminal.onData((data) => {
      const code = data.charCodeAt(0);

      if (data === "\r") {
        // Enter
        terminal!.writeln("");
        if (commandBuffer.trim()) {
          commandHistory.push(commandBuffer);
          historyIndex = commandHistory.length;
          executeCommand(commandBuffer.trim());
        } else {
          writePrompt();
        }
        commandBuffer = "";
      } else if (code === 127 || data === "\b") {
        // Backspace
        if (commandBuffer.length > 0) {
          commandBuffer = commandBuffer.slice(0, -1);
          terminal!.write("\b \b");
        }
      } else if (data === "\x1b[A") {
        // Up arrow - history
        if (historyIndex > 0) {
          clearCurrentLine();
          historyIndex--;
          commandBuffer = commandHistory[historyIndex];
          terminal!.write(commandBuffer);
        }
      } else if (data === "\x1b[B") {
        // Down arrow - history
        clearCurrentLine();
        if (historyIndex < commandHistory.length - 1) {
          historyIndex++;
          commandBuffer = commandHistory[historyIndex];
          terminal!.write(commandBuffer);
        } else {
          historyIndex = commandHistory.length;
          commandBuffer = "";
        }
      } else if (code === 3) {
        // Ctrl+C
        terminal!.writeln("^C");
        commandBuffer = "";
        writePrompt();
      } else if (code === 12) {
        // Ctrl+L - clear
        terminal!.clear();
        writePrompt();
      } else if (code >= 32) {
        // Printable character
        commandBuffer += data;
        terminal!.write(data);
      }
    });

    function clearCurrentLine() {
      terminal!.write("\r\x1b[K");
      writePrompt();
    }

    async function executeCommand(command: string) {
      try {
        terminal!.writeln("\x1b[90mExecuting...\x1b[0m");

        const res = await fetch("/tools/execute", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            name: "run_command",
            args: { command },
          }),
        });

        const data = await res.json();

        if (data.error) {
          terminal!.writeln(`\x1b[1;31m${data.error}\x1b[0m`);
        } else if (data.result) {
          const lines = data.result.split("\n");
          for (const line of lines) {
            terminal!.writeln(`\x1b[37m${line}\x1b[0m`);
          }
        }

        if (data.is_error) {
          terminal!.writeln(`\x1b[1;31mCommand exited with error\x1b[0m`);
        }
      } catch (err) {
        terminal!.writeln(`\x1b[1;31mError: ${err instanceof Error ? err.message : String(err)}\x1b[0m`);
      }

      writePrompt();
    }
  });

  onCleanup(() => {
    resizeObserver?.disconnect();
    terminal?.dispose();
  });

  return (
    <div ref={containerRef} class="h-full w-full bg-[#1e1e1e]" />
  );
}
