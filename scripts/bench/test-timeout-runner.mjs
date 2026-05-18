import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(dirname, "..", "..");
const runner = path.join(root, "scripts/bench/run-with-timeout.sh");

const success = spawnSync("bash", [runner, "2", "--", "node", "-e", "process.exit(7)"], {
  encoding: "utf8",
});
assert.equal(success.status, 7, "runner should preserve child exit status");

const timeout = spawnSync("bash", [runner, "1", "--", "node", "-e", "setTimeout(() => {}, 5000)"], {
  encoding: "utf8",
});
assert.equal(timeout.status, 124, "runner should map killed timeout to exit 124");
