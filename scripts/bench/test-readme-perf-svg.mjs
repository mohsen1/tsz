import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  createReadmePerfSummary,
  renderReadmePerfPng,
  renderReadmePerfSvg,
} from "./readme-perf-svg.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(SCRIPT_DIR, "readme-perf-svg.mjs");

const artifact = {
  generated_at: "2026-05-28T02:14:24.444Z",
  source_commit: "0123456789abcdef0123456789abcdef01234567",
  results: [
    {
      name: "wide-union",
      lines: 300,
      tsz_ms: 1000,
      tsgo_ms: 4000,
      winner: "tsz",
    },
    {
      name: "generic-stress",
      lines: 250,
      tsz_ms: 500,
      tsgo_ms: 500,
      winner: "tie",
    },
    {
      name: "tiny-startup",
      lines: 20,
      tsz_ms: 10,
      tsgo_ms: 10000,
      winner: "tsz",
    },
    {
      name: "utility-types-project",
      lines: 2000,
      tsz_ms: 30,
      tsgo_ms: 9000,
      winner: "tsz",
    },
    {
      name: "failed-row",
      lines: 300,
      tsz_ms: null,
      tsgo_ms: null,
      winner: "error",
      status: "timeout",
    },
  ],
};

const summary = createReadmePerfSummary(artifact);
assert.equal(summary.rows, 2);
assert.equal(summary.totalRows, 5);
assert.equal(summary.tszMs, 1500);
assert.equal(summary.tsgoMs, 4500);
assert.equal(summary.speedup, 3);
assert.equal(summary.winner, "tsz");
assert.equal(summary.generatedAt, "2026-05-28T02:14:24Z");
assert.equal(summary.sourceCommit, "0123456789ab");

const svg = renderReadmePerfSvg(artifact);
assert.match(svg, /<svg[^>]+role="img"/);
assert.match(svg, /font-family="'SF Mono','Cascadia Code','JetBrains Mono','Fira Code',Menlo,Consolas,monospace"/);
assert.match(svg, /fill="#cf222e"/);
assert.match(renderReadmePerfSvg(artifact, { theme: "dark" }), /fill="#ff7b72"/);
assert.doesNotMatch(svg, /stroke="/);
assert.doesNotMatch(svg, />Latest benchmark snapshot</);
assert.doesNotMatch(svg, />2 successful micro rows</);
assert.doesNotMatch(svg, />tsz 3\.0x faster</);
assert.match(svg, /1\.5s/);
assert.match(svg, /4\.5s/);
assert.doesNotMatch(svg, /Project-mode and tiny startup fixtures are excluded/);

const lightPng = await renderReadmePerfPng(artifact, { theme: "light" });
const darkPng = await renderReadmePerfPng(artifact, { theme: "dark" });
assert.equal(lightPng.slice(0, 8).toString("hex"), "89504e470d0a1a0a");
assert.equal(lightPng.readUInt32BE(16), 760);
assert.equal(lightPng.readUInt32BE(20), 112);
assert.equal(darkPng.slice(0, 8).toString("hex"), "89504e470d0a1a0a");
assert.notEqual(lightPng.toString("base64"), darkPng.toString("base64"));

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-readme-perf-"));
try {
  const input = path.join(tempDir, "bench.json");
  const output = path.join(tempDir, "chart.png");
  fs.writeFileSync(input, `${JSON.stringify(artifact, null, 2)}\n`);
  const result = spawnSync(process.execPath, [SCRIPT, "--theme", "dark", input, output], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(fs.readFileSync(output).slice(0, 8).toString("hex"), "89504e470d0a1a0a");
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

const emptySummary = createReadmePerfSummary({ results: [] });
assert.equal(emptySummary.rows, 0);
assert.equal(emptySummary.speedup, null);
assert.match(
  renderReadmePerfSvg({ results: [] }),
  /No successful benchmark timing pairs were available/,
);

const tieArtifact = {
  results: [{ name: "even-row", lines: 300, tsz_ms: 100, tsgo_ms: 100, winner: "tie" }],
};
assert.equal(createReadmePerfSummary(tieArtifact).winner, "tie");
assert.match(renderReadmePerfSvg(tieArtifact), /tsz and tsgo are even/);
