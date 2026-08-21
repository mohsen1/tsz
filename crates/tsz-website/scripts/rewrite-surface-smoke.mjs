import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(websiteRoot, "..", "..");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

const retiredWebsiteFiles = [
  "crates/tsz-website/scripts/build-playground.mjs",
  "crates/tsz-website/scripts/playground-smoke.mjs",
  "crates/tsz-website/src/_includes/partials/playground.html",
  "crates/tsz-website/src/playground-app/examples.js",
  "crates/tsz-website/src/playground-app/main.jsx",
  "crates/tsz-website/static/playground-app.js",
  "crates/tsz-website/static/playground-app.js.map",
  "crates/tsz-website/static/playground.css",
  "scripts/emit/src/emit-worker.ts",
];

for (const relativePath of retiredWebsiteFiles) {
  assert(!fs.existsSync(path.join(repoRoot, relativePath)), `${relativePath} must stay retired`);
}

const manifest = JSON.parse(read("crates/tsz-website/package.json"));
const packages = {
  ...manifest.dependencies,
  ...manifest.devDependencies,
};
assert(!manifest.scripts["build:playground"], "the retired playground build script must not return");
assert(!/playground/i.test(manifest.scripts.build), "the production build must not bundle a playground client");
for (const dependency of ["react", "react-dom", "esbuild"]) {
  assert(!packages[dependency], `${dependency} is not part of the static website build`);
}

const activeInputs = [
  "crates/tsz-website/.eleventy.js",
  "crates/tsz-website/scripts/dev.mjs",
  "crates/tsz-website/serve.mjs",
  "crates/tsz-website/src/_data/loc.js",
  ".github/workflows/gh-pages.yml",
];
const retiredRuntimePattern = /(?:\bwasm\b|tsz[-_]wasm|playground-app|build-playground|pkg\/web|tsz-website\/rust)/i;
for (const relativePath of activeInputs) {
  assert(
    !retiredRuntimePattern.test(read(relativePath)),
    `${relativePath} must not build, copy, serve, or publish the retired browser runtime`,
  );
}

const pagesWorkflow = read(".github/workflows/gh-pages.yml");
assert(!pagesWorkflow.includes("crates/**/*.rs"), "Rust implementation changes are not Pages inputs");
assert(!pagesWorkflow.includes("crates/**/Cargo.toml"), "Rust package changes are not Pages inputs");

const playgroundTemplate = read("crates/tsz-website/src/playground.njk");
assert(
  playgroundTemplate.includes("Playground unavailable during the clean-slate rewrite."),
  "the playground route must state its R0 availability honestly",
);
assert(
  !/(?:extra_scripts|playground-app|\/wasm\/|tsz_wasm)/i.test(playgroundTemplate),
  "the playground route must remain a static status page",
);

function collectTypeScriptSources(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectTypeScriptSources(entryPath);
    return entry.isFile() && entry.name.endsWith(".ts") ? [entryPath] : [];
  });
}

const emitSourceRoot = path.join(repoRoot, "scripts", "emit", "src");
for (const sourcePath of collectTypeScriptSources(emitSourceRoot)) {
  const source = fs.readFileSync(sourcePath, "utf8");
  const relativePath = path.relative(repoRoot, sourcePath);
  assert(!/\bwasm\b|tsz[-_]wasm/i.test(source), `${relativePath} must use the native process contract`);
}

if (process.argv.includes("--dist")) {
  const distRoot = path.join(websiteRoot, "dist");
  const playgroundHtml = fs.readFileSync(
    path.join(distRoot, "playground", "index.html"),
    "utf8",
  );
  assert(
    playgroundHtml.includes("Playground unavailable during the clean-slate rewrite."),
    "the production playground page must preserve the availability notice",
  );
  assert(
    !/(?:playground-app|\/wasm\/|tsz_wasm)/i.test(playgroundHtml),
    "the production playground page must not load a retired runtime",
  );
  for (const output of ["playground-app.js", "playground-app.js.map", "playground.css", "wasm"]) {
    assert(!fs.existsSync(path.join(distRoot, output)), `dist/${output} must not be published`);
  }
}

console.log("rewrite website surface smoke passed");
