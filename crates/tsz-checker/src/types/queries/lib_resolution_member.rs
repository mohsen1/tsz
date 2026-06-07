//! Single-member resolution for simple lib-interface property access.
//!
//! Value-position property access on a lib-interface receiver (e.g.
//! `document.title`) only needs the accessed member's type, not the receiver's
//! entire structural shape. [`resolve_lib_type_by_name`] lowers **every** member
//! and the transitive `extends` closure (~9216 interned types for `Document`);
//! this helper lowers **only** the requested property by reusing the exact same
//! `TypeLowering` configuration the full path uses, so the resulting member type
//! is byte-identical.
//!
//! Scope (intentionally narrow for soundness — anything else returns `None` and
//! falls back to the full-materialization path):
//! - Plain property signatures (`prop: T` / `prop?: T`) and unambiguous method
//!   signature groups. Accessors, index signatures, call/construct signatures,
//!   readonly writes, optional methods, and computed/symbol-named members take
//!   the full path.
//! - A single own property declaration, or one method overload group in one
//!   arena. Split declarations and mixed property/method members take the full
//!   path.
//! - Heritage-inherited members can be resolved when the inherited annotation
//!   does not reference the base interface's type parameters. Parameter-dependent
//!   inherited members fall back to full materialization for substitution.
//!
//! [`resolve_lib_type_by_name`]: super::lib_resolution::CheckerState::resolve_lib_type_by_name

use tsz_lowering::TypeLowering;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext::{self, METHOD_SIGNATURE, PROPERTY_SIGNATURE};
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::lib_decls::{collect_lib_decls_with_arenas_in_contexts, resolve_lib_fallback_arena};
use super::lib_name_text::entity_name_text_in_arena;
use super::lib_resolution::{lib_def_id_from_node, resolve_lib_node_in_arenas};
use super::lib_resolution_selected::selected_lib_symbol_for_name;

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId, symbol_flags};

impl CheckerState<'_> {
    /// Resolve a single plain property or unambiguous method group `prop_name`
    /// of the simple lib interface named `name`, returning its lowered member
    /// type without materializing the rest of the interface.
    ///
    /// Returns `None` (caller falls back to full materialization) when:
    /// - the interface symbol cannot be selected,
    /// - the member is not a plain property signature,
    /// - the member is declared more than once (overload/split declaration),
    /// - the member has a computed/symbol name, or
    /// - lowering the member's annotation fails.
    ///
    /// The member's type is produced by the same `TypeLowering::lower_type` call
    /// the full lib path (`lower_merged_interface_declarations`) uses, so the
    /// result is byte-identical to full materialization for the eligible shape.
    pub(crate) fn resolve_simple_lib_interface_own_property(
        &mut self,
        name: &str,
        prop_name: &str,
    ) -> Option<TypeId> {
        let mut visited = FxHashSet::default();
        self.resolve_simple_lib_interface_property(name, prop_name, &mut visited)
    }

    fn resolve_simple_lib_interface_property(
        &mut self,
        name: &str,
        prop_name: &str,
        visited: &mut FxHashSet<SymbolId>,
    ) -> Option<TypeId> {
        if self.ctx.skip_lib_type_resolution {
            return None;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();
        let lib_binders = self.get_lib_binders();

        let sym_id = if self.ctx.file_local_type_shadow_for_lib_name(name) {
            None
        } else {
            self.ctx.binder.file_locals.get(name)
        }
        .or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        });

        let (sym_id, selected_binder_arc) =
            selected_lib_symbol_for_name(&self.ctx, name, sym_id, &lib_binders)?;
        if !visited.insert(sym_id) {
            return None;
        }
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        let symbol = selected_binder.get_symbol_with_libs(sym_id, &lib_binders)?;

        let fallback_arena =
            resolve_lib_fallback_arena(selected_binder, sym_id, &lib_contexts, self.ctx.arena);
        let decls_with_arenas = collect_lib_decls_with_arenas_in_contexts(
            selected_binder,
            sym_id,
            &symbol.declarations,
            fallback_arena,
            &lib_contexts,
            Some(self.ctx.arena),
        );

        // Find the single own plain-property-signature declaration of `prop_name`
        // across the interface's declarations. Bail (None) on any ambiguity so
        // overloads/split declarations keep their full-path semantics.
        enum MemberMatch<'arena> {
            Property(NodeIndex, &'arena NodeArena, Vec<SymbolId>),
            Methods(Vec<NodeIndex>, &'arena NodeArena, Vec<SymbolId>),
        }
        let mut member: Option<MemberMatch<'_>> = None;
        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                // A merged declaration that is not an interface body (e.g. the
                // companion `declare var Document: { ... }` value declaration).
                // Skip it; the interface body decl is elsewhere in the list.
                continue;
            };
            let type_param_symbols = interface
                .type_parameters
                .as_ref()
                .map(|params| {
                    self.lib_interface_type_param_symbols(
                        selected_binder,
                        arena,
                        params,
                        &decls_with_arenas,
                        fallback_arena,
                    )
                })
                .unwrap_or_default();
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != PROPERTY_SIGNATURE && member_node.kind != METHOD_SIGNATURE {
                    continue;
                }
                let Some(sig) = arena.get_signature(member_node) else {
                    continue;
                };
                // Plain identifier member name only — string-literal, computed,
                // and symbol names take the full path so their exact naming
                // semantics (quoting, symbol keys) stay authoritative.
                let Some(member_name) = arena.get_identifier_text(sig.name) else {
                    continue;
                };
                if member_name != prop_name {
                    continue;
                }
                match (&mut member, member_node.kind) {
                    (None, k) if k == PROPERTY_SIGNATURE => {
                        member = Some(MemberMatch::Property(
                            member_idx,
                            arena,
                            type_param_symbols.clone(),
                        ));
                    }
                    (None, k) if k == METHOD_SIGNATURE => {
                        if sig.question_token {
                            return None;
                        }
                        member = Some(MemberMatch::Methods(
                            vec![member_idx],
                            arena,
                            type_param_symbols.clone(),
                        ));
                    }
                    (Some(MemberMatch::Methods(methods, existing_arena, _)), k)
                        if k == METHOD_SIGNATURE && std::ptr::eq(*existing_arena, arena) =>
                    {
                        if sig.question_token {
                            return None;
                        }
                        methods.push(member_idx);
                    }
                    _ => {
                        // Mixed property/method declarations, duplicate properties, or
                        // overloads split across arenas are ambiguous; fall back.
                        return None;
                    }
                }
            }
        }

        let member = if let Some(member) = member {
            member
        } else {
            let mut heritage_bases = Vec::new();
            for &(decl_idx, arena) in &decls_with_arenas {
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                let Some(interface) = arena.get_interface(node) else {
                    continue;
                };
                let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                    continue;
                };
                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = arena.get(type_idx) else {
                            continue;
                        };
                        let (expr_idx, _type_arguments) =
                            if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                                (
                                    expr_type_args.expression,
                                    expr_type_args.type_arguments.as_ref(),
                                )
                            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                if let Some(type_ref) = arena.get_type_ref(type_node) {
                                    (type_ref.type_name, type_ref.type_arguments.as_ref())
                                } else {
                                    (type_idx, None)
                                }
                            } else {
                                (type_idx, None)
                            };
                        if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx) {
                            heritage_bases.push(base_name.to_string());
                        }
                    }
                }
            }
            let mut inherited_member = None;
            for base_name in heritage_bases {
                if let Some(member_type) =
                    self.resolve_simple_lib_interface_property(&base_name, prop_name, visited)
                {
                    if inherited_member.is_some() {
                        return None;
                    }
                    inherited_member = Some(member_type);
                }
            }
            return inherited_member;
        };
        let (member_idx, member_arena, type_param_symbols) = match member {
            MemberMatch::Property(member_idx, member_arena, type_param_symbols) => {
                (member_idx, member_arena, type_param_symbols)
            }
            MemberMatch::Methods(methods, member_arena, type_param_symbols) => {
                return self.lower_simple_lib_interface_method_group(
                    member_arena,
                    &methods,
                    &type_param_symbols,
                    selected_binder,
                    &decls_with_arenas,
                    fallback_arena,
                );
            }
        };
        let member_node = member_arena.get(member_idx)?;
        let sig = member_arena.get_signature(member_node)?;
        if sig.type_annotation == NodeIndex::NONE {
            // `prop;` with no annotation lowers to `any` in the full path; that
            // is cheap, but keep the full path authoritative for the implicit
            // shape rather than reimplement the default here.
            return None;
        }
        if !type_param_symbols.is_empty()
            && self.type_annotation_references_type_params(
                selected_binder,
                member_arena,
                sig.type_annotation,
                &type_param_symbols,
                &decls_with_arenas,
                fallback_arena,
            )
        {
            return None;
        }
        // Readonly properties carry extra write semantics. Leave those on the
        // full path so their exact behavior is authoritative. Optional plain
        // properties are safe here because property access returns the read
        // annotation type; optionality itself is tracked by full object shapes.
        if self.has_readonly_modifier(&sig.modifiers) {
            return None;
        }

        // Build the same hybrid-resolver TypeLowering the full lib path uses, so
        // the member annotation lowers to a byte-identical type.
        let binder = selected_binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
                .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            lib_def_id_from_node(
                &self.ctx,
                binder,
                node_idx,
                &decls_with_arenas,
                fallback_arena,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            fallback_arena,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &resolver,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_name_def_id_resolver(&name_resolver);
        let lowering =
            if self.ctx.all_binders.is_some() || self.ctx.global_file_locals_index.is_some() {
                lowering.prefer_name_def_id_resolution()
            } else {
                lowering
            };

        let member_type = lowering
            .with_arena(member_arena)
            .lower_type(sig.type_annotation);
        if member_type == TypeId::ERROR {
            return None;
        }
        // A member whose annotation is itself a bare lib-interface reference
        // (e.g. `body: HTMLElement`) can stay lazy: lowering produces a
        // `Lazy(DefId)` ref — the same shape PR #8638 keeps for type-position
        // annotations — so chained access like `document.body.innerHTML` resolves
        // each link through the single-member fast path instead of materializing
        // the intermediate interface. Keep the lazy result only when the lowered
        // reference is itself an eligible simple lib interface (non-generic,
        // unmerged, unaugmented, unshadowed); that guarantees downstream property
        // access on the returned `Lazy(DefId)` resolves identically to
        // materializing the reference. The eligibility check is necessarily
        // post-lowering: it inspects the lowered shape (a bare `Lazy` stays
        // eligible, while a generic/augmented reference becomes an `Application`
        // and falls back to full materialization with its authoritative shape).
        if self.type_annotation_is_lib_interface_reference(
            member_arena,
            sig.type_annotation,
            &lib_binders,
        ) && self.lazy_lib_member_receiver_def_id(member_type).is_none()
        {
            return None;
        }
        Some(member_type)
    }

    fn lower_simple_lib_interface_method_group(
        &mut self,
        member_arena: &NodeArena,
        methods: &[NodeIndex],
        type_param_symbols: &[SymbolId],
        selected_binder: &BinderState,
        decls_with_arenas: &[(NodeIndex, &NodeArena)],
        fallback_arena: &NodeArena,
    ) -> Option<TypeId> {
        if methods.is_empty() || !type_param_symbols.is_empty() {
            return None;
        }

        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(selected_binder, node_idx, decls_with_arenas, fallback_arena)
                .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            lib_def_id_from_node(
                &self.ctx,
                selected_binder,
                node_idx,
                decls_with_arenas,
                fallback_arena,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            fallback_arena,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &resolver,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_name_def_id_resolver(&name_resolver);
        let lowering =
            if self.ctx.all_binders.is_some() || self.ctx.global_file_locals_index.is_some() {
                lowering.prefer_name_def_id_resolution()
            } else {
                lowering
            };
        lowering
            .with_arena(member_arena)
            .lower_method_signature_group(methods)
    }

    fn type_annotation_is_lib_interface_reference(
        &self,
        arena: &NodeArena,
        type_idx: NodeIndex,
        lib_binders: &[Arc<BinderState>],
    ) -> bool {
        let Some(type_ref) = arena
            .get(type_idx)
            .and_then(|node| arena.get_type_ref(node))
        else {
            return false;
        };
        let Some(type_name) = entity_name_text_in_arena(arena, type_ref.type_name) else {
            return false;
        };
        if self.ctx.file_local_type_shadow_for_lib_name(&type_name) {
            return false;
        }
        let sym_id = self.ctx.binder.file_locals.get(&type_name).or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(&type_name, lib_binders)
        });
        let Some((sym_id, selected_binder_arc)) =
            selected_lib_symbol_for_name(&self.ctx, &type_name, sym_id, lib_binders)
        else {
            return false;
        };
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        selected_binder
            .get_symbol_with_libs(sym_id, lib_binders)
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::INTERFACE))
    }

    fn lib_interface_type_param_symbols(
        &self,
        binder: &tsz_binder::BinderState,
        arena: &NodeArena,
        params: &tsz_parser::parser::NodeList,
        decls_with_arenas: &[(NodeIndex, &NodeArena)],
        fallback_arena: &NodeArena,
    ) -> Vec<SymbolId> {
        params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                resolve_lib_node_in_arenas(binder, param.name, decls_with_arenas, fallback_arena)
            })
            .collect()
    }

    fn type_annotation_references_type_params(
        &self,
        binder: &tsz_binder::BinderState,
        arena: &NodeArena,
        root: NodeIndex,
        type_param_symbols: &[SymbolId],
        decls_with_arenas: &[(NodeIndex, &NodeArena)],
        fallback_arena: &NodeArena,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if arena.get_identifier_text(idx).is_some()
                && let Some(sym_id) =
                    resolve_lib_node_in_arenas(binder, idx, decls_with_arenas, fallback_arena)
                && type_param_symbols.contains(&sym_id)
            {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }
}
