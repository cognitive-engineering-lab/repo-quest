import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

let alias = {
  "@wcrichto/rust-editor/dist/lib.css": path.resolve(
    __dirname,
    "rust-editor-placeholder.css"
  ),
  "@wcrichto/rust-editor": path.resolve(__dirname, "rust-editor-placeholder.js")
};

export default defineConfig(({ mode }) => ({
  base: "./",
  define: {
    "process.env.NODE_ENV": JSON.stringify(mode)
  },
  plugins: [react()],
  resolve: { alias },
  test: {
    environment: "jsdom",
    setupFiles: "tests/setup.ts",
    deps: {
      inline: [/^(?!.*vitest).*$/]
    }
  }
}));
