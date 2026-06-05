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
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const LIB = path.join(ROOT, "scripts", "ci", "lib", "project-compile-fingerprint.sh");

assert.ok(fs.existsSync(LIB), `fingerprint library missing: ${LIB}`);

// Drive the sourced library from bash. Emits `KEY=value` lines we parse below.
// `fp <label> <tsconfig> <src_dir>` prints the fingerprint for one invocation.
const harness = String.raw`
set -Eeuo pipefail
source "$LIB_PATH"

# A fixed stand-in for the tsz binary hash; the real guard derives this from the
# built binary. Holding it constant isolates the source-identity behaviour.
export _TSZ_BINARY_HASH="deadbeef"

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

# Dirty the tracked tree (no commit). The content-sensitive dirty marker must
# move the key so a stale tree cannot falsely hit.
printf 'export const y: number = 99;\n' > "$FIX/src/lib.ts"
emit B_DIRTY "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"

# Commit it: HEAD changes, and the tree is clean again.
git_quiet -C "$FIX" add -A
git_quiet -C "$FIX" commit -m "fixture v2"
emit B_AFTER_COMMIT "$(compute_compile_fingerprint git-fixture "$FIX/tsconfig.json" "$FIX/src")"
`;

const result = spawnSync("bash", ["-c", harness], {
  encoding: "utf8",
  env: { ...process.env, LIB_PATH: LIB },
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
  "OUTER_HEAD",
  "B_FIRST",
  "B_AFTER_OUTER_COMMIT",
  "B_DIRTY",
  "B_AFTER_COMMIT",
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

console.log("project-compile-guard fingerprint: ok");
