#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const readmePath = path.join(repoRoot, "README.md");
const metricsDir = process.env.CI_METRICS_DIR
  ? path.resolve(repoRoot, process.env.CI_METRICS_DIR)
  : path.join(repoRoot, ".ci-metrics");

function exists(p) {
  try {
    fs.accessSync(p, fs.constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

function readJsonIfExists(p) {
  if (!exists(p)) return null;
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

function toNumber(value) {
  const n = Number.parseFloat(String(value ?? "").trim());
  return Number.isFinite(n) ? n : null;
}

function toInt(value) {
  const n = Number.parseInt(String(value ?? "").trim(), 10);
  return Number.isFinite(n) ? n : null;
}

function formatInt(value) {
  return Number(value).toLocaleString("en-US");
}

function formatRate(value) {
  return Number(value).toFixed(1);
}

function progressBar(percent) {
  const blocks = 20;
  const normalized = Math.max(0, Math.min(100, Number(percent)));
  const filled = Math.round((normalized / 100) * blocks);
  return "█".repeat(filled) + "░".repeat(blocks - filled);
}

function replaceSection(content, startMarker, endMarker, replacement) {
  const start = content.indexOf(startMarker);
  const end = content.indexOf(endMarker);
  if (start === -1 || end === -1 || end < start) {
    throw new Error(`Section markers not found: ${startMarker} / ${endMarker}`);
  }
  const before = content.slice(0, start + startMarker.length);
  const after = content.slice(end);
  return `${before}\n${replacement}\n${after}`;
}

function validSuiteMetrics(rate, passed, total) {
  return rate !== null && passed !== null && total !== null && total > 0;
}

if (!exists(readmePath)) {
  console.error(`README not found: ${readmePath}`);
  process.exit(1);
}

const conformance = readJsonIfExists(path.join(metricsDir, "conformance.json"));
const emit = readJsonIfExists(path.join(metricsDir, "emit.json"));
const fourslash = readJsonIfExists(path.join(metricsDir, "fourslash.json"));

if (!conformance && !emit && !fourslash) {
  console.log(`No metrics artifacts found in ${metricsDir}; nothing to update.`);
  process.exit(0);
}

let readme = fs.readFileSync(readmePath, "utf8");
let changed = false;

if (conformance) {
  const rate = toNumber(conformance.pass_rate);
  const passed = toInt(conformance.passed);
  const total = toInt(conformance.total);
  if (validSuiteMetrics(rate, passed, total)) {
    const block = [
      "```",
      `Progress: [${progressBar(rate)}] ${formatRate(rate)}% (${formatInt(passed)}/${formatInt(total)} tests)`,
      "```",
    ].join("\n");
    const updated = replaceSection(readme, "<!-- CONFORMANCE_START -->", "<!-- CONFORMANCE_END -->", block);
    changed ||= updated !== readme;
    readme = updated;
    console.log(`Conformance: ${formatRate(rate)}% (${passed}/${total})`);
  } else {
    console.log("Conformance metrics missing/invalid; skipping conformance section.");
  }
}

if (emit) {
  const jsRate = toNumber(emit.js_pass_rate);
  const jsPassed = toInt(emit.js_passed);
  const jsTotal = toInt(emit.js_total);
  const dtsRate = toNumber(emit.dts_pass_rate);
  const dtsPassed = toInt(emit.dts_passed);
  const dtsTotal = toInt(emit.dts_total);
  if (validSuiteMetrics(jsRate, jsPassed, jsTotal) && validSuiteMetrics(dtsRate, dtsPassed, dtsTotal)) {
    const block = [
      "```",
      `JavaScript:  [${progressBar(jsRate)}] ${formatRate(jsRate)}% (${formatInt(jsPassed)} / ${formatInt(jsTotal)} tests)`,
      `Declaration: [${progressBar(dtsRate)}] ${formatRate(dtsRate)}% (${formatInt(dtsPassed)} / ${formatInt(dtsTotal)} tests)`,
      "```",
    ].join("\n");
    const updated = replaceSection(readme, "<!-- EMIT_START -->", "<!-- EMIT_END -->", block);
    changed ||= updated !== readme;
    readme = updated;
    console.log(`Emit JS: ${formatRate(jsRate)}% (${jsPassed}/${jsTotal})`);
    console.log(`Emit DTS: ${formatRate(dtsRate)}% (${dtsPassed}/${dtsTotal})`);
  } else {
    console.log("Emit metrics missing/invalid; skipping emit section.");
  }
}

if (fourslash) {
  const rate = toNumber(fourslash.pass_rate);
  const passed = toInt(fourslash.passed);
  const total = toInt(fourslash.total);
  if (validSuiteMetrics(rate, passed, total)) {
    const block = [
      "```",
      `Progress: [${progressBar(rate)}] ${formatRate(rate)}% (${formatInt(passed)} / ${formatInt(total)} tests)`,
      "```",
    ].join("\n");
    const updated = replaceSection(readme, "<!-- FOURSLASH_START -->", "<!-- FOURSLASH_END -->", block);
    changed ||= updated !== readme;
    readme = updated;
    console.log(`Fourslash: ${formatRate(rate)}% (${passed}/${total})`);
  } else {
    console.log("Fourslash metrics missing/invalid; skipping fourslash section.");
  }
}

if (!changed) {
  console.log("README metrics already up to date.");
  process.exit(0);
}

fs.writeFileSync(readmePath, readme);
console.log("README.md updated from CI metrics.");
