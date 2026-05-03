---
name: parser declarator-list missing-comma recovery for literal-value initializer errors
status: claimed
timestamp: 2026-05-03 16:55:00
branch: fix/parser-decl-list-recover-after-initializer-value-error
---

# Claim

Workstream 1 (Diagnostic Conformance) — fingerprint parity for TS1005
recovery message after legacy octal / leading-zero / bigint-form errors
inside variable declarator initializers.

## Problem

Given:

```ts
{ const legacyOct = 0123n; }
```

tsc emits:
- `TS1121` "Octal literals are not allowed." at the `0` of `0123`
- `TS1005` "',' expected." at the `n`

(The recovery treats the input as `const legacyOct = 0123, n;`.)

tsz emits TS1121 correctly but reports `TS1005` "';' expected." at the
`n` instead of "',' expected.".

Root cause:

1. `scan_legacy_octal_number` returns `0123` as a complete numeric
   literal token and **does not** call
   `check_for_identifier_start_after_numeric_literal`, so `n` stays
   as a separate identifier token (no TS1353 emitted).
2. The parser's literal handler emits `TS1121` while parsing the
   initializer. That bumps `parse_diagnostics.len()`, so
   `decl_had_error` is `true` after `parse_variable_declaration_with_flags`
   returns — even though the declarator is structurally complete.
3. The early-break at `state_statements.rs:2018` (intended for
   "name itself is malformed" cases like `const export`) fires
   because `decl_had_error` is true.
4. `parse_semicolon` then reports "';' expected." at `n`.

## Fix

Narrow the early-break: when the only diagnostics added during the
declarator parse are **numeric-literal-value** errors (TS1121, TS1125,
TS1177, TS1178, TS1351, TS1352, TS1353, TS1489, TS6188, TS6189), the
declarator's structure is intact — only the literal's value is illegal.
If the next token can start a new declarator (identifier/keyword or
`{`/`[`), let control fall through to the existing `can_continue`
branch, which emits "',' expected." and treats the next token as a
new declarator.

Codes in the carve-out are exclusively literal-value diagnostics from
the scanner/literal-parser path; they don't indicate a structurally
broken expression. Other parser diagnostics (e.g., `'=>' expected.`
from arrow-head recovery in `(x: T).props`) keep the original
break-and-emit-semicolon behavior.

## Tests

- New: `legacy_octal_with_bigint_suffix_recovers_with_comma_expected`
  in `crates/tsz-parser/tests/state_expression_tests.rs`. Asserts:
  - TS1121 emitted for `0123n`
  - TS1005 with "',' expected." present
  - TS1005 with "';' expected." absent
- New: `legacy_octal_with_bigint_suffix_recovers_param_name_independent`
  with a different binding name (`arbitraryName`/`0567n`) — locks the
  rule as structural per anti-hardcoding directive.
- Existing tests verified: 763/763 parser tests pass, including
  `test_typed_parenthesized_expression_followed_by_property_access_prefers_missing_arrow`
  (which exercises the `decl_had_error` early-break on a
  non-literal-value error).
- Crate suite: 6862/6862 parser+checker tests pass.

## Conformance impact

`parseBigInt.ts` flips PASS (single TS1005 fingerprint mismatch
resolved). Targeted run via
`./scripts/conformance/conformance.sh run --filter "parseBigInt"`
shows `1/1 passed`. Full conformance run pending.
