#!/usr/bin/env node
// Guards the #13747 cargo-target-deps staleness reporting in gcp-cache.sh.
//
// The measured #13747 rollout (republish the workspace-rlib blob on every green
// main build) is gated on reading how stale the restored blob is. The consumer
// (dist-binaries) checks out fetch-depth:1, so a `git rev-list blob..HEAD`
// commit-distance reports "unknown" on the real runner. The wall-clock age
// derived from the `.tsz-cache-built-at` marker is depth-independent and is the
// number the rollout decision actually needs, so this verifies it resolves even
// when the commit-distance cannot.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const CACHE_SCRIPT = path.join(ROOT, "scripts", "ci", "gcp-cache.sh");
const cacheScript = fs.readFileSync(CACHE_SCRIPT, "utf8");

// --- Static guards: the markers + summary plumbing must stay wired. ----------
assert.match(
  cacheScript,
  /date -u \+%s > \.target\/dist-fast\/\.tsz-cache-built-at/,
  "save must write the epoch build-time marker alongside the commit marker",
);
assert.match(
  cacheScript,
  /\.tsz-cache-built-at/,
  "reader must consult the build-time marker",
);
assert.match(
  cacheScript,
  /GITHUB_STEP_SUMMARY/,
  "staleness must surface to the job summary, not only stdout logs",
);
assert.match(
  cacheScript,
  /if \[\[ "\$\{BASH_SOURCE\[0\]\}" == "\$\{0\}" \]\]; then\n\s*main "\$@"/,
  "main must be guarded so the script can be sourced for testing",
);

// --- Functional: drive the real bash helpers in isolation. -------------------
const work = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stale-"));
const stubBin = path.join(work, "bin");
fs.mkdirSync(stubBin, { recursive: true });
// `git` stub: rev-parse always resolves HEAD; rev-list either fails (shallow
// clone — blob commit absent) or echoes a count, controlled via env.
fs.writeFileSync(
  path.join(stubBin, "git"),
  [
    "#!/usr/bin/env bash",
    'case "$1" in',
    '  rev-parse) echo "${STUB_HEAD:-headsha0000}" ;;',
    '  rev-list) if [[ -n "${STUB_REVLIST_COUNT:-}" ]]; then echo "$STUB_REVLIST_COUNT"; else exit 1; fi ;;',
    "  *) exit 0 ;;",
    "esac",
    "",
  ].join("\n"),
  { mode: 0o755 },
);

// One bash invocation exercises both clone shapes (the git stub's rev-list is
// toggled mid-run) plus the formatter, so the script is sourced only once.
const harness = `
set -uo pipefail
export _TSZ_CI_CACHE_BUCKET="gs://example/tsz-test-cache"
export PATH="${stubBin}:$PATH"
source "${CACHE_SCRIPT}"
hash -r
work="${work}"
cd "$work"
mkdir -p .target/dist-fast
echo "blobsha1111" > .target/dist-fast/.tsz-cache-commit
echo "$(( $(date -u +%s) - 7200 ))" > .target/dist-fast/.tsz-cache-built-at
export GITHUB_STEP_SUMMARY="$work/summary.md"
: > "$work/summary.md"
echo "===SHALLOW==="          # rev-list fails: blob commit absent under fetch-depth:1
unset STUB_REVLIST_COUNT
log_cargo_target_deps_staleness
echo "===DEEP==="             # rev-list resolves: full history available
export STUB_REVLIST_COUNT=5
log_cargo_target_deps_staleness
echo "===FMT==="
format_duration 90061
format_duration 3661
format_duration 600
format_duration notanumber
`;

const out = execFileSync("bash", ["-c", harness], {
  cwd: ROOT,
  env: process.env,
  encoding: "utf8",
});

function staleLine(section) {
  const header = `===${section}===`;
  const start = out.indexOf(header);
  return out
    .slice(start + header.length, out.indexOf("===", start + header.length))
    .split("\n")
    .find((l) => l.startsWith("cargo-target-deps staleness:"));
}

// Shallow clone — rev-list fails, age still resolves from built-at.
const shallowLine = staleLine("SHALLOW");
assert.ok(shallowLine, "staleness line must be printed");
assert.match(shallowLine, /blob built at blobsha1111/, "blob commit reported");
assert.match(shallowLine, /age 2h 0m/, "wall-clock age resolves under shallow clone");
assert.match(
  shallowLine,
  /is unknown commit\(s\) behind/,
  "commit-distance is unknown when rev-list cannot resolve the blob commit",
);

// Full history — rev-list resolves, distance reported alongside the age.
const deepLine = staleLine("DEEP");
assert.match(deepLine, /is 5 commit\(s\) behind/, "commit-distance reported when available");
assert.match(deepLine, /age 2h 0m/, "age still reported alongside distance");

const summary = fs.readFileSync(path.join(work, "summary.md"), "utf8");
assert.match(summary, /cargo-target-deps staleness:/, "staleness appended to job summary");

// format_duration shapes.
const fmt = out.slice(out.indexOf("===FMT===")).split("\n");
assert.equal(fmt[1], "1d 1h", "format_duration days");
assert.equal(fmt[2], "1h 1m", "format_duration hours");
assert.equal(fmt[3], "10m", "format_duration minutes");
assert.equal(fmt[4], "unknown", "format_duration rejects non-numeric");

fs.rmSync(work, { recursive: true, force: true });
console.log("test-gcp-cache-staleness: ok");
