# picker: print test source after random pick

- **Date**: 2026-05-04
- **Branch**: `claude/brave-thompson-WYft0`
- **PR**: TBD
- **Status**: ready
- **Workstream**: tooling (conformance picker)

## Intent

Extend the canonical conformance picker (`scripts/session/pick.py`) with a
`--show-source` flag on the `quick` and `category` subcommands so the next
agent picking a random failure can see the test body inline without a
separate `cat` step. Per `.claude/CLAUDE.md` and
`scripts/session/conformance-agent-prompt.md`, no new picker scripts are
allowed; this extends the shared picker behind the existing
`scripts/session/quick-pick.sh` wrapper instead of forking yet another
`random-*.sh` script.

Also fixes a path-resolution bug in `command_show`: the snapshot stores the
absolute path from the machine that produced it (e.g.
`/tmp/tsz-snap-refresh/TypeScript/...`), so `root / failure.path` returned a
non-existent location on every other machine. The new `resolve_test_source`
helper anchors on the `TypeScript/` segment of the snapshot path and falls
back to the local submodule.

## Files Touched

- `scripts/session/pick.py` — new `resolve_test_source` / `print_test_source`
  helpers, `--show-source` flag wired into `command_quick` / `command_category`,
  `command_show` refactored to use the shared helper.
- `scripts/session/quick-pick.sh` — usage comment now lists `--show-source`.
- `scripts/session/conformance-agent-prompt.md` — adds `--show-source` example
  alongside the existing `--seed` / `--code` / `--run` examples.
- `scripts/session/test_pick.py` — new behavior-lock unit tests for
  `resolve_test_source` covering the three input shapes the snapshot can
  produce (snapshot-absolute, repo-relative, locally-absolute) plus the
  missing-file fallback.

## Notes on `genericRestParameters3.ts`

The first random pick from this session was
`conformance/types/rest/genericRestParameters3.ts` (fingerprint-only). After
extended investigation, the test's failure spans three independent solver /
printer gaps:

1. **Tuple-rest of tuple-union** is not handled in the call dispatcher: when
   the rest parameter's type is `[string] | [number, boolean]` (a union of
   non-prefix-aligned tuples), `unpack_tuple_rest_parameter` returns the
   parameter unchanged, and the dispatcher falls back to per-element checks
   against `rest_argument_element_type(...)`, which yields the position-0
   union (`string | number`) rather than the tuple-union itself. Spreading a
   value of the same union type then raises a false-positive TS2345 with
   message `'string | number | boolean' is not assignable to 'string | number'`.
2. **Function-to-function compatibility with tuple-rest** has the relation
   inverted in this corner: tsz reports the error on `f2 = f1` / `f3 = f1`
   (which should be valid) and stays silent on `f1 = f2` / `f1 = f3` (which
   should error).
3. **Source-form preservation in type-parameter constraints** — tsc displays
   `<T extends any[]>` when the source uses `any[]` and `<T extends Array<X>>`
   when the source uses `Array<X>`. tsz unconditionally forces `Array<X>`
   form via `preserve_array_generic_form = true` in
   `crates/tsz-solver/src/diagnostics/format/compound.rs:319-347`, which
   passes 6 tests but breaks the one in `genericRestParameters3.ts`. A
   correct fix needs source-form tracking on the type parameter info.

Each of these is a multi-day fix that warrants its own claim and PR. None is
in scope for this picker-tooling claim.

## Verification

- `python3 -m unittest scripts.session.test_pick` (4/4 pass)
- `scripts/session/quick-pick.sh --seed 42 --show-source` prints the picked
  test's source body from the local TypeScript submodule.
- `cargo fmt --all --check` (clean)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` (clean)
- `scripts/session/verify-all.sh --quick` (no Rust changes; expected
  conformance/emit/test deltas are all zero).
