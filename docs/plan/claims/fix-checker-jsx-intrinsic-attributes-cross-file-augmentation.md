---
name: JSX IntrinsicAttributes lookup across cross-file global augmentations
status: claimed
timestamp: 2026-05-03 08:12:52
branch: fix/checker-jsx-intrinsic-attributes-cross-file-augmentation
---

# Claim

Workstream 1 (Diagnostic Conformance) — JSX SFC TS2322 target display includes the
`IntrinsicAttributes &` prefix when `JSX.IntrinsicAttributes` is declared in a
cross-file global augmentation (e.g. `react16.d.ts`'s `declare global { namespace
JSX { interface IntrinsicAttributes extends React.Attributes {} } }`).

## Problem

`get_intrinsic_attributes_lazy_type` looked up `IntrinsicAttributes` only in the
exports of the *first* JSX namespace symbol that
`get_jsx_namespace_type` returned. For modules that augment `JSX` locally with
`IntrinsicElements` only (e.g. `jsxElementType.tsx`'s `declare global { namespace
JSX { interface IntrinsicElements { ... } } }`) but rely on the lib's react16.d.ts
to provide `IntrinsicAttributes`, the local symbol's `exports` map carried only
`IntrinsicElements`. The lib augmentation's symbol was a separate symbol id and
its exports were never consulted, so the SFC TS2322 target dropped the
`IntrinsicAttributes &` prefix.

## Fix

Route `get_intrinsic_attributes_lazy_type` through the existing
`get_jsx_namespace_export_symbol_id` helper, which already walks all global
augmentations in the local binder, every cross-file binder, and every lib binder
to find the named export. This mirrors how `IntrinsicElements` lookup works on
the orchestration path.

Tighten `build_jsx_display_target_with_preferred_props` to skip the
`IntrinsicAttributes &` prefix when `component_type` is `None` — that is, the
intrinsic-element path (`<a:b ... />`). tsc only wraps `IntrinsicAttributes &`
on SFC and class component targets; intrinsic elements show the props type
directly. Without this guard, fixing the lookup above incorrectly added the
prefix to intrinsic-element TS2322 messages.

## Tests

- New: `jsx_sfc_excess_property_target_includes_intrinsic_attributes_prefix`
  in `crates/tsz-checker/src/checkers_domain/jsx/tests.rs`
- All 235 existing JSX tests pass, including
  `jsx_sfc_free_type_param_no_props_reports_plain_type_param_target` (no
  prefix when target is just a bare type parameter) and
  `jsx_sfc_free_type_param_spread_reports_intrinsic_attrs_target` (prefix
  when component target is a spread of an unconstrained type parameter).

## Conformance impact

Net **+3** vs current main — `objectCreate-errors.ts` is shadowed by PR #2493
(also in flight); the JSX changes here flip the SFC-prefix fingerprints in
`jsxElementType.tsx` (no longer in the fingerprint mismatch list) and produce
3 incidental wins (`strictOptionalProperties3.ts`, `tsxAttributeResolution6.tsx`,
`typeFromParamTagForFunction.ts`). Zero regressions.
