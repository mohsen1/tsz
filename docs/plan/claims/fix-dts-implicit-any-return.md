Status: ready
Branch: fix-dts-implicit-any-return
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for returned locals explicitly annotated as `any`, targeting
`implicitAnyAnyReturningFunction`.

## Scope

- Preserve a returned identifier's declared `any` annotation when selecting a
  source return type for declaration emit.
- Keep the change in the shared preferred-return helper so functions and class
  methods agree with tsc.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=implicitAnyAnyReturningFunction --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-implicit-any-return-rebased.json`
