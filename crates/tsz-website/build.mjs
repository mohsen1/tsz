#!/usr/bin/env node

/**
 * TSZ Website Build Script
 *
 * Generates a static website from markdown content, injecting
 * live metrics from README/CI and benchmark data.
 *
 * Usage: node build.mjs [--watch]
 */

import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";
import { marked } from "marked";

const ROOT = path.resolve(import.meta.dirname, "..", "..");
const WEBSITE = import.meta.dirname;
const DIST = path.join(WEBSITE, "dist");

// ── Helpers ──────────────────────────────────────────────────

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function copyFileSync(src, dest) {
  ensureDir(path.dirname(dest));
  fs.copyFileSync(src, dest);
}

function copyDirSync(src, dest) {
  ensureDir(dest);
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDirSync(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

function readIfExists(p) {
  try { return fs.readFileSync(p, "utf8"); } catch { return null; }
}

function readJsonIfExists(p) {
  const text = readIfExists(p);
  return text ? JSON.parse(text) : null;
}

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

// ── Step 1: Extract metrics ─────────────────────────────────

function extractMetrics() {
  const metrics = {
    conformance_rate: "0", conformance_passed: "0", conformance_total: "0",
    emit_js_rate: "0", emit_js_passed: "0", emit_js_total: "0",
    emit_dts_rate: "0", emit_dts_passed: "0", emit_dts_total: "0",
    fourslash_rate: "0", fourslash_passed: "0", fourslash_total: "0",
    ts_version: "unknown",
  };

  // Try CI metrics first
  const metricsDir = path.join(ROOT, ".ci-metrics");
  const conformance = readJsonIfExists(path.join(metricsDir, "conformance.json"));
  const emit = readJsonIfExists(path.join(metricsDir, "emit.json"));
  const fourslash = readJsonIfExists(path.join(metricsDir, "fourslash.json"));

  if (conformance) {
    metrics.conformance_rate = Number(conformance.pass_rate).toFixed(1);
    metrics.conformance_passed = fmt(conformance.passed);
    metrics.conformance_total = fmt(conformance.total);
  }
  if (emit) {
    metrics.emit_js_rate = Number(emit.js_pass_rate).toFixed(1);
    metrics.emit_js_passed = fmt(emit.js_passed);
    metrics.emit_js_total = fmt(emit.js_total);
    metrics.emit_dts_rate = Number(emit.dts_pass_rate).toFixed(1);
    metrics.emit_dts_passed = fmt(emit.dts_passed);
    metrics.emit_dts_total = fmt(emit.dts_total);
  }
  if (fourslash) {
    metrics.fourslash_rate = Number(fourslash.pass_rate).toFixed(1);
    metrics.fourslash_passed = fmt(fourslash.passed);
    metrics.fourslash_total = fmt(fourslash.total);
  }

  // Fall back to parsing README if CI metrics not available
  if (!conformance || !emit || !fourslash) {
    const readme = readIfExists(path.join(ROOT, "README.md"));
    if (readme) {
      // Parse TS version
      const versionMatch = readme.match(/TypeScript.*?`([\d.]+[^`]*)`/);
      if (versionMatch) metrics.ts_version = versionMatch[1];

      if (!conformance) {
        const confSection = readme.match(/<!-- CONFORMANCE_START -->([\s\S]*?)<!-- CONFORMANCE_END -->/);
        if (confSection) {
          const m = confSection[1].match(/([\d.]+)%\s*\(([\d,]+)\/([\d,]+)/);
          if (m) {
            metrics.conformance_rate = m[1];
            metrics.conformance_passed = m[2];
            metrics.conformance_total = m[3];
          }
        }
      }

      if (!emit) {
        const emitSection = readme.match(/<!-- EMIT_START -->([\s\S]*?)<!-- EMIT_END -->/);
        if (emitSection) {
          const lines = emitSection[1].split("\n");
          for (const line of lines) {
            const m = line.match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
            if (m) {
              if (line.includes("JavaScript")) {
                metrics.emit_js_rate = m[1];
                metrics.emit_js_passed = m[2].trim();
                metrics.emit_js_total = m[3].trim();
              } else if (line.includes("Declaration")) {
                metrics.emit_dts_rate = m[1];
                metrics.emit_dts_passed = m[2].trim();
                metrics.emit_dts_total = m[3].trim();
              }
            }
          }
        }
      }

      if (!fourslash) {
        const fsSection = readme.match(/<!-- FOURSLASH_START -->([\s\S]*?)<!-- FOURSLASH_END -->/);
        if (fsSection) {
          const m = fsSection[1].match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
          if (m) {
            metrics.fourslash_rate = m[1];
            metrics.fourslash_passed = m[2].trim();
            metrics.fourslash_total = m[3].trim();
          }
        }
      }

      // Also get TS version if not set from CI
      if (metrics.ts_version === "unknown" && versionMatch) {
        metrics.ts_version = versionMatch[1];
      }
    }
  }

  // Always try to get TS version from README
  if (metrics.ts_version === "unknown") {
    const readme = readIfExists(path.join(ROOT, "README.md"));
    if (readme) {
      const m = readme.match(/TypeScript.*?`([\d.]+[^`]*)`/);
      if (m) metrics.ts_version = m[1];
    }
  }

  return metrics;
}

// ── Step 2: Compute LOC ─────────────────────────────────────

function computeLoc() {
  try {
    const output = execSync(
      `find crates -path '*/target/*' -prune -o -path '*/src/*' -type f -name '*.rs' -print | xargs wc -l`,
      { cwd: ROOT, encoding: "utf8", maxBuffer: 10 * 1024 * 1024 }
    );
    const lines = output.trim().split("\n");
    const totalLine = lines[lines.length - 1];
    const totalMatch = totalLine.match(/^\s*(\d+)\s+total/);
    const total = totalMatch ? Number(totalMatch[1]) : 0;

    // Count crates
    const cratesDir = path.join(ROOT, "crates");
    const crates = fs.readdirSync(cratesDir, { withFileTypes: true })
      .filter(d => d.isDirectory())
      .length;

    return { total: fmt(total), num_crates: String(crates) };
  } catch {
    return { total: "N/A", num_crates: "N/A" };
  }
}

// ── Step 3: Load benchmark data ─────────────────────────────

function loadBenchmarks() {
  const artifactsDir = path.join(ROOT, "artifacts");
  const locations = (() => {
    try {
      return fs.readdirSync(artifactsDir)
        .filter(f => f.startsWith("bench-vs-tsgo-") && f.endsWith(".json"))
        .sort()
        .reverse()
        .map(f => path.join(artifactsDir, f));
    } catch { return []; }
  })();

  // Fall back to the committed snapshot so the site always shows real data
  // even when the CI GCS download hasn't run yet.
  const snapshot = path.join(WEBSITE, "bench-snapshot.json");
  const searchPaths = [...locations, snapshot];

  for (const loc of searchPaths) {
    const data = readJsonIfExists(loc);
    if (data?.results) {
      console.log(`  Loaded benchmarks from ${path.relative(ROOT, loc)}`);
      return data;
    }
  }
  return null;
}

function generateBenchmarkCharts(data) {
  if (!data?.results?.length) {
    return `<div class="bench-placeholder">
      <p>No benchmark data available.</p>
      <p>Run <code>./scripts/bench/bench-vs-tsgo.sh --json</code> to generate benchmarks.</p>
    </div>`;
  }

  const allResults = data.results;
  const measurable = allResults.filter(r => r.tsz_ms != null && r.tsgo_ms != null);
  if (!allResults.length) return `<div class="bench-placeholder">No valid benchmark results found.</div>`;

  // Use measurable cases for the bar-width scale; error rows render
  // without bars so they don't need to participate in the max.
  const maxMs = measurable.length
    ? Math.max(...measurable.map(r => Math.max(r.tsz_ms, r.tsgo_ms)))
    : 1;
  const barMaxWidth = 400; // px

  let html = `<div class="bench-chart">\n`;

  for (const r of allResults) {
    const isError = r.tsz_ms == null || r.tsgo_ms == null;

    if (isError) {
      // Render an error/timeout row with status text in place of bars.
      // This way every benchmark that the runner attempted is visible
      // on the site, not just the ones tsz currently completes.
      const status = r.status || (r.winner === "error" ? "error" : "no data");
      html += `  <div class="bench-row bench-row-error">
    <div class="bench-name">${escapeHtml(r.name)}</div>
    <div class="bench-meta">${fmt(r.lines || 0)} lines, ${fmt(r.kb || 0)} KB</div>
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        <span class="bench-bar-status">${escapeHtml(status)}</span>
      </div>
    </div>
  </div>\n`;
      continue;
    }

    const tszWidth = Math.max(2, (r.tsz_ms / maxMs) * barMaxWidth);
    const tsgoWidth = Math.max(2, (r.tsgo_ms / maxMs) * barMaxWidth);
    const winnerLabel = r.winner === "tsz"
      ? `tsz ${r.factor?.toFixed(1)}x faster`
      : r.winner === "tsgo"
        ? `tsgo ${r.factor?.toFixed(1)}x faster`
        : "";

    html += `  <div class="bench-row">
    <div class="bench-name">${escapeHtml(r.name)}</div>
    <div class="bench-meta">${fmt(r.lines || 0)} lines, ${fmt(r.kb || 0)} KB</div>
    <div class="bench-bars">
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsz</span>
        <div class="bench-bar tsz" style="width: ${tszWidth}px"></div>
        <span class="bench-bar-time">${r.tsz_ms.toFixed(0)}ms</span>
      </div>
      <div class="bench-bar-row">
        <span class="bench-bar-label">tsgo</span>
        <div class="bench-bar tsgo" style="width: ${tsgoWidth}px"></div>
        <span class="bench-bar-time">${r.tsgo_ms.toFixed(0)}ms</span>
        ${winnerLabel ? `<span class="bench-winner">${winnerLabel}</span>` : ""}
      </div>
    </div>
  </div>\n`;
  }

  html += `</div>`;
  return html;
}

function escapeHtml(str) {
  return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

// ── Step 4: Template engine ─────────────────────────────────

function applyTemplate(template, vars) {
  return template.replace(/\{\{(\w+)\}\}/g, (match, key) => {
    return vars[key] ?? match;
  });
}

function wrapInLayout(content, vars) {
  const layout = fs.readFileSync(path.join(WEBSITE, "templates", "layout.html"), "utf8");
  return applyTemplate(layout, {
    content,
    title: vars.title || "TSZ",
    page_class: vars.page_class || "",
    extra_head: vars.extra_head || "",
    extra_scripts: vars.extra_scripts || "",
  });
}

// ── Step 5: Process markdown page ───────────────────────────

function processMarkdownPage(mdPath, vars, outputDir) {
  let md = fs.readFileSync(mdPath, "utf8");
  md = applyTemplate(md, vars);

  // marked renders markdown to HTML — but we have raw HTML blocks too
  // Use marked with html: true so our div blocks pass through
  const contentHtml = marked.parse(md, { breaks: false });
  const html = wrapInLayout(contentHtml, vars);

  ensureDir(outputDir);
  fs.writeFileSync(path.join(outputDir, "index.html"), html);
}

// ── Build ────────────────────────────────────────────────────

function build() {
  console.log("Building tsz website...\n");

  // Clean dist
  fs.rmSync(DIST, { recursive: true, force: true });
  ensureDir(DIST);

  // Gather data
  console.log("  Extracting metrics...");
  const metrics = extractMetrics();
  console.log(`    Conformance: ${metrics.conformance_rate}%`);
  console.log(`    JS Emit: ${metrics.emit_js_rate}%`);
  console.log(`    DTS Emit: ${metrics.emit_dts_rate}%`);
  console.log(`    Fourslash: ${metrics.fourslash_rate}%`);

  console.log("  Computing LOC...");
  const loc = computeLoc();
  console.log(`    Total: ${loc.total} lines, ${loc.num_crates} crates`);

  console.log("  Loading benchmarks...");
  const benchData = loadBenchmarks();
  const benchmarkCharts = generateBenchmarkCharts(benchData);

  // Template variables
  const vars = {
    ...metrics,
    total_loc: loc.total,
    num_crates: loc.num_crates,
    benchmark_charts: benchmarkCharts,
  };

  // Copy static files
  console.log("  Copying static assets...");
  for (const file of fs.readdirSync(path.join(WEBSITE, "static"))) {
    copyFileSync(
      path.join(WEBSITE, "static", file),
      path.join(DIST, file)
    );
  }

  // Process content pages
  console.log("  Building landing page...");
  processMarkdownPage(
    path.join(WEBSITE, "content", "index.md"),
    { ...vars, title: "Project Zang", page_class: "home" },
    DIST
  );

  console.log("  Building benchmarks page...");
  processMarkdownPage(
    path.join(WEBSITE, "content", "benchmarks.md"),
    { ...vars, title: "Benchmarks", page_class: "benchmarks" },
    path.join(DIST, "benchmarks")
  );

  console.log("  Building sound mode page...");
  processMarkdownPage(
    path.join(WEBSITE, "content", "sound-mode.md"),
    { ...vars, title: "Sound Mode", page_class: "sound-mode", extra_scripts: `<script src="/sound-mode-editors.js"></script>` },
    path.join(DIST, "sound-mode")
  );

  // Copy architecture.html with nav injection
  console.log("  Copying architecture deep dive...");
  const archDir = path.join(DIST, "architecture");
  ensureDir(archDir);
  let archHtml = fs.readFileSync(path.join(ROOT, "docs", "architecture.html"), "utf8");
  // Inject a small nav banner at the top of the body
  const archNavStyle = `<style>
    .arch-nav {
      position: fixed; top: 0; left: 0; right: 0; z-index: 100; height: 3rem;
      background: var(--bg-subtle); border-bottom: 1px solid var(--border);
      padding: 0 2rem; display: flex; align-items: center; gap: 1.5rem;
      font-family: var(--font); font-size: 0.875rem;
      overflow-x: auto; white-space: nowrap;
    }
    .arch-nav a { text-decoration: none; flex-shrink: 0; }
    html { scroll-padding-top: 3.5rem; }
    .sidebar { top: 3rem !important; height: calc(100vh - 3rem) !important; }
    .content { padding-top: 5rem !important; }
    @media (max-width: 768px) {
      .arch-nav { padding: 0 1rem; gap: 1rem; font-size: 0.8rem; }
    }
  </style>`;
  const navBanner = `<nav class="arch-nav">
    <a href="/" style="font-weight: 800; font-family: var(--mono); color: var(--text); font-size: 1.1rem;">tsz</a>
    <a href="/playground/" style="color: var(--text-secondary);">Playground</a>
    <a href="/benchmarks/" style="color: var(--text-secondary);">Benchmarks</a>
    <a href="/architecture/" style="color: var(--text); font-weight: 600;">Deep Dive</a>
    <a href="/sound-mode/" style="color: var(--text-secondary);">Sound Mode</a>
    <a href="https://github.com/mohsen1/tsz" style="color: var(--text-secondary); margin-left: auto;">GitHub</a>
  </nav>`;
  archHtml = archHtml.replace("</head>", `${archNavStyle}\n</head>`);
  archHtml = archHtml.replace("<body>", `<body>\n${navBanner}`);
  fs.writeFileSync(path.join(archDir, "index.html"), archHtml);

  // Build playground page
  console.log("  Building playground...");
  const playgroundTemplate = readIfExists(path.join(WEBSITE, "templates", "playground.html"));
  if (playgroundTemplate) {
    const playgroundPage = wrapInLayout(playgroundTemplate, {
      title: "Playground",
      page_class: "playground-page",
      extra_head: `<link rel="stylesheet" href="/playground.css">`,
      extra_scripts: `<script src="/playground.js" type="module"></script>`,
    });
    ensureDir(path.join(DIST, "playground"));
    fs.writeFileSync(path.join(DIST, "playground", "index.html"), playgroundPage);
  }

  // Copy WASM if available
  const wasmSources = [
    path.join(ROOT, "pkg", "web"),
    path.join(ROOT, "npm", "tsz", "wasm", "web"),
    path.join(ROOT, "pkg", "bundler"),
    path.join(ROOT, "pkg"),
  ];
  for (const wasmSrc of wasmSources) {
    if (fs.existsSync(wasmSrc)) {
      console.log(`  Copying WASM from ${path.relative(ROOT, wasmSrc)}...`);
      const wasmDest = path.join(DIST, "wasm");
      ensureDir(wasmDest);
      for (const file of fs.readdirSync(wasmSrc)) {
        if (file.endsWith(".wasm") || file.endsWith(".js") || file.endsWith(".d.ts")) {
          copyFileSync(path.join(wasmSrc, file), path.join(wasmDest, file));
        }
      }
      break;
    }
  }

  // Copy lib files for playground
  const libAssetsDir = path.join(ROOT, "crates", "tsz-core", "src", "lib-assets");
  if (fs.existsSync(libAssetsDir)) {
    console.log("  Copying lib files for playground...");
    const libDest = path.join(DIST, "lib");
    ensureDir(libDest);
    // Files in lib-assets have no "lib." prefix (e.g. "es5.d.ts" not "lib.es5.d.ts")
    const essentialLibs = [
      "es5.d.ts",
      "es2015.d.ts",
      "es2015.core.d.ts",
      "es2015.collection.d.ts",
      "es2015.promise.d.ts",
      "es2015.symbol.d.ts",
      "es2015.iterable.d.ts",
      "es2015.generator.d.ts",
      "dom.d.ts",
      "decorators.d.ts",
      "decorators.legacy.d.ts",
    ];
    for (const libFile of essentialLibs) {
      const src = path.join(libAssetsDir, libFile);
      if (fs.existsSync(src)) {
        // Copy with "lib." prefix so the WASM module finds them
        copyFileSync(src, path.join(libDest, `lib.${libFile}`));
      }
    }
  }

  // Create .nojekyll and CNAME for gh-pages
  fs.writeFileSync(path.join(DIST, ".nojekyll"), "");
  fs.writeFileSync(path.join(DIST, "CNAME"), "tsz.dev");

  console.log(`\nDone. Output: ${path.relative(ROOT, DIST)}/\n`);
}

build();
