#!/usr/bin/env bash
# Apply tsz-specific patches to the compiled TypeScript test harness.
#
# These patches fix issues in TypeScript's test infrastructure client
# (client.js) that prevent certain fourslash tests from passing correctly
# against non-tsc servers.
set -euo pipefail

TS_DIR="${1:?Usage: apply-harness-patches.sh <TypeScript-dir>}"
CLIENT_JS="$TS_DIR/built/local/harness/client.js"

if [[ ! -f "$CLIENT_JS" ]]; then
    echo "  Warning: $CLIENT_JS not found, skipping patches"
    exit 0
fi

# ---------------------------------------------------------------------------
# Patch 1: Fix rename triggerSpan decoding
#
# TypeScript's SessionClient.getRenameInfo() hardcodes triggerSpan to
# createTextSpanFromBounds(position, position) which produces a zero-length
# span. The server sends the correct triggerSpan in the response body, but
# the client ignores it. This patch reads the triggerSpan from the response.
# ---------------------------------------------------------------------------
if ! grep -q 'body.info.triggerSpan && body.info.triggerSpan.length' "$CLIENT_JS"; then
    node -e "
const fs = require('fs');
let src = fs.readFileSync('$CLIENT_JS', 'utf-8');
const old = '(0, ts_js_1.createTextSpanFromBounds)(position, position)';
const replacement = \`(body.info.triggerSpan && body.info.triggerSpan.length
                    ? (0, ts_js_1.createTextSpanFromBounds)(
                        this.lineOffsetToPosition(fileName, body.info.triggerSpan.start, this.getLineMap(fileName)),
                        this.lineOffsetToPosition(fileName, body.info.triggerSpan.start, this.getLineMap(fileName)) + body.info.triggerSpan.length,
                    )
                    : (0, ts_js_1.createTextSpanFromBounds)(position, position))\`;
src = src.replace(old, replacement);
fs.writeFileSync('$CLIENT_JS', src);
"
    echo "  Applied: fix-rename-trigger-span"
fi
