#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

function usage() {
  console.error(
    "usage: run-attribution-plan.mjs <tsgo-winners.json> <artifact-prefix> <manifest.json>",
  );
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function toPortablePath(file) {
  return file.split(path.sep).join("/");
}

function unresolvedPlaceholder(command) {
  const placeholders = [...String(command).matchAll(/<([^>]+)>/g)].map((match) => match[1]);
  return placeholders.find((name) => name !== "artifact") ?? null;
}

function expectedPerfPath(command) {
  const match = String(command).match(/--perf-counters-json\s+("[^"]+"|'[^']+'|\S+)/);
  if (!match) return null;
  return match[1].replace(/^["']|["']$/g, "");
}

function main() {
  const [reportPath, artifactPrefix, manifestPath] = process.argv.slice(2);
  if (!reportPath || !artifactPrefix || !manifestPath) {
    usage();
    process.exit(2);
  }

  const report = readJson(reportPath);
  const rows = Array.isArray(report?.two_x_target?.missing_attribution_plan)
    ? report.two_x_target.missing_attribution_plan
    : [];
  const manifest = {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    source_report: toPortablePath(reportPath),
    artifact_prefix: artifactPrefix,
    rows: [],
  };

  for (const row of rows) {
    const commandTemplate = row?.attribution_command;
    if (typeof commandTemplate !== "string" || commandTemplate.trim() === "") {
      manifest.rows.push({
        name: row?.name ?? null,
        status: "skipped",
        reason: "missing attribution_command",
      });
      continue;
    }

    const unresolved = unresolvedPlaceholder(commandTemplate);
    if (unresolved) {
      manifest.rows.push({
        name: row?.name ?? null,
        status: "skipped",
        reason: `unresolved placeholder <${unresolved}>`,
        command_template: commandTemplate,
      });
      continue;
    }

    const command = commandTemplate.replaceAll("<artifact>", artifactPrefix);
    const perfPath = expectedPerfPath(command);
    const result = spawnSync(command, {
      shell: true,
      stdio: "inherit",
      env: { ...process.env, TSZ_PERF_COUNTERS: process.env.TSZ_PERF_COUNTERS ?? "1" },
    });
    const status = result.status === 0 && perfPath && fs.existsSync(perfPath)
      ? "success"
      : "failed";
    manifest.rows.push({
      name: row?.name ?? null,
      status,
      exit_code: result.status,
      signal: result.signal,
      perf_path: perfPath ? toPortablePath(perfPath) : null,
      command,
    });
  }

  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

  const counts = manifest.rows.reduce((acc, row) => {
    acc[row.status] = (acc[row.status] ?? 0) + 1;
    return acc;
  }, {});
  console.log(
    `attribution plan: success=${counts.success ?? 0} failed=${counts.failed ?? 0} skipped=${counts.skipped ?? 0}`,
  );
}

main();
