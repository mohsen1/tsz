// Static inventory of the ambient `declare module` shims and `any`-typed
// members the no-install external/canary fixtures depend on (#16311 ask 4).
//
// Fixture setup never runs `npm install`; instead `project-fixtures.sh` hand-
// writes ambient module stubs so `tsc`/`tsz` don't see a `TS2307` wall for a
// project's real dependencies. A row measured against a stub loses coverage
// at exactly its dependency boundaries, so a green row with zero stubs and a
// green row whose dependency graph is erased to `any` are different claims.
// This module counts that per-project, from source, with no build step:
// each `tsz_write_<slug>_config()` in `project-fixtures.sh` that calls a
// `tsz_write_<slug>_(external|canary)_stubs()` writer is matched to that
// writer's body in the two stub libraries, and the body is scanned for
// `declare module` blocks and `any`-typed members (`: any` / `= any`).
import fs from "node:fs";
import crypto from "node:crypto";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const FIXTURE_STUB_EVIDENCE_SCHEMA = 1;

const CONFIG_FN_RE = /^tsz_write_([a-zA-Z0-9_]+)_config\(\)\s*\{/;
const STUB_CALL_RE = /\b(tsz_write_[a-zA-Z0-9_]+_(?:external|canary)_stubs)\b/;
const STUB_CALL_GLOBAL_RE = /\b(tsz_write_[a-zA-Z0-9_]+_(?:external|canary)_stubs)\b/g;
const STUB_FN_RE = /^(tsz_write_[a-zA-Z0-9_]+_(?:external|canary)_stubs)\(\)\s*\{/;
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

// Returns `{ [projectRowName]: { stubbedModules, stubbedAnyMembers } }` for
// every project row whose fixture config calls a stub writer. Rows absent
// from the result install real dependencies (or need no stubs at all).
export function computeFixtureStubInventory(root) {
  const fixturesPath = path.join(root, "scripts/bench/project-fixtures.sh");
  const stubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs.sh");
  const canaryStubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs-canary.sh");

  const fixturesSource = readRequired(fixturesPath, "scripts/bench/project-fixtures.sh");

  const stubFunctions = new Map();
  for (const [stubsSource, label] of [
    [readRequired(stubsPath, "scripts/bench/lib/project-fixture-stubs.sh"), "external"],
    [readRequired(canaryStubsPath, "scripts/bench/lib/project-fixture-stubs-canary.sh"), "canary"],
  ]) {
    for (const { name, body } of splitTopLevelFunctions(stubsSource, STUB_FN_RE)) {
      if (stubFunctions.has(name)) {
        throw new Error(`duplicate ${label} fixture stub writer: ${name}`);
      }
      stubFunctions.set(name, body);
    }
  }
  if (stubFunctions.size === 0) {
    throw new Error("fixture stub inventory parsed zero stub writers");
  }

  const inventory = {};
  const configFunctions = splitTopLevelFunctions(fixturesSource, CONFIG_FN_RE);
  if (configFunctions.length === 0) {
    throw new Error("fixture stub inventory parsed zero config writers");
  }
  const linkedStubCalls = new Set();
  for (const { name: slug, body } of configFunctions) {
    const call = STUB_CALL_RE.exec(body);
    if (!call) continue;
    linkedStubCalls.add(call[1]);
    const stubBody = stubFunctions.get(call[1]);
    if (!stubBody) {
      throw new Error(`fixture config ${slug} references missing stub writer ${call[1]}`);
    }

    const projectName = `${slug.replace(/_/g, "-")}-project`;
    inventory[projectName] = {
      stubbedModules: countMatches(stubBody, DECLARE_MODULE_RE),
      stubbedAnyMembers: countMatches(stubBody, ANY_MEMBER_RE),
    };
  }
  const referencedStubCalls = new Set(fixturesSource.match(STUB_CALL_GLOBAL_RE) || []);
  for (const writer of referencedStubCalls) {
    if (!linkedStubCalls.has(writer)) {
      throw new Error(`fixture stub call was not linked to a config writer: ${writer}`);
    }
  }
  if (Object.keys(inventory).length === 0) {
    throw new Error("fixture stub inventory parsed zero project rows");
  }
  return inventory;
}

function stubEvidenceFingerprint(stubbedModules, stubbedAnyMembers) {
  const payload = JSON.stringify({
    schema_version: FIXTURE_STUB_EVIDENCE_SCHEMA,
    stubbed_modules: stubbedModules,
    stubbed_any_members: stubbedAnyMembers,
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
  const counts = inventory[projectName] || { stubbedModules: 0, stubbedAnyMembers: 0 };
  for (const [field, value] of Object.entries(counts)) {
    if (!Number.isInteger(value) || value < 0) {
      throw new Error(`fixture stub inventory ${projectName}.${field} must be a nonnegative integer`);
    }
  }
  return {
    stubInventorySchema: FIXTURE_STUB_EVIDENCE_SCHEMA,
    stubbedModules: counts.stubbedModules,
    stubbedAnyMembers: counts.stubbedAnyMembers,
    stubInventoryFingerprint: stubEvidenceFingerprint(
      counts.stubbedModules,
      counts.stubbedAnyMembers,
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
  ].join("\t") + "\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
