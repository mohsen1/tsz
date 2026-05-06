# cli: explicit false overrides true config values for plain boolean flags

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-q68yZ`
- **PR**: TBD
- **Status**: claim
- **Workstream**: CLI / config plumbing (issue #3861)

## Intent

`--flag false` for plain `bool` compiler-option flags (e.g. `--strict false`,
`--noEmit false`, `--noUnusedLocals false`) is currently dropped during
`preprocess_tsc_args`, so it cannot override a `true` value loaded from
`tsconfig.json`. This claim covers passing the explicit-false intent through
CLI preprocessing into the override pipeline so it overrides config, matching
`tsc` behavior, and reflecting the override in `--showConfig`.

## Files Touched

- `crates/tsz-cli/src/commands/args.rs` (hidden side-channel field)
- `crates/tsz-cli/src/bin/tsz.rs` (preprocess + showConfig wiring)
- `crates/tsz-cli/src/driver/core.rs` (apply explicit-false overrides)
- `crates/tsz-cli/src/bin/tsz/tests.rs` (preprocess unit tests)

## Verification

- `cargo nextest run -p tsz-cli`
- Manual: `--strict false`, `--noEmit false`, `--noUnusedLocals false` against
  configs that set the corresponding option to `true`.
