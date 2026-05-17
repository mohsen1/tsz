//! Structural attribution helpers for source-file alias direct-lowering misses.

use tsz_binder::{BinderState, symbol_flags};
use tsz_common::perf_counters::{
    DirectSourceFileTypeAliasBodyRejectionKind,
    DirectSourceFileTypeAliasTypeReferenceRejectionKind,
    record_direct_source_file_type_alias_body_rejection_kind,
    record_direct_source_file_type_alias_type_reference_rejection_kind,
};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;

pub(crate) fn record_source_alias_rejection_kinds(
    arena: &NodeArena,
    delegate_binder: &BinderState,
    type_alias: &TypeAliasData,
    type_param_names: &[String],
) {
    let node_idx = type_alias.type_node;
    let body_kind = body_rejection_kind(arena, node_idx);
    record_direct_source_file_type_alias_body_rejection_kind(body_kind);
    if body_kind == DirectSourceFileTypeAliasBodyRejectionKind::TypeReference {
        record_direct_source_file_type_alias_type_reference_rejection_kind(
            type_reference_rejection_kind(arena, delegate_binder, node_idx, type_param_names),
        );
    }
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

    let Some(sym_id) = delegate_binder.file_locals.get(name) else {
        return Kind::UnresolvedIdentifier;
    };
    let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
        return Kind::UnresolvedIdentifier;
    };
    if symbol.flags & symbol_flags::ALIAS != 0 {
        if let Some(resolved_sym_id) = delegate_binder.resolve_import_symbol(sym_id)
            && resolved_sym_id != sym_id
            && let Some(resolved_symbol) = delegate_binder.get_symbol(resolved_sym_id)
        {
            return classify_type_reference_rejection_symbol(resolved_symbol, has_type_arguments);
        }
        return Kind::LocalAliasSymbol;
    }

    classify_type_reference_rejection_symbol(symbol, has_type_arguments)
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
