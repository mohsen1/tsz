import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const ROOT = path.resolve(import.meta.dirname, "..", "..", "..", "..");

function readIfExists(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch {
    return null;
  }
}

function readJsonIfExists(p) {
  const text = readIfExists(p);
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function fmt(n) {
  return Number(n).toLocaleString("en-US");
}

function toNumber(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function toInt(value) {
  const n = Number.parseInt(String(value ?? "").replaceAll(",", "").trim(), 10);
  return Number.isFinite(n) ? n : null;
}

function suiteSourceLabel(source) {
  switch (source) {
    case "ci":
      return "CI artifacts";
    case "readme":
      return "README fallback";
    default:
      return "Unavailable";
  }
}

function setSuiteUnavailable(metrics, key) {
  metrics[`${key}_rate`] = "N/A";
  metrics[`${key}_rate_label`] = "N/A";
  metrics[`${key}_bar_rate`] = "0";
  metrics[`${key}_passed`] = "N/A";
  metrics[`${key}_total`] = "N/A";
  metrics[`${key}_source`] = "missing";
  metrics[`${key}_source_label`] = suiteSourceLabel("missing");
}

function setSuiteMetrics(metrics, key, rate, passed, total, source) {
  const normalizedRate = Number(rate).toFixed(1);
  metrics[`${key}_rate`] = normalizedRate;
  metrics[`${key}_rate_label`] = `${normalizedRate}%`;
  metrics[`${key}_bar_rate`] = normalizedRate;
  metrics[`${key}_passed`] = fmt(passed);
  metrics[`${key}_total`] = fmt(total);
  metrics[`${key}_source`] = source;
  metrics[`${key}_source_label`] = suiteSourceLabel(source);
}

function setSuiteIfValid(metrics, key, rate, passed, total, source) {
  if (rate === null || passed === null || total === null || total <= 0) {
    return false;
  }
  setSuiteMetrics(metrics, key, rate, passed, total, source);
  return true;
}

function extractMetrics() {
  const metrics = {
    ts_version: "unknown",
    metrics_notice: "",
    metrics_source_summary: "",
    uses_fallback_metrics: false,
    has_missing_metrics: false,
  };
  setSuiteUnavailable(metrics, "conformance");
  setSuiteUnavailable(metrics, "emit_js");
  setSuiteUnavailable(metrics, "emit_dts");
  setSuiteUnavailable(metrics, "fourslash");

  const metricsDir = path.join(ROOT, ".ci-metrics");
  const conformance = readJsonIfExists(path.join(metricsDir, "conformance.json"));
  const emit = readJsonIfExists(path.join(metricsDir, "emit.json"));
  const fourslash = readJsonIfExists(path.join(metricsDir, "fourslash.json"));

  const readme = readIfExists(path.join(ROOT, "README.md"));
  if (readme) {
    const versionMatch = readme.match(/TypeScript.*?`([\d.]+[^`]*)`/);
    if (versionMatch) metrics.ts_version = versionMatch[1];
  }

  const hasCiConformance = conformance
    ? setSuiteIfValid(
        metrics,
        "conformance",
        toNumber(conformance.pass_rate),
        toInt(conformance.passed),
        toInt(conformance.total),
        "ci",
      )
    : false;
  const hasCiEmitJs = emit
    ? setSuiteIfValid(
        metrics,
        "emit_js",
        toNumber(emit.js_pass_rate),
        toInt(emit.js_passed),
        toInt(emit.js_total),
        "ci",
      )
    : false;
  const hasCiEmitDts = emit
    ? setSuiteIfValid(
        metrics,
        "emit_dts",
        toNumber(emit.dts_pass_rate),
        toInt(emit.dts_passed),
        toInt(emit.dts_total),
        "ci",
      )
    : false;
  const hasCiFourslash = fourslash
    ? setSuiteIfValid(
        metrics,
        "fourslash",
        toNumber(fourslash.pass_rate),
        toInt(fourslash.passed),
        toInt(fourslash.total),
        "ci",
      )
    : false;

  if (readme) {
    if (!hasCiConformance) {
      const confSection = readme.match(
        /<!-- CONFORMANCE_START -->([\s\S]*?)<!-- CONFORMANCE_END -->/,
      );
      if (confSection) {
        const m = confSection[1].match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
        if (m) {
          setSuiteIfValid(
            metrics,
            "conformance",
            toNumber(m[1]),
            toInt(m[2]),
            toInt(m[3]),
            "readme",
          );
        }
      }
    }

    if (!hasCiEmitJs || !hasCiEmitDts) {
      const emitSection = readme.match(/<!-- EMIT_START -->([\s\S]*?)<!-- EMIT_END -->/);
      if (emitSection) {
        const lines = emitSection[1].split("\n");
        for (const line of lines) {
          const m = line.match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
          if (!m) continue;
          if (!hasCiEmitJs && line.includes("JavaScript")) {
            setSuiteIfValid(
              metrics,
              "emit_js",
              toNumber(m[1]),
              toInt(m[2]),
              toInt(m[3]),
              "readme",
            );
          } else if (!hasCiEmitDts && line.includes("Declaration")) {
            setSuiteIfValid(
              metrics,
              "emit_dts",
              toNumber(m[1]),
              toInt(m[2]),
              toInt(m[3]),
              "readme",
            );
          }
        }
      }
    }

    if (!hasCiFourslash) {
      const fsSection = readme.match(
        /<!-- FOURSLASH_START -->([\s\S]*?)<!-- FOURSLASH_END -->/,
      );
      if (fsSection) {
        const m = fsSection[1].match(/([\d.]+)%\s*\(([\d,]+)\s*\/\s*([\d,]+)/);
        if (m) {
          setSuiteIfValid(
            metrics,
            "fourslash",
            toNumber(m[1]),
            toInt(m[2]),
            toInt(m[3]),
            "readme",
          );
        }
      }
    }
  }

  const sources = [
    ["Conformance", metrics.conformance_source_label],
    ["JS Emit", metrics.emit_js_source_label],
    ["Declaration Emit", metrics.emit_dts_source_label],
    ["Language Service", metrics.fourslash_source_label],
  ];
  metrics.metrics_source_summary = sources.map(([k, v]) => `${k}: ${v}`).join(" | ");
  metrics.uses_fallback_metrics = [
    metrics.conformance_source,
    metrics.emit_js_source,
    metrics.emit_dts_source,
    metrics.fourslash_source,
  ].includes("readme");
  metrics.has_missing_metrics = [
    metrics.conformance_source,
    metrics.emit_js_source,
    metrics.emit_dts_source,
    metrics.fourslash_source,
  ].includes("missing");
  if (metrics.uses_fallback_metrics && metrics.has_missing_metrics) {
    metrics.metrics_notice =
      "Some progress metrics are sourced from README fallback and others are unavailable. README-derived values may be stale.";
  } else if (metrics.uses_fallback_metrics) {
    metrics.metrics_notice =
      "Some progress metrics are sourced from README fallback because CI artifacts were unavailable. README-derived values may be stale.";
  } else if (metrics.has_missing_metrics) {
    metrics.metrics_notice =
      "Some progress metrics are currently unavailable because CI artifacts are missing or invalid.";
  }

  try {
    const output = execSync(
      "find crates -path '*/target/*' -prune -o -path '*/src/*' -type f -name '*.rs' -print | xargs wc -l",
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
