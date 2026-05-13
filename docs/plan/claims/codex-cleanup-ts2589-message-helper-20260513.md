# chore(checker-tests): reuse TS2589 diagnostic helper

Branch: `codex/cleanup-ts2589-message-helper-20260513`
PR: [#6122](https://github.com/mohsen1/tsz/pull/6122)
Status: blocked

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts2589_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2589_tests::)' --no-fail-fast`

## Blocker

PR CI is blocked by a deterministic current-base DTS emit gate failure outside
this cleanup's touched surface:

- CI emit: `DTS 1529/1669`, below snapshot floor `1531`
- Local DTS-only emit on this branch reproduces `1529/1669`
- New DTS failures versus `scripts/emit/emit-detail.json`: `constAssertions`,
  `correlatedUnions`, `deferredLookupTypeResolution`, `genericRestParameters3`,
  `jsDeclarationsDefaultsErr(target=es2015)`, and
  `jsDeclarationsDefaultsErr(target=es5)`
- Previously failing `genericRestParameters1` and `instantiationExpressions`
  now pass, so the net gate drop is smaller than the raw new-failure list
