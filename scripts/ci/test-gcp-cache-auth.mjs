#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const cacheScript = fs.readFileSync(
  path.join(ROOT, "scripts", "ci", "gcp-cache.sh"),
  "utf8",
);
const fullCiScript = fs.readFileSync(
  path.join(ROOT, "scripts", "ci", "gcp-full-ci.sh"),
  "utf8",
);

function shellFunction(script, name, fileName) {
  const match = script.match(new RegExp(`${name}\\(\\) \\{[\\s\\S]+?\\n\\}`));
  assert.ok(match, `${fileName} should define ${name}`);
  return match[0];
}

for (const [fileName, script] of [
  ["gcp-cache.sh", cacheScript],
  ["gcp-full-ci.sh", fullCiScript],
]) {
  const authFn = shellFunction(script, "ensure_gcs_auth", fileName);
  assert.match(
    authFn,
    /SCCACHE_GCS_KEY_JSON[\s\S]+GOOGLE_APPLICATION_CREDENTIALS/,
    `${fileName} GCS auth should accept the same service-account secret used by sccache`,
  );
  assert.match(
    authFn,
    /gcloud auth activate-service-account[\s\S]+--key-file="\$GOOGLE_APPLICATION_CREDENTIALS"/,
    `${fileName} GCS auth should activate the service-account key for gcloud`,
  );
  assert.match(
    authFn,
    /pass_credentials_to_gsutil true/,
    `${fileName} GCS auth should let gsutil use the active gcloud account`,
  );
}

const mainOffset = cacheScript.indexOf("main() {");
assert.notEqual(mainOffset, -1, "gcp-cache.sh should keep a main function");
const mainBody = cacheScript.slice(mainOffset, cacheScript.indexOf("case \"${1:-}\"", mainOffset));
assert.match(
  mainBody,
  /ensure_gcs_auth/,
  "gcp-cache.sh should authenticate before restore/save dispatch",
);

const commonSetup = shellFunction(fullCiScript, "run_common_setup", "gcp-full-ci.sh");
assert.match(
  commonSetup,
  /ensure_gcs_auth/,
  "gcp-full-ci.sh should authenticate before suite-level gsutil transfers",
);

console.log("test-gcp-cache-auth: ok");
