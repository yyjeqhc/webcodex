import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["test-v2/**/*.test.{ts,tsx}"],
    setupFiles: ["./test-v2/setup.ts"],
    restoreMocks: true,
    clearMocks: true,
  },
});
