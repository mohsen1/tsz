#!/usr/bin/env node
/**
 * Fidelity audit + ratchet for the benchmark project-fixture module stubs.
 *
 * The corpus fixtures do not install real dependency packages; instead each
 * `tsz_write_*` fixture writer emits hand-authored `declare module` shims whose
 * members are overwhelmingly typed `any` (see #16311). That model keeps the
 * fixtures network-free and lets pinned `tsc` reach exit 0, but it erases the
 * dependency types the corpus is supposed to exercise, and — until now — the
 * erosion was unmeasured: nothing stopped a writer from being made *more* `any`,
 * and nothing recorded how much surface each row stubs away.
 *
 * This audit statically scans the stub `.d.ts` heredocs across the three
 * fixture-writer shards and reports, per writer:
 *   - `declareModules`: number of `declare module` shims,
 *   - `anyTokens`:      number of `any` type tokens (the erosion surface),
 *   - `exports`:        number of exported declarations (stubbed breadth).
 *
 * A stub heredoc is any `cat > "<path>" <<'DELIM' … DELIM` block whose *body* is
 * a TypeScript declaration file rather than a JSON tsconfig. Classifying by body
 * (a tsconfig starts with `{`; a stub starts with `declare`/`export`/`import`/
 * `interface`) — rather than by the redirect target's spelling — is what keeps
 * stubs written to a `"$output"` variable (e.g. `tsz_write_nextjs_bench_globals`)
 * in scope. Each heredoc is counted once, at the function that textually defines
 * it, so a config writer that *calls* a stub helper never double-counts the
 * helper's output.
 *
 * Parsing the shell source is deliberate over running the writers (the technique
 * `test-project-fixture-deprecations.mjs` uses for tsconfigs): the writer graph
 * has config writers that delegate to shared `*_stubs` helpers, so *running*
 * every writer would count the same generated `.d.ts` under both the helper and
 * its caller. The static scan attributes each heredoc to its single defining
 * function. It is network-free, needs no repo checkout, and runs instantly.
 *
 * A committed baseline (`project-fixture-stub-fidelity-baseline.json`) pins the
 * current per-writer counts. The `--check` mode (run by `full-ci.sh`) fails when
 * any existing writer's `anyTokens`/`declareModules` grow past baseline, or when
 * a new stub writer lands without a baseline entry — turning the erosion into a
 * tracked, monotonically-improving quantity. Converting a stub member from `any`
 * to a real type lowers `anyTokens`; re-pin with `--update-baseline`.
 *
 * Usage:
 *   node scripts/bench/project-fixture-stub-fidelity.mjs            # human summary
 *   node scripts/bench/project-fixture-stub-fidelity.mjs --markdown # ranked report
 *   node scripts/bench/project-fixture-stub-fidelity.mjs --json     # machine output
 *   node scripts/bench/project-fixture-stub-fidelity.mjs --check    # ratchet vs baseline (exit 1 on regression)
 *   node scripts/bench/project-fixture-stub-fidelity.mjs --update-baseline
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");

/** Shell shards that contain `tsz_write_*` fixture writers with `.d.ts` stubs. */
const STUB_SOURCE_FILES = [
  "scripts/bench/project-fixtures.sh",
  "scripts/bench/lib/project-fixture-stubs.sh",
  "scripts/bench/lib/project-fixture-stubs-canary.sh",
];

export const BASELINE_PATH = path.join(
  SCRIPT_DIR,
  "project-fixture-stub-fidelity-baseline.json",
);

/**
 * A heredoc body is a stub `.d.ts` (as opposed to a JSON tsconfig) when its
 * first meaningful line — ignoring blank lines and comments — is TypeScript
 * declaration syntax rather than the `{` that opens every tsconfig.
 */
export function isStubBody(body) {
  for (const raw of body.split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("//") || line.startsWith("/*") || line.startsWith("*")) {
      continue;
    }
    return !line.startsWith("{");
  }
  return false;
}

/**
 * Extract, from one shell shard's text, every stub `.d.ts` heredoc body
 * attributed to the `tsz_write_*` function that encloses it.
 *
 * A heredoc looks like `cat > "<target>" <<'DELIM'` … `DELIM`. The redirect
 * target is not inspected (writers spell it `"$output"`, `"$fixture_dir/x.d.ts"`,
 * …); classification is by body via `isStubBody`, so JSON tsconfig heredocs are
 * dropped and TS stub heredocs are kept regardless of target. The delimiter is
 * read from the `<<'DELIM'` marker so the scan is agnostic to the token used.
 *
 * Returns an array of `{ writer, module, body }` records, one per stub heredoc.
 */
export function extractStubHeredocs(text, sourceLabel) {
  const lines = text.split("\n");
  const records = [];
  let currentWriter = null;

  const writerRe = /^([A-Za-z_][A-Za-z0-9_]*)\s*\(\)\s*\{?\s*$/;
  const heredocStartRe =
    /cat\s+>{1,2}\s*"([^"]*)"\s*<<-?\s*['"]?([A-Za-z_][A-Za-z0-9_]*)['"]?\s*$/;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    const writerMatch = line.match(writerRe);
    if (writerMatch && writerMatch[1].startsWith("tsz_write_")) {
      currentWriter = writerMatch[1];
      continue;
    }

    const heredocMatch = line.match(heredocStartRe);
    if (!heredocMatch) continue;

    const target = heredocMatch[1];
    const delimiter = heredocMatch[2];
    const bodyLines = [];
    let closed = false;
    for (let j = i + 1; j < lines.length; j++) {
      // A heredoc terminator is the delimiter alone on its line (bash also
      // allows leading tabs with `<<-`; trim tabs only, matching that rule).
      if (lines[j].replace(/^\t+/, "") === delimiter) {
        i = j;
        closed = true;
        break;
      }
      bodyLines.push(lines[j]);
    }
    if (!closed) {
      throw new Error(
        `${sourceLabel}: unterminated heredoc '${delimiter}' for ${target} ` +
          `(started at line ${i + 1})`,
      );
    }

    const body = bodyLines.join("\n");
    if (!isStubBody(body)) continue; // JSON tsconfig heredoc, not a stub.

    records.push({
      writer: currentWriter ?? `<file:${path.basename(sourceLabel)}>`,
      module: path.basename(target),
      body,
    });
  }

  return records;
}

/** Count the fidelity metrics of a single `.d.ts` stub body. */
export function measureStubBody(body) {
  // Strip line and block comments so prose ("… every other member `any`")
  // never inflates the erosion count; only real declarations are measured.
  // This is a coarse, string-unaware strip (a `/*` or `//` inside a string
  // literal would be treated as a comment); that is acceptable because the
  // result is a *ratchet* baseline guarded by the byte-stability test, not an
  // exact member census — a self-consistent slight under/over-count is fine.
  const code = body
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");

  const declareModules = (code.match(/\bdeclare\s+module\b/g) || []).length;
  const anyTokens = (code.match(/\bany\b/g) || []).length;
  const exports = (code.match(/\bexport\b/g) || []).length;

  return { declareModules, anyTokens, exports };
}

/**
 * Audit stub source files and aggregate metrics per writer.
 *
 * @param {string} [root] repository root (defaults to the checkout this script
 *   lives in); injectable so the unit test can point at a temp tree.
 * @param {string[]} [sources] repo-relative shard paths to scan; defaults to the
 *   committed set. A configured shard that is absent is a hard error — silently
 *   skipping it would drop every writer it defines from the ratchet while
 *   `--check` still passed (they would look like removals, not regressions).
 * @returns {{ writers: Record<string, object>, totals: object }}
 */
export function auditStubFidelity(root = ROOT, sources = STUB_SOURCE_FILES) {
  const writers = {};

  for (const rel of sources) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) {
      throw new Error(
        `stub source shard not found: ${rel}. If it was renamed or removed, ` +
          `update STUB_SOURCE_FILES — do not let its writers vanish from the ratchet.`,
      );
    }
    const heredocs = extractStubHeredocs(fs.readFileSync(abs, "utf8"), rel);

    for (const rec of heredocs) {
      const m = measureStubBody(rec.body);
      const entry = (writers[rec.writer] ??= {
        declareModules: 0,
        anyTokens: 0,
        exports: 0,
        modules: [],
      });
      entry.declareModules += m.declareModules;
      entry.anyTokens += m.anyTokens;
      entry.exports += m.exports;
      if (!entry.modules.includes(rec.module)) entry.modules.push(rec.module);
    }
  }

  const totals = { declareModules: 0, anyTokens: 0, exports: 0, writers: 0 };
  for (const entry of Object.values(writers)) {
    entry.modules.sort();
    totals.declareModules += entry.declareModules;
    totals.anyTokens += entry.anyTokens;
    totals.exports += entry.exports;
    totals.writers += 1;
  }

  return { writers, totals };
}

/** Writers ranked by `any` erosion (worst first) — the campaign's work-list. */
function rankedWriters(audit) {
  return Object.entries(audit.writers)
    .map(([name, e]) => ({ name, ...e }))
    .sort((a, b) => b.anyTokens - a.anyTokens || a.name.localeCompare(b.name));
}

/** Reduce the audit to the stable shape stored in the baseline. */
export function toBaseline(audit) {
  const writers = {};
  for (const [name, entry] of Object.entries(audit.writers)) {
    writers[name] = {
      declareModules: entry.declareModules,
      anyTokens: entry.anyTokens,
      exports: entry.exports,
    };
  }
  return {
    // A short contract note so a future reader understands the ratchet intent
    // without opening this script.
    _note:
      "Per-writer fidelity ratchet for #16311. anyTokens/declareModules must " +
      "not grow past these values; lower them by replacing `any` stub members " +
      "with real types, then re-pin with " +
      "`node scripts/bench/project-fixture-stub-fidelity.mjs --update-baseline`.",
    totals: { ...audit.totals },
    writers,
  };
}

/**
 * Compare a fresh audit against the pinned baseline.
 *
 * @returns {{ ok: boolean, regressions: string[], improvements: string[], missing: string[] }}
 *   `regressions` = a tracked metric grew (or a new unrecorded writer appeared);
 *   `improvements` = a metric shrank (re-pin welcome, not a failure);
 *   `missing` = a baseline writer no longer exists (also an improvement/removal).
 */
export function checkAgainstBaseline(audit, baseline) {
  const regressions = [];
  const improvements = [];
  const missing = [];

  for (const [name, cur] of Object.entries(audit.writers)) {
    const base = baseline.writers[name];
    if (!base) {
      regressions.push(
        `new stub writer '${name}' (anyTokens=${cur.anyTokens}, ` +
          `declareModules=${cur.declareModules}) is not in the fidelity ` +
          `baseline — acknowledge its erosion with --update-baseline`,
      );
      continue;
    }
    for (const metric of ["anyTokens", "declareModules"]) {
      if (cur[metric] > base[metric]) {
        regressions.push(
          `${name}.${metric} grew ${base[metric]} -> ${cur[metric]} ` +
            `(fixture stubs must not become more \`any\`-eroded)`,
        );
      } else if (cur[metric] < base[metric]) {
        improvements.push(
          `${name}.${metric} improved ${base[metric]} -> ${cur[metric]}`,
        );
      }
    }
  }

  for (const name of Object.keys(baseline.writers)) {
    if (!audit.writers[name]) missing.push(name);
  }

  return { ok: regressions.length === 0, regressions, improvements, missing };
}

export function loadBaseline(baselinePath = BASELINE_PATH) {
  return JSON.parse(fs.readFileSync(baselinePath, "utf8"));
}

function formatMarkdown(audit) {
  const rows = rankedWriters(audit);
  const lines = [];
  lines.push("# Project-fixture stub fidelity (#16311)");
  lines.push("");
  lines.push(
    `Total: **${audit.totals.anyTokens} \`any\` tokens**, ` +
      `**${audit.totals.declareModules} \`declare module\` shims**, ` +
      `**${audit.totals.exports} exports** across ` +
      `**${audit.totals.writers} stub writers**.`,
  );
  lines.push("");
  lines.push("Ranked by `any` erosion (worst first) — the campaign's work-list:");
  lines.push("");
  lines.push("| writer | any | shims | exports | modules |");
  lines.push("| --- | ---: | ---: | ---: | --- |");
  for (const r of rows) {
    lines.push(
      `| \`${r.name}\` | ${r.anyTokens} | ${r.declareModules} | ` +
        `${r.exports} | ${r.modules.join(", ")} |`,
    );
  }
  lines.push("");
  return lines.join("\n");
}

function formatSummary(audit) {
  const rows = rankedWriters(audit);
  const lines = [];
  lines.push(
    `stub fidelity: ${audit.totals.anyTokens} any tokens, ` +
      `${audit.totals.declareModules} declare-module shims, ` +
      `${audit.totals.exports} exports, ${audit.totals.writers} writers`,
  );
  const worst = rows.slice(0, 5);
  for (const r of worst) {
    lines.push(
      `  ${r.name}: any=${r.anyTokens} shims=${r.declareModules} ` +
        `exports=${r.exports}`,
    );
  }
  if (rows.length > worst.length) {
    lines.push(`  … and ${rows.length - worst.length} more writers`);
  }
  return lines.join("\n");
}

function main(argv) {
  const args = new Set(argv.slice(2));
  const audit = auditStubFidelity();

  if (args.has("--update-baseline")) {
    fs.writeFileSync(
      BASELINE_PATH,
      JSON.stringify(toBaseline(audit), null, 2) + "\n",
    );
    console.log(
      `wrote baseline: ${audit.totals.anyTokens} any tokens, ` +
        `${audit.totals.declareModules} shims across ${audit.totals.writers} writers`,
    );
    return 0;
  }

  if (args.has("--json")) {
    console.log(JSON.stringify(audit, null, 2));
    return 0;
  }

  if (args.has("--markdown")) {
    console.log(formatMarkdown(audit));
    return 0;
  }

  if (args.has("--check")) {
    if (!fs.existsSync(BASELINE_PATH)) {
      console.error(
        `missing baseline ${path.relative(ROOT, BASELINE_PATH)}; run --update-baseline`,
      );
      return 1;
    }
    const result = checkAgainstBaseline(audit, loadBaseline());
    for (const msg of result.improvements) console.log(`improvement: ${msg}`);
    for (const name of result.missing) {
      console.log(`removed writer (re-pin welcome): ${name}`);
    }
    if (!result.ok) {
      for (const msg of result.regressions) console.error(`regression: ${msg}`);
      console.error(
        `stub fidelity ratchet failed: ${result.regressions.length} regression(s)`,
      );
      return 1;
    }
    console.log(
      `stub fidelity ratchet ok (${audit.totals.anyTokens} any tokens, ` +
        `${audit.totals.declareModules} shims)`,
    );
    return 0;
  }

  console.log(formatSummary(audit));
  return 0;
}

// Run only when invoked directly, not when imported by the test.
if (import.meta.url === `file://${process.argv[1]}`) {
  process.exit(main(process.argv));
}
