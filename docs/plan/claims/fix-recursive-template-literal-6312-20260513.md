# Fix recursive template literal widening (#6312)

Status: ready
PR: #6337

## Scope

Investigate and fix recursive template literal evaluation with string intrinsics where a concrete literal result widens to `string`.

## Summary

- `InferSubstitutor` now substitutes infer bindings through `StringIntrinsic` wrappers such as `Lowercase<infer L>` and `Capitalize<infer R>`.
- This lets recursive template literal branches evaluate `CamelCase<"hello_world">` to `"helloworld"` instead of leaving intrinsic-wrapped `infer` placeholders deferred.
- Added focused solver, checker, and CLI compatibility coverage for the recursive CamelCase repro from #6312.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/.target-tsz-pr6337-recursive-template-literal CARGO_BUILD_JOBS=2 cargo test -p tsz-solver recursive_template_literal_application -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/.target-tsz-pr6337-recursive-template-literal CARGO_BUILD_JOBS=2 cargo test -p tsz-checker --test conditional_infer_tests recursive_template_literal_with_string_intrinsics_resolves_to_literal -- --exact --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/.target-tsz-pr6337-recursive-template-literal CARGO_BUILD_JOBS=2 cargo test -p tsz-cli --test tsc_compat_tests recursive_template_literal_intrinsics_evaluate_to_specific_literal -- --exact --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/.target-tsz-pr6337-recursive-template-literal CARGO_BUILD_JOBS=2 cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false <temp issue #6312 repro>` produced the expected TS2322: `"anything"` is not assignable to `"helloworld"`.
