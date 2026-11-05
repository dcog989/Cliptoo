import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import { readmePlugin } from "./readme.js";

export default defineConfig({
  root: "website",
  base: "./",
  plugins: [viteSingleFile(), readmePlugin()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsInlineLimit: 0,
  },
});
