//! Direct cross-file query fast paths that avoid constructing child checkers.

use crate::state::CheckerState;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_common::perf_counters::DirectCrossFileInterfaceLoweringOutcome;
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(crate) fn is_builtin_lib_file_name(file_name: &str) -> bool {
    let basename = std::path::Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);

    if basename.starts_with("lib.") && basename.ends_with(".d.ts") {
        return true;
    }

    let stem = basename
        .strip_suffix(".generated.d.ts")
        .or_else(|| basename.strip_suffix(".d.ts"))
        .unwrap_or(basename);

    stem == "lib"
        || stem == "scripthost"
        || stem == "decorators"
        || stem == "decorators.legacy"
        || stem == "dom"
        || stem.starts_with("dom.")
        || stem == "webworker"
        || stem.starts_with("webworker.")
        || stem == "esnext"
        || stem.starts_with("esnext.")
        || (stem.starts_with("es") && stem.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
}

fn is_builtin_lib_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        if !source_file.is_declaration_file {
            return false;
        }
        is_builtin_lib_file_name(&source_file.file_name)
    })
}

fn is_external_package_declaration_file_name(file_name: &str) -> bool {
    file_name.starts_with("node_modules/")
        || file_name.starts_with("node_modules\\")
        || file_name.contains("/node_modules/")
        || file_name.contains("\\node_modules\\")
}

fn is_direct_lowering_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file
            && is_external_package_declaration_file_name(&source_file.file_name)
            && !is_builtin_lib_file_name(&source_file.file_name)
    })
}

fn is_direct_lowering_source_file_arena(arena: &NodeArena) -> bool {
    arena
        .source_files
        .first()
        .is_some_and(|source_file| !source_file.is_declaration_file)
}

impl<'a> CheckerState<'a> {
    fn cross_file_interface_declarations<'b>(
        &self,
        sym_id: SymbolId,
        delegate_binder: &'b BinderState,
        fallback_arena: &'b NodeArena,
    ) -> Option<Vec<(NodeIndex, &'b NodeArena)>> {
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            return None;
        }

        let mut declarations = Vec::new();
        for decl_idx in symbol.declarations.iter().copied() {
            let mut found = false;
            if let Some(arenas) = delegate_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas.iter() {
                    if arena
                        .get(decl_idx)
                        .and_then(|node| arena.get_interface(node))
                        .is_some()
                    {
                        declarations.push((decl_idx, arena.as_ref()));
                        found = true;
                    }
                }
            }

            if !found
                && fallback_arena
                    .get(decl_idx)
                    .and_then(|node| fallback_arena.get_interface(node))
                    .is_some()
            {
                declarations.push((decl_idx, fallback_arena));
            }
        }

        (!declarations.is_empty()).then_some(declarations)
    }

    fn interface_declarations_have_heritage_or_computed_names(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            if interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|clauses| !clauses.nodes.is_empty())
            {
                return true;
            }

            interface.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                let name_idx = arena
                    .get_signature(member_node)
                    .map(|signature| signature.name)
                    .or_else(|| {
                        arena
                            .get_accessor(member_node)
                            .map(|accessor| accessor.name)
                    });
                name_idx
                    .and_then(|idx| arena.get(idx))
                    .is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    })
            })
        })
    }

    fn source_file_type_node_is_scope_independent(arena: &NodeArena, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::AnyKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UnknownKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NeverKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::VoidKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UndefinedKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NullKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BooleanKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NumberKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::StringKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BigIntKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::SymbolKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::ObjectKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => true,
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };
                    match name {
                        "any" | "unknown" | "never" | "void" | "undefined" | "null" | "boolean"
                        | "number" | "string" | "bigint" | "symbol" | "object" => type_ref
                            .type_arguments
                            .as_ref()
                            .is_none_or(|args| args.nodes.is_empty()),
                        "Array" | "ReadonlyArray" => {
                            type_ref.type_arguments.as_ref().is_some_and(|args| {
                                args.nodes.len() == 1
                                    && Self::source_file_type_node_is_scope_independent(
                                        arena,
                                        args.nodes[0],
                                    )
                            })
                        }
                        _ => false,
                    }
                })
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => arena.get_literal_type(node).is_some(),
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_scope_independent(arena, member)
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_scope_independent(arena, array.element_type)
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_scope_independent(arena, element)
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_scope_independent(arena, wrapped.type_node)
                })
            }
            _ => false,
        }
    }

    fn source_file_interface_declarations_are_direct_lowerable(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().all(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            if interface
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                return false;
            }

            interface.members.nodes.iter().copied().all(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    return false;
                }
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                signature
                    .parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
                    && signature
                        .type_parameters
                        .as_ref()
                        .is_none_or(|params| params.nodes.is_empty())
                    && Self::source_file_type_node_is_scope_independent(
                        arena,
                        signature.type_annotation,
                    )
            })
        })
    }

    pub(super) fn direct_cross_file_interface_lowering(
        &self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_complex_declarations: bool,
        allow_source_file_arena: bool,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let record = |outcome: DirectCrossFileInterfaceLoweringOutcome| {
            tsz_common::perf_counters::record_direct_cross_file_interface_lowering_outcome(outcome);
        };

        // Source and local test-fixture interfaces need exact binder-local symbol
        // resolution for diagnostics. Built-in libs depend on merged declarations
        // across many lib files and special canonical DefId handling. Keep both
        // on the mature checker path.
        let direct_declaration_arena = is_direct_lowering_declaration_arena(symbol_arena);
        let direct_source_file_arena =
            allow_source_file_arena && is_direct_lowering_source_file_arena(symbol_arena);
        if !direct_declaration_arena && !direct_source_file_arena {
            record(DirectCrossFileInterfaceLoweringOutcome::RejectedNonDirectArena);
            return None;
        }

        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingSymbol);
            return None;
        };
        let disallowed_merge_flags = symbol_flags::CLASS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::NotInterface);
            return None;
        }
        if symbol.flags & disallowed_merge_flags != 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::DisallowedMergeFlags);
            return None;
        }

        let Some(declarations) =
            self.cross_file_interface_declarations(sym_id, delegate_binder, symbol_arena)
        else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingDeclarations);
            return None;
        };
        if direct_source_file_arena {
            if Self::interface_declarations_have_heritage_or_computed_names(&declarations)
                || !Self::source_file_interface_declarations_are_direct_lowerable(&declarations)
            {
                record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
                return None;
            }
        } else if !allow_complex_declarations
            && Self::interface_declarations_have_heritage_or_computed_names(&declarations)
        {
            record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_name_def_id_resolver(&name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_preferred_self_reference(symbol.escaped_name.clone(), def_id)
        .prefer_name_def_id_resolution();

        let (interface_type, params) =
            lowering.lower_merged_interface_declarations_with_symbol(&declarations, Some(sym_id));
        if interface_type == TypeId::UNKNOWN || interface_type == TypeId::ERROR {
            record(DirectCrossFileInterfaceLoweringOutcome::UnknownOrError);
            return None;
        }
        record(DirectCrossFileInterfaceLoweringOutcome::Success);

        if !params.is_empty() {
            self.ctx.insert_def_type_params(def_id, params.clone());
        }
        self.ctx.definition_store.set_body(def_id, interface_type);
        self.ctx
            .definition_store
            .register_type_to_def(interface_type, def_id);
        Some((interface_type, params))
    }

    pub(super) fn direct_cross_file_interface_member_simple_types(
        &self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
        interface_arena: &NodeArena,
        delegate_binder: &BinderState,
        type_args: Option<&[TypeId]>,
    ) -> Option<rustc_hash::FxHashMap<NodeIndex, TypeId>> {
        let sym_id = delegate_binder.get_node_symbol(interface_idx).or_else(|| {
            let arena_ptr = interface_arena as *const NodeArena as usize;
            self.ctx
                .cross_file_node_symbols_for_arena(delegate_binder, arena_ptr)
                .and_then(|symbols| symbols.get(&interface_idx.0).copied())
        })?;

        let (interface_type, params) = self.direct_cross_file_interface_lowering(
            sym_id,
            delegate_binder,
            interface_arena,
            true,
            false,
        )?;

        let substitution = type_args
            .filter(|type_args| !params.is_empty() && type_args.len() <= params.len())
            .and_then(|type_args| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    self.ctx.types,
                    type_args,
                    &params,
                )
            })
            .map(|type_args| {
                crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    &params,
                    &type_args,
                )
            });

        let mut results = rustc_hash::FxHashMap::default();
        for &member_idx in member_indices {
            let Some(member_node) = interface_arena.get(member_idx) else {
                continue;
            };
            let name_idx = interface_arena
                .get_signature(member_node)
                .map(|signature| signature.name)
                .or_else(|| {
                    interface_arena
                        .get_accessor(member_node)
                        .map(|accessor| accessor.name)
                });
            let Some(name) = name_idx.and_then(|idx| {
                crate::types_domain::queries::core::get_literal_property_name(interface_arena, idx)
            }) else {
                continue;
            };
            let atom = self.ctx.types.intern_string(&name);
            let Some(mut member_type) = crate::query_boundaries::common::raw_property_type(
                self.ctx.types,
                interface_type,
                atom,
            ) else {
                continue;
            };
            if let Some(substitution) = substitution.as_ref() {
                member_type = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    member_type,
                    substitution,
                );
            }
            if member_type != TypeId::UNKNOWN && member_type != TypeId::ERROR {
                results.insert(member_idx, member_type);
            }
        }

        (!results.is_empty()).then_some(results)
    }
}

#[cfg(test)]
mod tests {
    use super::{is_builtin_lib_file_name, is_external_package_declaration_file_name};
    use crate::state::CheckerState;
    use tsz_parser::parser::{ParserState, syntax_kind_ext};

    fn parse_interface_declarations(
        source: &str,
    ) -> (
        tsz_parser::parser::node::NodeArena,
        Vec<tsz_parser::NodeIndex>,
    ) {
        let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let source_file = arena
            .get_source_file_at(root)
            .expect("source file should parse");
        let declarations = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .filter(|idx| {
                arena
                    .get(*idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
            })
            .collect();
        (arena, declarations)
    }

    #[test]
    fn detects_npm_and_source_tree_builtin_lib_names() {
        assert!(is_builtin_lib_file_name("lib.es2024.d.ts"));
        assert!(is_builtin_lib_file_name("lib.dom.d.ts"));
        assert!(is_builtin_lib_file_name("es2024.d.ts"));
        assert!(is_builtin_lib_file_name("es2024.full.d.ts"));
        assert!(is_builtin_lib_file_name("dom.generated.d.ts"));
        assert!(is_builtin_lib_file_name("dom.iterable.generated.d.ts"));
        assert!(is_builtin_lib_file_name("webworker.asynciterable.d.ts"));
        assert!(is_builtin_lib_file_name("decorators.legacy.d.ts"));
    }

    #[test]
    fn does_not_treat_arbitrary_declaration_files_as_builtin_libs() {
        assert!(!is_builtin_lib_file_name("react/index.d.ts"));
        assert!(!is_builtin_lib_file_name(
            "node_modules/@types/node/fs.d.ts"
        ));
        assert!(!is_builtin_lib_file_name("packages/foo/src/types.d.ts"));
    }

    #[test]
    fn detects_external_package_declaration_paths() {
        assert!(is_external_package_declaration_file_name(
            "node_modules/react/index.d.ts"
        ));
        assert!(is_external_package_declaration_file_name(
            "/repo/node_modules/@types/node/fs.d.ts"
        ));
        assert!(is_external_package_declaration_file_name(
            r"C:\repo\node_modules\@types\node\fs.d.ts"
        ));
    }

    #[test]
    fn does_not_treat_local_declaration_paths_as_external_packages() {
        assert!(!is_external_package_declaration_file_name(
            "packages/foo/src/types.d.ts"
        ));
        assert!(!is_external_package_declaration_file_name(
            "/repo/fixtures/node-modules-like/types.d.ts"
        ));
    }

    #[test]
    fn source_file_direct_interface_lowering_accepts_scope_independent_members() {
        let (arena, declarations) = parse_interface_declarations(
            r#"
                interface Leaf {
                    value: number;
                    tag: "leaf";
                    flags: true | false;
                }
            "#,
        );
        let declarations = vec![(declarations[0], &arena)];

        assert!(
            CheckerState::source_file_interface_declarations_are_direct_lowerable(&declarations,)
        );
    }

    #[test]
    fn source_file_direct_interface_lowering_rejects_scope_dependent_members() {
        let (arena, declarations) = parse_interface_declarations(
            r#"
                interface Local { value: number; }
                interface UsesLocal { value: Local; }
            "#,
        );
        let declarations = vec![(declarations[1], &arena)];

        assert!(
            !CheckerState::source_file_interface_declarations_are_direct_lowerable(&declarations,)
        );
    }
}
