// Static inventory of the ambient `declare module` shims and `any`-typed
// members the no-install external/canary fixtures depend on (#16311 ask 4).
//
// Fixture setup never runs `npm install`; instead `project-fixtures.sh` hand-
// writes ambient module stubs so `tsc`/`tsz` don't see a `TS2307` wall for a
// project's real dependencies. A row measured against a stub loses coverage
// at exactly its dependency boundaries, so a green row with zero stubs and a
// green row whose dependency graph is erased to `any` are different claims.
// This module counts that per-project, from source, with no build step:
// each `run_project_row` arm is followed through the shell helper call graph
// into config, wrapper, and declaration writers. This includes guard-local
// wrappers such as `write_kysely_config`, whose typed ambient declarations are
// still a nonzero owner even when they contain no `any` tokens.
import fs from "node:fs";
import crypto from "node:crypto";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { extractStubHeredocs } from "../project-fixture-stub-fidelity.mjs";

export const FIXTURE_STUB_EVIDENCE_SCHEMA = 2;

const TOP_LEVEL_FN_RE = /^([a-zA-Z_][a-zA-Z0-9_]*)\(\)\s*\{/;
const DECLARE_MODULE_RE = /\bdeclare module\b/g;
const ANY_MEMBER_RE = /[:=]\s*any\b/g;

// Splits a script's top-level function definitions (`name() {` at column 0)
// into `{ name, body }` segments running to the next top-level definition or
// EOF. Good enough for this repo's stub-writer style (heredocs and nested
// blocks never re-open a column-0 function definition); not a general bash
// parser.
function splitTopLevelFunctions(source, headerRe) {
  const lines = source.split("\n");
  const starts = [];
  for (let i = 0; i < lines.length; i += 1) {
    const match = headerRe.exec(lines[i]);
    headerRe.lastIndex = 0;
    if (match) starts.push({ name: match[1], line: i });
  }
  return starts.map(({ name, line }, index) => {
    const end = index + 1 < starts.length ? starts[index + 1].line : lines.length;
    return { name, body: lines.slice(line, end).join("\n") };
  });
}

function countMatches(text, re) {
  const matches = text.match(re);
  return matches ? matches.length : 0;
}

function readIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch {
    return null;
  }
}

function readRequired(filePath, label) {
  const source = readIfExists(filePath);
  if (source === null) {
    throw new Error(`fixture stub inventory source missing: ${label}`);
  }
  return source;
}

function referencedFunctions(body, knownNames) {
  const called = new Set();
  for (const match of body.matchAll(/\b[A-Za-z_][A-Za-z0-9_]*\b/g)) {
    if (knownNames.has(match[0])) called.add(match[0]);
  }
  return [...called];
}

function projectRowCaseArms(runProjectRowBody) {
  const rows = new Map();
  const lines = runProjectRowBody.split("\n");
  let names = null;
  let body = [];
  for (const line of lines) {
    if (names === null) {
      const match = line.match(/^\s+([a-z0-9][a-z0-9-]*(?:\|[a-z0-9][a-z0-9-]*)*)\)$/);
      if (match) {
        names = match[1].split("|").filter((name) => name.endsWith("-project") || name.endsWith("-app"));
        body = [];
      }
      continue;
    }
    if (/^\s*;;\s*$/.test(line)) {
      for (const name of names) rows.set(name, body.join("\n"));
      names = null;
      body = [];
      continue;
    }
    body.push(line);
  }
  if (names !== null) throw new Error("fixture stub inventory found unterminated run_project_row arm");
  if (rows.size === 0) throw new Error("fixture stub inventory parsed zero project row arms");
  return rows;
}

// Returns `{ [projectRowName]: { stubbedModules, stubbedAnyMembers } }` for
// every project row whose fixture config calls a stub writer. Rows absent
// from the result install real dependencies (or need no stubs at all).
export function computeFixtureStubInventory(root) {
  const fixturesPath = path.join(root, "scripts/bench/project-fixtures.sh");
  const stubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs.sh");
  const canaryStubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs-canary.sh");
  const guardPath = path.join(root, "scripts/ci/project-compile-guard.sh");

  const sources = [
    [fixturesPath, "scripts/bench/project-fixtures.sh"],
    [stubsPath, "scripts/bench/lib/project-fixture-stubs.sh"],
    [canaryStubsPath, "scripts/bench/lib/project-fixture-stubs-canary.sh"],
    [guardPath, "scripts/ci/project-compile-guard.sh"],
  ];
  const functions = new Map();
  for (const [filePath, label] of sources) {
    const source = readRequired(filePath, label);
    for (const fn of splitTopLevelFunctions(source, TOP_LEVEL_FN_RE)) {
      if (functions.has(fn.name)) throw new Error(`duplicate fixture helper function: ${fn.name}`);
      functions.set(fn.name, { ...fn, label });
    }
  }
  const knownNames = new Set(functions.keys());
  const declarationWriters = new Map();
  const calls = new Map();
  for (const [name, fn] of functions) {
    calls.set(name, referencedFunctions(fn.body, knownNames).filter((called) => called !== name));
    const heredocs = extractStubHeredocs(fn.body, `${fn.label}:${name}`);
    if (heredocs.length === 0) continue;
    declarationWriters.set(name, {
      stubbedModules: heredocs.reduce((sum, heredoc) => sum + countMatches(heredoc.body, DECLARE_MODULE_RE), 0),
      stubbedAnyMembers: heredocs.reduce((sum, heredoc) => sum + countMatches(heredoc.body, ANY_MEMBER_RE), 0),
    });
  }

  const inventory = {};
  const runProjectRow = functions.get("run_project_row");
  if (!runProjectRow) throw new Error("fixture stub inventory could not find run_project_row");
  for (const [projectName, armBody] of projectRowCaseArms(runProjectRow.body)) {
    let stubbedModules = 0;
    let stubbedAnyMembers = 0;
    const writers = [];
    const pending = referencedFunctions(armBody, knownNames);
    const visited = new Set();
    while (pending.length > 0) {
      const owner = pending.pop();
      if (visited.has(owner)) continue;
      visited.add(owner);
      const declaration = declarationWriters.get(owner);
      if (declaration) {
        stubbedModules += declaration.stubbedModules;
        stubbedAnyMembers += declaration.stubbedAnyMembers;
        writers.push(owner);
      }
      for (const called of calls.get(owner) || []) pending.push(called);
    }
    if (writers.length === 0) continue;
    inventory[projectName] = {
      stubbedModules,
      stubbedAnyMembers,
      writers: [...new Set(writers)].sort(),
    };
  }
  if (Object.keys(inventory).length === 0) {
    throw new Error("fixture stub inventory parsed zero project rows");
  }
  return inventory;
}

export function fixtureStubEvidenceFingerprint(stubbedModules, stubbedAnyMembers, writers) {
  const payload = JSON.stringify({
    schema_version: FIXTURE_STUB_EVIDENCE_SCHEMA,
    stubbed_modules: stubbedModules,
    stubbed_any_members: stubbedAnyMembers,
    writers,
  });
  return crypto.createHash("sha256").update(payload).digest("hex");
}

// Convert the source-derived inventory entry for one row into the persisted
// evidence shape. Absence from a successfully computed inventory is an exact
// zero-stub result; failure to compute the inventory itself throws above.
export function fixtureStubEvidenceFromInventory(inventory, projectName) {
  if (typeof projectName !== "string" || projectName.length === 0) {
    throw new Error("project row name is required for fixture stub evidence");
  }
  const counts = inventory[projectName] || {
    stubbedModules: 0,
    stubbedAnyMembers: 0,
    writers: [],
  };
  for (const [field, value] of Object.entries({
    stubbedModules: counts.stubbedModules,
    stubbedAnyMembers: counts.stubbedAnyMembers,
  })) {
    if (!Number.isInteger(value) || value < 0) {
      throw new Error(`fixture stub inventory ${projectName}.${field} must be a nonnegative integer`);
    }
  }
  return {
    stubInventorySchema: FIXTURE_STUB_EVIDENCE_SCHEMA,
    stubbedModules: counts.stubbedModules,
    stubbedAnyMembers: counts.stubbedAnyMembers,
    stubInventoryOwners: [...(counts.writers || [])].sort(),
    stubInventoryFingerprint: fixtureStubEvidenceFingerprint(
      counts.stubbedModules,
      counts.stubbedAnyMembers,
      [...(counts.writers || [])].sort(),
    ),
  };
}

export function fixtureStubEvidenceFor(root, projectName) {
  return fixtureStubEvidenceFromInventory(computeFixtureStubInventory(root), projectName);
}

function main() {
  if (process.argv[2] !== "row-evidence" || !process.argv[3] || !process.argv[4]) {
    console.error("usage: fixture-stub-inventory.mjs row-evidence <repo-root> <project-row>");
    process.exit(2);
  }
  const evidence = fixtureStubEvidenceFor(process.argv[3], process.argv[4]);
  process.stdout.write([
    evidence.stubInventorySchema,
    evidence.stubbedModules,
    evidence.stubbedAnyMembers,
    evidence.stubInventoryFingerprint,
    JSON.stringify(evidence.stubInventoryOwners),
  ].join("\t") + "\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
