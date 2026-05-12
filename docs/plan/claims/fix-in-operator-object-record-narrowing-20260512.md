# fix(checker): preserve object record narrowing after `in`

- **Date**: 2026-05-12
- **Branch**: `fix-in-operator-object-record-narrowing-20260512`
- **Issue**: #5970
- **Status**: ready
- **Workstream**: 1 (diagnostic conformance / false-positive narrowing)

## Intent

Make `tsz` match `tsc` when an `unknown` value is narrowed through
`typeof x === "object"`, `x !== null`, and a string-literal `in` check. The
positive `in` branch should preserve an object-with-property shape so
subsequent property access does not see `never`.

## Final Scope

- Mark all already-flow-narrowed identifier reads so `get_type_of_node` does
  not apply a second flow pass to the narrowed result.
- Add regression coverage for `unknown` narrowed through
  `typeof x === "object" && x !== null && "foo" in x`.

## Verification

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test in_chain_narrows_unconstrained_type_param_tests -- --nocapture`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- `git diff --check`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual #5970 repro: `./scripts/node_modules/.bin/tsc --noEmit --strict --lib ES2020 in_unknown.ts` and `.target/release/tsz --noEmit --strict --lib ES2020 in_unknown.ts` both exit 0.
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib`
