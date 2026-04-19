import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    setupFiles: ["./setupTests.ts"],
    include: ["app/**/*.test.ts", "app/**/*.test.tsx"],
  },
});
