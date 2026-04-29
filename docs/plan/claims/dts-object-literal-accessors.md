# fix(dts): skip synthetic anonymous accessor containers

- **Branch**: `codex/dts-next-impact-20260429`
- **Workstream**: Workstream 2 - Declaration emit pass rate
- **Status**: ready
- **Created**: 2026-04-29 06:43:27 UTC

## Intent

Fix declaration emit for object literals with accessor members where tsz currently
prints a synthetic anonymous `: { ... }` member inside the generated object
type.

## Scope

- Reproduce and fix `declFileObjectLiteralWithAccessors`.
- Check whether the same path covers setter-only object-literal declaration
  emit failures.
- Keep the fix in declaration-emitter member serialization and type-text
  normalization rather than adding baseline-specific filtering.

## Verification

- `./scripts/emit/run.sh --filter=declFileObjectLiteralWithAccessors --dts-only --skip-build`
- Relevant focused setter-only/accessor DTS runs if the fix touches the shared path.

## Result

- `declFileObjectLiteralWithAccessors` now passes for ES5 and ES2015.
- `declFileObjectLiteralWithOnlySetter` remains passing for ES5 and ES2015.
