# codex/dts-index-signatures

Date: 2026-04-29
Branch: `codex/dts-index-signatures`
PR: TBD
Status: verified locally; PR pending

## Workstream

Workstream 2: declaration emit parity.

## Intent

Fix emit runner DTS baseline parsing for TypeScript baselines that include `!!!! File X differs from original emit in noCheck emit`. Those repeated output-file sections contain unified diff metadata, not the expected full-check `.d.ts`, and were inflating DTS failures such as `indexSignatures1` from real emitter diffs into large parser artifacts.

## Files Touched

- `scripts/emit/src/baseline-parser.ts`

## Verification

- `../node_modules/.bin/tsc -p tsconfig.json` in `scripts/emit` passed.
- Parser assertion passed for `indexSignatures1`, `deferredLookupTypeResolution`, and `variadicTuples1`: expected DTS no longer includes the noCheck diff marker or unified diff body.
- `./scripts/emit/run.sh --filter=indexSignatures1 --dts-only --skip-build --json-out=/tmp/indexSignatures1.parser-fix.json` now compares against the real full-check `.d.ts`; the test still fails on remaining emitter/type-inference differences rather than baseline metadata.
