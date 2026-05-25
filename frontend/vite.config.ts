import { defineConfig } from "vite";
import solidPlugin from "vite-plugin-solid";

export default defineConfig({
  plugins: [solidPlugin()],
  server: {
    port: 3000,
    proxy: {
      "/ws": {
        target: "ws://localhost:8090",
        ws: true,
        changeOrigin: true,
      },
      "/v1": {
        target: "http://localhost:8090",
        changeOrigin: true,
      },
      "/health": {
        target: "http://localhost:8090",
        changeOrigin: true,
      },
      "/sessions": {
        target: "http://localhost:8090",
        changeOrigin: true,
      },
      "/tools": {
        target: "http://localhost:8090",
        changeOrigin: true,
      },
      "/config": {
        target: "http://localhost:8090",
        changeOrigin: true,
      },
    },
  },
  build: {
    target: "esnext",
  },
});
