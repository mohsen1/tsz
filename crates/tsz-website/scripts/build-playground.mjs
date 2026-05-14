import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const watchMode = process.argv.includes("--watch");

const sharedBuildOptions = {
  bundle: true,
  format: "esm",
  platform: "browser",
  target: ["es2020"],
  sourcemap: true,
  external: ["/wasm/*"],
  logLevel: "info",
};

const builds = [
  {
    ...sharedBuildOptions,
    entryPoints: [path.join(root, "src", "playground-app", "main.jsx")],
    jsx: "automatic",
    outfile: path.join(root, "static", "playground-app.js"),
  },
  {
    ...sharedBuildOptions,
    entryPoints: [path.join(root, "src", "sound-mode-page", "main.js")],
    outfile: path.join(root, "static", "sound-mode-page.js"),
  },
];

if (watchMode) {
  const contexts = await Promise.all(builds.map(buildOptions => esbuild.context(buildOptions)));
  await Promise.all(contexts.map(context => context.watch()));
  console.log("playground builds watching");
} else {
  await Promise.all(builds.map(buildOptions => esbuild.build(buildOptions)));
}
