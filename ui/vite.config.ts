import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Cấu hình cho Tauri: cổng cố định 5173, không xoá màn hình để thấy log Rust.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022", chunkSizeWarningLimit: 1500 },
});
