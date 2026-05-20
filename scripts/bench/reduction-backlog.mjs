#!/usr/bin/env node
/**
 * Groups compatibility diagnostic deltas by subsystem and first diagnostic
 * code, then emits one reduction task per pattern with enough repro metadata
 * to create a focused checker/solver test without re-running the full project.
 *
 * Usage:
 *   node scripts/bench/reduction-backlog.mjs <compat.jsonl|summary.json>
 *       [--issues open-issues.json]
 *       [--output reduction-backlog.json]
 *       [--markdown reduction-backlog.md]
 *       [--min-rows N]
 *
 * Input formats:
 *   JSONL  One compatibility row per line (from `project-compatibility.mjs record`)
 *   JSON   Summary with `.rows[]` (from `project-compatibility.mjs summary`)
 *
 * Issues manifest (--issues):
 *   Pre-fetched JSON array of GitHub issues with shape:
 *     { number, title, labels: string[], state, html_url? }
 *   To generate:
 *     gh api repos/mohsen1/tsz/issues --paginate \
 *       | jq '[.[] | {number, title, labels: [.labels[].name], state, html_url}]' \
 *       > open-issues.json
 */
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  CRATE_BY_SUBSYSTEM,
  LABELS_BY_SUBSYSTEM,
  ownerTrackForSubsystem,
  subsystemForCode,
} from "../ci/diagnostic-subsystems.mjs";

const EXIT_CLASS_SUBSYSTEM = new Map([
  ["timeout", "runtime-timeout"],
  ["oom", "runtime-oom"],
  ["crash", "runtime-crash"],
  ["fixture invalid", "project-config"],
  ["runner error", "runner-error"],
  ["tsz unavailable", "runner-error"],
]);

export function readCompatibilityRows(file) {
  const content = fs.readFileSync(file, "utf8").trim();
  // Use a cheap heuristic to distinguish JSON from JSONL before attempting parse.
  if (content.startsWith("{") || content.startsWith("[")) {
    try {
      const parsed = JSON.parse(content);
      if (Array.isArray(parsed.rows)) return parsed.rows;
      if (Array.isArray(parsed)) return parsed;
      // Single-row JSON object (undocumented but tolerated as a convenience).
      if (parsed && typeof parsed === "object") return [parsed];
      throw new Error(`unexpected compatibility JSON shape: expected {rows:[]} or an array`);
    } catch {
      // fall through to JSONL
    }
  }
  return content.split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
}

function readOptionalJson(file) {
  if (!file) return null;
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

function primarySubsystemForRow(row) {
  const sub = row.primary_subsystem ?? row.diagnostic_subsystems?.[0]?.subsystem;
  if (sub) return sub;
  return EXIT_CLASS_SUBSYSTEM.get(row.exit_class) ?? "unclassified";
}

function occurrenceCountForRow(row, primaryCode) {
  if (!primaryCode) return 0;
  const sub = Array.isArray(row.diagnostic_subsystems)
    ? row.diagnostic_subsystems.find((g) => g.codes?.includes(primaryCode))
    : null;
  return sub?.count ?? 1;
}

export function groupByPattern(rows) {
  const groups = new Map();
  let nonGreenCount = 0;
  for (const row of rows) {
    if (row.state === "green") continue;
    nonGreenCount++;

    const subsystem = primarySubsystemForRow(row);
    const primaryCode = row.diagnostic_codes?.[0] ?? null;
    const key = `${subsystem}\0${primaryCode ?? ""}`;

    let group = groups.get(key);
    if (!group) {
      group = {
        subsystem,
        primary_code: primaryCode,
        all_codes: new Set(primaryCode ? [primaryCode] : []),
        rows: [],
        total_occurrences: 0,
      };
      groups.set(key, group);
    }

    group.rows.push(row);
    group.total_occurrences += occurrenceCountForRow(row, primaryCode);
    for (const code of row.diagnostic_codes ?? []) {
      group.all_codes.add(code);
    }
  }
  return { groups, nonGreenCount };
}

function bestReproRow(rows) {
  const score = (r) => {
    let s = 0;
    if (r.repro?.command) s += 4;
    if (r.repro?.first_failure_path) s += 2;
    if (r.repro?.tsconfig_path) s += 1;
    if (r.repro?.first_failure_code) s += 1;
    return s;
  };
  let best = rows[0];
  let bestScore = score(best);
  for (let i = 1; i < rows.length; i++) {
    const s = score(rows[i]);
    if (s >= bestScore) {
      best = rows[i];
      bestScore = s;
    }
  }
  return best;
}

function collectExamples(rows, limit) {
  const seen = new Set();
  const examples = [];
  for (const row of rows) {
    if (examples.length >= limit) break;
    for (const delta of row.diagnostic_deltas ?? []) {
      if (seen.has(delta)) continue;
      seen.add(delta);
      examples.push({ row: row.name, delta });
      if (examples.length >= limit) break;
    }
  }
  return examples;
}

function linkedIssues(subsystem, codes, issues) {
  if (issues.length === 0) return [];
  const codesArray = codes.map((c) => c.toLowerCase());
  const subsystemLower = subsystem.toLowerCase();
  const normalizedSub = subsystem.replace(/-/g, " ").toLowerCase();
  const subsystemWords = normalizedSub.split(/[/\s]+/).filter((w) => w.length > 3);

  return issues
    .filter((issue) => {
      if (issue.state && issue.state !== "open") return false;

      const labels = Array.isArray(issue.labels)
        ? issue.labels.map((l) => String(l).toLowerCase().replace(/-/g, " "))
        : [];
      if (labels.some((l) => l === normalizedSub || l === subsystemLower)) return true;

      const title = String(issue.title ?? "").toLowerCase();
      if (codesArray.some((code) => title.includes(code))) return true;
      if (subsystemWords.some((word) => title.includes(word))) return true;

      return false;
    })
    .slice(0, 5)
    .map((issue) => ({
      number: issue.number,
      title: issue.title,
      labels: Array.isArray(issue.labels) ? issue.labels : [],
      state: issue.state ?? "open",
      url: issue.html_url ?? `https://github.com/mohsen1/tsz/issues/${issue.number}`,
    }));
}

export function buildReductionTasks(groups, issues = [], { minRows = 1 } = {}) {
  const tasks = [];

  for (const group of groups.values()) {
    if (group.rows.length < minRows) continue;

    const codes = [...group.all_codes];
    const subsystem = group.subsystem;
    const ownerTrack = ownerTrackForSubsystem(subsystem);
    const ownerCrate = CRATE_BY_SUBSYSTEM.get(subsystem) ?? null;
    const baseLabels = LABELS_BY_SUBSYSTEM.get(subsystem) ?? [];
    const reproRow = bestReproRow(group.rows);
    const examples = collectExamples(group.rows, 5);

    const affectedRows = group.rows.map((row) => ({
      name: row.name,
      state: row.state ?? "unknown",
      oracle_classification: row.oracle_classification ?? "unknown",
      occurrence_count: occurrenceCountForRow(row, group.primary_code),
    }));

    const codeLabel = codes.length > 0 ? ` (${codes.slice(0, 3).join(", ")})` : "";
    const rowLabel = group.rows.length > 1 ? ` across ${group.rows.length} project rows` : "";
    const scope = ownerCrate ?? "solver";
    const suggestedTitle = `fix(${scope}): resolve ${subsystem}${codeLabel} divergence${rowLabel}`;

    tasks.push({
      subsystem,
      primary_code: group.primary_code,
      codes,
      owner_track: ownerTrack,
      owner_crate: ownerCrate,
      row_count: group.rows.length,
      total_occurrences: group.total_occurrences,
      affected_rows: affectedRows,
      examples,
      repro: {
        ...(reproRow.repro ?? {}),
        source_row: reproRow.name ?? null,
      },
      suggested_issue_title: suggestedTitle,
      suggested_labels: [...new Set(["bench", ...baseLabels])],
      linked_issues: linkedIssues(subsystem, codes, issues),
    });
  }

  tasks.sort((a, b) => {
    const rowDelta = b.row_count - a.row_count;
    if (rowDelta !== 0) return rowDelta;
    return b.total_occurrences - a.total_occurrences;
  });

  return tasks;
}

export function renderMarkdown(report) {
  const lines = [];
  lines.push("# Reduction Backlog");
  lines.push("");
  lines.push(`Generated: ${report.generated_at}`);
  lines.push(`Source: \`${report.source ?? "unknown"}\``);
  lines.push("");

  const { totals } = report;
  lines.push("## Summary");
  lines.push("");
  lines.push(`- Non-green rows: ${totals.non_green_rows}`);
  lines.push(`- Unique (subsystem, code) patterns: ${totals.unique_patterns}`);
  lines.push(`- Reduction tasks emitted: ${totals.reduction_tasks}`);
  lines.push("");

  if (report.reduction_tasks.length === 0) {
    lines.push("No reduction tasks. All rows are green.");
    return `${lines.join("\n")}\n`;
  }

  lines.push("## Reduction Tasks");
  lines.push("");

  for (const [i, task] of report.reduction_tasks.entries()) {
    const codeLabel = task.codes.length > 0 ? ` — ${task.codes.join(", ")}` : "";
    lines.push(`### ${i + 1}. ${task.subsystem}${codeLabel}`);
    lines.push("");
    lines.push(`**Owner track:** ${task.owner_track}`);
    if (task.owner_crate) lines.push(`**Owner crate:** \`${task.owner_crate}\``);
    lines.push(`**Affected rows:** ${task.row_count} (${task.total_occurrences} total occurrences)`);
    lines.push("");

    lines.push("| Row | State | Oracle | Occurrences |");
    lines.push("| --- | --- | --- | --- |");
    for (const row of task.affected_rows) {
      lines.push(`| ${row.name} | ${row.state} | ${row.oracle_classification} | ${row.occurrence_count} |`);
    }
    lines.push("");

    if (task.examples.length > 0) {
      lines.push("**Example deltas:**");
      lines.push("");
      lines.push("```");
      for (const ex of task.examples) {
        lines.push(`# ${ex.row}`);
        lines.push(ex.delta);
      }
      lines.push("```");
      lines.push("");
    }

    const repro = task.repro;
    if (repro?.command || repro?.first_failure_path) {
      lines.push("**Repro:**");
      lines.push("");
      if (repro.command) {
        lines.push("```sh");
        lines.push(repro.command);
        lines.push("```");
      }
      if (repro.first_failure_path) {
        const loc = repro.first_failure_line
          ? `${repro.first_failure_path}:${repro.first_failure_line}:${repro.first_failure_column ?? 1}`
          : repro.first_failure_path;
        lines.push(`First failure: \`${loc}\``);
        if (repro.first_failure_code) lines.push(`Code: \`${repro.first_failure_code}\``);
        if (repro.source_row) lines.push(`Source row: \`${repro.source_row}\``);
      }
      lines.push("");
    }

    lines.push(`**Suggested issue title:** ${task.suggested_issue_title}`);
    lines.push(`**Suggested labels:** ${task.suggested_labels.map((l) => `\`${l}\``).join(", ")}`);

    if (task.linked_issues.length > 0) {
      lines.push("");
      lines.push("**Linked open issues:**");
      for (const issue of task.linked_issues) {
        lines.push(`- [#${issue.number}: ${issue.title}](${issue.url})`);
      }
    }

    lines.push("");
    lines.push("---");
    lines.push("");
  }

  return lines.join("\n");
}

export function createReductionBacklog(inputPath, { issues = [], minRows = 1 } = {}) {
  const rows = readCompatibilityRows(inputPath);
  const { groups, nonGreenCount } = groupByPattern(rows);
  const tasks = buildReductionTasks(groups, issues, { minRows });

  return {
    generated_at: new Date().toISOString(),
    source: path.relative(process.cwd(), path.resolve(inputPath)),
    totals: {
      non_green_rows: nonGreenCount,
      unique_patterns: groups.size,
      reduction_tasks: tasks.length,
    },
    reduction_tasks: tasks,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  let inputPath = null;
  let issuesPath = null;
  let outputPath = null;
  let markdownPath = null;
  let minRows = 1;

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--issues":
        issuesPath = args[++i];
        break;
      case "--output":
        outputPath = args[++i];
        break;
      case "--markdown":
        markdownPath = args[++i];
        break;
      case "--min-rows":
        minRows = Number(args[++i]) || 1;
        break;
      default:
        if (!inputPath && !args[i].startsWith("--")) {
          inputPath = args[i];
        } else {
          process.stderr.write(`unknown argument: ${args[i]}\n`);
          process.exit(2);
        }
    }
  }

  if (!inputPath) {
    process.stderr.write(
      "usage: reduction-backlog.mjs <compat.jsonl|summary.json>" +
      " [--issues open-issues.json]" +
      " [--output reduction-backlog.json]" +
      " [--markdown reduction-backlog.md]" +
      " [--min-rows N]\n",
    );
    process.exit(2);
  }

  const issuesList = readOptionalJson(issuesPath) ?? [];
  const report = createReductionBacklog(inputPath, { issues: issuesList, minRows });
  const json = `${JSON.stringify(report, null, 2)}\n`;

  if (outputPath) {
    fs.mkdirSync(path.dirname(path.resolve(outputPath)), { recursive: true });
    fs.writeFileSync(outputPath, json, "utf8");
  } else {
    process.stdout.write(json);
  }

  if (markdownPath) {
    fs.mkdirSync(path.dirname(path.resolve(markdownPath)), { recursive: true });
    fs.writeFileSync(markdownPath, renderMarkdown(report), "utf8");
  }

  // Summary stats go to stderr when JSON is on stdout so callers can parse stdout cleanly.
  const summaryOut = outputPath ? process.stdout : process.stderr;
  summaryOut.write(
    [
      `non-green rows: ${report.totals.non_green_rows}`,
      `unique patterns: ${report.totals.unique_patterns}`,
      `reduction tasks: ${report.totals.reduction_tasks}`,
      outputPath ? `report: ${path.relative(process.cwd(), outputPath)}` : "",
      markdownPath ? `markdown: ${path.relative(process.cwd(), markdownPath)}` : "",
    ]
      .filter(Boolean)
      .join("\n") + "\n",
  );
}
