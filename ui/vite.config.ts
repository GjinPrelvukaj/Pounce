import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri drives this dev server, so the port is fixed and failing loudly beats
// silently moving to 1421 — the desktop shell would then load nothing.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // The Rust side has its own rebuild loop; watching it here would restart
      // Vite on every `cargo` write.
      ignored: ["**/target/**", "**/crates/**"],
    },
  },
  build: {
    // Tauri ships a known WebView per platform, so there is no old-browser
    // floor to support and no reason to down-level modern syntax.
    target: "es2022",
    // A desktop bundle nobody downloads over a network: source maps cost disk,
    // not load time, and they are what makes a user's stack trace legible.
    sourcemap: true,
  },
});
