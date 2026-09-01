#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  KNOWN_UNIT_CRATES,
  classifyGatePaths,
  normalizePathList,
} from "./gate-path-classifier.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(SCRIPT_DIR, "gate-path-classifier.mjs");

function classify(paths) {
  return classifyGatePaths(paths.join("\n"));
}

assert.deepEqual(KNOWN_UNIT_CRATES, ["tsz-core", "tsz-cli", "tsz-conformance"]);

assert.deepEqual(normalizePathList("docs/a.md\n\ndocs/a.md\r\nREADME.md\n"), [
  "README.md",
  "docs/a.md",
]);

assert.equal(classify(["docs/usage.md", "README.md", "LICENSE"]).docsOnly, true);
assert.equal(classify(["docs/usage.md", "src/lib.rs"]).docsOnly, false);

assert.equal(
  classify(["crates/tsz-core/src/lib.rs"]).compilerChecksRequired,
  true,
  "active core changes must require compiler CI",
);
assert.equal(
  classify([".github/workflows/bench.yml"]).compilerChecksRequired,
  true,
  "bench workflow changes must require compiler CI",
);
assert.equal(
  classify(["scripts/ci/gate-path-classifier.mjs"]).compilerChecksRequired,
  true,
  "the gate classifier itself must require compiler CI",
);
assert.equal(
  classify(["scripts/ci/ci-resources.sh"]).compilerChecksRequired,
  true,
  "CI resource sizing changes must require compiler CI",
);
assert.equal(classify(["docs/usage.md"]).compilerChecksRequired, false);

{
  const result = classify(["scripts/bench/build-fixture.sh", "docs/bench.md"]);
  assert.equal(result.benchShellOnly, true);
  assert.deepEqual(result.benchShellPaths, ["scripts/bench/build-fixture.sh"]);
}

assert.equal(
  classify(["scripts/bench/build-fixture.sh", "crates/tsz-core/src/lib.rs"]).benchShellOnly,
  false,
);
assert.equal(classify(["scripts/perf/collect.py", "docs/perf.md"]).perfToolOnly, true);
assert.equal(
  classify(["scripts/perf/forced-parallel-project-determinism.sh", "docs/perf.md"]).perfToolOnly,
  true,
  "perf shell harness changes should run the perf tool smoke path",
);
assert.equal(
  classify(["scripts/perf/collect.py", "scripts/perf/forced-parallel-project-determinism.sh"]).perfToolOnly,
  true,
);
assert.equal(classify(["scripts/arch/guard.py", "docs/arch.md"]).archToolOnly, true);

{
  const result = classify([
    "crates/tsz-core/src/semantics/relation.rs",
    "crates/tsz-cli/tests/process.rs",
    "docs/checker.md",
  ]);
  assert.equal(result.draftUnitNarrow.canNarrow, true);
  assert.deepEqual(result.draftUnitNarrow.unitPackages, ["tsz-cli", "tsz-core"]);
}

{
  const result = classify(["crates/tsz-cli/src/main.rs"]);
  assert.equal(result.draftUnitNarrow.canNarrow, true);
  assert.deepEqual(result.draftUnitNarrow.unitPackages, ["tsz-cli"]);
}

{
  const result = classify(["crates/not-a-workspace-member/src/lib.rs"]);
  assert.equal(result.compilerChecksRequired, false);
  assert.equal(result.draftUnitNarrow.canNarrow, false);
  assert.match(result.draftUnitNarrow.reason, /non-unit crate paths touched/);
}

{
  const result = classify(["scripts/ci/full-ci.sh"]);
  assert.equal(result.draftUnitNarrow.canNarrow, false);
  assert.match(result.draftUnitNarrow.reason, /blast-radius paths touched/);
}

{
  const result = classify(["Cargo.lock", ".cargo/config.toml", ".cargo/other.toml"]);
  assert.deepEqual(result.cacheKeyInputPaths, [".cargo/config.toml", "Cargo.lock"]);
}

const cli = spawnSync(process.execPath, [SCRIPT], {
  input: "docs/readme.md\n.github/workflows/bench.yml\n",
  encoding: "utf8",
});
assert.equal(cli.status, 0, cli.stderr);
assert.equal(JSON.parse(cli.stdout).compilerChecksRequired, true);

console.log("test-gate-path-classifier: ok");
