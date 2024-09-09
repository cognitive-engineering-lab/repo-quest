import fs from "node:fs";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

let manifest = JSON.parse(fs.readFileSync("package.json", "utf-8"));
export default defineConfig(({ mode }) => ({
  base: "./",
  define: {
    "process.env.NODE_ENV": JSON.stringify(mode)
  },
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "tests/setup.ts",
    deps: {
      inline: [/^(?!.*vitest).*$/]
    }
  }
}));
