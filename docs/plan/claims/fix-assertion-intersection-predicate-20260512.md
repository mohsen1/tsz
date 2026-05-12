# fix(checker): allow assertion predicates with narrowing intersections

- **Date**: 2026-05-12
- **Branch**: `fix-assertion-intersection-predicate-20260512`
- **Base**: `main`
- **Issue**: [#6082](https://github.com/mohsen1/tsz/issues/6082)
- **PR**: [#6084](https://github.com/mohsen1/tsz/pull/6084)
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` for assertion functions whose predicate narrows a
parameter to an intersection of the parameter type and a stricter object shape.
The predicate type should be accepted when it is assignable to the parameter
type.

## Final Scope

- Added a predicate-specific assignability helper for TS2677 checks that accepts
  intersection predicates when any intersection member is assignable to the
  asserted parameter type.
- Routed both assertion function declarations and assertion function type nodes
  through that helper, preserving the existing checker/type-database
  assignability paths at the call sites.
- Added focused regression coverage for assertion declarations and function type
  aliases using `asserts d is Data & { ... }`.
- Kept genuine widening predicates rejected.

## Verification

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker assertion_predicate -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #6082 repro comparison:
  - `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 "$repro"` exited 0
  - `.target/release/tsz --noEmit --strict --lib ES2020 "$repro"` exited 0
- `git diff --check`
