import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      // Dev server proxies API calls to the running rr daemon.
      "/api": {
        target: "http://127.0.0.1:7070",
        changeOrigin: true,
      },
    },
  },
});
