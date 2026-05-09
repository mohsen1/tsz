Status: ready
Branch: fix-dts-overloaded-private-inference
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for `declarationEmitOverloadedPrivateInference`, where a
public class property initialized from private overloaded method calls emitted
`any` members instead of the selected overload return types.

## Planned Scope

- Declaration emitter call-return reuse for source overload method signatures.
- Function-typed generic argument substitution for simple callable parameters.
- Object-literal class property fallback when checker output preserves `any`
  member types.
- Focused regression coverage for the private overloaded method initializer.

## Verification Plan

- `cargo fmt --package tsz-emitter -- --check`
- `cargo clippy -p tsz-emitter --lib -- -D warnings`
- `cargo test -p tsz-emitter test_private_overloaded_method_initializer_reuses_matching_signature_return_type --lib`
- `cargo test -p tsz-emitter test_overloaded_call_initializer_does_not_use_first_signature_return_type --lib`
- `./scripts/emit/run.sh --dts-only --filter=declarationEmitOverloadedPrivateInference --verbose --concurrency=1 --timeout=30000`
