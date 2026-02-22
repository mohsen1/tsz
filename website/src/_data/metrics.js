import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..");

function readIfExists(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch {
    return null;
  }
}

function readJsonIfExists(p) {
  const text = readIfExists(p);
  return text ? JSON.parse(text) : null;
}

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

function extractMetrics() {
  const metrics = {
    conformance_rate: "0",
    conformance_passed: "0",
    conformance_total: "0",
    emit_js_rate: "0",
    emit_js_passed: "0",
    emit_js_total: "0",
    emit_dts_rate: "0",
    emit_dts_passed: "0",
    emit_dts_total: "0",
    fourslash_rate: "0",
    fourslash_passed: "0",
    fourslash_total: "0",
    ts_version: "unknown",
  };

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

  const readme = readIfExists(path.join(ROOT, "README.md"));
  if (readme) {
    const versionMatch = readme.match(/TypeScript.*?`([\d.]+[^`]*)`/);
    if (versionMatch) metrics.ts_version = versionMatch[1];

    if (!conformance) {
      const confSection = readme.match(/<!-- CONFORMANCE_START -->([\s\S]*?)<!-- CONFORMANCE_END -->/);
      if (confSection) {
        const m = confSection[1].match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
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
          if (!m) continue;
          if (line.includes("JavaScript")) {
            metrics.emit_js_rate = m[1];
            metrics.emit_js_passed = m[2];
            metrics.emit_js_total = m[3];
          } else if (line.includes("Declaration")) {
            metrics.emit_dts_rate = m[1];
            metrics.emit_dts_passed = m[2];
            metrics.emit_dts_total = m[3];
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
          metrics.fourslash_passed = m[2];
          metrics.fourslash_total = m[3];
        }
      }
    }
  }

  try {
    const output = execSync(
      "find crates/ src/ -name '*.rs' -not -path '*/target/*' | xargs wc -l",
      { cwd: ROOT, encoding: "utf8", maxBuffer: 10 * 1024 * 1024 },
    );
    const lines = output.trim().split("\n");
    const totalLine = lines[lines.length - 1];
    const totalMatch = totalLine.match(/^\s*(\d+)\s+total/);
    metrics.total_loc = totalMatch ? fmt(Number(totalMatch[1])) : "N/A";

    const cratesDir = path.join(ROOT, "crates");
    const crateCount = fs.readdirSync(cratesDir, { withFileTypes: true }).filter((d) => d.isDirectory()).length;
    metrics.num_crates = String(crateCount);
  } catch {
    metrics.total_loc = "N/A";
    metrics.num_crates = "N/A";
  }

  return metrics;
}

export default extractMetrics();
