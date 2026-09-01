#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const FINGERPRINT_HELPER = path.join(ROOT, "scripts/ci/lib/project-compile-fingerprint.sh");
const EVIDENCE_HELPER = path.join(ROOT, "scripts/ci/lib/project-compat-evidence.sh");
const BUILD_MANIFEST = path.join(ROOT, "scripts/conformance/build-manifest.py");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  assert.equal(result.status, 0, `${command} ${args.join(" ")}\n${result.stderr}`);
  return result.stdout.trim();
}

function git(repo, ...args) {
  return run("git", ["-C", repo, ...args]);
}

function shell(repo, body) {
  return run("bash", ["-c", `
    set -euo pipefail
    source "${FINGERPRINT_HELPER}"
    source "${EVIDENCE_HELPER}"
    ${body}
  `], { env: { ...process.env, CHECKOUT: repo } });
}

function withRepo(fn) {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-compat-evidence-"));
  try {
    git(repo, "init", "-q");
    git(repo, "config", "user.email", "test@example.invalid");
    git(repo, "config", "user.name", "TSZ Test");
    fs.writeFileSync(path.join(repo, "tracked.txt"), "base\n");
    git(repo, "add", "tracked.txt");
    git(repo, "commit", "-qm", "base");
    fn(repo);
  } finally {
    fs.rmSync(repo, { recursive: true, force: true });
  }
}

withRepo((repo) => {
  const clean = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s %s" "$TSZ_COMPAT_SOURCE_DIRTY" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.match(clean, /^false [0-9a-f]{64}$/);
  const cleanFingerprint = clean.split(" ")[1];

  fs.writeFileSync(path.join(repo, "tracked.txt"), "unstaged\n");
  const unstaged = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s %s" "$TSZ_COMPAT_SOURCE_DIRTY" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.match(unstaged, /^true [0-9a-f]{64}$/);
  assert.notEqual(unstaged.split(" ")[1], cleanFingerprint);

  git(repo, "add", "tracked.txt");
  const staged = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.notEqual(staged, unstaged.split(" ")[1], "index state participates in the source fingerprint");

  fs.writeFileSync(path.join(repo, "untracked.txt"), "untracked\n");
  const untracked = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.notEqual(untracked, staged);
  assert.equal(
    shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"'),
    untracked,
    "unchanged dirty trees fingerprint deterministically",
  );

  fs.writeFileSync(path.join(repo, "line\nbreak.txt"), "newline path\n");
  const newlinePath = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.notEqual(newlinePath, untracked, "NUL path transport preserves newline-containing paths");

  fs.chmodSync(path.join(repo, "untracked.txt"), 0o755);
  const executable = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.notEqual(executable, newlinePath, "untracked executable mode participates in source identity");

  const link = path.join(repo, "raw-target-link");
  fs.symlinkSync("target-with-newline\n", link);
  const trailingNewlineTarget = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  fs.unlinkSync(link);
  fs.symlinkSync("target-with-newline", link);
  const plainTarget = shell(repo, 'tsz_capture_checkout_evidence "$CHECKOUT"; printf "%s" "$TSZ_COMPAT_SOURCE_TREE_FINGERPRINT"');
  assert.notEqual(
    plainTarget,
    trailingNewlineTarget,
    "raw symlink bytes preserve a trailing newline in the target",
  );

  const stability = shell(repo, `
    tsz_pin_checkout_evidence "$CHECKOUT"
    printf changed > "$CHECKOUT/changed-mid-run.txt"
    if tsz_refresh_checkout_evidence "$CHECKOUT"; then exit 9; fi
    printf "%s" "$TSZ_COMPAT_SOURCE_STABLE"
  `);
  assert.equal(stability, "false", "mid-run checkout changes invalidate later rows");
});

withRepo((repo) => {
  const fixtureRoot = path.join(repo, "fixtures");
  const project = path.join(fixtureRoot, "demo");
  fs.mkdirSync(path.join(project, "src"), { recursive: true });
  fs.writeFileSync(path.join(project, "tsconfig.json"), '{"files":["src/index.ts"]}\n');
  fs.writeFileSync(path.join(project, "src", "index.ts"), "export const value = 1;\n");
  const stable = shell(repo, `
    export FIXTURE_ROOT="$CHECKOUT/fixtures"
    export _TSZ_BINARY_HASH=binary-v1 _TSZ_TSC_ORACLE_HASH=oracle-v1
    export _TSZ_SOURCE_OVERLAY_HASH=overlay-v1 _TSZ_EVIDENCE_PROTOCOL_HASH=protocol-v1
    LAST_COMPILE_INPUT_FINGERPRINT="$(tsz_compile_input_fingerprint demo-project "$CHECKOUT/fixtures/demo/tsconfig.json" "$CHECKOUT/fixtures/demo/src")"
    tsz_refresh_compile_input_evidence demo-project "$CHECKOUT/fixtures/demo/tsconfig.json" "$CHECKOUT/fixtures/demo/src"
    printf "%s" "$LAST_COMPILE_INPUT_STABLE"
  `);
  assert.equal(stable, "true", "identical start/end compile inputs are stable");
  const changed = shell(repo, `
    export FIXTURE_ROOT="$CHECKOUT/fixtures"
    export _TSZ_BINARY_HASH=binary-v1 _TSZ_TSC_ORACLE_HASH=oracle-v1
    export _TSZ_SOURCE_OVERLAY_HASH=overlay-v1 _TSZ_EVIDENCE_PROTOCOL_HASH=protocol-v1
    LAST_COMPILE_INPUT_FINGERPRINT="$(tsz_compile_input_fingerprint demo-project "$CHECKOUT/fixtures/demo/tsconfig.json" "$CHECKOUT/fixtures/demo/src")"
    printf 'export const value = 2;\n' > "$CHECKOUT/fixtures/demo/src/index.ts"
    tsz_refresh_compile_input_evidence demo-project "$CHECKOUT/fixtures/demo/tsconfig.json" "$CHECKOUT/fixtures/demo/src" || true
    printf "%s" "$LAST_COMPILE_INPUT_STABLE"
  `);
  assert.equal(changed, "false", "mid-row fixture changes invalidate schema-3 compile evidence");
});

withRepo((repo) => {
  for (const relative of ["crates/tsz-core", "crates/tsz-cli", "crates/conformance", "scripts/conformance", ".target/test"]) {
    fs.mkdirSync(path.join(repo, relative), { recursive: true });
  }
  fs.writeFileSync(path.join(repo, "Cargo.toml"), "[workspace]\nmembers = []\n");
  fs.writeFileSync(path.join(repo, "Cargo.lock"), "version = 3\n");
  fs.copyFileSync(BUILD_MANIFEST, path.join(repo, "scripts/conformance/build-manifest.py"));
  const binary = path.join(repo, ".target/test/tsz");
  fs.writeFileSync(binary, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  const manifest = path.join(repo, ".target/test/conformance-build-manifest.json");
  run("python3", [path.join(repo, "scripts/conformance/build-manifest.py"), "write", "--repo", repo, "--manifest", manifest, "--binary", `tsz=${binary}`], {
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
  });
  const binarySha = run("shasum", ["-a", "256", binary]).split(/\s+/)[0];
  const verified = shell(repo, `
    tsz_verify_build_manifest "$CHECKOUT" "$CHECKOUT/.target/test/conformance-build-manifest.json" "${binarySha}"
    printf "%s %s %s" "$TSZ_COMPAT_BUILD_MANIFEST_SHA256" "$TSZ_COMPAT_BUILD_INPUTS_SHA256" "$TSZ_COMPAT_BUILD_MANIFEST_BINARY_SHA256"
  `);
  assert.match(verified, /^[0-9a-f]{64} [0-9a-f]{64} [0-9a-f]{64}$/);
  assert.equal(verified.split(" ")[2], binarySha);

  const manifestScript = path.join(repo, "scripts/conformance/build-manifest.py");
  const originalManifestScript = fs.readFileSync(manifestScript, "utf8");
  fs.writeFileSync(
    manifestScript,
    `#!/usr/bin/env python3
import json
import sys
from pathlib import Path
manifest = Path(sys.argv[sys.argv.index("--manifest") + 1])
value = json.loads(manifest.read_text(encoding="utf-8"))
value["inputs"]["sha256"] = "0" * 64
manifest.write_text(json.dumps(value) + "\\n", encoding="utf-8")
`,
    { mode: 0o755 },
  );
  fs.chmodSync(manifestScript, 0o755);
  const raced = spawnSync("bash", ["-c", `
    source "${FINGERPRINT_HELPER}"
    source "${EVIDENCE_HELPER}"
    tsz_verify_build_manifest "$CHECKOUT" "$CHECKOUT/.target/test/conformance-build-manifest.json" "${binarySha}"
  `], { encoding: "utf8", env: { ...process.env, CHECKOUT: repo } });
  assert.notEqual(raced.status, 0, "a manifest replaced during verification must fail closed");
  fs.writeFileSync(manifestScript, originalManifestScript, { mode: 0o755 });
  fs.chmodSync(manifestScript, 0o755);
  run("python3", [manifestScript, "write", "--repo", repo, "--manifest", manifest, "--binary", `tsz=${binary}`], {
    env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
  });

  fs.appendFileSync(binary, "# changed\n");
  const stale = spawnSync("bash", ["-c", `
    source "${FINGERPRINT_HELPER}"
    source "${EVIDENCE_HELPER}"
    tsz_verify_build_manifest "$CHECKOUT" "$CHECKOUT/.target/test/conformance-build-manifest.json" "${binarySha}"
  `], { encoding: "utf8", env: { ...process.env, CHECKOUT: repo } });
  assert.notEqual(stale.status, 0, "changed manifest binary must fail closed");
});

console.log("project compatibility evidence contract: ok");
