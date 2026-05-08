---
status: WIP
issue: 3213
agent: claude (auto-loop)
started: 2026-05-08 05:06:23 UTC
---

# JSDoc indexed access bypasses missing-property diagnostics (#3213)

## Problem
`/** @type {import("./dep")["Foo"]} */` against a module that only
exports `Bar` previously produced no diagnostic at the JSDoc site. The
unresolved IndexAccess type was constructed anyway and surfaced as a
downstream TS2322 against the printed form `typeof import("dep")["Foo"]`.
tsc reports TS2339 at the JSDoc type expression itself.

## Fix
In `jsdoc_type_from_expression`, after parsing
`base_str["index_str"]`:

1. **`import(...)` base, string-literal index.** Resolve the imported
   member via `resolve_jsdoc_import_member`. If the member doesn't
   exist, emit TS2339 and return `TypeId::ERROR` instead of building
   the IndexAccess. The `import("./dep")` form does not round-trip
   through `resolve_jsdoc_type_str` for ESM-only imports (no
   commonjs-style module value type exists), so we cannot reach the
   property check below for that case.
2. **Other base, string-literal index.** Resolve both sides through
   `resolve_jsdoc_type_str`, then call
   `resolve_property_access`. If the property is `PropertyNotFound`,
   emit TS2339 and return `ERROR`.

A small `strip_quoted_string` helper handles either `"..."` or
`'...'` index quoting.

## Out of scope
- The diagnostic anchors at the JSDoc node position, not at the start
  of the index expression inside the comment (`(3,1)` vs tsc's
  `(3,28)`). Improving the anchor needs `jsdoc_type_expression_span_for_node`
  threading; left for follow-up.
- The diagnostic fires twice per `@type` annotation because
  `jsdoc_type_from_expression` is invoked from multiple call sites
  (e.g. type lookup + flow narrowing). The existing
  `diagnostic_dedup_key` only merges identical positions; with
  different positions the duplicate slips through. Both are
  fingerprint-quality issues, not code-set bugs.

## Files
- `crates/tsz-checker/src/jsdoc/resolution/name_resolution.rs` —
  add the property/import-member lookup before constructing the
  IndexAccess.
