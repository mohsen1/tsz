#!/usr/bin/env node
// Static guard on `.config/nextest.toml` slow-timeout / heavy-test overrides
// (#17675, part 3). Runs in the per-merge fast lane
// (scripts/ci/check-unit-gate-contracts.sh) — where authors actually look — and
// fails loudly on two ways an override silently stops protecting its test:
//
//   1. Orphaned filter: a literal `test(NAME)` predicate that matches no test
//      in the tree. When a heavy test is renamed/moved/deleted its filter goes
//      dead, and the next time it (under its new name) drifts over the base
//      budget the nightly unit lane goes red for a non-correctness reason —
//      the recurring failure mode of this issue and of #17203.
//   2. Dropped block: an `[[profile.X.overrides]]` header whose `filter = '...'`
//      did not parse (e.g. a formatter wrapped the long line across lines), so
//      parseOverrides drops the whole block and its protection vanishes from
//      both this guard and the gate's budget view. The header count must equal
//      the parsed-record count.
//
// A "literal" is nextest's default substring predicate (`test(foo)`, or exact
// `test(=foo)`); regex predicates (`test(/re/)`) are not name-checkable this way
// and are skipped. A filter is satisfied when its substring appears anywhere in
// the Rust sources — lenient on purpose: the job is to catch "this filter
// matches nothing", not to re-derive nextest's matcher, and a bare-identifier
// substring avoids false orphans for macro-minted tests with no literal `fn NAME`.
//
// Exit codes: 0 clean; 1 one or more orphaned/dropped; 2 config error.
import fs from "node:fs";
import { parseOverrides, collectLiteralFilters, countOverrideHeaders } from "./nextest-overrides.mjs";

const CONFIG = ".config/nextest.toml";
const SEARCH_ROOTS = ["crates"];

function collectRustFiles(dir, out) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const full = `${dir}/${entry.name}`;
    if (entry.isDirectory()) {
      if (entry.name === "target" || entry.name === "node_modules") continue;
      collectRustFiles(full, out);
    } else if (entry.name.endsWith(".rs")) {
      out.push(full);
    }
  }
}

// Which of `literals` appear nowhere in the Rust sources? One pass over the
// tree: read each file once and drop every literal it contains, so the search
// cost is O(sources), not O(sources × literals). No ripgrep dependency — this
// runs in the fast lane and must not hard-depend on an external binary.
export function findOrphanedLiterals(literals) {
  const pending = new Set(literals);
  if (pending.size === 0) return [];
  const files = [];
  for (const root of SEARCH_ROOTS) collectRustFiles(root, files);
  for (const file of files) {
    if (pending.size === 0) break;
    let text;
    try {
      text = fs.readFileSync(file, "utf8");
    } catch {
      continue;
    }
    for (const literal of pending) {
      if (text.includes(literal)) pending.delete(literal);
    }
  }
  return [...pending].sort();
}

function main() {
  let tomlText;
  try {
    tomlText = fs.readFileSync(CONFIG, "utf8");
  } catch (err) {
    process.stderr.write(`check-nextest-overrides: cannot read ${CONFIG}: ${err.message}\n`);
    process.exit(2);
  }

  const overrides = parseOverrides(tomlText);
  const headerCount = countOverrideHeaders(tomlText);
  const literals = collectLiteralFilters(overrides);
  const orphanSet = new Set(findOrphanedLiterals(literals.map((e) => e.literal)));
  const orphaned = literals.filter((e) => orphanSet.has(e.literal));

  let ok = true;
  if (overrides.length !== headerCount) {
    ok = false;
    process.stderr.write(
      `::error title=nextest override block dropped::${CONFIG} has ${headerCount} ` +
        `[[profile.*.overrides]] block(s) but only ${overrides.length} parsed a filter. ` +
        `A block's \`filter = '...'\` likely got wrapped across lines by a formatter — ` +
        `keep each filter on one line so its override keeps applying.\n`,
    );
  }
  for (const entry of orphaned) {
    ok = false;
    process.stderr.write(
      `::error title=Orphaned nextest override filter::` +
        `test(${entry.literal}) [profile ${entry.profiles.join(", ")}] in ${CONFIG} ` +
        `matches no test under ${SEARCH_ROOTS.join(", ")}. ` +
        `The heavy test it protected was likely renamed or removed — update the filter to the new name, ` +
        `or delete the dead override entry.\n`,
    );
  }

  if (!ok) process.exit(1);
  process.stdout.write(
    `check-nextest-overrides: ${literals.length} literal filter(s) across ${headerCount} ` +
      `override block(s) all resolve to a test under ${SEARCH_ROOTS.join(", ")}.\n`,
  );
  process.exit(0);
}

const invokedDirectly =
  process.argv[1] && fs.realpathSync(process.argv[1]) === fs.realpathSync(new URL(import.meta.url).pathname);
if (invokedDirectly) main();
