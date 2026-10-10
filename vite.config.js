import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";

// Tauri 需要固定端口；两个窗口各是一个多页入口
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    // Windows 下 "localhost" 可能解析到 ::1，Tauri 探测时用 127.0.0.1 会连不上，
    // 这里显式绑定 IPv4 回环，和 tauri.conf.json 的 devUrl 保持一致
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        reader: resolve(__dirname, "reader.html"),
      },
    },
  },
});
