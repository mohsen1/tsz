---
name: Async generator class method parser extras
status: ready
timestamp: 2026-05-06 12:06:28
branch: fix/conformance-next-20260506-120628
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/conformance/parser/ecmascript2018/asyncGenerators/parser.asyncGenerators.classMethods.es2018.ts`.

## Scope

Remove the extra TS1212/TS1213 parser diagnostics while preserving the expected
TS1005, TS1109, and TS5024 output for async generator class method fixtures.

## Verification Plan

- Focused parser/checker unit coverage in the owning area.
- `cargo nextest run` for affected tests.
- `./scripts/conformance/conformance.sh run --filter "parser.asyncGenerators.classMethods.es2018" --verbose`

## Verification

- `cargo nextest run -p tsz-checker --test class_reserved_word_diagnostics_tests -E 'test(class_reserved_word_diagnostics_match_strict_class_context)'`
- `cargo nextest run -p tsz-checker --test conformance_issues -E 'test(test_ts7006_reserved_word_parameter_in_generator_strict_mode)'`
- `./scripts/conformance/conformance.sh run --filter "parser.asyncGenerators.classMethods.es2018" --verbose` -> 1/1 passed
