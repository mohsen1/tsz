# fix(checker): suppress JSX pragma namespace circular alias

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-pragma-namespace-cycle`
- **PR**: https://github.com/mohsen1/tsz/pull/3398
- **Status**: draft
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the conformance mismatch in
`TypeScript/tests/cases/compiler/jsxNamespaceImplicitImportJSXNamespaceFromPragmaPickedOverGlobalOne.tsx`.
The current fingerprint has the expected duplicate identifier diagnostic but
also reports an extra `TS2456` circular type alias diagnostic. The fix will
identify why the JSX pragma namespace path is treated as an alias cycle and
suppress only the false circularity report.

## Files Touched

- TBD

## Verification

- Pending
