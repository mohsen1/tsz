# codex/followup-1779-method-variance-review-20260513

Status: ready for review

Scope:
- Address Devin review feedback from #1779 for method parameter traversal when `suppress_method_bivariance` is true.
- Keep method parameters independent in indexed-access/bivariance-hack traversal while preserving bivariant traversal for ordinary method signatures.

Verification plan:
- `cargo fmt --check`
- Targeted solver/checker variance tests, then broader unit checks if the patch is non-trivial.

Verification:
- `cargo fmt --check` passed.
- `cargo nextest run -p tsz-checker --test promise_callback_variance_tests --no-fail-fast` passed.
- Draft PR CI passed lint, unit, and dist-binaries.
- `CARGO_INCREMENTAL=0 scripts/safe-run.sh cargo nextest run --workspace --all-targets --no-fail-fast` completed with existing unrelated local failures outside this test-only diff: 27032 passed, 38 failed, 1 timed out.
