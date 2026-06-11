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
assert.doesNotMatch(
  success.stderr,
  /run-with-timeout:/,
  "completed runs should not emit a timeout note",
);

// A sleeping child consumes ~no CPU during the timeout window: the runner must
// flag the kill as likely CPU contention / unmeasured rather than implying the
// command was genuinely slow (issue #13174).
const idleTimeout = spawnSync(
  "bash",
  [runner, "1", "--", "node", "-e", "setTimeout(() => {}, 5000)"],
  { encoding: "utf8" },
);
assert.equal(idleTimeout.status, 124, "runner should map killed timeout to exit 124");
assert.match(idleTimeout.stderr, /run-with-timeout: wall timeout after 1s/, idleTimeout.stderr);
assert.match(
  idleTimeout.stderr,
  /likely CPU contention|CPU time unavailable/,
  `idle timeout must not be reported as CPU-bound: ${idleTimeout.stderr}`,
);

// A busy-looping child is genuinely CPU-bound: the note must say so and must
// not suggest contention.
const busyTimeout = spawnSync(
  "bash",
  [runner, "3", "--", "node", "-e", "const t = Date.now(); while (Date.now() - t < 20000) {}"],
  { encoding: "utf8" },
);
assert.equal(busyTimeout.status, 124, "busy timeout still exits 124");
assert.match(
  busyTimeout.stderr,
  /CPU-bound timeout/,
  `busy timeout should be classified CPU-bound: ${busyTimeout.stderr}`,
);
assert.doesNotMatch(busyTimeout.stderr, /contention/, busyTimeout.stderr);
