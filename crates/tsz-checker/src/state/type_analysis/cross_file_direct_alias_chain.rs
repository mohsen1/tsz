use super::source_alias_attribution::record_source_alias_rejection_kinds;
use crate::state::CheckerState;
use tsz_binder::{BinderState, Symbol, SymbolId, symbol_flags};
use tsz_parser::NodeList;
use tsz_parser::parser::node::{NodeArena, TypeAliasData};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceFileAliasProofKey {
    file_idx: Option<usize>,
    sym_id: SymbolId,
    guarded: bool,
}

type SourceFileImportAliasTarget<'a> =
    dyn Fn(usize, &BinderState, SymbolId) -> Option<SourceFileAliasSymbol<'a>> + 'a;

pub(super) struct SourceFileAliasProofContext<'a> {
    pub(super) current_file_idx: Option<usize>,
    pub(super) global_type_is_lowerable: &'a dyn Fn(&BinderState, &str) -> bool,
    pub(super) import_alias_target: Option<&'a SourceFileImportAliasTarget<'a>>,
}

#[derive(Clone, Copy)]
pub(super) struct SourceFileAliasSymbol<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) binder: &'a BinderState,
    pub(super) file_idx: Option<usize>,
    pub(super) sym_id: SymbolId,
}

impl<'a> SourceFileAliasProofContext<'a> {
    fn for_file(&self, current_file_idx: Option<usize>) -> SourceFileAliasProofContext<'a> {
        SourceFileAliasProofContext {
            current_file_idx,
            global_type_is_lowerable: self.global_type_is_lowerable,
            import_alias_target: self.import_alias_target,
        }
    }
}

impl<'a> CheckerState<'a> {
    const SOURCE_FILE_ALIAS_PROOF_DEPTH_LIMIT: usize = 128;

    fn source_file_alias_proof_seen_contains(
        seen: &[SourceFileAliasProofKey],
        key: SourceFileAliasProofKey,
    ) -> bool {
        seen.iter()
            .any(|visited| visited.file_idx == key.file_idx && visited.sym_id == key.sym_id)
    }

    fn source_file_alias_proof_cycle_is_guarded(
        seen: &[SourceFileAliasProofKey],
        key: SourceFileAliasProofKey,
    ) -> bool {
        let Some(index) = seen
            .iter()
            .position(|visited| visited.file_idx == key.file_idx && visited.sym_id == key.sym_id)
        else {
            return false;
        };
        key.guarded || seen[index + 1..].iter().any(|visited| visited.guarded)
    }

    fn source_file_alias_proof_seen_push(
        seen: &mut Vec<SourceFileAliasProofKey>,
        key: SourceFileAliasProofKey,
    ) -> bool {
        if seen.len() >= Self::SOURCE_FILE_ALIAS_PROOF_DEPTH_LIMIT
            || Self::source_file_alias_proof_seen_contains(seen, key)
        {
            return false;
        }
        seen.push(key);
        true
    }

    fn source_file_alias_proof_seen_pop(
        seen: &mut Vec<SourceFileAliasProofKey>,
        key: SourceFileAliasProofKey,
    ) {
        if seen
            .last()
            .is_some_and(|visited| visited.file_idx == key.file_idx && visited.sym_id == key.sym_id)
        {
            seen.pop();
        }
    }

    pub(super) fn source_file_type_node_is_generic_local_alias_application_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        global_type_is_lowerable: &dyn Fn(&str) -> bool,
    ) -> bool {
        let global_type_is_lowerable_for_binder =
            |_: &BinderState, type_name: &str| global_type_is_lowerable(type_name);
        let proof = SourceFileAliasProofContext {
            current_file_idx: None,
            global_type_is_lowerable: &global_type_is_lowerable_for_binder,
            import_alias_target: None,
        };
        let mut seen = Vec::new();
        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
            arena,
            binder,
            node_idx,
            type_param_names,
            &mut seen,
            &proof,
        )
    }

    pub(super) fn source_file_type_node_is_non_generic_local_alias_chain_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        global_type_is_lowerable: &dyn Fn(&str) -> bool,
    ) -> bool {
        let global_type_is_lowerable_for_binder =
            |_: &BinderState, type_name: &str| global_type_is_lowerable(type_name);
        let proof = SourceFileAliasProofContext {
            current_file_idx: None,
            global_type_is_lowerable: &global_type_is_lowerable_for_binder,
            import_alias_target: None,
        };
        let mut seen = Vec::new();
        Self::source_file_type_node_is_local_alias_chain_lowerable(
            arena, binder, node_idx, &mut seen, &proof,
        )
    }

    pub(super) fn source_file_alias_body_node_is_direct_lowerable_for_attribution(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        current_file_idx: usize,
        direct_source_file_arena: bool,
        type_param_names: &[String],
        node_idx: NodeIndex,
    ) -> bool {
        let global_type_is_lowerable = |binder: &BinderState, type_name: &str| {
            self.source_file_global_type_is_direct_lowerable(binder, type_name)
        };
        let import_alias_target =
            |source_file_idx: usize, binder: &BinderState, sym_id: SymbolId| {
                self.source_file_import_alias_target_for_lowering(source_file_idx, binder, sym_id)
            };
        let proof = SourceFileAliasProofContext {
            current_file_idx: Some(current_file_idx),
            global_type_is_lowerable: &global_type_is_lowerable,
            import_alias_target: Some(&import_alias_target),
        };
        let mut seen = Vec::new();
        if type_param_names.is_empty() {
            Self::source_file_type_node_is_scope_independent(arena, node_idx)
                || (direct_source_file_arena
                    && Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena, binder, node_idx, &mut seen, &proof,
                    ))
        } else if direct_source_file_arena {
            Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                arena,
                binder,
                node_idx,
                type_param_names,
                &mut seen,
                &proof,
            )
        } else {
            Self::source_file_type_node_is_generic_scope_independent(
                arena,
                node_idx,
                type_param_names,
            )
        }
    }

    pub(super) fn record_source_alias_rejection_kinds_for_direct_proof(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        type_alias: &TypeAliasData,
        current_file_idx: usize,
        direct_source_file_arena: bool,
        type_param_names: &[String],
    ) {
        let type_node_is_lowerable = |node_idx| {
            self.source_file_alias_body_node_is_direct_lowerable_for_attribution(
                arena,
                binder,
                current_file_idx,
                direct_source_file_arena,
                type_param_names,
                node_idx,
            )
        };
        record_source_alias_rejection_kinds(
            arena,
            binder,
            type_alias,
            type_param_names,
            &type_node_is_lowerable,
        );
    }

    pub(super) fn source_file_type_node_is_generic_local_alias_application_lowerable_with_seen<
        'b,
    >(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
            arena,
            binder,
            node_idx,
            type_param_names,
            seen,
            proof,
            false,
        )
    }

    fn source_file_type_node_is_generic_local_alias_application_lowerable_with_guard<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        if Self::source_file_type_node_is_generic_scope_independent(
            arena,
            node_idx,
            type_param_names,
        ) {
            return true;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        node_idx,
                        seen,
                        proof,
                    );
                };
                if args.nodes.is_empty() {
                    return Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        node_idx,
                        seen,
                        proof,
                    );
                }
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };
                let Some(raw_sym_id) = binder.file_locals.get(name) else {
                    return (proof.global_type_is_lowerable)(binder, name)
                        && args.nodes.iter().copied().all(|arg| {
                            Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                                arena,
                                binder,
                                arg,
                                type_param_names,
                                seen,
                                proof,
                                recursion_guarded,
                            )
                        });
                };
                if Self::source_file_local_symbol_can_fall_back_to_global_type(
                    arena, binder, raw_sym_id,
                ) {
                    return (proof.global_type_is_lowerable)(binder, name)
                        && args.nodes.iter().copied().all(|arg| {
                            Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                                arena,
                                binder,
                                arg,
                                type_param_names,
                                seen,
                                proof,
                                recursion_guarded,
                            )
                        });
                }
                let Some(resolved) =
                    Self::source_file_resolve_alias_symbol_for_lowering(arena, binder, raw_sym_id, proof)
                else {
                    return false;
                };
                let key = SourceFileAliasProofKey {
                    file_idx: resolved.file_idx,
                    sym_id: resolved.sym_id,
                    guarded: recursion_guarded,
                };
                if Self::source_file_alias_proof_seen_contains(seen, key) {
                    return Self::source_file_alias_proof_cycle_is_guarded(seen, key);
                }
                let Some(symbol) = resolved.binder.get_symbol(resolved.sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::INTERFACE != 0 {
                    return Self::source_file_local_interface_application_is_lowerable(
                        resolved.arena,
                        symbol,
                        args.nodes.len(),
                    ) && args.nodes.iter().copied().all(|arg| {
                        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                            arena,
                            binder,
                            arg,
                            type_param_names,
                            seen,
                            proof,
                            recursion_guarded,
                        )
                    });
                }
                if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
                    return false;
                }
                let disallowed = symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if symbol.flags & disallowed != 0 || symbol.declarations.len() != 1 {
                    return false;
                }
                let decl_idx = symbol.declarations[0];
                let Some(decl_node) = resolved.arena.get(decl_idx) else {
                    return false;
                };
                let Some(type_alias) = resolved.arena.get_type_alias(decl_node) else {
                    return false;
                };
                let resolved_proof = proof.for_file(resolved.file_idx);
                let Some(target_param_names) =
                    Self::source_file_type_alias_application_param_names_are_lowerable(
                        resolved.arena,
                        resolved.binder,
                        type_alias,
                        args.nodes.len(),
                        seen,
                        &resolved_proof,
                    )
                else {
                    return false;
                };
                if !args.nodes.iter().copied().all(|arg| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        arg,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                }) {
                    return false;
                }
                if Self::source_file_type_node_contains_kind(
                    resolved.arena,
                    type_alias.type_node,
                    syntax_kind_ext::TYPE_QUERY,
                ) {
                    return false;
                }
                if !Self::source_file_alias_proof_seen_push(seen, key) {
                    return false;
                }
                let result =
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        resolved.arena,
                        resolved.binder,
                        type_alias.type_node,
                        &target_param_names,
                        seen,
                        &resolved_proof,
                        false,
                    );
                Self::source_file_alias_proof_seen_pop(seen, key);
                result
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                arena.get_conditional_type(node).is_some_and(|conditional| {
                    let mut true_branch_names = type_param_names.to_vec();
                    Self::collect_infer_type_param_names(
                        arena,
                        conditional.extends_type,
                        &mut true_branch_names,
                    );
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        conditional.check_type,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        conditional.extends_type,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        conditional.true_type,
                        &true_branch_names,
                        seen,
                        proof,
                        recursion_guarded,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        conditional.false_type,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => arena.get_array_type(node).is_some_and(|array| {
                Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    array.element_type,
                    type_param_names,
                    seen,
                    proof,
                    true,
                )
            }),
            k if k == syntax_kind_ext::TUPLE_TYPE => arena.get_tuple_type(node).is_some_and(|tuple| {
                tuple.elements.nodes.iter().copied().all(|element| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        element,
                        type_param_names,
                        seen,
                        proof,
                        true,
                    )
                })
            }),
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                            arena,
                            binder,
                            member,
                            type_param_names,
                            seen,
                            proof,
                            recursion_guarded,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        wrapped.type_node,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        operator.type_node,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_indexed_access_object_is_generic_local_alias_application_lowerable(
                        arena,
                        binder,
                        indexed.object_type,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                        arena,
                        binder,
                        indexed.index_type,
                        type_param_names,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                Self::source_file_mapped_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                Self::source_file_type_literal_has_lowerable_properties(
                    arena,
                    binder,
                    node,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                Self::source_file_template_literal_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                Self::source_file_function_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            _ => false,
        }
    }

    pub(super) fn source_file_type_node_is_local_alias_chain_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
            arena, binder, node_idx, seen, proof, false,
        )
    }

    fn source_file_type_node_is_local_alias_chain_lowerable_with_guard<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let has_type_arguments = type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };
                let Some(raw_sym_id) = binder.file_locals.get(name) else {
                    return (proof.global_type_is_lowerable)(binder, name)
                        && (!has_type_arguments
                            || type_ref.type_arguments.as_ref().is_some_and(|args| {
                                args.nodes.iter().copied().all(|arg| {
                                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                                        arena,
                                        binder,
                                        arg,
                                        seen,
                                        proof,
                                        recursion_guarded,
                                    )
                                })
                            }));
                };
                if Self::source_file_local_symbol_can_fall_back_to_global_type(
                    arena, binder, raw_sym_id,
                ) {
                    return (proof.global_type_is_lowerable)(binder, name)
                        && (!has_type_arguments
                            || type_ref.type_arguments.as_ref().is_some_and(|args| {
                                args.nodes.iter().copied().all(|arg| {
                                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                                        arena,
                                        binder,
                                        arg,
                                        seen,
                                        proof,
                                        recursion_guarded,
                                    )
                                })
                            }));
                }
                let Some(resolved) = Self::source_file_resolve_alias_symbol_for_lowering(
                    arena, binder, raw_sym_id, proof,
                ) else {
                    return false;
                };
                let key = SourceFileAliasProofKey {
                    file_idx: resolved.file_idx,
                    sym_id: resolved.sym_id,
                    guarded: recursion_guarded,
                };
                if Self::source_file_alias_proof_seen_contains(seen, key) {
                    return Self::source_file_alias_proof_cycle_is_guarded(seen, key);
                }
                let Some(symbol) = resolved.binder.get_symbol(resolved.sym_id) else {
                    return false;
                };
                if has_type_arguments && symbol.flags & symbol_flags::INTERFACE != 0 {
                    let Some(args) = type_ref.type_arguments.as_ref() else {
                        return false;
                    };
                    return Self::source_file_local_interface_application_is_lowerable(
                        resolved.arena,
                        symbol,
                        args.nodes.len(),
                    ) && args.nodes.iter().copied().all(|arg| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                            arena,
                            binder,
                            arg,
                            seen,
                            proof,
                            recursion_guarded,
                        )
                    });
                }
                if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
                    return false;
                }
                let disallowed = symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if symbol.flags & disallowed != 0 || symbol.declarations.len() != 1 {
                    return false;
                }
                let decl_idx = symbol.declarations[0];
                let Some(decl_node) = resolved.arena.get(decl_idx) else {
                    return false;
                };
                let Some(type_alias) = resolved.arena.get_type_alias(decl_node) else {
                    return false;
                };
                let resolved_proof = proof.for_file(resolved.file_idx);
                if has_type_arguments {
                    let Some(args) = type_ref.type_arguments.as_ref() else {
                        return false;
                    };
                    let Some(target_param_names) =
                        Self::source_file_type_alias_application_param_names_are_lowerable(
                            resolved.arena,
                            resolved.binder,
                            type_alias,
                            args.nodes.len(),
                            seen,
                            &resolved_proof,
                        )
                    else {
                        return false;
                    };
                    if !args.nodes.iter().copied().all(|arg| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                            arena,
                            binder,
                            arg,
                            seen,
                            proof,
                            recursion_guarded,
                        )
                    }) {
                        return false;
                    }
                    if Self::source_file_type_node_contains_kind(
                        resolved.arena,
                        type_alias.type_node,
                        syntax_kind_ext::TYPE_QUERY,
                    ) || !Self::source_file_alias_proof_seen_push(seen, key)
                    {
                        return false;
                    }
                    let result =
                        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                            resolved.arena,
                            resolved.binder,
                            type_alias.type_node,
                            &target_param_names,
                            seen,
                            &resolved_proof,
                        );
                    Self::source_file_alias_proof_seen_pop(seen, key);
                    if !result {
                        return false;
                    }
                    return true;
                }
                let Some(target_param_names) =
                    Self::source_file_type_alias_application_param_names_are_lowerable(
                        resolved.arena,
                        resolved.binder,
                        type_alias,
                        0,
                        seen,
                        &resolved_proof,
                    )
                else {
                    return false;
                };
                if !Self::source_file_alias_proof_seen_push(seen, key) {
                    return false;
                }
                let result = if target_param_names.is_empty() {
                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                        resolved.arena,
                        resolved.binder,
                        type_alias.type_node,
                        seen,
                        &resolved_proof,
                        false,
                    )
                } else if Self::source_file_type_node_contains_kind(
                    resolved.arena,
                    type_alias.type_node,
                    syntax_kind_ext::TYPE_QUERY,
                ) {
                    false
                } else {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        resolved.arena,
                        resolved.binder,
                        type_alias.type_node,
                        &target_param_names,
                        seen,
                        &resolved_proof,
                    )
                };
                Self::source_file_alias_proof_seen_pop(seen, key);
                result
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                            arena,
                            binder,
                            member,
                            seen,
                            proof,
                            recursion_guarded,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                        arena,
                        binder,
                        array.element_type,
                        seen,
                        proof,
                        true,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                            arena, binder, element, seen, proof, true,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                        arena,
                        binder,
                        wrapped.type_node,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                        arena,
                        binder,
                        operator.type_node,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_indexed_access_object_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        indexed.object_type,
                        seen,
                        proof,
                        recursion_guarded,
                    ) && Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                        arena,
                        binder,
                        indexed.index_type,
                        seen,
                        proof,
                        recursion_guarded,
                    )
                })
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                Self::source_file_mapped_type_is_local_alias_chain_lowerable(
                    arena,
                    binder,
                    node,
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                Self::source_file_type_literal_has_local_alias_chain_lowerable_properties(
                    arena, binder, node, seen, proof,
                )
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                Self::source_file_template_literal_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    &[],
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                Self::source_file_function_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    &[],
                    seen,
                    proof,
                    recursion_guarded,
                )
            }
            _ => false,
        }
    }

    fn source_file_local_symbol_can_fall_back_to_global_type(
        arena: &NodeArena,
        binder: &BinderState,
        sym_id: SymbolId,
    ) -> bool {
        let Some(symbol) = binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::ALIAS != 0 {
            return false;
        }
        if symbol.flags & symbol_flags::TYPE == 0 {
            return true;
        }
        !Self::source_file_symbol_has_local_type_declaration(arena, symbol)
    }

    fn source_file_symbol_has_local_type_declaration(arena: &NodeArena, symbol: &Symbol) -> bool {
        symbol.declarations.iter().copied().any(|decl_idx| {
            arena.get(decl_idx).is_some_and(|decl| {
                decl.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || decl.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || decl.kind == syntax_kind_ext::CLASS_DECLARATION
                    || decl.kind == syntax_kind_ext::ENUM_DECLARATION
            })
        })
    }

    fn source_file_local_interface_application_is_lowerable(
        arena: &NodeArena,
        symbol: &Symbol,
        arg_count: usize,
    ) -> bool {
        let disallowed = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & symbol_flags::INTERFACE == 0
            || symbol.flags & disallowed != 0
            || symbol.declarations.len() != 1
        {
            return false;
        }

        let decl_idx = symbol.declarations[0];
        if !Self::lib_declaration_name_matches(arena, decl_idx, &symbol.escaped_name) {
            return false;
        }
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(interface) = arena.get_interface(decl_node) else {
            return false;
        };
        let param_count = interface
            .type_parameters
            .as_ref()
            .map_or(0, |params| params.nodes.len());
        if arg_count == 0 || arg_count != param_count {
            return false;
        }

        true
    }

    pub(super) fn source_file_import_alias_target_for_lowering<'b>(
        &'b self,
        source_file_idx: usize,
        binder: &BinderState,
        sym_id: SymbolId,
    ) -> Option<SourceFileAliasSymbol<'b>> {
        let symbol = binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        if import_name == "*" {
            return None;
        }
        let target_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let file_name = target_arena.source_files.first()?.file_name.as_str();
        let (target_sym_id, _) = target_binder
            .resolve_import_with_reexports_type_only(file_name, import_name)
            .or_else(|| {
                target_binder
                    .file_locals
                    .get(import_name)
                    .map(|sym_id| (sym_id, false))
            })?;
        self.ctx
            .register_symbol_file_target(target_sym_id, target_idx);
        Some(SourceFileAliasSymbol {
            arena: target_arena,
            binder: target_binder,
            file_idx: Some(target_idx),
            sym_id: target_sym_id,
        })
    }

    fn source_file_resolve_alias_symbol_for_lowering<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        sym_id: SymbolId,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> Option<SourceFileAliasSymbol<'b>> {
        let symbol = binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(SourceFileAliasSymbol {
                arena,
                binder,
                file_idx: proof.current_file_idx,
                sym_id,
            });
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        if import_name == "*" {
            return None;
        }
        if let Some((target_sym_id, _)) =
            binder.resolve_import_with_reexports_type_only(module_specifier, import_name)
            && target_sym_id != sym_id
        {
            return Some(SourceFileAliasSymbol {
                arena,
                binder,
                file_idx: proof.current_file_idx,
                sym_id: target_sym_id,
            });
        }
        let source_file_idx = proof.current_file_idx?;
        let import_alias_target = proof.import_alias_target?;
        import_alias_target(source_file_idx, binder, sym_id)
    }

    fn source_file_type_alias_application_param_names_are_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        type_alias: &tsz_parser::parser::node::TypeAliasData,
        arg_count: usize,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> Option<Vec<String>> {
        let Some(params) = type_alias.type_parameters.as_ref() else {
            return (arg_count == 0).then(Vec::new);
        };
        if arg_count > params.nodes.len() {
            return None;
        }
        let mut target_param_names = Vec::with_capacity(params.nodes.len());
        let mut param_nodes = Vec::with_capacity(params.nodes.len());
        for param_idx in params.nodes.iter().copied() {
            let param_node = arena.get(param_idx)?;
            let param = arena.get_type_parameter(param_node)?;
            let name_node = arena.get(param.name)?;
            let name = arena.get_identifier(name_node)?;
            target_param_names.push(name.escaped_text.to_string());
            param_nodes.push(param);
        }

        for param in param_nodes.iter().skip(arg_count) {
            if param.default.is_none()
                || !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                    arena,
                    binder,
                    param.default,
                    &target_param_names,
                    seen,
                    proof,
                )
            {
                return None;
            }
        }

        Some(target_param_names)
    }

    fn source_file_mapped_type_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        let Some(mapped) = arena.get_mapped_type(node) else {
            return false;
        };
        if mapped
            .members
            .as_ref()
            .is_some_and(|members| !members.nodes.is_empty())
        {
            return false;
        }
        let Some(type_param_node) = arena.get(mapped.type_parameter) else {
            return false;
        };
        let Some(type_param) = arena.get_type_parameter(type_param_node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_param.name) else {
            return false;
        };
        let Some(name) = arena.get_identifier(name_node) else {
            return false;
        };

        if !type_param.constraint.is_none()
            && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                type_param.constraint,
                type_param_names,
                seen,
                proof,
                recursion_guarded,
            )
        {
            return false;
        }
        if !type_param.default.is_none()
            && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                type_param.default,
                type_param_names,
                seen,
                proof,
                recursion_guarded,
            )
        {
            return false;
        }

        let mut mapped_param_names = type_param_names.to_vec();
        if !mapped_param_names
            .iter()
            .any(|param| param == &name.escaped_text)
        {
            mapped_param_names.push(name.escaped_text.to_string());
        }

        (mapped.name_type.is_none()
            || Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                mapped.name_type,
                &mapped_param_names,
                seen,
                proof,
                recursion_guarded,
            ))
            && (mapped.type_node.is_none()
                || Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    mapped.type_node,
                    &mapped_param_names,
                    seen,
                    proof,
                    true,
                ))
    }

    fn source_file_mapped_type_is_local_alias_chain_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        let Some(mapped) = arena.get_mapped_type(node) else {
            return false;
        };
        if mapped
            .members
            .as_ref()
            .is_some_and(|members| !members.nodes.is_empty())
        {
            return false;
        }
        let Some(type_param_node) = arena.get(mapped.type_parameter) else {
            return false;
        };
        let Some(type_param) = arena.get_type_parameter(type_param_node) else {
            return false;
        };

        if type_param.constraint.is_some()
            && !Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                type_param.constraint,
                seen,
                proof,
                recursion_guarded,
            )
        {
            return false;
        }
        if type_param.default.is_some()
            && !Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                type_param.default,
                seen,
                proof,
                recursion_guarded,
            )
        {
            return false;
        }

        (mapped.name_type.is_none()
            || Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                mapped.name_type,
                seen,
                proof,
                recursion_guarded,
            ))
            && (mapped.type_node.is_none()
                || Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                    arena,
                    binder,
                    mapped.type_node,
                    seen,
                    proof,
                    true,
                ))
    }

    fn source_file_indexed_access_object_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        if let Some(node) = arena.get(node_idx)
            && node.kind == syntax_kind_ext::TYPE_LITERAL
        {
            return Self::source_file_type_literal_has_lowerable_properties(
                arena,
                binder,
                node,
                type_param_names,
                seen,
                proof,
                recursion_guarded,
            );
        }
        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
            arena,
            binder,
            node_idx,
            type_param_names,
            seen,
            proof,
            recursion_guarded,
        )
    }

    fn source_file_indexed_access_object_is_local_alias_chain_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        if let Some(node) = arena.get(node_idx)
            && node.kind == syntax_kind_ext::TYPE_LITERAL
        {
            return Self::source_file_type_literal_has_local_alias_chain_lowerable_properties(
                arena, binder, node, seen, proof,
            );
        }
        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
            arena,
            binder,
            node_idx,
            seen,
            proof,
            recursion_guarded,
        )
    }

    fn source_file_type_literal_has_generic_scope_independent_properties(
        arena: &NodeArena,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(arena, node, |type_node| {
            Self::source_file_type_node_is_generic_scope_independent(
                arena,
                type_node,
                type_param_names,
            )
        })
    }

    fn source_file_type_literal_has_lowerable_properties<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        _recursion_guarded: bool,
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(arena, node, |type_node| {
            Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                type_node,
                type_param_names,
                seen,
                proof,
                true,
            )
        })
    }

    fn source_file_type_literal_has_local_alias_chain_lowerable_properties<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(arena, node, |type_node| {
            Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena, binder, type_node, seen, proof, true,
            )
        })
    }

    fn source_file_template_literal_type_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        let Some(template) = arena.get_template_literal_type(node) else {
            return false;
        };
        if arena.get(template.head).is_none() {
            return false;
        }
        template.template_spans.nodes.iter().copied().all(|span_idx| {
            let Some(span_node) = arena.get(span_idx) else {
                return false;
            };
            let Some(span) = arena.get_template_span(span_node) else {
                return false;
            };
            arena.get(span.literal).is_some()
                && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    span.expression,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
        })
    }

    fn source_file_function_type_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> bool {
        let Some(function_type) = arena.get_function_type(node) else {
            return false;
        };
        let Some(function_type_param_names) =
            Self::source_file_function_type_param_names_are_lowerable(
                arena,
                binder,
                function_type.type_parameters.as_ref(),
                type_param_names,
                seen,
                proof,
                recursion_guarded,
            )
        else {
            return false;
        };
        let active_type_param_names = if function_type_param_names.is_empty() {
            type_param_names.to_vec()
        } else {
            function_type_param_names
        };
        function_type.parameters.nodes.iter().copied().all(|param_idx| {
            let Some(param_node) = arena.get(param_idx) else {
                return false;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                return false;
            };
            param.type_annotation.is_some()
                && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.type_annotation,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
        }) && function_type.type_annotation.is_some()
            && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                function_type.type_annotation,
                &active_type_param_names,
                seen,
                proof,
                recursion_guarded,
            )
    }

    fn source_file_function_type_param_names_are_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        params: Option<&NodeList>,
        outer_type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
    ) -> Option<Vec<String>> {
        let Some(params) = params else {
            return Some(Vec::new());
        };
        let mut active_type_param_names = outer_type_param_names.to_vec();
        let mut param_data = Vec::with_capacity(params.nodes.len());
        for param_idx in params.nodes.iter().copied() {
            let param_node = arena.get(param_idx)?;
            let param = arena.get_type_parameter(param_node)?;
            let name_node = arena.get(param.name)?;
            let name = arena.get_identifier(name_node)?;
            if !active_type_param_names
                .iter()
                .any(|param_name| param_name == &name.escaped_text)
            {
                active_type_param_names.push(name.escaped_text.to_string());
            }
            param_data.push(param);
        }

        for param in param_data {
            if param.constraint.is_some()
                && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.constraint,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            {
                return None;
            }
            if param.default.is_some()
                && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.default,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                )
            {
                return None;
            }
        }

        Some(active_type_param_names)
    }

    fn source_file_type_literal_properties_are_lowerable(
        arena: &NodeArena,
        node: &tsz_parser::parser::node::Node,
        mut value_is_lowerable: impl FnMut(NodeIndex) -> bool,
    ) -> bool {
        let Some(type_literal) = arena.get_type_literal(node) else {
            return false;
        };
        type_literal
            .members
            .nodes
            .iter()
            .copied()
            .all(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    return false;
                }
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                if signature.type_parameters.is_some()
                    || signature.parameters.is_some()
                    || signature.type_annotation.is_none()
                {
                    return false;
                }
                if arena
                    .get(signature.name)
                    .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    return false;
                }
                value_is_lowerable(signature.type_annotation)
            })
    }
}
