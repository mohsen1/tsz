# Fix JSX same-name component/interface stack overflow

- **Date**: 2026-05-13
- **Branch**: `fix-jsx-same-name-component-stack-6262`
- **PR**: #6268
- **Status**: ready
- **Workstream**: checker crash / urgent public issue #6262

## Intent

Close #6262 by preventing stack overflow when a JSX function component shares
its name with its props interface. The slice will add a crash regression for the
reported TSX pattern, then fix the narrow recursion path in JSX/component type
resolution without changing normal JSX prop checking.

## Files Touched

- `docs/plan/claims/fix-jsx-same-name-component-stack-6262.md`
- `crates/tsz-checker/src/checkers/jsx/diagnostics.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Result

The crash was in JSX diagnostic display text, not semantic props resolution.
When a component declaration and props interface share a name, diagnostic
annotation lookup could resolve the parameter type reference back to the same
function declaration and recurse indefinitely. The fix adds a visited-node guard
for this diagnostic-only lookup path and keeps JSX prop checking unchanged.

## Verification

- `cargo test -p tsz-checker --test jsx_component_attribute_tests jsx_function_component_same_name_as_props_interface_does_not_recurse -- --nocapture` (pass)
- `cargo test -p tsz-checker --test jsx_component_attribute_tests -- --nocapture` (184 passed, 1 ignored)
- `cargo fmt --all -- --check` (pass)
- `git diff --check` (pass)
