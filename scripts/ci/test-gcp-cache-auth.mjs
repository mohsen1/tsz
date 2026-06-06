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

const authFn = cacheScript.match(/ensure_gcs_auth\(\) \{[\s\S]+?\n\}/);
assert.ok(authFn, "gcp-cache.sh should define ensure_gcs_auth");
assert.match(
  authFn[0],
  /SCCACHE_GCS_KEY_JSON[\s\S]+GOOGLE_APPLICATION_CREDENTIALS/,
  "GCS cache auth should accept the same service-account secret used by sccache",
);
assert.match(
  authFn[0],
  /gcloud auth print-access-token[\s\S]+pass_credentials_to_gsutil true/,
  "GCS cache auth should let gsutil use an already-authenticated gcloud account",
);

const mainOffset = cacheScript.indexOf("main() {");
assert.notEqual(mainOffset, -1, "gcp-cache.sh should keep a main function");
const mainBody = cacheScript.slice(mainOffset, cacheScript.indexOf("case \"${1:-}\"", mainOffset));
assert.match(
  mainBody,
  /ensure_gcs_auth/,
  "gcp-cache.sh should authenticate before restore/save dispatch",
);

console.log("test-gcp-cache-auth: ok");
