# Fix generic type argument constraint diagnostics (#6339)

Status: ready
PR: #6345

## Scope

Investigate and fix missing TS2344 diagnostics when explicit type arguments violate constrained generic type parameters, starting with generic type alias instantiation.

## Assumptions

- #6340 overlaps the existing `fix-mapped-type-as-clauses-fingerprint` claim, so this slice focuses on #6339 instead.
- The first PR-sized target is explicit type alias/type reference constraint checking; broader class/interface coverage will be included only if it falls out naturally from the same code path.

## Verification plan

- Reproduce #6339 with a focused CLI case against `tsz` and `tsc`.
- Add focused regression coverage for TS2344 on invalid explicit type arguments.
- Run the targeted test and formatting check.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6339.ts` now reports TS2344 for explicit `unknown`, `string`, and object type arguments.
- `cargo test -p tsz-cli --test tsc_compat_tests explicit_type_arguments_violate_function_constraint -- --nocapture` passed.
- `cargo fmt --all -- --check` passed.
