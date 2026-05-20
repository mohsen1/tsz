//! Structural attribution helpers for source-file alias direct-lowering misses.

use std::collections::HashSet;
use tsz_binder::{BinderState, symbol_flags};
use tsz_common::perf_counters::{
    DirectSourceFileTypeAliasBodyRejectionKind,
    DirectSourceFileTypeAliasTypeReferenceRejectionKind, enabled_fast,
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
    if body_kind == DirectSourceFileTypeAliasBodyRejectionKind::TypeReference && enabled_fast() {
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

fn resolve_import_symbol_for_attribution_no_cache(
    binder: &BinderState,
    sym_id: tsz_binder::SymbolId,
) -> Option<tsz_binder::SymbolId> {
    let symbol = binder.get_symbol(sym_id)?;
    let module_specifier = symbol.import_module.as_ref()?;
    let export_name = symbol.import_name.as_deref().unwrap_or("export=");
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
        for source_module in source_modules {
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

    #[test]
    fn source_file_alias_type_reference_attribution_resolves_import_alias_target() {
        let mut parser =
            ParserState::new("fixture.ts".to_string(), "type Box = Alias;".to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let alias_body = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|idx| {
                arena
                    .get(idx)
                    .and_then(|node| arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })
            .expect("type alias body");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::TYPE_ALIAS, "Target".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Alias".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.import_module = Some("./target".to_string());
        alias_symbol.import_name = Some("Target".to_string());
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
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "type Box = Array<string>;".to_string(),
        );
        let root = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let alias_body = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|idx| {
                arena
                    .get(idx)
                    .and_then(|node| arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })
            .expect("type alias body");

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
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "type Box = Array<string>;".to_string(),
        );
        let root = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let alias_body = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|idx| {
                arena
                    .get(idx)
                    .and_then(|node| arena.get_type_alias(node))
                    .map(|alias| alias.type_node)
            })
            .expect("type alias body");

        let mut binder = BinderState::new();
        let target_sym = binder
            .symbols
            .alloc(symbol_flags::INTERFACE, "Array".to_string());
        let alias_sym = binder
            .symbols
            .alloc(symbol_flags::ALIAS, "Array".to_string());
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("alias symbol");
        alias_symbol.import_module = Some("./target".to_string());
        alias_symbol.import_name = Some("Array".to_string());
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
}
