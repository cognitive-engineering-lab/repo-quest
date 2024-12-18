import * as cp from "node:child_process";
import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import packageJson from "./package.json";

let commitHash = cp.execSync("git rev-parse HEAD").toString("utf-8").trim();

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
    "process.env.NODE_ENV": JSON.stringify(mode),
    VERSION: JSON.stringify(packageJson.version),
    COMMIT_HASH: JSON.stringify(commitHash),
    TELEMETRY_URL: JSON.stringify("https://rust-book.willcrichton.net/logs")
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
