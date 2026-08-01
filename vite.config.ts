import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

// Tauri 期望的固定开发端口，不要改 host/port 配置
export default defineConfig({
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
    },
    envPrefix: ["VITE_", "TAURI_"],
    build: {
        // Tauri 在 Windows 上使用 Chromium，支持现代特性
        target: "es2021",
        rollupOptions: {
            input: {
                main: resolve(__dirname, "index.html"),
                settings: resolve(__dirname, "settings.html"),
                widget: resolve(__dirname, "widget.html"),
            },
        },
    },
});
