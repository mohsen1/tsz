#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..", "..");
const profileScript = path.join(scriptDir, "perf-flat-profile.sh");
const script = fs.readFileSync(profileScript, "utf8");

assert.match(script, /--json-file <path>/, "help text should document JSON output");
assert.match(script, /--json-file\) JSON_FILE="\$2"; shift 2 ;;/, "CLI should parse --json-file");
assert.match(
  script,
  /python3 - "\$PROFILE_JSON" "\$TSZ_BIN" "\$TEXT_BASE" "\$TOP" "\$JSON_FILE" "\$\{TSZ_ARGS\[@\]\}"/,
  "profile parser should receive the output path and original tsz args",
);
assert.match(script, /"schema_version": 1/, "JSON payload should be schema-versioned");
assert.match(script, /"self_samples": sc/, "JSON rows should include self sample counts");
assert.match(
  script,
  /path\.write_text\(json\.dumps\(payload, indent=2\) \+ "\\n", encoding="utf8"\)/,
  "JSON output should be deterministic pretty-printed UTF-8",
);

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-flat-profile-"));
try {
  const toolsDir = path.join(tmpDir, "bin");
  fs.mkdirSync(toolsDir);

  fs.writeFileSync(
    path.join(toolsDir, "otool"),
    `#!/usr/bin/env bash
cat <<'OUT'
segname __TEXT
    vmaddr 0x100000000
OUT
`,
    { mode: 0o755 },
  );

  fs.writeFileSync(
    path.join(toolsDir, "atos"),
    `#!/usr/bin/env bash
set -euo pipefail
skip=false
idx=0
for arg in "$@"; do
  if [[ "$skip" == true ]]; then skip=false; continue; fi
  if [[ "$arg" == "-l" ]]; then skip=true; continue; fi
  if [[ "$arg" == 0x* ]]; then
    if [[ "$idx" -eq 0 ]]; then
      echo "tsz_checker::root::h1111111111111111 (in tsz) (root.rs:1)"
    else
      echo "tsz_solver::hot_leaf::h2222222222222222 (in tsz) (leaf.rs:1)"
    fi
    idx=$((idx + 1))
  fi
done
`,
    { mode: 0o755 },
  );

  fs.writeFileSync(
    path.join(toolsDir, "samply"),
    `#!/usr/bin/env bash
set -euo pipefail
out=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    --) break ;;
    *) shift ;;
  esac
done
python3 - "$out" <<'PY'
import gzip
import json
import sys

payload = {
    "threads": [
        {
            "frameTable": {"address": [1, 2]},
            "stackTable": {"frame": [0, 1], "prefix": [None, 0]},
            "samples": {"stack": [1, 1, 0]},
        }
    ]
}
with gzip.open(sys.argv[1], "wt", encoding="utf8") as f:
    json.dump(payload, f)
PY
`,
    { mode: 0o755 },
  );

  const fakeTsz = path.join(tmpDir, "tsz");
  fs.writeFileSync(fakeTsz, "#!/usr/bin/env bash\nexit 0\n", { mode: 0o755 });
  const jsonFile = path.join(tmpDir, "profile.json");
  const result = spawnSync(
    "bash",
    [
      profileScript,
      "--no-build",
      "--bin",
      fakeTsz,
      "--iterations",
      "2",
      "--top",
      "2",
      "--json-file",
      jsonFile,
      "input.ts",
    ],
    {
      cwd: repoRoot,
      encoding: "utf8",
      env: { ...process.env, PATH: `${toolsDir}${path.delimiter}${process.env.PATH}` },
    },
  );
  assert.equal(result.status, 0, `${result.stderr}\n${result.stdout}`);
  assert.match(result.stdout, /flat profile JSON written/);

  const payload = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
  assert.equal(payload.schema_version, 1);
  assert.equal(payload.samples, 3);
  assert.equal(payload.binary, fakeTsz);
  assert.deepEqual(payload.args, ["input.ts"]);
  assert.equal(payload.rows[0].function, "tsz_solver::hot_leaf");
  assert.equal(payload.rows[0].self_samples, 2);
  assert.equal(payload.rows[1].function, "tsz_checker::root");
  assert.equal(payload.rows[1].inclusive_samples, 3);
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

console.log("perf flat profile JSON contract checks passed");
