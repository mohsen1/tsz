#!/usr/bin/env node
import assert from "node:assert/strict";

import { benchReadinessMessages } from "./bench-readiness-banner.mjs";

assert.deepEqual(benchReadinessMessages(null), []);

assert.match(
  benchReadinessMessages({ artifact_absent: true }).join(" "),
  /No recent benchmark artifact/,
);

assert.match(
  benchReadinessMessages({ missing: 2 }).join(" "),
  /missing 2 required row\(s\)/,
);

assert.match(
  benchReadinessMessages({
    duplicate_rows: [{ name: "utility-types-project", count: 2 }],
  }).join(" "),
  /duplicate required row\(s\)/,
);

assert.match(
  benchReadinessMessages({
    source_freshness: {
      current: false,
      warning: "source abc123 differs from expected def456",
    },
  }).join(" "),
  /not current release truth/,
);

assert.match(
  benchReadinessMessages({
    metadata_clean: false,
    metadata_warnings_total: 3,
  }).join(" "),
  /metadata has 3 warning\(s\)/,
);

assert.doesNotMatch(
  benchReadinessMessages({
    metadata_clean: true,
    metadata_warnings_total: 0,
    source_freshness: { current: true },
  }).join(" "),
  /warning|stale|missing|duplicate/i,
);

console.log("bench readiness banner tests passed");
