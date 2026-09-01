#!/usr/bin/env node
import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";

import {
  compilerStatsFrom,
  normalizeProjectPath,
  pathGraphFingerprint,
  pathMultisetFingerprint,
  rootFilePathsFromShowConfig,
  sourceFilePathsFromListFilesOnly,
} from "./project-compile-stats.mjs";

const projectRoot = path.join(os.tmpdir(), "tsz-project-graph");
const configDir = path.join(projectRoot, "apps", "web");
const rootPaths = ["./src/a.ts", "./src/b.ts"];
const sourcePaths = [
  path.join(configDir, "src", "a.ts"),
  path.join(configDir, "src", "b.ts"),
];

const statsPayload = (semantic_completion) => ({
  schema_version: 2,
  stats: {
    semantic_completion,
    root_files: 2,
    source_files: 2,
    files: 2,
    root_file_paths: rootPaths,
    source_file_paths: sourcePaths,
  },
});
const parsed = compilerStatsFrom(statsPayload("complete"));
assert.equal(parsed.semanticCompletion, "complete");
assert.deepEqual(parsed.rootFilePaths, rootPaths);
assert.deepEqual(parsed.sourceFilePaths, sourcePaths);
for (const semanticCompletion of ["complete", "deferred", "cycle", "limit"]) {
  assert.equal(
    compilerStatsFrom(statsPayload(semanticCompletion)).semanticCompletion,
    semanticCompletion,
    `${semanticCompletion} is valid typed completion telemetry`,
  );
}

for (const invalid of [
  { schema_version: 1, stats: parsed },
  { schema_version: 2, stats: { root_files: 1, source_files: 1, files: 1 } },
  {
    schema_version: 2,
    stats: {
      semantic_completion: "complete",
      root_files: 1,
      source_files: 1,
      files: 1,
      root_file_paths: [],
      source_file_paths: ["a.ts"],
    },
  },
  {
    schema_version: 2,
    stats: {
      semantic_completion: "complete",
      root_files: 1,
      source_files: 1,
      files: 2,
      root_file_paths: ["a.ts"],
      source_file_paths: ["a.ts"],
    },
  },
]) {
  assert.throws(() => compilerStatsFrom(invalid));
}

for (const semantic_completion of [undefined, null, "", "Complete", "complete ", "unknown", 0]) {
  assert.throws(
    () => compilerStatsFrom({
      schema_version: 2,
      stats: {
        semantic_completion,
        root_files: 1,
        source_files: 1,
        files: 1,
        root_file_paths: ["a.ts"],
        source_file_paths: ["a.ts"],
      },
    }),
    /semantic_completion/,
    `${String(semantic_completion)} must not certify project evidence`,
  );
}

assert.deepEqual(rootFilePathsFromShowConfig({ files: rootPaths }), rootPaths);
assert.equal(
  normalizeProjectPath("./src/a.ts", configDir, projectRoot),
  "apps/web/src/a.ts",
);
assert.equal(
  pathGraphFingerprint(rootPaths, configDir, projectRoot),
  pathGraphFingerprint(sourcePaths, configDir, projectRoot),
  "relative TSZ/showConfig and absolute listFiles spellings normalize identically",
);
assert.notEqual(
  pathGraphFingerprint(rootPaths, configDir, projectRoot),
  pathGraphFingerprint([...rootPaths].reverse(), configDir, projectRoot),
  "configured root/source order is evidence",
);
assert.equal(
  pathMultisetFingerprint(rootPaths, configDir, projectRoot),
  pathMultisetFingerprint([...rootPaths].reverse(), configDir, projectRoot),
  "multiset helper preserves membership while the graph fingerprint also checks order",
);

const builtinLibDir = path.join(os.tmpdir(), "typescript", "lib");
const retainedDeclaration = path.join(projectRoot, "node_modules", "@types", "pkg", "index.d.ts");
assert.deepEqual(
  sourceFilePathsFromListFilesOnly(
    `${path.join(builtinLibDir, "lib.es2022.d.ts")}\n${retainedDeclaration}\n`,
    builtinLibDir,
  ),
  [retainedDeclaration],
  "only canonical TypeScript built-ins are filtered from source graph evidence",
);

console.log("test-project-compile-stats: all tests passed");
