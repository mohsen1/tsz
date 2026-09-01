#!/usr/bin/env node
// Regression: the project-compile-guard result-cache fingerprint must give
// unchanged rows a *stable* no-op fast path while staying correct.
//
// The defect this guards against: generated-app rows (vite/next) have no
// per-fixture .git and live under $FIXTURE_ROOT inside the tsz checkout, so the
// old `git -C "$(dirname tsconfig)" rev-parse HEAD` walked UP into the tsz
// repository and folded the *tsz* HEAD into the fingerprint. That made the
// fingerprint (a) change on every tsz commit -- so the row never hit the fast
// path and burned compile budget -- and (b) ignore the generated sources that
// actually determine the result.
//
// We exercise the real, sourced library (scripts/ci/lib/project-compile-
// fingerprint.sh) against throwaway git repos so the contract is tested, not a
// mirror of it.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const LIB = path.join(ROOT, "scripts", "ci", "lib", "project-compile-fingerprint.sh");
const BATCH_HASHER = path.join(ROOT, "scripts", "ci", "lib", "project-source-tree-hash.mjs");
const ORACLE_LIB = path.join(ROOT, "scripts", "ci", "lib", "project-tsc-oracle.sh");
const { SourceGraphWalker } = await import(pathToFileURL(BATCH_HASHER).href);

assert.ok(fs.existsSync(LIB), `fingerprint library missing: ${LIB}`);
assert.ok(fs.existsSync(BATCH_HASHER), `batch hasher missing: ${BATCH_HASHER}`);
assert.doesNotMatch(
  fs.readFileSync(LIB, "utf8"),
  /while IFS= read -r f;[\s\S]*sha256_of_file/,
  "source-tree hashing must not spawn one checksum process per file",
);
assert.doesNotMatch(
  fs.readFileSync(LIB, "utf8"),
  /\bfind\s+-L\b/,
  "project hashing must not expand symlink aliases through a global find listing",
);
assert.doesNotMatch(
  fs.readFileSync(BATCH_HASHER, "utf8"),
  /node:child_process|\bspawn(?:Sync)?\b|\bexec(?:File|Sync)?\b/,
  "the batch helper must hash files in-process",
);

// Drive the sourced library from bash. Emits `KEY=value` lines we parse below.
// `fp <label> <tsconfig> <src_dir>` prints the fingerprint for one invocation.
const harness = String.raw`
set -Eeuo pipefail
source "$LIB_PATH"
source "$ORACLE_LIB_PATH"

# A fixed stand-in for the tsz binary hash; the real guard derives this from the
# built binary. Holding it constant isolates the source-identity behaviour.
export _TSZ_BINARY_HASH="deadbeef"
export _TSZ_TSC_ORACLE_HASH="oracle-v1"
export _TSZ_SOURCE_OVERLAY_HASH="overlay-v1"
export _TSZ_EVIDENCE_PROTOCOL_HASH="evidence-v1"

git_quiet() { git -c init.defaultBranch=main -c user.email=t@t -c user.name=t -c commit.gpgsign=false "$@" >/dev/null 2>&1; }

emit() { printf '%s=%s\n' "$1" "$2"; }

# --- Build an OUTER repo that stands in for the tsz checkout. ---------------
OUTER="$(mktemp -d)"
trap 'rm -rf "$OUTER"' EXIT
git_quiet -C "$OUTER" init
echo "outer-v1" > "$OUTER/README.md"
git_quiet -C "$OUTER" add -A
git_quiet -C "$OUTER" commit -m "outer v1"

# FIXTURE_ROOT lives inside the outer repo, exactly like .target/... does.
export FIXTURE_ROOT="$OUTER/.target/project-compile-guard"
mkdir -p "$FIXTURE_ROOT"

# === Case A: generated-app row (NO per-fixture .git, nested in outer repo) ===
GEN="$FIXTURE_ROOT/gen-app"
mkdir -p "$GEN/src"
printf '{"compilerOptions":{"noEmit":true}}\n' > "$GEN/tsconfig.json"
printf 'export const x: number = 1;\n' > "$GEN/src/main.ts"
emit A_FIRST  "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"

# Move the OUTER repo HEAD. A stable fast path must NOT notice this.
echo "outer-v2" > "$OUTER/README.md"
git_quiet -C "$OUTER" add -A
git_quiet -C "$OUTER" commit -m "outer v2"
emit A_AFTER_OUTER_COMMIT "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"

# Change the generated source. The fingerprint MUST change.
printf 'export const x: number = 2;\n' > "$GEN/src/main.ts"
emit A_AFTER_SRC_EDIT "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"

export _TSZ_TSC_ORACLE_HASH="unavailable"
emit A_AFTER_ORACLE_CHANGE "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"
export _TSZ_TSC_ORACLE_HASH="oracle-v1"
export _TSZ_SOURCE_OVERLAY_HASH="overlay-v2"
emit A_AFTER_OVERLAY_CHANGE "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"
export _TSZ_SOURCE_OVERLAY_HASH="overlay-v1"
export _TSZ_EVIDENCE_PROTOCOL_HASH="evidence-v2"
emit A_AFTER_PROTOCOL_CHANGE "$(compute_compile_fingerprint gen-app "$GEN/tsconfig.json" "$GEN/src")"
export _TSZ_EVIDENCE_PROTOCOL_HASH="evidence-v1"

# Capture the outer HEAD so the test can assert it never leaks into the key.
emit OUTER_HEAD "$(git -C "$OUTER" rev-parse HEAD)"

# === Case B: git-backed fixture (its OWN repo under FIXTURE_ROOT) ============
FIX="$FIXTURE_ROOT/git-fixture"
mkdir -p "$FIX/src"
git_quiet -C "$FIX" init
printf 'export const y: number = 1;\n' > "$FIX/src/lib.ts"
git_quiet -C "$FIX" add -A
git_quiet -C "$FIX" commit -m "fixture v1"
printf '{"compilerOptions":{"noEmit":true}}\n' > "$FIX/tsconfig.json"
emit B_FIRST "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# Another outer commit must not move a git-fixture key either.
echo "outer-v3" > "$OUTER/README.md"
git_quiet -C "$OUTER" add -A
git_quiet -C "$OUTER" commit -m "outer v3"
emit B_AFTER_OUTER_COMMIT "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# Add an untracked compiled source file. The git diff marker omits this case, so
# the git-fixture identity must also include the compiled source-tree hash.
printf 'export const z: number = 1;\n' > "$FIX/src/untracked.ts"
emit B_UNTRACKED "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# Dirty the tracked tree (no commit). The content-sensitive dirty marker must
# move the key so a stale tree cannot falsely hit.
printf 'export const y: number = 99;\n' > "$FIX/src/lib.ts"
emit B_DIRTY "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# Commit it: HEAD changes, and the tree is clean again.
git_quiet -C "$FIX" add -A
git_quiet -C "$FIX" commit -m "fixture v2"
emit B_AFTER_COMMIT "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# A nested application config may extend a base config and import sources
# outside the conventional src_dir. Both live inputs must invalidate caches.
mkdir -p "$FIX/apps/web/src" "$FIX/shared"
printf '{"compilerOptions":{"strict":true}}\n' > "$FIX/tsconfig.base.json"
printf '{"extends":"../../tsconfig.base.json","files":["src/main.ts"]}\n' > "$FIX/apps/web/tsconfig.json"
printf 'import "../../../shared/imported";\n' > "$FIX/apps/web/src/main.ts"
printf 'export const imported = 1;\n' > "$FIX/shared/imported.ts"
emit C_FIRST "$(compute_compile_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit C_ORACLE_FIRST "$(tsz_tsc_oracle_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"

printf '{"compilerOptions":{"strict":false}}\n' > "$FIX/tsconfig.base.json"
emit C_BASE_EDIT "$(compute_compile_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit C_ORACLE_BASE_EDIT "$(tsz_tsc_oracle_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"

printf 'export const imported = 2;\n' > "$FIX/shared/imported.ts"
emit C_EXTERNAL_EDIT "$(compute_compile_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit C_ORACLE_EXTERNAL_EDIT "$(tsz_tsc_oracle_fingerprint nested-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"

# Installed declarations and generated Next types can be explicit compiler
# inputs. Both result and oracle cache keys must move when either tree changes.
mkdir -p "$FIX/node_modules/pkg" "$FIX/.next/types"
printf '{"name":"pkg","types":"index.d.ts"}\n' > "$FIX/node_modules/pkg/package.json"
printf 'export declare const dependency: 1;\n' > "$FIX/node_modules/pkg/index.d.ts"
printf 'export declare const route: "/a";\n' > "$FIX/.next/types/routes.d.ts"
emit D_FIRST "$(compute_compile_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit D_ORACLE_FIRST "$(tsz_tsc_oracle_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"
printf 'export declare const dependency: 2;\n' > "$FIX/node_modules/pkg/index.d.ts"
emit D_NODE_MODULE_EDIT "$(compute_compile_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit D_ORACLE_NODE_MODULE_EDIT "$(tsz_tsc_oracle_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"
printf 'export declare const route: "/b";\n' > "$FIX/.next/types/routes.d.ts"
emit D_NEXT_EDIT "$(compute_compile_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src")"
emit D_ORACLE_NEXT_EDIT "$(tsz_tsc_oracle_fingerprint dependency-app "$FIX/apps/web/tsconfig.json" "$FIX/apps/web/src" oracle-v1)"

# A broken dependency link must fail closed: partial tree hashes are never
# cache keys.
CYCLE="$FIXTURE_ROOT/cycle-row"
mkdir -p "$CYCLE/src"
printf '{"files":["src/main.ts"]}\n' > "$CYCLE/tsconfig.json"
printf 'export const main = 1;\n' > "$CYCLE/src/main.ts"
ln -s ../missing-package "$CYCLE/node_modules"
cycle_result="$(compute_compile_fingerprint cycle-row "$CYCLE/tsconfig.json" "$CYCLE/src" 2>/dev/null || true)"
cycle_oracle="$(tsz_tsc_oracle_fingerprint cycle-row "$CYCLE/tsconfig.json" "$CYCLE/src" oracle-v1 2>/dev/null || true)"
emit E_RESULT_LENGTH "$(printf '%s' "$cycle_result" | wc -c | tr -d ' ')"
emit E_ORACLE_LENGTH "$(printf '%s' "$cycle_oracle" | wc -c | tr -d ' ')"
`;

const result = spawnSync("bash", ["-c", harness], {
  encoding: "utf8",
  env: { ...process.env, LIB_PATH: LIB, ORACLE_LIB_PATH: ORACLE_LIB },
});
assert.equal(result.status, 0, `harness failed: ${result.stderr}`);

const kv = Object.fromEntries(
  result.stdout
    .split(/\r?\n/)
    .filter((l) => l.includes("="))
    .map((l) => {
      const i = l.indexOf("=");
      return [l.slice(0, i), l.slice(i + 1)];
    }),
);

for (const key of [
  "A_FIRST",
  "A_AFTER_OUTER_COMMIT",
  "A_AFTER_SRC_EDIT",
  "A_AFTER_ORACLE_CHANGE",
  "A_AFTER_OVERLAY_CHANGE",
  "A_AFTER_PROTOCOL_CHANGE",
  "OUTER_HEAD",
  "B_FIRST",
  "B_AFTER_OUTER_COMMIT",
  "B_UNTRACKED",
  "B_DIRTY",
  "B_AFTER_COMMIT",
  "C_FIRST",
  "C_BASE_EDIT",
  "C_EXTERNAL_EDIT",
  "C_ORACLE_FIRST",
  "C_ORACLE_BASE_EDIT",
  "C_ORACLE_EXTERNAL_EDIT",
  "D_FIRST",
  "D_ORACLE_FIRST",
  "D_NODE_MODULE_EDIT",
  "D_ORACLE_NODE_MODULE_EDIT",
  "D_NEXT_EDIT",
  "D_ORACLE_NEXT_EDIT",
  "E_RESULT_LENGTH",
  "E_ORACLE_LENGTH",
]) {
  assert.ok(kv[key], `missing harness output: ${key}\n${result.stdout}`);
}

// --- Case A: generated-app stability + content sensitivity ------------------
assert.equal(
  kv.A_FIRST,
  kv.A_AFTER_OUTER_COMMIT,
  "generated-app fingerprint must be stable across outer (tsz) commits",
);
assert.notEqual(
  kv.A_FIRST,
  kv.A_AFTER_SRC_EDIT,
  "generated-app fingerprint must change when a compiled source file changes",
);
assert.notEqual(
  kv.A_AFTER_SRC_EDIT,
  kv.A_AFTER_ORACLE_CHANGE,
  "result cache fingerprint must change when pinned oracle evidence becomes unavailable",
);
assert.notEqual(
  kv.A_AFTER_SRC_EDIT,
  kv.A_AFTER_OVERLAY_CHANGE,
  "result cache fingerprint must reject an entry from an older source overlay",
);
assert.notEqual(
  kv.A_AFTER_SRC_EDIT,
  kv.A_AFTER_PROTOCOL_CHANGE,
  "result cache fingerprint must reject an entry from an older evidence protocol",
);
assert.ok(
  kv.A_FIRST.includes("|tree:"),
  `generated-app row should use a content-tree identity, got: ${kv.A_FIRST}`,
);
// The outer (tsz) HEAD must never leak into a generated-app fingerprint.
assert.ok(
  !kv.A_FIRST.includes(kv.OUTER_HEAD),
  "outer repo HEAD must not appear in the generated-app fingerprint",
);

// --- Case B: git-fixture HEAD + dirty marker --------------------------------
assert.ok(
  kv.B_FIRST.includes("|git:"),
  `git-fixture row should use a git identity, got: ${kv.B_FIRST}`,
);
assert.equal(
  kv.B_FIRST,
  kv.B_AFTER_OUTER_COMMIT,
  "git-fixture fingerprint must be stable across outer (tsz) commits",
);
assert.notEqual(
  kv.B_FIRST,
  kv.B_UNTRACKED,
  "git-fixture fingerprint must change when an untracked compiled source file is added",
);
assert.notEqual(
  kv.B_FIRST,
  kv.B_DIRTY,
  "git-fixture fingerprint must change when the tracked tree is dirtied",
);
assert.notEqual(
  kv.B_FIRST,
  kv.B_AFTER_COMMIT,
  "git-fixture fingerprint must change when HEAD advances",
);
assert.notEqual(
  kv.B_DIRTY,
  kv.B_AFTER_COMMIT,
  "committing a dirty edit must not collide with the dirty-tree fingerprint",
);

// --- Case C: nested app config and imported source outside src_dir ----------
assert.notEqual(
  kv.C_FIRST,
  kv.C_BASE_EDIT,
  "nested app fingerprint must include extended/base config content",
);
assert.notEqual(
  kv.C_BASE_EDIT,
  kv.C_EXTERNAL_EDIT,
  "nested app fingerprint must include imported sources outside the declared src_dir",
);
assert.notEqual(
  kv.C_ORACLE_FIRST,
  kv.C_ORACLE_BASE_EDIT,
  "tsc-oracle cache fingerprint must include extended/base config content",
);
assert.notEqual(
  kv.C_ORACLE_BASE_EDIT,
  kv.C_ORACLE_EXTERNAL_EDIT,
  "tsc-oracle cache fingerprint must include imported sources outside src_dir",
);

// --- Case D/E: dependency/generated inputs and fail-closed traversal --------
assert.notEqual(
  kv.D_FIRST,
  kv.D_NODE_MODULE_EDIT,
  "result cache fingerprint must include node_modules declarations",
);
assert.notEqual(
  kv.D_ORACLE_FIRST,
  kv.D_ORACLE_NODE_MODULE_EDIT,
  "tsc-oracle fingerprint must include node_modules declarations",
);
assert.notEqual(
  kv.D_NODE_MODULE_EDIT,
  kv.D_NEXT_EDIT,
  "result cache fingerprint must include .next generated types",
);
assert.notEqual(
  kv.D_ORACLE_NODE_MODULE_EDIT,
  kv.D_ORACLE_NEXT_EDIT,
  "tsc-oracle fingerprint must include .next generated types",
);
assert.equal(kv.E_RESULT_LENGTH, "0", "broken symlinks disable result caching");
assert.equal(kv.E_ORACLE_LENGTH, "0", "broken symlinks disable oracle caching");

// A one-file and a many-file tree must each use exactly one batch process and
// no platform checksum command. The v2 tree identity is NUL-path-safe and also
// binds lstat mode and raw symlink target bytes.
const batchRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-batch-hash-"));
try {
  const wrapperBin = path.join(batchRoot, "bin");
  const processLog = path.join(batchRoot, "processes.log");
  fs.mkdirSync(wrapperBin);
  for (const command of ["sha256sum", "shasum"]) {
    const wrapper = path.join(wrapperBin, command);
    fs.writeFileSync(wrapper, `#!/usr/bin/env bash\nprintf '${command}\\n' >> "$HASH_PROCESS_LOG"\nexit 99\n`);
    fs.chmodSync(wrapper, 0o755);
  }
  const nodeWrapper = path.join(wrapperBin, "node");
  fs.writeFileSync(
    nodeWrapper,
    '#!/usr/bin/env bash\nprintf \'node\\n\' >> "$HASH_PROCESS_LOG"\n' +
      '[[ -z "${HASH_REMOVE_BEFORE_READ:-}" ]] || rm -f -- "$HASH_REMOVE_BEFORE_READ"\n' +
      'exec "$REAL_NODE" "$@"\n',
  );
  fs.chmodSync(nodeWrapper, 0o755);

  const createTree = (name, count) => {
    const tree = path.join(batchRoot, name);
    const files = [];
    fs.mkdirSync(tree);
    for (let index = 0; index < count; index += 1) {
      const file = path.join(tree, "src", `part-${String(index).padStart(4, "0")}.ts`);
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, `export const value${index} = ${index};\n`);
      files.push(file);
    }
    return { tree, files };
  };
  const empty = createTree("empty", 0);
  const tiny = createTree("tiny", 1);
  const many = createTree("many", 768);
  for (const [relative, content] of [
    ["node_modules/pkg/index.d.ts", "export declare const dependency: 1;\n"],
    [".next/types/routes.json", '{"route":"/batch"}\n'],
    ["src/with space.ts", "export const spaced = true;\n"],
  ]) {
    const file = path.join(many.tree, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, content);
    many.files.push(file);
  }
  fs.symlinkSync(path.join(many.tree, "node_modules", "pkg"), path.join(many.tree, "linked-types"));
  many.files.push(path.join(many.tree, "linked-types", "index.d.ts"));

  const graphEnv = (extra = {}) => ({
    ...process.env,
    TSZ_PROJECT_SOURCE_HASH_MAX_NODES: "100000",
    TSZ_PROJECT_SOURCE_HASH_MAX_EDGES: "200000",
    TSZ_PROJECT_SOURCE_HASH_MAX_DEPTH: "1024",
    TSZ_PROJECT_SOURCE_HASH_MAX_DIRECTORY_ENTRIES: "200000",
    TSZ_PROJECT_SOURCE_HASH_MAX_BYTES: String(1024 * 1024 * 1024),
    TSZ_PROJECT_SOURCE_HASH_MAX_PATH_BYTES: String(64 * 1024 * 1024),
    TSZ_PROJECT_SOURCE_HASH_MAX_MILLISECONDS: "30000",
    ...extra,
  });
  const runBatch = (fixture, extraEnv = {}) => {
    fs.writeFileSync(processLog, "");
    const batch = spawnSync("bash", ["-c", 'source "$LIB_PATH"; hash_source_tree "$TREE_PATH"'], {
      encoding: "utf8",
      timeout: 15_000,
      env: {
        ...graphEnv(extraEnv),
        PATH: `${wrapperBin}:${process.env.PATH}`,
        HASH_PROCESS_LOG: processLog,
        LIB_PATH: LIB,
        REAL_NODE: process.execPath,
        TREE_PATH: fixture.tree,
      },
    });
    assert.equal(batch.status, 0, batch.stderr);
    assert.deepEqual(
      fs.readFileSync(processLog, "utf8").trim().split(/\r?\n/),
      ["node"],
      "source-tree hashing uses one batch process regardless of file count",
    );
    assert.match(batch.stdout, /^[0-9a-f]{64}$/);
    return batch.stdout;
  };

  assert.equal(runBatch(empty), runBatch(empty), "empty tree hashing is deterministic");
  assert.equal(runBatch(tiny), runBatch(tiny), "one-file hashing is deterministic");
  assert.equal(runBatch(many), runBatch(many), "many-file hashing is deterministic");

  const binarySafe = createTree("binary-safe", 1);
  const newlinePath = path.join(binarySafe.tree, "src", "line\nbreak.ts");
  fs.writeFileSync(newlinePath, "export const newlinePath = true;\n");
  const newlineDigest = runBatch(binarySafe);
  fs.appendFileSync(newlinePath, "export const changed = true;\n");
  assert.notEqual(runBatch(binarySafe), newlineDigest, "newline-containing paths reach the hasher intact");

  const modeFile = binarySafe.files[0];
  const modeBefore = runBatch(binarySafe);
  fs.chmodSync(modeFile, 0o755);
  assert.notEqual(runBatch(binarySafe), modeBefore, "executable mode is part of tree identity");

  const targetOne = path.join(binarySafe.tree, "target-one.ts");
  const targetTwo = path.join(binarySafe.tree, "target-two.ts");
  fs.writeFileSync(targetOne, "export const target = 1;\n");
  fs.writeFileSync(targetTwo, "export const target = 1;\n");
  const link = path.join(binarySafe.tree, "src", "linked.ts");
  fs.symlinkSync("../target-one.ts", link);
  const linkOne = runBatch(binarySafe);
  fs.unlinkSync(link);
  fs.symlinkSync("../target-two.ts", link);
  assert.notEqual(
    runBatch(binarySafe),
    linkOne,
    "raw symlink target identity changes even when target contents agree",
  );

  const orderedA = createTree("ordered-a", 0);
  const orderedB = createTree("ordered-b", 0);
  for (const file of ["zeta.ts", "alpha.ts", "middle.json"]) {
    fs.mkdirSync(path.join(orderedA.tree, "src"), { recursive: true });
    fs.writeFileSync(path.join(orderedA.tree, "src", file), `${file}\n`);
  }
  for (const file of ["middle.json", "alpha.ts", "zeta.ts"]) {
    fs.mkdirSync(path.join(orderedB.tree, "src"), { recursive: true });
    fs.writeFileSync(path.join(orderedB.tree, "src", file), `${file}\n`);
  }
  assert.equal(
    runBatch(orderedA),
    runBatch(orderedB),
    "raw-byte ordering makes creation order irrelevant",
  );

  const cycle = createTree("actual-cycle", 1);
  fs.mkdirSync(path.join(cycle.tree, "cycle"));
  fs.symlinkSync("..", path.join(cycle.tree, "cycle", "back-to-root"));
  assert.equal(
    runBatch(cycle),
    runBatch(cycle),
    "an actual directory cycle becomes a deterministic physical-node back-reference",
  );

  const store = path.join(batchRoot, "pnpm-store", "pkg");
  fs.mkdirSync(store, { recursive: true });
  fs.writeFileSync(path.join(store, "package.json"), '{"name":"pkg"}\n');
  fs.writeFileSync(path.join(store, "index.d.ts"), "export declare const value: 1;\n");
  const pnpm = createTree("pnpm-fanout", 0);
  const pnpmModules = path.join(pnpm.tree, "node_modules");
  fs.mkdirSync(pnpmModules);
  const pnpmTarget = path.relative(pnpmModules, store);
  for (let index = 0; index < 128; index += 1) {
    fs.symlinkSync(pnpmTarget, path.join(pnpmModules, `alias-${String(index).padStart(3, "0")}`));
  }
  const pnpmDigest = runBatch(pnpm, {
    TSZ_PROJECT_SOURCE_HASH_MAX_NODES: "8",
    TSZ_PROJECT_SOURCE_HASH_MAX_EDGES: "256",
  });
  assert.equal(
    runBatch(pnpm, {
      TSZ_PROJECT_SOURCE_HASH_MAX_NODES: "8",
      TSZ_PROJECT_SOURCE_HASH_MAX_EDGES: "256",
    }),
    pnpmDigest,
    "pnpm-style aliases hash one external physical package and deterministic back-references",
  );

  const retarget = createTree("alias-retarget", 0);
  const targetA = path.join(batchRoot, "retarget-a");
  const targetB = path.join(batchRoot, "retarget-b");
  fs.mkdirSync(targetA);
  fs.mkdirSync(targetB);
  fs.writeFileSync(path.join(targetA, "index.d.ts"), "export declare const same: 1;\n");
  fs.writeFileSync(path.join(targetB, "index.d.ts"), "export declare const same: 1;\n");
  const packageLink = path.join(retarget.tree, "package");
  fs.symlinkSync(path.relative(retarget.tree, targetA), packageLink);
  const targetADigest = runBatch(retarget);
  fs.unlinkSync(packageLink);
  fs.symlinkSync(path.relative(retarget.tree, targetB), packageLink);
  assert.notEqual(
    runBatch(retarget),
    targetADigest,
    "retargeting an alias changes graph identity even when target contents agree",
  );

  const conservative = createTree("arbitrary-extension", 1);
  const asset = path.join(conservative.tree, "notes.md");
  const component = path.join(conservative.tree, "component.vue");
  const extensionless = path.join(conservative.tree, "generated-root");
  fs.writeFileSync(asset, "ordinary non-source content\n");
  fs.writeFileSync(component, "<script>export const value = 1;</script>\n");
  fs.writeFileSync(extensionless, "export const extensionless = 1;\n");
  let arbitraryDigest = runBatch(conservative);
  for (const [file, text, description] of [
    [asset, "asset edit\n", "ordinary files conservatively participate"],
    [component, "<!-- component edit -->\n", "allowNonTsExtensions .vue roots participate"],
    [extensionless, "// extensionless edit\n", "extensionless roots participate"],
  ]) {
    fs.appendFileSync(file, text);
    const editedDigest = runBatch(conservative);
    assert.notEqual(editedDigest, arbitraryDigest, description);
    arbitraryDigest = editedDigest;
  }
  const opaqueA = path.join(batchRoot, "opaque-a.bin");
  const opaqueB = path.join(batchRoot, "opaque-b.bin");
  fs.writeFileSync(opaqueA, "same opaque target\n");
  fs.writeFileSync(opaqueB, "same opaque target\n");
  const opaqueLink = path.join(conservative.tree, "dependency-alias");
  fs.symlinkSync(path.relative(conservative.tree, opaqueA), opaqueLink);
  const opaqueADigest = runBatch(conservative);
  fs.unlinkSync(opaqueLink);
  fs.symlinkSync(path.relative(conservative.tree, opaqueB), opaqueLink);
  assert.notEqual(
    runBatch(conservative),
    opaqueADigest,
    "all symlink topology is bound even when a file alias has no source suffix",
  );

  const retainedTree = path.join(batchRoot, "retained-target-budget");
  const retainedTarget = path.join(batchRoot, "retained-target-with-a-long-name.bin");
  fs.mkdirSync(retainedTree);
  fs.writeFileSync(retainedTarget, "target\n");
  const retainedLink = path.join(retainedTree, "alias");
  const retainedRawTarget = path.relative(retainedTree, retainedTarget);
  fs.symlinkSync(retainedRawTarget, retainedLink);
  const measuredWalker = new SourceGraphWalker("source");
  measuredWalker.hash(retainedTree);
  const exactRetainedBytes = Buffer.byteLength(retainedTree)
    + (2 * Buffer.byteLength(retainedLink))
    + Buffer.byteLength(retainedRawTarget);
  assert.equal(
    measuredWalker.verificationPathBytes,
    exactRetainedBytes,
    "retained-byte accounting includes the raw symlink target",
  );
  assert.match(
    new SourceGraphWalker("source", { maxPathBytes: exactRetainedBytes }).hash(retainedTree),
    /^[0-9a-f]{64}$/,
    "the exact retained-byte boundary is allowed",
  );
  assert.throws(
    () => new SourceGraphWalker("source", {
      maxPathBytes: exactRetainedBytes - 1,
    }).hash(retainedTree),
    /retained-byte budget exceeded/,
    "one byte past the retained-byte budget fails closed",
  );

  const budgetFailure = spawnSync(
    process.execPath,
    [BATCH_HASHER, tiny.tree, "--source-tree"],
    {
      encoding: "utf8",
      env: graphEnv({ TSZ_PROJECT_SOURCE_HASH_MAX_NODES: "2" }),
    },
  );
  assert.notEqual(budgetFailure.status, 0, "a configured physical-node budget is enforced");
  assert.equal(budgetFailure.stdout, "", "budget failure must never publish a partial digest");

  const elapsedFailure = spawnSync(
    process.execPath,
    [BATCH_HASHER, many.tree, "--source-tree"],
    {
      encoding: "utf8",
      env: graphEnv({ TSZ_PROJECT_SOURCE_HASH_MAX_MILLISECONDS: "1" }),
    },
  );
  assert.notEqual(elapsedFailure.status, 0, "the monotonic elapsed-time budget is enforced");
  assert.equal(elapsedFailure.stdout, "", "elapsed-time failure must never publish a partial digest");

  let boundaryStarted = false;
  const exactBoundary = new SourceGraphWalker("source", {
    maxMilliseconds: 1,
    now: () => {
      if (!boundaryStarted) {
        boundaryStarted = true;
        return 0n;
      }
      return 1_000_000n;
    },
  }).hash(empty.tree);
  assert.match(exactBoundary, /^[0-9a-f]{64}$/, "the exact elapsed-time boundary is allowed");

  let exceededStarted = false;
  assert.throws(
    () => new SourceGraphWalker("source", {
      maxMilliseconds: 1,
      now: () => {
        if (!exceededStarted) {
          exceededStarted = true;
          return 0n;
        }
        return 1_000_001n;
      },
    }).hash(empty.tree),
    /elapsed-time budget exceeded/,
    "one nanosecond past the elapsed-time budget fails closed",
  );

  const mutating = createTree("mutating", 1);
  const originalLstat = fs.lstatSync;
  let rootObservations = 0;
  try {
    fs.lstatSync = (candidate, options) => {
      if (Buffer.isBuffer(candidate)
        && candidate.equals(Buffer.from(mutating.tree))
        && (rootObservations += 1) === 3) {
        fs.appendFileSync(mutating.files[0], "changed before final verification\n");
      }
      return originalLstat(candidate, options);
    };
    assert.throws(
      () => new SourceGraphWalker("source").hash(mutating.tree),
      /changed while its source-tree fingerprint was computed/,
      "a node changed after traversal is rejected by final-state verification",
    );
  } finally {
    fs.lstatSync = originalLstat;
  }
} finally {
  fs.rmSync(batchRoot, { recursive: true, force: true });
}

// Oracle cache/artifact identity covers the pinned mapping, JS launcher,
// native compiler, and every builtin declaration file with unambiguous frames.
const oracleRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-oracle-identity-"));
try {
  const libDir = path.join(oracleRoot, "platform", "lib");
  const native = path.join(libDir, "tsc");
  const launcher = path.join(oracleRoot, "wrapper", "lib", "tsc.js");
  const mapping = path.join(oracleRoot, "typescript-versions.json");
  const wrapperPackage = path.join(oracleRoot, "wrapper", "package.json");
  fs.mkdirSync(libDir, { recursive: true });
  fs.mkdirSync(path.dirname(launcher), { recursive: true });
  fs.writeFileSync(path.join(libDir, "lib.d.ts"), "/// <reference no-default-lib=\"true\"/>\n");
  fs.writeFileSync(path.join(libDir, "lib.es5.d.ts"), "interface Array<T> { length: number }\n");
  fs.writeFileSync(native, "native-v1\n", { mode: 0o755 });
  fs.writeFileSync(launcher, "launcher-v1\n");
  fs.writeFileSync(mapping, '{"current":"pin-a","mappings":{"pin-a":{"npm":"7.0.2"}}}\n');
  fs.writeFileSync(wrapperPackage, '{"name":"typescript","version":"7.0.2"}\n');
  fs.writeFileSync(path.join(oracleRoot, "platform", "package.json"), '{"version":"7.0.2"}\n');

  const oracleIdentity = () => {
    const value = spawnSync(
      "bash",
      ["-c", 'source "$LIB_PATH"; tsz_oracle_identity_fingerprint protocol-v1 "$LIB_DIR" "$NATIVE" "$MAPPING" "$WRAPPER" "$LAUNCHER"'],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          LIB_PATH: LIB,
          LIB_DIR: libDir,
          NATIVE: native,
          MAPPING: mapping,
          WRAPPER: wrapperPackage,
          LAUNCHER: launcher,
        },
      },
    );
    assert.equal(value.status, 0, value.stderr);
    const fingerprint = value.stdout.trim();
    assert.match(fingerprint, /^[0-9a-f]{64}$/);
    return fingerprint;
  };

  const baseline = oracleIdentity();
  fs.appendFileSync(mapping, " ");
  const mappingChanged = oracleIdentity();
  assert.notEqual(mappingChanged, baseline, "pinned package/version mapping is oracle identity");
  fs.appendFileSync(launcher, "launcher-v2\n");
  const launcherChanged = oracleIdentity();
  assert.notEqual(launcherChanged, mappingChanged, "launcher content is oracle identity");
  fs.appendFileSync(native, "native-v2\n");
  const nativeChanged = oracleIdentity();
  assert.notEqual(nativeChanged, launcherChanged, "native compiler content is oracle identity");
  fs.appendFileSync(path.join(libDir, "lib.es5.d.ts"), "interface ReadonlyArray<T> {}\n");
  assert.notEqual(oracleIdentity(), nativeChanged, "every consumed builtin lib content is oracle identity");
} finally {
  fs.rmSync(oracleRoot, { recursive: true, force: true });
}

console.log("project-compile-guard fingerprint: ok");
