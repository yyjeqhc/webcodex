import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const frontendRoot = fileURLToPath(new URL(".", import.meta.url));
const runtimeRoot = fileURLToPath(new URL("./src/runtime-v2/", import.meta.url));

export default defineConfig({
  root: runtimeRoot,
  plugins: [react()],
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  publicDir: false,
  server: {
    host: "127.0.0.1",
    port: 4178,
  },
  preview: {
    host: "127.0.0.1",
    port: 4178,
  },
  build: {
    outDir: fileURLToPath(new URL("./dist/", import.meta.url)),
    emptyOutDir: false,
    target: "es2022",
    minify: "esbuild",
    cssCodeSplit: false,
    lib: {
      entry: fileURLToPath(new URL("./src/runtime-v2/main.tsx", import.meta.url)),
      formats: ["es"],
      fileName: () => "app.js",
      cssFileName: "styles",
    },
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "chunk-[name].js",
        assetFileNames: (asset) => asset.name?.endsWith(".css") ? "styles.css" : "[name][extname]",
        codeSplitting: false,
      },
    },
  },
  resolve: {
    alias: {
      "@runtime-v2": fileURLToPath(new URL("./src/runtime-v2/", import.meta.url)),
    },
  },
});
