//! Structural attribution helpers for source-file alias direct-lowering misses.

use crate::query_boundaries::common::{
    TypeDatabase, is_array_type, is_conditional_type, is_index_access_type, is_intersection_type,
    is_string_intrinsic_type, is_tuple_type, is_union_type, literal_value, union_members,
};
#[cfg(test)]
use crate::state::CheckerState;
use std::collections::HashSet;
use tsz_binder::{BinderState, symbol_flags};
use tsz_common::perf_counters::{
    DirectSourceFileTypeAliasBodyRejectionKind, DirectSourceFileTypeAliasBodyRejectionResidueInput,
    DirectSourceFileTypeAliasTypeReferenceRejectionKind, enabled_fast,
    record_direct_source_file_type_alias_body_rejection_kind,
    record_direct_source_file_type_alias_body_rejection_residue,
    record_direct_source_file_type_alias_first_type_reference_rejection_kind,
    record_direct_source_file_type_alias_type_reference_rejection_kind,
};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(crate) fn record_source_alias_rejection_kinds(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    type_alias: &TypeAliasData,
    type_param_names: &[String],
    type_node_is_lowerable: &dyn Fn(NodeIndex) -> bool,
) {
    let node_idx = type_alias.type_node;
    let body_kind = body_rejection_kind(arena, node_idx);
    record_direct_source_file_type_alias_body_rejection_kind(body_kind);
    if enabled_fast() {
        let first_type_reference_kind = first_type_reference_rejection_kind_in_node(
            arena,
            delegate_binder,
            node_idx,
            type_param_names,
        );
        if let Some(kind) = first_type_reference_kind {
            record_direct_source_file_type_alias_first_type_reference_rejection_kind(kind);
        }
        let first_non_lowerable_type_reference = first_non_lowerable_type_reference_in_node(
            arena,
            delegate_binder,
            node_idx,
            type_param_names,
            type_node_is_lowerable,
        );
        let first_non_lowerable_leaf_type_reference =
            first_non_lowerable_leaf_type_reference_in_node(
                arena,
                delegate_binder,
                node_idx,
                type_param_names,
                type_node_is_lowerable,
            );
        record_direct_source_file_type_alias_body_rejection_residue(
            DirectSourceFileTypeAliasBodyRejectionResidueInput {
                name: type_alias_name(arena, type_alias).unwrap_or("<unknown>"),
                body_kind,
                first_type_reference_kind,
                first_type_reference_name: first_type_reference_name_in_node(arena, node_idx),
                first_non_lowerable_type_reference_kind: first_non_lowerable_type_reference
                    .map(|reference| reference.kind),
                first_non_lowerable_type_reference_name: first_non_lowerable_type_reference
                    .and_then(|reference| reference.name),
                first_non_lowerable_leaf_type_reference_kind:
                    first_non_lowerable_leaf_type_reference.map(|reference| reference.kind),
                first_non_lowerable_leaf_type_reference_name:
                    first_non_lowerable_leaf_type_reference.and_then(|reference| reference.name),
                target_file: arena
                    .source_files
                    .first()
                    .map(|source_file| source_file.file_name.as_str()),
            },
        );
        record_non_lowerable_type_reference_rejection_kinds_in_node(
            arena,
            delegate_binder,
            node_idx,
            type_param_names,
            type_node_is_lowerable,
        );
    }
}

/// Decide whether a type-alias declaration body is a reducing operator whose
/// `result` tsc renders structurally (without an `aliasSymbol`), so the
/// definition store should mark `result` as a "computed" body.
///
/// - An **intersection** drops the alias name only when it collapses to a
///   union of purely primitive/literal members (`T1 & ("a"|"b")` ->
///   `"a"|"b"`); a distribution into object-typed members keeps the name.
/// - A **conditional** resolves away into its branch type, which is a
///   pre-existing type that never carries the alias's `aliasSymbol`, so tsc
///   renders the evaluated underlying type — including a bare object, mapped
///   shape, or a primitive/literal union branch (`A extends B ? number |
///   boolean : never` elaborates as `number | boolean`, never the alias name).
/// - An **indexed access** is the carve-out: when its result is a *union* (an
///   index over a union / `keyof` that builds a fresh union via tsc's
///   `getUnionType(propTypes, …, aliasSymbol)`), that union carries the alias
///   symbol, so it is **not** computed and tsc keeps the alias name
///   (`type W = T[keyof T]` -> `W`). Only a single-key access that resolves to
///   one existing member type (no fresh construction, no alias symbol) is
///   computed and rendered structurally.
///   Unions/intersections that mix in objects stay deferred to the separate
///   elaboration path; directly-written aliases sharing a bare object shape are
///   protected by the def store's "direct wins" provenance.
/// - A **template literal** (`` `${"a" | "b"}-x` ``) and a **string-mapping
///   intrinsic** (`Capitalize<"a" | "b">`) drop the alias name when they reduce
///   to a finite literal union: tsc's `getTemplateLiteralType` /
///   `getStringMappingType` build the result via `getUnionType` /
///   `getStringLiteralType` directly, with no `aliasSymbol`, so the expanded
///   union is shown (`"a-x" | "b-x"`, `"A" | "B"`), never the alias name. A
///   directly-written union alias (`type Plain = "a" | "b"`) keeps its name —
///   its body node is a `UnionType`, not a template/intrinsic, so it never
///   reaches these arms.
/// - A **generic alias application** (`type Rest = Tail<[1, 2, 3]>`) whose
///   evaluated result collapses to a pre-existing array or tuple carries no
///   `aliasSymbol`: tsc threads an alias symbol only through the
///   union/intersection/object/mapped constructors, never through a bare
///   `TypeReference` body, so the reduced array/tuple is shown structurally (or by
///   the inner application's own alias). Object results are owned by the
///   `reducing_object_application` gate at the call site.
///
/// `evaluated` is the reduced form of `result` (equal to `result` when the alias
/// is not safely evaluable). The AST-body-kind arms that inspect a shape which is
/// already present on the raw result read `result`; the application-collapse arm
/// reads `evaluated`, since an unevaluated `Application` has not yet taken its
/// collapsed array/tuple shape.
pub(crate) fn alias_declaration_body_is_computed(
    arena: &NodeArena,
    db: &dyn TypeDatabase,
    decl_idx: NodeIndex,
    result: TypeId,
    evaluated: TypeId,
) -> bool {
    let Some(decl_node) = arena.get(decl_idx) else {
        return false;
    };
    let Some(type_alias) = arena.get_type_alias(decl_node) else {
        return false;
    };
    let Some(body_node) = arena.get(type_alias.type_node) else {
        return false;
    };
    // A string-mapping intrinsic body (see the doc-comment rule above) is keyed
    // on the raw (unevaluated) `StringIntrinsic` result shape, not a body syntax
    // kind (its body parses as a `TypeReference` to `Capitalize`/…), so it is
    // handled before the AST-kind dispatch. A still-deferred intrinsic over a
    // non-literal arg (`Capitalize<string>`) already renders structurally, so
    // the mark is a harmless no-op there.
    if is_string_intrinsic_type(db, result) {
        return true;
    }
    match body_node.kind {
        // A template-literal body that reduces to a union is freshly built by
        // tsc's `getTemplateLiteralType` (`mapType` -> `getUnionType`) with no
        // `aliasSymbol`, so the expanded union is printed, not the alias name.
        // A single-literal or still-deferred pattern result is not a union and
        // is rendered structurally elsewhere (not in the composite reverse
        // lookup), so it needs no mark.
        syntax_kind_ext::TEMPLATE_LITERAL_TYPE => is_union_type(db, result),
        syntax_kind_ext::INTERSECTION_TYPE => {
            // tsc's `getIntersectionType` keeps the alias's `aliasSymbol` only
            // when it constructs a fresh intersection (`{ a } & { b }` stays
            // `Both`). When the set collapses it returns the reduced operand
            // directly, carrying no alias: `T & ("a" | "b")` distributes to the
            // primitive/literal union `"a" | "b"`, and `string[] & Array<string>`
            // dedupes to the single pre-existing `string[]`. Both drop the name,
            // so both are computed bodies rendered structurally. The
            // still-an-intersection guard keeps an object/union distribution that
            // stays a fresh intersection (which keeps its alias) out of the
            // array/tuple collapse arm. The array/tuple collapse is judged on the
            // *evaluated* result: `string[] & Array<string>` interns pre-collapsed,
            // but `number[] & Array<number>` (or any aliased operand) only takes
            // its reduced array/tuple shape after evaluation, so the raw `result`
            // is still an unevaluated intersection there.
            result_is_primitive_literal_union(db, result)
                || (!is_intersection_type(db, evaluated)
                    && (is_array_type(db, evaluated) || is_tuple_type(db, evaluated)))
        }
        syntax_kind_ext::CONDITIONAL_TYPE => {
            !is_conditional_type(db, result)
                && !is_index_access_type(db, result)
                && !crate::query_boundaries::diagnostics::union_or_intersection_with_object(
                    db, result,
                )
        }
        syntax_kind_ext::INDEXED_ACCESS_TYPE => {
            !is_conditional_type(db, result)
                && !is_index_access_type(db, result)
                // A union result is freshly constructed by `getUnionType(…,
                // aliasSymbol)` and therefore carries the alias symbol — tsc
                // keeps the alias name, so it is *not* a computed body. A
                // single-key access yields one existing member type (no alias
                // symbol) and is rendered structurally.
                && !is_union_type(db, result)
        }
        // `keyof { ... }` over an inline object *type literal* is a reducing
        // operator like indexed access: it resolves away into the operand's key
        // set (a literal/primitive union) and never carries the alias's
        // `aliasSymbol`, so tsc renders the underlying union, not the alias name.
        // Verified against tsc 6.0.2: `type K = keyof { a: 1; b: 2 }` elaborates
        // as `"a" | "b"`, never `K`.
        //
        // The gate is the *syntactic* operand shape: `keyof <TypeLiteral>` is
        // anonymous (no writable name), so the alias name is the only handle and
        // tsc drops it. `keyof Foo` over a named type reference keeps the
        // `keyof Foo` spelling, and a generic `keyof T` stays deferred — neither
        // reaches this flag because their operand node is not a `TypeLiteral`.
        // This mirrors the conditional / indexed-access arms above.
        syntax_kind_ext::TYPE_OPERATOR
            if arena.get_type_operator(body_node).is_some_and(|op| {
                op.operator == SyntaxKind::KeyOfKeyword as u16
                    && arena
                        .get(op.type_node)
                        .is_some_and(|operand| operand.kind == syntax_kind_ext::TYPE_LITERAL)
            }) =>
        {
            true
        }
        // A non-generic alias whose body is a generic *application*
        // (`type Rest = Tail<[1, 2, 3]>`, `type Mapped = Copy<1[]>`) never carries
        // the *outer* alias's `aliasSymbol`: tsc threads an alias symbol only
        // through the union/intersection/object/mapped constructors, never through
        // a bare `TypeReference` body. When that application evaluates to an array
        // or tuple, tsc renders it structurally (or by the inner application's own
        // alias) — `Tail<[1, 2, 3]>` reduces to the existing tuple `[2, 3]`, a
        // variadic `infer` tail with no fresh construction. The shape gate runs on
        // the *evaluated* result, since the raw `result` is still an unevaluated
        // `Application` that has not taken its array/tuple shape. Object results
        // are owned by the `reducing_object_application` gate at the call site, so
        // this arm stays array/tuple only.
        syntax_kind_ext::TYPE_REFERENCE => {
            is_array_type(db, evaluated) || is_tuple_type(db, evaluated)
        }
        _ => false,
    }
}

/// True when a non-generic type alias declaration body is a (optionally
/// parenthesized) tuple type literal with a top-level spread element
/// (`type T = [...X, c]`).
///
/// This is the cheap syntactic half of the spread-flattened-tuple alias-display
/// rule: a fixed-tuple spread (`...[a, b]`, or `...Inner` where `Inner` is a
/// fixed tuple) flattens into a fresh tuple that `tsc` stamps with no
/// `aliasSymbol`, so its diagnostics render the structural tuple (`[a, b, c]`)
/// rather than `T`. The caller pairs this with a check that the *evaluated*
/// alias type is a non-variadic tuple — a rest array such as `[...number[], c]`
/// stays variadic and keeps its alias name, matching `tsc`. Keying is per def
/// (the flattened tuple interns to the same shape as a directly-written
/// `type T = [a, b, c]`, which `tsc` displays by name).
pub(crate) fn tuple_alias_declaration_body_has_top_level_spread(
    arena: &NodeArena,
    decl_idx: NodeIndex,
) -> bool {
    let Some(decl_node) = arena.get(decl_idx) else {
        return false;
    };
    let Some(type_alias) = arena.get_type_alias(decl_node) else {
        return false;
    };
    // Unwrap parenthesized wrappers to reach the tuple type literal.
    let node_idx = crate::types_domain::unique_symbol_arena::unwrap_parenthesized_type(
        arena,
        type_alias.type_node,
    );
    let Some(node) = arena.get(node_idx) else {
        return false;
    };
    if node.kind != syntax_kind_ext::TUPLE_TYPE {
        return false;
    }
    let Some(tuple) = arena.get_tuple_type(node) else {
        return false;
    };
    tuple.elements.nodes.iter().any(|&elem_idx| {
        if elem_idx.is_none() {
            return false;
        }
        let Some(elem) = arena.get(elem_idx) else {
            return false;
        };
        if elem.kind == syntax_kind_ext::REST_TYPE {
            return true;
        }
        elem.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
            && arena
                .get_named_tuple_member(elem)
                .is_some_and(|member| member.dot_dot_dot_token)
    })
}

fn result_is_primitive_literal_union(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    union_members(db, ty).is_some_and(|members| {
        members.iter().all(|&m| {
            literal_value(db, m).is_some()
                || m == TypeId::STRING
                || m == TypeId::NUMBER
                || m == TypeId::BOOLEAN
                || m == TypeId::BIGINT
                || m == TypeId::SYMBOL
                || m == TypeId::UNDEFINED
                || m == TypeId::NULL
                || m == TypeId::VOID
                || m == TypeId::NEVER
        })
    })
}

#[derive(Copy, Clone)]
struct TypeReferenceAttribution<'a> {
    kind: DirectSourceFileTypeAliasTypeReferenceRejectionKind,
    name: Option<&'a str>,
}

fn type_alias_name<'a>(arena: &'a NodeArena, type_alias: &TypeAliasData) -> Option<&'a str> {
    arena
        .get(type_alias.name)
        .and_then(|node| arena.get_identifier(node))
        .map(|identifier| identifier.escaped_text.as_str())
}

fn body_rejection_kind(
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> DirectSourceFileTypeAliasBodyRejectionKind {
    use DirectSourceFileTypeAliasBodyRejectionKind as Kind;

    let Some(node) = arena.get(node_idx) else {
        return Kind::Other;
    };
    match node.kind {
        k if k == syntax_kind_ext::TYPE_REFERENCE => Kind::TypeReference,
        k if k == syntax_kind_ext::CONDITIONAL_TYPE => Kind::ConditionalType,
        k if k == syntax_kind_ext::TYPE_OPERATOR => Kind::TypeOperator,
        k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => Kind::IndexedAccessType,
        k if k == syntax_kind_ext::MAPPED_TYPE => Kind::MappedType,
        k if k == syntax_kind_ext::TYPE_LITERAL => Kind::TypeLiteral,
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => Kind::TemplateLiteralType,
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            Kind::UnionOrIntersectionType
        }
        k if k == syntax_kind_ext::ARRAY_TYPE || k == syntax_kind_ext::TUPLE_TYPE => {
            Kind::ArrayOrTupleType
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            Kind::WrappedType
        }
        k if k == syntax_kind_ext::INFER_TYPE => Kind::InferType,
        _ => Kind::Other,
    }
}

fn type_reference_rejection_kind(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    node_idx: NodeIndex,
    type_param_names: &[String],
) -> DirectSourceFileTypeAliasTypeReferenceRejectionKind {
    use DirectSourceFileTypeAliasTypeReferenceRejectionKind as Kind;

    let Some(node) = arena.get(node_idx) else {
        return Kind::Other;
    };
    let Some(type_ref) = arena.get_type_ref(node) else {
        return Kind::Other;
    };
    let Some(name_node) = arena.get(type_ref.type_name) else {
        return Kind::Other;
    };
    if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
        return Kind::QualifiedName;
    }
    let Some(name) = arena
        .get_identifier(name_node)
        .map(|ident| ident.escaped_text.as_str())
    else {
        return Kind::Other;
    };

    let has_type_arguments = type_ref
        .type_arguments
        .as_ref()
        .is_some_and(|args| !args.nodes.is_empty());
    if type_param_names.iter().any(|param| param == name) {
        return if has_type_arguments {
            Kind::OwnTypeParamWithTypeArguments
        } else {
            Kind::LocalTypeParameter
        };
    }

    if let Some(sym_id) = delegate_binder.file_locals.get(name) {
        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            return Kind::UnresolvedIdentifier;
        };
        if symbol.flags & symbol_flags::ALIAS != 0 {
            if let Some(resolved_sym_id) =
                resolve_import_symbol_for_attribution_no_cache(delegate_binder, sym_id)
                && resolved_sym_id != sym_id
                && let Some(resolved_symbol) = delegate_binder.get_symbol(resolved_sym_id)
            {
                return classify_type_reference_rejection_symbol(
                    resolved_symbol,
                    has_type_arguments,
                );
            }
            return Kind::LocalAliasSymbol;
        }

        return classify_type_reference_rejection_symbol(symbol, has_type_arguments);
    }

    if matches!(name, "Array" | "ReadonlyArray") {
        let Some(args) = type_ref.type_arguments.as_ref() else {
            return Kind::BuiltinArrayWrongArity;
        };
        return if args.nodes.len() == 1 {
            Kind::BuiltinArrayNonDirectArgument
        } else {
            Kind::BuiltinArrayWrongArity
        };
    }

    Kind::UnresolvedIdentifier
}

fn record_type_reference_rejection_kinds_in_node(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
) {
    for kind in
        type_reference_rejection_kinds_in_node(arena, delegate_binder, root_idx, type_param_names)
    {
        record_direct_source_file_type_alias_type_reference_rejection_kind(kind);
    }
}

fn record_non_lowerable_type_reference_rejection_kinds_in_node(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
    type_node_is_lowerable: &dyn Fn(NodeIndex) -> bool,
) {
    for kind in non_lowerable_type_reference_rejection_kinds_in_node(
        arena,
        delegate_binder,
        root_idx,
        type_param_names,
        type_node_is_lowerable,
    ) {
        record_direct_source_file_type_alias_type_reference_rejection_kind(kind);
    }
}

fn first_type_reference_rejection_kind_in_node(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
) -> Option<DirectSourceFileTypeAliasTypeReferenceRejectionKind> {
    let mut stack = vec![root_idx];
    while let Some(node_idx) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return Some(type_reference_rejection_kind(
                arena,
                delegate_binder,
                node_idx,
                type_param_names,
            ));
        }
        let children = arena.get_children(node_idx);
        stack.extend(children.into_iter().rev());
    }
    None
}

fn first_type_reference_name_in_node(arena: &NodeArena, root_idx: NodeIndex) -> Option<&str> {
    let mut stack = vec![root_idx];
    while let Some(node_idx) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return type_reference_name(arena, node_idx);
        }
        let children = arena.get_children(node_idx);
        stack.extend(children.into_iter().rev());
    }
    None
}

fn first_non_lowerable_type_reference_in_node<'a>(
    arena: &'a NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
    type_node_is_lowerable: &dyn Fn(NodeIndex) -> bool,
) -> Option<TypeReferenceAttribution<'a>> {
    let mut stack = vec![root_idx];
    while let Some(node_idx) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE && !type_node_is_lowerable(node_idx) {
            return Some(TypeReferenceAttribution {
                kind: type_reference_rejection_kind(
                    arena,
                    delegate_binder,
                    node_idx,
                    type_param_names,
                ),
                name: type_reference_name(arena, node_idx),
            });
        }
        let children = arena.get_children(node_idx);
        stack.extend(children.into_iter().rev());
    }
    None
}

fn first_non_lowerable_leaf_type_reference_in_node<'a>(
    arena: &'a NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
    type_node_is_lowerable: &dyn Fn(NodeIndex) -> bool,
) -> Option<TypeReferenceAttribution<'a>> {
    if type_node_is_lowerable(root_idx) {
        return None;
    }

    let node = arena.get(root_idx)?;
    for child in arena.get_children(root_idx) {
        if let Some(reference) = first_non_lowerable_leaf_type_reference_in_node(
            arena,
            delegate_binder,
            child,
            type_param_names,
            type_node_is_lowerable,
        ) {
            return Some(reference);
        }
    }

    (node.kind == syntax_kind_ext::TYPE_REFERENCE).then(|| TypeReferenceAttribution {
        kind: type_reference_rejection_kind(arena, delegate_binder, root_idx, type_param_names),
        name: type_reference_name(arena, root_idx),
    })
}

fn type_reference_name(arena: &NodeArena, node_idx: NodeIndex) -> Option<&str> {
    let node = arena.get(node_idx)?;
    let type_ref = arena.get_type_ref(node)?;
    let name_node = arena.get(type_ref.type_name)?;
    arena
        .get_identifier(name_node)
        .map(|identifier| identifier.escaped_text.as_str())
}

fn type_reference_rejection_kinds_in_node(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
) -> Vec<DirectSourceFileTypeAliasTypeReferenceRejectionKind> {
    let mut kinds = Vec::new();
    let mut stack = vec![root_idx];
    while let Some(node_idx) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            kinds.push(type_reference_rejection_kind(
                arena,
                delegate_binder,
                node_idx,
                type_param_names,
            ));
        }
        stack.extend(arena.get_children(node_idx));
    }
    kinds
}

fn non_lowerable_type_reference_rejection_kinds_in_node(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    root_idx: NodeIndex,
    type_param_names: &[String],
    type_node_is_lowerable: &dyn Fn(NodeIndex) -> bool,
) -> Vec<DirectSourceFileTypeAliasTypeReferenceRejectionKind> {
    let mut kinds = Vec::new();
    let mut stack = vec![root_idx];
    while let Some(node_idx) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            if type_node_is_lowerable(node_idx) {
                continue;
            } else {
                kinds.push(type_reference_rejection_kind(
                    arena,
                    delegate_binder,
                    node_idx,
                    type_param_names,
                ));
            }
        }
        stack.extend(arena.get_children(node_idx));
    }
    kinds
}

fn resolve_import_symbol_for_attribution_no_cache(
    binder: &BinderState,
    sym_id: tsz_binder::SymbolId,
) -> Option<tsz_binder::SymbolId> {
    let symbol = binder.get_symbol(sym_id)?;
    let module_specifier = symbol.import_module()?;
    let export_name = symbol.import_name().unwrap_or("export=");
    let mut visited = HashSet::new();
    resolve_import_with_reexports_for_attribution_no_cache(
        binder,
        module_specifier,
        export_name,
        &mut visited,
    )
}

fn resolve_import_with_reexports_for_attribution_no_cache(
    binder: &BinderState,
    module_specifier: &str,
    export_name: &str,
    visited: &mut HashSet<(String, String)>,
) -> Option<tsz_binder::SymbolId> {
    let key = (module_specifier.to_string(), export_name.to_string());
    if !visited.insert(key) {
        return None;
    }

    if let Some(module_table) = binder.module_exports.get(module_specifier) {
        if let Some(sym_id) = module_table.get(export_name) {
            return Some(sym_id);
        }
        if export_name == "default"
            && let Some(sym_id) = module_table.get("export=")
        {
            return Some(sym_id);
        }
    }

    if let Some(file_reexports) = binder.reexports.get(module_specifier)
        && let Some((source_module, original_name)) = file_reexports.get(export_name)
    {
        let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
        return resolve_import_with_reexports_for_attribution_no_cache(
            binder,
            source_module,
            name_to_lookup,
            visited,
        );
    }

    if let Some(source_modules) = binder.wildcard_reexports.get(module_specifier) {
        for (source_module, _is_type_only) in source_modules {
            if let Some(sym_id) = resolve_import_with_reexports_for_attribution_no_cache(
                binder,
                source_module,
                export_name,
                visited,
            ) {
                return Some(sym_id);
            }
        }
    }

    None
}

const fn classify_type_reference_rejection_symbol(
    symbol: &tsz_binder::Symbol,
    has_type_arguments: bool,
) -> DirectSourceFileTypeAliasTypeReferenceRejectionKind {
    use DirectSourceFileTypeAliasTypeReferenceRejectionKind as Kind;

    if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
        return if has_type_arguments {
            Kind::LocalTypeAliasWithArguments
        } else {
            Kind::LocalTypeAliasNoArguments
        };
    }
    if symbol.flags & symbol_flags::INTERFACE != 0 {
        return if has_type_arguments {
            Kind::LocalInterfaceWithArguments
        } else {
            Kind::LocalInterfaceNoArguments
        };
    }
    if symbol.flags & symbol_flags::TYPE_PARAMETER != 0 {
        return Kind::LocalTypeParameter;
    }
    if symbol.flags & symbol_flags::ALIAS != 0 {
        return Kind::LocalAliasSymbol;
    }
    if symbol.flags & symbol_flags::NAMESPACE != 0 {
        return Kind::LocalNamespaceSymbol;
    }
    if symbol.flags & symbol_flags::VALUE != 0 {
        return Kind::LocalValueSymbol;
    }
    if symbol.flags & symbol_flags::TYPE_LITERAL != 0 {
        return Kind::LocalTypeLiteralSymbol;
    }
    if symbol.flags & symbol_flags::TRANSIENT != 0 {
        return Kind::LocalTransientSymbol;
    }
    Kind::LocalOtherSymbol
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tsz_binder::SymbolTable;
    use tsz_parser::parser::ParserState;

    fn alias_body_from_source(source: &str) -> (NodeArena, NodeIndex) {
        let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let alias_body = source_file
            .statements
            .nodes
            .iter()
            .rev()
            .copied()
            .find_map(|idx| {
                arena
                    .get(idx)
                    .and_then(|node| arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })
            .expect("type alias body");
        (arena, alias_body)
    }

    fn bound_alias_body_from_source(source: &str) -> (NodeArena, BinderState, NodeIndex) {
        let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let alias_body = source_file
            .statements
            .nodes
            .iter()
            .rev()
            .copied()
            .find_map(|idx| {
                arena
                    .get(idx)
                    .and_then(|node| arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })
            .expect("type alias body");
        (arena, binder, alias_body)
    }

    #[test]
    fn source_file_alias_type_reference_attribution_resolves_import_alias_target() {
        let (arena, alias_body) = alias_body_from_source("type Box = Alias;");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Target".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Alias".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Target".to_string()));
        binder.file_locals.set("Alias".to_string(), alias_sym);
        let mut exports = SymbolTable::new();
        exports.set("Target".to_string(), target_sym);
        Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasNoArguments,
            "import aliases should be bucketed by resolved type target shape",
        );
        assert_eq!(
            binder.resolution_cache_statistics().export_cache_entries,
            0,
            "attribution must not populate semantic import-resolution caches",
        );
    }

    #[test]
    fn source_file_alias_type_reference_attribution_prefers_shadowing_array_symbol() {
        let (arena, alias_body) = alias_body_from_source("type Box = Array<string>;");

        let mut binder = BinderState::new();
        let array_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Array".to_string());
        binder.file_locals.set("Array".to_string(), array_sym);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
            "a local Array symbol should be bucketed by symbol shape, not builtin name",
        );
    }

    #[test]
    fn source_file_alias_type_reference_attribution_resolves_imported_array_symbol() {
        let (arena, alias_body) = alias_body_from_source("type Box = Array<string>;");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::INTERFACE, "Array".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Array".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Array".to_string()));
        binder.file_locals.set("Array".to_string(), alias_sym);
        let mut exports = SymbolTable::new();
        exports.set("Array".to_string(), target_sym);
        Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

        let kind = type_reference_rejection_kind(&arena, &binder, alias_body, &[]);

        assert_eq!(
            kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalInterfaceWithArguments,
            "an imported Array symbol should resolve before builtin name buckets",
        );
        assert_eq!(
            binder.resolution_cache_statistics().export_cache_entries,
            0,
            "attribution must not populate semantic import-resolution caches",
        );
    }

    #[test]
    fn source_file_alias_type_reference_attribution_walks_composite_bodies() {
        let (arena, alias_body) = alias_body_from_source(
            "type Box<T> = T | null;\ntype Item = string;\ntype Result<T> = Box<T> | Item;",
        );
        let mut binder = BinderState::new();
        let box_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Box".to_string());
        let item_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Item".to_string());
        binder.file_locals.set("Box".to_string(), box_sym);
        binder.file_locals.set("Item".to_string(), item_sym);

        let kinds = type_reference_rejection_kinds_in_node(
            &arena,
            &binder,
            alias_body,
            &[String::from("T")],
        );

        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
        ));
        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasNoArguments,
        ));
        assert!(kinds.contains(
            &DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeParameter,
        ));
    }

    #[test]
    fn source_file_alias_type_reference_counts_skip_lowerable_subtrees() {
        let (arena, binder, alias_body) = bound_alias_body_from_source(
            "type Leaf = string;\ntype Box<T> = T | Leaf;\ntype Result<T> = Array<Box<T>> | Missing<T>;",
        );
        let global_type_is_lowerable = |name: &str| name == "Array";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let kinds = non_lowerable_type_reference_rejection_kinds_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        );

        assert_eq!(
            kinds,
            vec![DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier],
            "aggregate rejection counters should skip lowerable helper subtrees",
        );
    }

    #[test]
    fn source_file_alias_first_type_reference_attribution_uses_source_order() {
        let (arena, alias_body) =
            alias_body_from_source("type Box<T> = T | null;\ntype Result<T> = Box<T> | Missing;");
        let mut binder = BinderState::new();
        let box_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Box".to_string());
        binder.file_locals.set("Box".to_string(), box_sym);

        let first = first_type_reference_rejection_kind_in_node(
            &arena,
            &binder,
            alias_body,
            &[String::from("T")],
        )
        .expect("first type reference");

        assert_eq!(
            first,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::LocalTypeAliasWithArguments,
            "first-reference attribution should classify the first source-order blocker",
        );
    }

    #[test]
    fn source_file_alias_non_lowerable_type_reference_skips_lowerable_globals() {
        let (arena, alias_body) = alias_body_from_source("type Result<T> = Array<T> | Missing<T>;");
        let binder = BinderState::new();
        let global_type_is_lowerable = |name: &str| name == "Array";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let first = first_non_lowerable_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable type reference");

        assert_eq!(
            first.kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier,
            "non-lowerable attribution should skip the lowerable Array<T> subtree",
        );
        assert_eq!(first.name, Some("Missing"));
    }

    #[test]
    fn source_file_alias_non_lowerable_leaf_type_reference_descends_into_outer_failure() {
        let (arena, alias_body) =
            alias_body_from_source("type Result<T> = Pick<T, Missing<keyof T>>;");
        let binder = BinderState::new();
        let global_type_is_lowerable = |name: &str| name == "Pick";
        let type_param_names = vec![String::from("T")];
        let type_node_is_lowerable = |node_idx| {
            CheckerState::source_file_type_node_is_generic_local_alias_application_lowerable(
                &arena,
                &binder,
                node_idx,
                &type_param_names,
                &global_type_is_lowerable,
            )
        };

        let first = first_non_lowerable_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable type reference");
        let leaf = first_non_lowerable_leaf_type_reference_in_node(
            &arena,
            &binder,
            alias_body,
            &type_param_names,
            &type_node_is_lowerable,
        )
        .expect("first non-lowerable leaf type reference");

        assert_eq!(first.name, Some("Pick"));
        assert_eq!(
            leaf.kind,
            DirectSourceFileTypeAliasTypeReferenceRejectionKind::UnresolvedIdentifier,
            "leaf attribution should identify the nested type reference that makes Pick fail",
        );
        assert_eq!(leaf.name, Some("Missing"));
    }
}
