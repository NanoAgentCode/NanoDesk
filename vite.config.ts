import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import brandConfig from "./brand.config.json";

export default defineConfig({
  plugins: [
    react(),
    {
      name: "brand-config",
      transformIndexHtml(html) {
        return html
          .split("%APP_DISPLAY_NAME%")
          .join(brandConfig.displayName)
          .split("%APP_STORAGE_PREFIX%")
          .join(brandConfig.storagePrefix);
      }
    }
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2020",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: Boolean(process.env.TAURI_DEBUG),
    rollupOptions: {
      output: {
        manualChunks(id) {
          const normalizedId = id.replace(/\\/g, "/");
          if (!normalizedId.includes("node_modules")) {
            return undefined;
          }
          if (
            normalizedId.includes("react-dom") ||
            normalizedId.includes("node_modules/react/") ||
            normalizedId.includes("scheduler") ||
            normalizedId.includes("loose-envify") ||
            normalizedId.includes("js-tokens")
          ) {
            return "vendor-react";
          }
          if (normalizedId.includes("@tauri-apps")) {
            return "vendor-tauri";
          }
          if (
            normalizedId.includes("@mantine") ||
            normalizedId.includes("@floating-ui") ||
            normalizedId.includes("react-remove-scroll") ||
            normalizedId.includes("use-sidecar") ||
            normalizedId.includes("clsx")
          ) {
            return undefined;
          }
          if (
            normalizedId.includes("react-markdown") ||
            normalizedId.includes("remark-") ||
            normalizedId.includes("rehype-") ||
            normalizedId.includes("katex") ||
            normalizedId.includes("micromark") ||
            normalizedId.includes("mdast") ||
            normalizedId.includes("hast") ||
            normalizedId.includes("unified") ||
            normalizedId.includes("unist") ||
            normalizedId.includes("vfile") ||
            normalizedId.includes("property-information") ||
            normalizedId.includes("space-separated-tokens") ||
            normalizedId.includes("comma-separated-tokens") ||
            normalizedId.includes("decode-named-character-reference") ||
            normalizedId.includes("character-entities")
          ) {
            return "vendor-markdown";
          }
          if (normalizedId.includes("lucide-react") || normalizedId.includes("lucide-static")) {
            return "vendor-icons";
          }
          return "vendor";
        }
      }
    }
  }
});
