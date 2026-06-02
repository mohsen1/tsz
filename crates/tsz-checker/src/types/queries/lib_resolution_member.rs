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
//! - Own or heritage-inherited **plain property signatures** only (`prop: T`).
//!   Methods, accessors, index signatures, call/construct signatures, and
//!   computed/symbol-named members take the full path.
//! - A single declaration of the member on any visited interface. Members
//!   declared more than once (overloads / split declarations / duplicate
//!   inheritance hits) take the full path.
//! - Fast inheritance is limited to non-generic `extends` entries. Any `extends`
//!   with type arguments or unsupported heritage shape falls back to the full
//!   path.
//!
//! [`resolve_lib_type_by_name`]: super::lib_resolution::CheckerState::resolve_lib_type_by_name

use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::lib_decls::{collect_lib_decls_with_arenas_in_contexts, resolve_lib_fallback_arena};
use super::lib_name_text::entity_name_text_in_arena;
use super::lib_resolution::{lib_def_id_from_node, resolve_lib_node_in_arenas};
use super::lib_resolution_selected::selected_lib_symbol_for_name;

use crate::state::CheckerState;

impl CheckerState<'_> {
    /// Resolve a single **plain property** `prop_name` of the simple lib
    /// interface named `name`, including inheritance through safe `extends` steps,
    /// without materializing the rest of the interface.
    ///
    /// Returns `None` (caller falls back to full materialization) when:
    /// - the interface symbol cannot be selected,
    /// - the interface/declarations are not a supported fast-path shape,
    /// - the member is not an own or inherited plain property signature,
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
        self.resolve_simple_lib_interface_property(name, prop_name, &mut FxHashSet::default())
            .ok()
            .flatten()
    }

    fn resolve_simple_lib_interface_property(
        &mut self,
        name: &str,
        prop_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> Result<Option<TypeId>, ()> {
        if !visited.insert(name.to_string()) {
            return Err(());
        }
        if self.ctx.skip_lib_type_resolution {
            return Err(());
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
            selected_lib_symbol_for_name(&self.ctx, name, sym_id, &lib_binders).ok_or(())?;
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        let symbol = selected_binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .ok_or(())?;
        if !symbol.has_any_flags(symbol_flags::INTERFACE) {
            return Err(());
        }

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

        // Find the single plain-property-signature declaration of `prop_name`
        // across this interface's declarations. Bail (`Err`) on any ambiguity so
        // overloads/split declarations keep their full-path semantics.
        let mut member: Option<(NodeIndex, &NodeArena)> = None;
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
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
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
                if member.is_some() {
                    // Declared more than once on this interface — ambiguous.
                    return Err(());
                }
                member = Some((member_idx, arena));
            }
        }

        let mut resolved_member = if let Some((member_idx, member_arena)) = member {
            let member_node = member_arena.get(member_idx).ok_or(())?;
            let sig = member_arena.get_signature(member_node).ok_or(())?;
            if sig.type_annotation == NodeIndex::NONE {
                // `prop;` with no annotation lowers to `any` in the full path;
                // that is cheap, but keep the full path authoritative for the
                // implicit shape rather than reimplement the default here.
                return Err(());
            }
            // Optional and readonly properties carry extra read/write semantics
            // (`?` interacts with `exactOptionalPropertyTypes`; `readonly`
            // affects the write type). Leave those on the full path so their exact
            // behavior is authoritative.
            if sig.question_token || self.has_readonly_modifier(&sig.modifiers) {
                return Err(());
            }

            // Build the same hybrid-resolver TypeLowering the full lib path uses, so
            // the member annotation lowers to a byte-identical type.
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                resolve_lib_node_in_arenas(
                    selected_binder,
                    node_idx,
                    &decls_with_arenas,
                    fallback_arena,
                )
                .map(|sym_id| sym_id.0)
            };
            let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                lib_def_id_from_node(
                    &self.ctx,
                    selected_binder,
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
                return Err(());
            }
            Some(member_type)
        } else {
            None
        };

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
                    if arena.get(type_idx).is_none() {
                        continue;
                    }
                    let Some((base_expr_idx, has_type_args)) =
                        self.get_heritage_expr_and_type_args(arena, type_idx)
                    else {
                        return Err(());
                    };
                    if has_type_args {
                        return Err(());
                    }
                    let Some(base_name) = entity_name_text_in_arena(arena, base_expr_idx) else {
                        return Err(());
                    };

                    match self.resolve_simple_lib_interface_property(&base_name, prop_name, visited)
                    {
                        Ok(Some(base_member)) => {
                            if resolved_member.is_some() {
                                return Err(());
                            }
                            resolved_member = Some(base_member);
                        }
                        Ok(None) => {}
                        Err(()) => return Err(()),
                    }
                }
            }
        }

        Ok(resolved_member)
    }

    fn get_heritage_expr_and_type_args(
        &self,
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<(NodeIndex, bool)> {
        let type_node = arena.get(type_idx)?;
        if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
            let has_type_args = expr_type_args
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty());
            return Some((expr_type_args.expression, has_type_args));
        }

        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let tr = arena.get_type_ref(type_node)?;
            let has_type_args = tr
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty());
            return Some((tr.type_name, has_type_args));
        }

        Some((type_idx, false))
    }
}
