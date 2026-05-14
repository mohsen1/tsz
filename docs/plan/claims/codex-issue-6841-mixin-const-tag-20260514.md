# Claim: issue 6841 mixin const tag inference

Status: ready
Owner: Codex
Branch: codex/issue-6841-mixin-const-tag-20260514
PR: #6877
Issue: #6841

## Scope
Add focused regression coverage and, if needed, fix the TS2322 false positive where a mixin return class property keeps an inferred `TTag` type parameter instead of the literal supplied with `as const`.

## Verification
- `bash -lc 'cargo test -p tsz-checker --test ts2322_tests mixin_inferred_const_literal_tag_substitutes_return_class_property -- --nocapture & pid=$!; for i in {1..45}; do if ! kill -0 "$pid" 2>/dev/null; then wait "$pid"; exit $?; fi; sleep 1; done; kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; echo "TIMEOUT after 45s"; exit 124'` (passed)
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_no_false_positive_const_type_param_multi -- --nocapture` (passed)
