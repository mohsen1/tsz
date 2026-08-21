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
import { GREEN_COMPAT } from "./row-utils.mjs";

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
      // Short fixtures are no longer excluded: this <200-line case must now
      // count toward the micro total like any other successful row.
      name: "tiny-startup",
      lines: 20,
      tsz_ms: 500,
      tsgo_ms: 1500,
      winner: "tsz",
    },
    {
      // tsz is 3x slower than tsgo here — past the 1.5x slowdown-failure
      // threshold, so this row must NOT count toward the README headline
      // even though the website benchmark page still charts its timing pair.
      name: "utility-types-project",
      lines: 2000,
      tsz_ms: 9000,
      tsgo_ms: 3000,
      winner: "tsgo",
      compatibility: GREEN_COMPAT,
    },
    {
      // tsz is ~1.17x slower — within the 1.5x threshold, so this project row
      // DOES count toward the headline.
      name: "rxjs-project",
      lines: 1500,
      tsz_ms: 3500,
      tsgo_ms: 3000,
      winner: "tsgo",
      compatibility: GREEN_COMPAT,
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
// utility-types-project (tsz 3x slower) is excluded from the README headline by
// the 1.5x slowdown threshold; only the within-threshold rxjs-project counts.
assert.equal(summary.rows, 1);
assert.equal(summary.projectRows, 1);
assert.equal(summary.microRows, 3);
assert.equal(summary.rowKind, "project");
assert.equal(summary.totalRows, 6);
assert.equal(summary.tszMs, 3500);
assert.equal(summary.tsgoMs, 3000);
assert.equal(summary.speedup, 3000 / 3500);
assert.equal(summary.winner, "tsgo");
assert.equal(summary.generatedAt, "2026-05-28T02:14:24Z");
assert.equal(summary.sourceCommit, "0123456789ab");

const svg = renderReadmePerfSvg(artifact);
assert.match(svg, /<svg[^>]+role="img"/);
assert.match(svg, /font-family="'SF Mono','Cascadia Code','JetBrains Mono','Fira Code',Menlo,Consolas,monospace"/);
assert.match(svg, /fill="#cf222e"/);
assert.match(renderReadmePerfSvg(artifact, { theme: "dark" }), /fill="#ff8182"/);
assert.doesNotMatch(svg, /stroke="/);
assert.doesNotMatch(svg, /rx="/);
assert.doesNotMatch(svg, /fill="#ffffff"/);
assert.doesNotMatch(renderReadmePerfSvg(artifact, { theme: "dark" }), /fill="#0d1117"/);
assert.doesNotMatch(svg, />Latest benchmark snapshot</);
assert.doesNotMatch(svg, />2 successful micro rows</);
assert.doesNotMatch(svg, />tsz 3\.0x faster</);
assert.match(svg, />tsgo is 1\.2x faster</);
assert.match(svg, />1 project rows</);
assert.match(svg, /3\.5s/);
assert.match(svg, /3\.0s/);
assert.doesNotMatch(svg, /Project-mode and tiny startup fixtures are excluded/);

const lightPng = await renderReadmePerfPng(artifact, { theme: "light" });
const darkPng = await renderReadmePerfPng(artifact, { theme: "dark" });
assert.equal(lightPng.slice(0, 8).toString("hex"), "89504e470d0a1a0a");
assert.equal(lightPng.readUInt32BE(16), 760);
assert.equal(lightPng.readUInt32BE(20), 128);
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

// #16196: a row where a compiler was killed (timeout) or exited non-zero must
// never contribute a speed ratio or an aggregate datapoint, even when the
// timeout ceiling lands UNDER the 1.5x slowdown-failure threshold and `winner`
// was left non-error — the case the slowdown heuristic and the `winner ===
// "error"` check both miss. Exclusion must key on the run's exit flags, not on
// the ceiling incidentally being large.
{
  const ceilingMs = 400;
  const tsgoMs = 350; // ceiling / tsgo = 1.14x, within the 1.5x slowdown threshold
  const dnfArtifact = {
    results: [
      // A healthy project row so the summary is otherwise non-empty.
      {
        name: "rxjs-project",
        lines: 1500,
        tsz_ms: 3000,
        tsgo_ms: 3300,
        winner: "tsz",
        compatibility: GREEN_COMPAT,
      },
      // A short-ceiling timeout: tsz was KILLED at a 400ms ceiling (exit 124),
      // `winner` left "tsz", ceiling under 1.5x tsgo. Only the explicit DNF
      // guard excludes it.
      {
        name: "large-ts-repo",
        lines: 200000,
        tsz_ms: ceilingMs,
        tsgo_ms: tsgoMs,
        winner: "tsz",
        compatibility: { exit_class: "timeout", exit_codes: { tsz: [124], tsgo: [0] } },
      },
    ],
  };
  const dnfSummary = createReadmePerfSummary(dnfArtifact);
  assert.equal(dnfSummary.projectRows, 1, "a killed project row must be excluded from the chart");
  assert.equal(dnfSummary.rows, 1);
  assert.equal(dnfSummary.tszMs, 3000, "aggregate tsz_ms must exclude the ceiling sentinel");
  assert.equal(dnfSummary.tsgoMs, 3300);
  // The invariant from #16196: the excluded row's ratio would have been
  // ceiling/other_time, a fabricated value that must never reach the aggregate.
  assert.notEqual(dnfSummary.tszMs, ceilingMs);

  // A non-zero tsgo exit is equally DNF even when tsz itself completed cleanly.
  const tsgoErrArtifact = {
    results: [
      {
        name: "rxjs-project",
        lines: 1500,
        tsz_ms: 3000,
        tsgo_ms: 3300,
        winner: "tsz",
        compatibility: GREEN_COMPAT,
      },
      {
        name: "large-ts-repo",
        lines: 200000,
        tsz_ms: 300,
        tsgo_ms: 350,
        winner: "tsz",
        compatibility: { exit_class: "exit success", exit_codes: { tsz: [0], tsgo: [1] } },
      },
    ],
  };
  assert.equal(
    createReadmePerfSummary(tsgoErrArtifact).projectRows,
    1,
    "a non-zero tsgo exit is DNF even when tsz completed",
  );
}
