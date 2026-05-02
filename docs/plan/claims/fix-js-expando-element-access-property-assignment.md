# fix(checker): infer JS expando element-access assignments

- **Date**: 2026-05-02
- **Branch**: `fix/js-expando-element-access-property-assignment`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Checked JavaScript should infer expando object shapes from simple string-literal element-access assignments, matching tsc's `typeFromPropertyAssignment39.ts` behavior. tsz currently records dot-property expando writes but misses assignments such as `foo["baz"] = {}`, so a later nested read like `foo["baz"]["blah"]` emits TS7053. This PR extends the existing expando key extraction path to treat literal bracket keys the same as dot keys when the chain is statically nameable.

## Files Touched

- `crates/tsz-checker/src/types/property_access_helpers/expando.rs`
- `crates/tsz-checker/tests/...` (targeted regression test)

## Verification

- Planned: targeted checker regression test
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter typeFromPropertyAssignment39 --verbose`
