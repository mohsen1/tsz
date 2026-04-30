# fix(checker): emit TS2448/TS2450 for method and parameter decorators

- **Date**: 2026-04-30
- **Branch**: `claude/exciting-keller-Rijxw`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`is_class_or_enum_used_before_declaration` in `crates/tsz-checker/src/flow/flow_analysis/tdz.rs`
bails out early when it encounters a `METHOD_DECLARATION` (function-like node) while walking up
the AST from a usage site. This prevents TS2448/TS2450 from being emitted when a `const`/`let`
variable or `enum` is referenced inside a method or parameter decorator before its declaration.

The fix adds an `in_decorator` flag to the walk loop. When the walk passes through a `DECORATOR`
node, the flag prevents the method-boundary bail-out — decorator arguments execute immediately at
class definition time, not deferred through the method boundary.

Fixes `decoratorUsedBeforeDeclaration.ts` (fingerprint-only → pass).

## Files Touched

- `crates/tsz-checker/src/flow/flow_analysis/tdz.rs` — `in_decorator` flag to prevent method boundary bail-out
- `crates/tsz-checker/src/types/computation/call_helpers.rs` — `decorated_class_member_owner_kind` same fix for TS2454 companion suppression
- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs` — TS7006 anchor at first decorator for decorated parameters
- `crates/tsz-checker/Cargo.toml` — register new test target
- `crates/tsz-checker/tests/decorator_method_tdz_tests.rs` — 10 unit tests
