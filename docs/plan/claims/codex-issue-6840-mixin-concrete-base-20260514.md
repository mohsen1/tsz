# Claim: issue 6840 abstract mixin applied to concrete base

Status: ready
Owner: codex
Issue: #6840
Branch: codex/issue-6840-mixin-concrete-base-20260514

## Scope

Fix the TS2511 false positive where an abstract mixin class returned from a function remains marked abstract after the mixin is applied to a concrete base constructor.

## Plan

- Add a focused TS2511 regression for the issue repro.
- Patch checker class/mixin return refinement so the instantiated result is concrete when the supplied base constructor is concrete and no abstract members remain.
- Run the smallest targeted checker test covering the regression.

## Status

Ready 2026-05-14.

Validation:
- cargo test -p tsz-checker --test conformance_issues test_abstract_mixin_applied_to_concrete_base_instantiation -- --nocapture (passed)
- cargo test -p tsz-checker --test conformance_issues test_mixed_constructor_unions_still_report_ts2511 -- --nocapture (passed)
- cargo test -p tsz-checker --test conformance_issues test_abstract_class_union_instantiation_shape_reports_all_ts2511s_with_libs -- --nocapture (passed)
