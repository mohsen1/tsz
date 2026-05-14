# fix(checker): preserve constrained mixin instance members in class bodies

- **Date**: 2026-05-14
- **Branch**: `codex/issue-6905-mixin-this-constraint-20260514`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance - class/mixin `this` typing

## Intent

Fix #6905, where a returned class extending a generic constructor constrained as `TBase extends Constructor<Nameable>` loses the instance members guaranteed by the constructor constraint. The expected behavior is that `this.name` inside the returned class body is typed as `string`, matching `tsc` and avoiding a false TS2532 diagnostic.

## Files Touched

- TBD

## Verification

- Pending targeted regression.
