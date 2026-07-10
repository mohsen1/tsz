# Vendored TypeScript test fixtures

Byte-identical copies of the small fixture files that tsz unit/integration
tests read from the TypeScript checkout, vendored so those tests run — and
produce the same result — in every environment, including checkouts where the
`TypeScript/` submodule is absent (issue #15685: tests that silently skipped
when a fixture was missing made *which* sibling fails depend on the host).

- Upstream: <https://github.com/microsoft/TypeScript>
- Pinned ref: the commit recorded in `scripts/ci/typescript-submodule-ref`
- License: Apache-2.0 (see `ThirdPartyNoticeText`/LICENSE in the upstream repo)

Layout mirrors the submodule (`tests/lib/...`, `tests/cases/...`), so
`tsz_checker::test_utils::load_typescript_fixture` resolves the same relative
path against `vendor/TypeScript/` first and the real `TypeScript/` checkout as
a fallback.

## Refreshing

When `scripts/ci/typescript-submodule-ref` is bumped, re-copy each file here
from the checkout at the new ref, e.g.:

```bash
for f in $(cd vendor/TypeScript && find tests -type f); do
  cp "TypeScript/$f" "vendor/TypeScript/$f"
done
```

The drift guard in
`crates/tsz-checker/tests/vendored_fixture_drift_tests.rs` fails the unit
suite if a vendored copy differs from the pinned checkout (it runs wherever
the checkout at the pinned ref is present — always true in the `unit` CI job).
