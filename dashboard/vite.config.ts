import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
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
