#!/usr/bin/env node

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import zlib from "node:zlib";
import { PROJECT_ROWS_BY_NAME } from "./project-rows.mjs";

const SVG_WIDTH = 760;
const SVG_HEIGHT = 112;
const PNG_WIDTH = SVG_WIDTH;
const PNG_HEIGHT = SVG_HEIGHT;
const BAR_X = 120;
const BAR_WIDTH = 530;
const BAR_HEIGHT = 30;
const FIRST_BAR_Y = 22;
const SECOND_BAR_Y = 62;
const TINY_BENCHMARK_MAX_LINES = 200;
const MONOSPACE_FONT = "'SF Mono','Cascadia Code','JetBrains Mono','Fira Code',Menlo,Consolas,monospace";
const THEMES = {
  light: {
    background: "#ffffff",
    border: "#d1d9e0",
    title: "#1f2328",
    text: "#59636e",
    muted: "#8b949e",
    track: "#f6f8fa",
    tsz: "#cf222e",
    tsgo: "#0550ae",
  },
  dark: {
    background: "#0d1117",
    border: "#30363d",
    title: "#e6edf3",
    text: "#8b949e",
    muted: "#6e7681",
    track: "#161b22",
    tsz: "#ff7b72",
    tsgo: "#58a6ff",
  },
};

const FONT_5X7 = {
  " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
  "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
  ".": ["00000", "00000", "00000", "00000", "00000", "01100", "01100"],
  ":": ["00000", "01100", "01100", "00000", "01100", "01100", "00000"],
  "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
  "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
  "2": ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
  "3": ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
  "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
  "5": ["11111", "10000", "10000", "11110", "00001", "00001", "11110"],
  "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
  "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
  "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
  "9": ["01110", "10001", "10001", "01111", "00001", "00010", "11100"],
  A: ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
  B: ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
  C: ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
  D: ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
  E: ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
  F: ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
  G: ["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
  H: ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
  I: ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
  J: ["00111", "00010", "00010", "00010", "00010", "10010", "01100"],
  K: ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
  L: ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
  M: ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
  N: ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
  O: ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
  P: ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
  Q: ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
  R: ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
  S: ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
  T: ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
  U: ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
  V: ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
  W: ["10001", "10001", "10001", "10101", "10101", "10101", "01010"],
  X: ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
  Y: ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
  Z: ["11111", "00001", "00010", "00100", "01000", "10000", "11111"],
};

function hexToRgba(color) {
  const normalized = color.replace(/^#/, "");
  const hex = normalized.length === 3
    ? normalized.split("").map((char) => char + char).join("")
    : normalized;
  const value = Number.parseInt(hex, 16);
  return [
    (value >> 16) & 0xff,
    (value >> 8) & 0xff,
    value & 0xff,
    0xff,
  ];
}

function createRgbaCanvas(width, height, background) {
  const rgba = Buffer.alloc(width * height * 4);
  const [r, g, b, a] = hexToRgba(background);
  for (let offset = 0; offset < rgba.length; offset += 4) {
    rgba[offset] = r;
    rgba[offset + 1] = g;
    rgba[offset + 2] = b;
    rgba[offset + 3] = a;
  }
  return { width, height, rgba };
}

function fillRect(canvas, x, y, width, height, color) {
  const [r, g, b, a] = hexToRgba(color);
  const left = Math.max(0, Math.floor(x));
  const top = Math.max(0, Math.floor(y));
  const right = Math.min(canvas.width, Math.ceil(x + width));
  const bottom = Math.min(canvas.height, Math.ceil(y + height));
  for (let row = top; row < bottom; row += 1) {
    for (let col = left; col < right; col += 1) {
      const offset = ((row * canvas.width) + col) * 4;
      canvas.rgba[offset] = r;
      canvas.rgba[offset + 1] = g;
      canvas.rgba[offset + 2] = b;
      canvas.rgba[offset + 3] = a;
    }
  }
}

function bitmapTextWidth(text, scale) {
  const chars = String(text).length;
  return chars === 0 ? 0 : ((chars * 6) - 1) * scale;
}

function drawBitmapText(canvas, text, x, y, scale, color) {
  const glyphColor = hexToRgba(color);
  let cursor = x;
  for (const rawChar of String(text).toUpperCase()) {
    const glyph = FONT_5X7[rawChar] ?? FONT_5X7[" "];
    for (let row = 0; row < glyph.length; row += 1) {
      for (let col = 0; col < glyph[row].length; col += 1) {
        if (glyph[row][col] !== "1") continue;
        const left = Math.floor(cursor + (col * scale));
        const top = Math.floor(y + (row * scale));
        const right = Math.min(canvas.width, left + scale);
        const bottom = Math.min(canvas.height, top + scale);
        for (let pxY = Math.max(0, top); pxY < bottom; pxY += 1) {
          for (let pxX = Math.max(0, left); pxX < right; pxX += 1) {
            const offset = ((pxY * canvas.width) + pxX) * 4;
            canvas.rgba[offset] = glyphColor[0];
            canvas.rgba[offset + 1] = glyphColor[1];
            canvas.rgba[offset + 2] = glyphColor[2];
            canvas.rgba[offset + 3] = glyphColor[3];
          }
        }
      }
    }
    cursor += 6 * scale;
  }
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data = Buffer.alloc(0)) {
  const typeBuffer = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 0);
  return Buffer.concat([length, typeBuffer, data, crc]);
}

function encodePng(canvas) {
  const stride = canvas.width * 4;
  const raw = Buffer.alloc((stride + 1) * canvas.height);
  for (let row = 0; row < canvas.height; row += 1) {
    const rawOffset = row * (stride + 1);
    raw[rawOffset] = 0;
    canvas.rgba.copy(raw, rawOffset + 1, row * stride, (row + 1) * stride);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(canvas.width, 0);
  ihdr.writeUInt32BE(canvas.height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    pngChunk("IEND"),
  ]);
}

async function loadSharp() {
  const packageJsons = [
    import.meta.url,
    path.join(process.cwd(), "package.json"),
    path.join(process.cwd(), "crates", "tsz-website", "package.json"),
  ];

  for (const packageJson of packageJsons) {
    if (typeof packageJson === "string" && packageJson !== import.meta.url && !fs.existsSync(packageJson)) {
      continue;
    }
    try {
      const require = createRequire(packageJson);
      const sharpPath = require.resolve("sharp");
      const sharp = await import(pathToFileURL(sharpPath).href);
      return sharp.default ?? sharp;
    } catch {
      // Try the next package boundary.
    }
  }

  throw new Error(
    "sharp is required for normal monospace PNG rendering. Run `cd crates/tsz-website && npm install`.",
  );
}

function finiteNumber(value) {
  if (value === null || value === undefined || value === "") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function hasSuccessfulTimingPair(row) {
  return !row?.status
    && row?.winner !== "error"
    && finiteNumber(row?.tsz_ms) > 0
    && finiteNumber(row?.tsgo_ms) > 0;
}

function isProjectBenchmark(row) {
  return Boolean(row?.name && PROJECT_ROWS_BY_NAME[row.name]);
}

function isTinyBenchmark(row) {
  const lines = finiteNumber(row?.lines);
  return lines !== null && lines < TINY_BENCHMARK_MAX_LINES;
}

function benchmarkRowsForReadme(data) {
  return Array.isArray(data?.results)
    ? data.results.filter((row) => (
      hasSuccessfulTimingPair(row)
        && !isProjectBenchmark(row)
        && !isTinyBenchmark(row)
    ))
    : [];
}

function formatDurationMs(value) {
  const ms = finiteNumber(value);
  if (ms === null) return "n/a";
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)}m`;
  if (ms >= 1_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${ms.toFixed(0)}ms`;
}

function formatTimestamp(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "local snapshot";
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function shortCommit(value) {
  const commit = String(value || "").trim();
  return /^[0-9a-f]{7,40}$/i.test(commit) ? commit.slice(0, 12) : null;
}

function escapeXml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function scaledBarWidth(value, maxValue) {
  const number = finiteNumber(value);
  const max = finiteNumber(maxValue);
  if (number === null || max === null || max <= 0) return 0;
  return Math.max(3, (number / max) * BAR_WIDTH);
}

function themeColors(theme) {
  return THEMES[theme] ?? THEMES.light;
}

export function createReadmePerfSummary(data) {
  const rows = benchmarkRowsForReadme(data);
  const tszMs = rows.reduce((total, row) => total + (finiteNumber(row.tsz_ms) ?? 0), 0);
  const tsgoMs = rows.reduce((total, row) => total + (finiteNumber(row.tsgo_ms) ?? 0), 0);
  const speedup = tszMs > 0 && tsgoMs > 0 ? tsgoMs / tszMs : null;
  const winner = speedup === null
    ? null
    : Math.abs(speedup - 1) < 0.005
      ? "tie"
      : speedup > 1
      ? "tsz"
      : "tsgo";

  return {
    rows: rows.length,
    totalRows: Array.isArray(data?.results) ? data.results.length : 0,
    tszMs,
    tsgoMs,
    speedup,
    winner,
    generatedAt: formatTimestamp(data?.generated_at),
    sourceCommit: shortCommit(data?.source_commit),
  };
}

function summaryLabel(summary) {
  if (!summary.rows || summary.speedup === null) {
    return "Benchmark data unavailable";
  }

  const factor = summary.winner === "tsz"
    ? summary.speedup
    : 1 / summary.speedup;
  return summary.winner === "tie"
    ? "tsz and tsgo are even"
    : `${summary.winner} ${factor.toFixed(1)}x faster`;
}

function renderBar({ y, label, value, maxValue, color, duration, colors }) {
  const width = scaledBarWidth(value, maxValue);
  return `<text x="34" y="${y + 20}" fill="${colors.text}" font-size="15" font-weight="700">${escapeXml(label)}</text>
  <rect x="${BAR_X}" y="${y}" width="${BAR_WIDTH}" height="${BAR_HEIGHT}" rx="7" fill="${colors.track}"/>
  <rect x="${BAR_X}" y="${y}" width="${width.toFixed(1)}" height="${BAR_HEIGHT}" rx="7" fill="${color}"/>
  <text x="${BAR_X + BAR_WIDTH + 16}" y="${y + 20}" fill="${colors.title}" font-size="15" font-weight="700">${escapeXml(duration)}</text>`;
}

export function renderReadmePerfSvg(data, { theme = "light" } = {}) {
  const colors = themeColors(theme);
  const summary = createReadmePerfSummary(data);
  const maxMs = Math.max(summary.tszMs, summary.tsgoMs, 1);
  const headline = summaryLabel(summary);
  const desc = summary.rows
    ? `${headline} across ${summary.rows} successful micro benchmark rows.`
    : "No successful benchmark timing pairs were available for the README performance chart.";

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${SVG_WIDTH}" height="${SVG_HEIGHT}" viewBox="0 0 ${SVG_WIDTH} ${SVG_HEIGHT}" role="img" aria-labelledby="title desc">
  <title id="title">tsz benchmark performance</title>
  <desc id="desc">${escapeXml(desc)}</desc>
  <rect width="${SVG_WIDTH}" height="${SVG_HEIGHT}" rx="12" fill="${colors.background}"/>
  <g font-family="${MONOSPACE_FONT}">
  ${renderBar({
    y: FIRST_BAR_Y,
    label: "tsz",
    value: summary.tszMs,
    maxValue: maxMs,
    color: colors.tsz,
    duration: formatDurationMs(summary.tszMs),
    colors,
  })}
  ${renderBar({
    y: SECOND_BAR_Y,
    label: "tsgo",
    value: summary.tsgoMs,
    maxValue: maxMs,
    color: colors.tsgo,
    duration: formatDurationMs(summary.tsgoMs),
    colors,
  })}
  </g>
</svg>
`;
}

function drawPngBar(canvas, { y, label, value, maxValue, color, duration, colors }) {
  const width = scaledBarWidth(value, maxValue);
  drawBitmapText(canvas, label, 34, y + 8, 2, colors.text);
  fillRect(canvas, BAR_X, y, BAR_WIDTH, BAR_HEIGHT, colors.track);
  fillRect(canvas, BAR_X, y, width, BAR_HEIGHT, color);
  drawBitmapText(canvas, duration, BAR_X + BAR_WIDTH + 16, y + 8, 2, colors.title);
}

function renderFallbackReadmePerfPng(data, { theme = "light" } = {}) {
  const colors = themeColors(theme);
  const summary = createReadmePerfSummary(data);
  const maxMs = Math.max(summary.tszMs, summary.tsgoMs, 1);

  const canvas = createRgbaCanvas(PNG_WIDTH, PNG_HEIGHT, colors.background);

  drawPngBar(canvas, {
    y: FIRST_BAR_Y,
    label: "tsz",
    value: summary.tszMs,
    maxValue: maxMs,
    color: colors.tsz,
    duration: formatDurationMs(summary.tszMs),
    colors,
  });
  drawPngBar(canvas, {
    y: SECOND_BAR_Y,
    label: "tsgo",
    value: summary.tsgoMs,
    maxValue: maxMs,
    color: colors.tsgo,
    duration: formatDurationMs(summary.tsgoMs),
    colors,
  });

  return encodePng(canvas);
}

export async function renderReadmePerfPng(data, { theme = "light" } = {}) {
  const svg = renderReadmePerfSvg(data, { theme });
  try {
    const sharp = await loadSharp();
    return await sharp(Buffer.from(svg)).png().toBuffer();
  } catch (error) {
    if (process.env.TSZ_README_PERF_REQUIRE_SHARP === "1") {
      throw error;
    }
    return renderFallbackReadmePerfPng(data, { theme });
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] || "").href) {
  const positional = [];
  let theme = "light";
  for (let index = 2; index < process.argv.length; index += 1) {
    const arg = process.argv[index];
    if (arg === "--theme") {
      theme = process.argv[index + 1] || "";
      index += 1;
    } else {
      positional.push(arg);
    }
  }
  const [inputPath, outputPath] = positional;
  if (!inputPath || !outputPath) {
    console.error("usage: node scripts/bench/readme-perf-svg.mjs [--theme light|dark] <benchmark.json> <output.svg|output.png>");
    process.exit(2);
  }
  if (!THEMES[theme]) {
    console.error("theme must be light or dark");
    process.exit(2);
  }

  const data = JSON.parse(fs.readFileSync(inputPath, "utf8"));
  if (outputPath.toLowerCase().endsWith(".png")) {
    fs.writeFileSync(outputPath, await renderReadmePerfPng(data, { theme }));
  } else if (outputPath.toLowerCase().endsWith(".svg")) {
    fs.writeFileSync(outputPath, renderReadmePerfSvg(data, { theme }));
  } else {
    console.error("output path must end in .svg or .png");
    process.exit(2);
  }
}
