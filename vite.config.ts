/// <reference types="vitest" />
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 本仓库是 workspace 布局，Rust 的 target/ 在仓库根目录而不是 src-tauri 下，
      // 只忽略 src-tauri 的话，Windows 上 `pnpm tauri dev` 编译期间 Vite 的
      // 文件监视器会撞到被 cargo 锁定的产物，抛 EBUSY 崩溃。
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{spec,test}.{ts,js}"],
  },
}));
