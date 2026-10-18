import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [react()],
  // Shared UI source must use the Desktop provider's library instances in Vite
  // and in Vitest's external dependency graph.
  resolve: {
    dedupe: ["react", "react-dom", "@mantine/core", "@mantine/hooks", "lucide-react"],
    alias: [
      { find: /^@mantine\/core$/, replacement: fileURLToPath(new URL("./node_modules/@mantine/core/esm/index.mjs", import.meta.url)) },
      { find: /^@mantine\/hooks$/, replacement: fileURLToPath(new URL("./node_modules/@mantine/hooks/esm/index.mjs", import.meta.url)) },
      // Keep unused icons tree-shakeable while resolving the Desktop instance.
      { find: /^lucide-react$/, replacement: fileURLToPath(new URL("./node_modules/lucide-react/dist/esm/lucide-react.mjs", import.meta.url)) },
    ],
  },
  clearScreen: false,
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/target/**"],
    },
  },
});
