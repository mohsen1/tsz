# Fix JSX same-name component/interface stack overflow

- **Date**: 2026-05-13
- **Branch**: `fix-jsx-same-name-component-stack-6262`
- **PR**: #6268
- **Status**: implemented
- **Workstream**: checker crash / urgent public issue #6262

## Intent

Close #6262 by preventing stack overflow when a JSX function component shares
its name with its props interface. The slice will add a crash regression for the
reported TSX pattern, then fix the narrow recursion path in JSX/component type
resolution without changing normal JSX prop checking.

## Root Cause

JSX child diagnostics ask for the declared `children` prop annotation so TS274x
messages can preserve source spelling. For a merged same-name component/type
symbol, resolving the props type reference (`Fragment`) can enumerate the
function component declaration before the interface declaration. The diagnostic
walker then re-enters the function's first parameter annotation, resolves the
same type reference again, and recurses until the checker thread overflows.

## Implementation

- Added an active declaration set to the JSX prop annotation/declaration
  walkers. Recursive expansions of a type reference now skip declarations
  already on the diagnostic-walker stack while still allowing the same merged
  symbol to fall through to the actual interface/type declaration.
- Added a focused JSX checker regression for a function component and props
  interface that share the same name and accept multiple JSX body children.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/diagnostics.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`
- `docs/plan/claims/fix-jsx-same-name-component-stack-6262.md`

## Verification

- `cargo fmt --check` - passed
- `cargo test -p tsz-checker --lib jsx_same_name_function_component_and_props_interface_does_not_recurse -- --nocapture` - passed
- `cargo test -p tsz-checker --test jsx_component_attribute_tests jsx_children_diagnostics_keep_declared_children_display_through_intrinsic_intersection -- --nocapture` - passed
- `cargo test -p tsz-checker --test jsx_component_attribute_tests jsx_react_multiple_render_prop_children_ts2322_message_preserves_react_child_alias -- --nocapture` - passed
- `cargo run -q -p tsz-cli --bin tsz -- --noEmit --jsx preserve /tmp/tsz-jsx-same-name-lldb-repro.tsx` - no crash; exited 2 with existing TS7026 diagnostics for module-local `declare namespace JSX`
- `cargo run -q -p tsz-cli --bin tsz -- --noEmit --jsx preserve /tmp/tsz-jsx-same-name-noexport.tsx` - passed
- `cargo clippy -p tsz-checker --lib -- -D warnings` - passed
