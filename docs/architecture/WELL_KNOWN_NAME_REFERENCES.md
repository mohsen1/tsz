# Well-Known Name References

Status: architecture inventory for anti-hardcoding work. Keep this file in
sync when adding or removing compiler decisions that mention built-in
TypeScript names such as `Promise`, `Object`, `Function`, `Iterator`, or
`[Symbol.iterator]`.

## Rule

User-chosen names are not semantic facts. Compiler decisions must not depend on
whether a user happened to name a type parameter `T`, a mapped key `P`, or an
alias `Foo`.

Well-known TypeScript and JavaScript names are different only when the code
proves that the reference is the built-in/lib declaration, or when the literal
is protocol syntax rather than user spelling. The preferred entrypoints are:

- binder/global tables for global values and interfaces,
- `DefId` or `SymbolId` identity for resolved declarations,
- solver-owned type queries for protocol and type-shape facts,
- `TypeId` reservations for intrinsic types.

String literals are acceptable for diagnostics and parsing syntax. They are not
acceptable as semantic proof by themselves.

## Approved Categories

| Name family | Current entrypoint | Rationale | Representative files |
| --- | --- | --- | --- |
| `Promise`, `PromiseLike` global types | `resolve_global_interface_type`, `resolve_lib_type_by_name`, `get_global_type_with_libs`, `ctx.has_name_in_lib`, and `context/lib_queries.rs` | Async return typing, promise unwrapping, and missing-global diagnostics must target the actual lib global, not an arbitrary user alias. Existing call sites should continue moving toward lib/global lookup before treating the name as semantic. | `crates/tsz-checker/src/types/computation/access_await.rs`, `crates/tsz-checker/src/checkers/signature_builder.rs`, `crates/tsz-checker/src/checkers/promise_checker.rs`, `crates/tsz-checker/src/context/lib_queries.rs` |
| `Object`, `Function` global values | `is_builtin_global_reference`, `known_global_value_has_local_shadow`, `resolve_lib_type_by_name`, and `TypeId::{OBJECT,FUNCTION}` | Object/Function constructor behavior depends on the built-in global and must not trigger for a locally shadowed identifier. | `crates/tsz-checker/src/flow/control_flow/type_guards.rs`, `crates/tsz-checker/src/error_reporter/properties.rs`, `crates/tsz-checker/src/types/type_checking/global.rs`, `crates/tsz-solver/src/diagnostics/format/mod.rs` |
| Iterator and iterable globals | `resolve_lib_type_by_name`, lib symbol lookup, and solver/checker iterable query boundaries | `Iterator`, `IteratorObject`, `Iterable`, and async variants are lib protocols. Checks should resolve the global type or query protocol shape before using the name. | `crates/tsz-checker/src/dispatch_yield.rs`, `crates/tsz-checker/src/types/property_access_helpers/iterator_methods.rs`, `crates/tsz-checker/src/checkers/iterable_checker.rs`, `crates/tsz-checker/src/types/queries/lib.rs` |
| Well-known symbol members | interned protocol atoms such as `[Symbol.iterator]`, `[Symbol.asyncIterator]`, and canonical internal spellings such as `__@iterator` | These are property/protocol keys, not user-selected binder names. They should still cross through property lookup or solver protocol helpers instead of ad hoc display-string tests. | `crates/tsz-solver/src/operations/iterators.rs`, `crates/tsz-solver/src/relations/judge.rs`, `crates/tsz-checker/src/checkers/iterable_checker.rs`, `crates/tsz-checker/src/types/computation/call_inference.rs` |
| Intrinsic display names | `TypeId` reservations and `IntrinsicKind` | Formatting `Function`, primitive wrapper names, and intrinsic names is output policy after semantic identity is known. | `crates/tsz-solver/src/diagnostics/format/mod.rs`, `crates/tsz-solver/src/diagnostics/format/intrinsic.rs`, `crates/tsz-solver/src/diagnostics/format/tracing_helpers.rs` |
| JSDoc built-in aliases | JSDoc parser/resolution entrypoints such as `resolve_jsdoc_implicit_any_builtin_type` | JSDoc accepts legacy textual spellings (`function`, `Object`, `Promise`) as syntax-level type names. These paths should resolve to global/builtin facts immediately after parsing. | `crates/tsz-checker/src/jsdoc/resolution/name_resolution.rs`, `crates/tsz-checker/src/jsdoc/params.rs`, `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs` |

## Migration Debt

The following patterns are visible today but should not be copied:

- Name checks that read `escaped_text` directly and compare against a built-in
  name before proving the symbol is the built-in declaration.
- Helper names such as `return_context_application_base_has_name` that accept
  `&["Promise", "PromiseLike"]` rather than a resolved lib/global identity.
- Diagnostic fingerprint or render-policy code that branches on rendered names
  such as `Object` after formatting has already happened.
- Protocol checks that compare against `[Symbol.iterator]` in checker code
  instead of going through an iterable/property-access query when a query is
  available.

Cleanup PRs should replace these with structural queries, global/lib lookup, or
stable semantic identities. If a temporary string path remains necessary, the
PR body should name the structural invariant and the later boundary that will
own it.

## Audit Command

Use this narrow discovery command before changing this inventory:

```bash
rg -n '"Promise"|"PromiseLike"|"Iterable"|"Iterator"|"IteratorObject"|"Symbol\\.iterator"|"\[Symbol\\.(?:iterator|asyncIterator)\]"|"Object"|"Function"' \
  crates/tsz-checker/src crates/tsz-solver/src --glob '*.rs'
```
