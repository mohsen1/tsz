import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const watchMode = process.argv.includes("--watch");

const buildOptions = {
  entryPoints: [path.join(root, "src", "playground-app", "main.jsx")],
  bundle: true,
  format: "esm",
  platform: "browser",
  target: ["es2020"],
  jsx: "automatic",
  outfile: path.join(root, "static", "playground-app.js"),
  sourcemap: true,
  external: ["/wasm/*"],
  logLevel: "info",
};

if (watchMode) {
  const context = await esbuild.context(buildOptions);
  await context.watch();
  console.log("playground build watching");
} else {
  await esbuild.build(buildOptions);
}