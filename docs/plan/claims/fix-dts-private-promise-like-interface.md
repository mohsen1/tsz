# fix-dts-private-promise-like-interface

Branch: `fix-dts-private-promise-like-interface`

Status: ready

## Claim

Fix `declarationEmitPrivatePromiseLikeInterface` by eliding trailing type
application arguments that match declared type parameter defaults during
declaration type printing.

## Verification

- `cargo fmt --package tsz-emitter -- --check`
- `cargo clippy -p tsz-emitter --lib -- -D warnings`
- `cargo test --package tsz-emitter test_type_application_elides_trailing_default_type_argument --lib`
- `./scripts/emit/run.sh --dts-only --filter=declarationEmitPrivatePromiseLikeInterface --verbose --concurrency=1 --timeout=30000`
