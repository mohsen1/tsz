# fix(checker): report TS18045 for accessor below ES2015

- **Date**: 2026-05-13
- **Branch**: `fix-accessor-modifier-es5-18045-6291-20260513`
- **PR**: #6296
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance / target feature checks)

## Intent

Issue #6291 reports that `accessor` class properties are accepted when targeting ES5, while tsc emits TS18045. The fix should use existing class-member declaration validation and add a focused CLI/checker regression without changing ES2015+ behavior.

After pulling latest `main`, the runtime behavior was already fixed by existing target feature validation. This PR records the missing regression so the issue stays closed.

## Files Touched

- `crates/tsz-cli/tests/tsc_compat_tests.rs`
- `docs/plan/claims/fix-accessor-modifier-es5-18045-6291-20260513.md`

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --target es5 --ignoreDeprecations 6.0 --pretty false /tmp/issue6291.ts` emitted one TS18045 as expected (exit 2 due diagnostics).
- `cargo test -p tsz-cli --test tsc_compat_tests accessor_modifier_below_es2015_reports_ts18045 -- --nocapture`
- `cargo fmt --all -- --check`
