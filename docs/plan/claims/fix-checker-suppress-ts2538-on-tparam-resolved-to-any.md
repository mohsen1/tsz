---
branch: fix/checker-suppress-ts2538-on-tparam-resolved-to-any
status: ready
created: 2026-05-03 05:20:00
---

**2026-05-03 05:20:00** · branch
`fix/checker-suppress-ts2538-on-tparam-resolved-to-any` ·
**Conformance fix: defer TS2538 when AST index is a bare type-parameter
reference even if our resolution collapsed it to `any`** ·
`check_indexed_access_type` emitted TS2538 ("Type 'any' cannot be used
as an index type") at `keyofAndIndexedAccess.ts:634:56` for
`Foo7<I7[K]>` where `interface I7 { x: any; }` and
`K extends keyof I7`. The check fires when our
`get_type_from_type_node(K_node)` collapses K to `TypeId::ANY` —
typically inside lazy/deferred indexed-access evaluation, where
`type_parameter_scope` does not carry the enclosing function's K and
the binder's `node_symbols` map has not yet bound the K identifier.
tsc preserves the syntactic form of the index and defers the rejection
to instantiation time, so we should too. The fix adds a small AST
helper
(`query_boundaries::type_checking_utilities::ast_index_node_is_in_scope_type_parameter`)
that walks the index AST node, treats a bare TYPE_REFERENCE-to-Identifier
as "still generic" when the name is in
`CheckerContext::type_parameter_scope` OR the binder's lexical name
resolver maps it to a TYPE_PARAMETER symbol. The TS2538 emission in
`indexed_access.rs` calls the helper before erroring. Net conformance:
12360 → 12361 (+1, `keyofAndIndexedAccess.ts`). No regressions, all
11585 unit tests pass. Unit test
`type_param_indexed_access_into_any_property_does_not_emit_ts2538`
locks the negative case.
