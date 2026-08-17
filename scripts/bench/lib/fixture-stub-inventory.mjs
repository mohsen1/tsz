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
import path from "node:path";

const CONFIG_FN_RE = /^tsz_write_([a-zA-Z0-9_]+)_config\(\)\s*\{/;
const STUB_CALL_RE = /\b(tsz_write_[a-zA-Z0-9_]+_(?:external|canary)_stubs)\b/;
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

// Returns `{ [projectRowName]: { stubbedModules, stubbedAnyMembers } }` for
// every project row whose fixture config calls a stub writer. Rows absent
// from the result install real dependencies (or need no stubs at all).
export function computeFixtureStubInventory(root) {
  const fixturesPath = path.join(root, "scripts/bench/project-fixtures.sh");
  const stubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs.sh");
  const canaryStubsPath = path.join(root, "scripts/bench/lib/project-fixture-stubs-canary.sh");

  const fixturesSource = readIfExists(fixturesPath);
  if (!fixturesSource) return {};

  const stubFunctions = new Map();
  for (const stubsSource of [readIfExists(stubsPath), readIfExists(canaryStubsPath)]) {
    if (!stubsSource) continue;
    for (const { name, body } of splitTopLevelFunctions(stubsSource, STUB_FN_RE)) {
      stubFunctions.set(name, body);
    }
  }

  const inventory = {};
  for (const { name: slug, body } of splitTopLevelFunctions(fixturesSource, CONFIG_FN_RE)) {
    const call = STUB_CALL_RE.exec(body);
    if (!call) continue;
    const stubBody = stubFunctions.get(call[1]);
    if (!stubBody) continue;

    const projectName = `${slug.replace(/_/g, "-")}-project`;
    inventory[projectName] = {
      stubbedModules: countMatches(stubBody, DECLARE_MODULE_RE),
      stubbedAnyMembers: countMatches(stubBody, ANY_MEMBER_RE),
    };
  }
  return inventory;
}
