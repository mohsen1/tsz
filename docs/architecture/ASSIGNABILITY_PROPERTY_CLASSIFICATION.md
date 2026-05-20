# Assignability Property Classification Name Audit

**Status**: Audit for #7023
**Scope**: `crates/tsz-checker/src/query_boundaries/assignability.rs`

This document records the current property-name conversion sites used by
`classify_object_properties` and its local helpers. It is intentionally a
behavior-preserving audit: it identifies where the hot relation-failure path
still works through string names before any helper or representation changes
are introduced.

## Classification Path

`classify_object_properties` runs after assignability has already failed and
the checker asks the query boundary for a structured property-level reason. The
result feeds diagnostic classification, but the work itself sits on a relation
failure path and can run repeatedly during real checking.

Current name handling:

| Site | Conversion | Allocation | Path | Notes |
|---|---|---:|---|---|
| `classify_object_properties` source-property loop | `db.resolve_atom_ref(source_prop.name)` | No | Hot classification | Borrows each source property name, then probes both `target_property_names` and `target_props` by `&str`. |
| `collect_target_properties` object target branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Builds `HashMap<String, TypeId>` for target properties. |
| `collect_target_properties` union member branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Repeats the owned conversion for each union member property. First inserted type wins. |
| `collect_target_properties` intersection member branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Repeats the owned conversion for each intersection member property. First inserted type wins. |
| `collect_target_property_names` object target branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Builds a separate `HashSet<String>` even though `collect_target_properties` also builds string keys for the same target shape. |
| `collect_target_property_names` union member branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Repeats the owned conversion for each union member property. |
| `collect_target_property_names` intersection member branch | `db.resolve_atom(prop.name)` | Yes | Hot classification | Repeats the owned conversion for each intersection member property. |
| `shape_index_signature_accepts_property` number-index check | `db.resolve_atom_ref(source_prop.name)` | No | Hot classification, excess candidates only | Borrows the source property name only after symbol-named properties are excluded and a numeric index check is needed. |
| `is_global_object_or_function_shape` prototype-name check | `db.resolve_atom_ref(prop.name)` | No | Hot classification, target-shape screening | Borrows each target property name to compare against static Object/Function prototype name lists. |

## Hot Versus Diagnostic-Only Work

Hot classification work:

- `collect_target_property_names` and `collect_target_properties` are both
  called for every non-empty source shape that reaches property classification.
  They each walk object, union, and intersection target properties and currently
  allocate owned `String` keys from `Atom` names.
- The source-property loop is hot but borrowed: it resolves each source property
  name with `resolve_atom_ref` and probes the string-keyed collections.
- `shape_index_signature_accepts_property` is hot only for unmatched source
  properties that still need index-signature screening.
- `is_global_object_or_function_shape` is target screening on the same
  classification path. It does not allocate, but it performs repeated string
  comparisons against static prototype-name lists.

Diagnostic-only work:

- Rendering final diagnostics outside this query-boundary classification may
  resolve atoms to display names, but those conversions are presentation work
  and are not part of this helper slice.
- `classify_object_properties` stores `Atom` handles in
  `PropertyClassification` for excess and incompatible properties, so diagnostic
  rendering is deferred rather than forcing display strings during
  classification.

## Follow-up Shape

The next narrow change should introduce a helper that compares properties using
stable property identities where available, while preserving a string fallback
for cases that still require textual names such as numeric-literal index checks
or prototype-name screening. The first migration should target one failure path
only, keeping checker/solver ownership explicit and avoiding a broad relation
rewrite.
