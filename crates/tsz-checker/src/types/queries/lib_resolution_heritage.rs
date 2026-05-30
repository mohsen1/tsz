//! Heritage (`extends`) merging for library interface types.
//!
//! Split out of `lib_resolution` to keep that module under the file-size cap.
//! These helpers walk a lib interface's `extends` clauses in the
//! declaration-specific (lib) arenas — `merge_interface_heritage_types` cannot,
//! because it reads `self.ctx.arena` (the user file arena) only.

use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::lib_decls::{collect_lib_decls_with_arenas_in_contexts, resolve_lib_fallback_arena};
use super::lib_name_text::entity_name_text_in_arena;
use super::lib_resolution::{keyword_name_to_type_id, keyword_syntax_to_type_id};
use super::lib_scoped_heritage::LibHeritageBase;

fn select_external_module_lib_interface(
    name: &str,
    actual_lib_file_count: usize,
    lib_contexts: &[crate::context::LibContext],
) -> Option<(SymbolId, Option<Arc<BinderState>>)> {
    lib_contexts
        .iter()
        .take(actual_lib_file_count)
        .filter(|lib_ctx| lib_ctx.binder.is_external_module())
        .find_map(|lib_ctx| {
            let sym_id = lib_ctx.binder.file_locals.get(name)?;
            let symbol = lib_ctx.binder.get_symbol(sym_id)?;
            (symbol.escaped_name == name && symbol.has_any_flags(symbol_flags::INTERFACE))
                .then_some((sym_id, Some(Arc::clone(&lib_ctx.binder))))
        })
}

impl<'a> CheckerState<'a> {
    /// Merge base interface members into a lib interface type by walking
    /// heritage (`extends`) clauses in declaration-specific arenas.
    ///
    /// This is needed because `merge_interface_heritage_types` uses `self.ctx.arena`
    /// (the user file arena) and cannot read lib declarations that live in lib arenas.
    /// Takes the interface name and looks up declarations from the binder.
    pub(crate) fn merge_lib_interface_heritage(
        &mut self,
        mut derived_type: TypeId,
        name: &str,
    ) -> TypeId {
        // Guard against infinite recursion in recursive generic hierarchies
        // (e.g., interface B<T extends B<T,S>> extends A<B<T,S>, B<T,S>>)
        if !self.ctx.enter_recursion() {
            return derived_type;
        }

        // Name-based cycle guard: prevent re-entrant heritage merging for the same
        // interface name. This breaks the resolve_lib_type_by_name ↔ merge_lib_interface_heritage
        // mutual recursion that occurs through deep heritage chains
        // (e.g., Array → ReadonlyArray → Iterable → ...), especially when child
        // CheckerStates are created for cross-arena type param resolution.
        if !self.ctx.lib_heritage_in_progress.insert(name.to_string()) {
            self.ctx.leave_recursion();
            return derived_type;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();
        let lib_binders = self.get_lib_binders();

        // Resolve the interface symbol. Preserve the existing current-binder
        // path first: ordinary global libs and user augmentations rely on those
        // merged symbol identities. Only fall back to an actual lib-context
        // binder for module-scoped declarations from external-module lib files;
        // those are the structural case absent from the active binder's
        // `file_locals`, and broadening the fallback to every lib context lets
        // unrelated lib-local type parameters collide with user symbols.
        let direct_sym_id = name
            .split_once('.')
            .and_then(|(namespace, export_name)| {
                self.resolve_lib_namespace_export_symbol(namespace, export_name)
            })
            .or_else(|| self.resolve_lib_symbol_by_entity_name(name));

        let selected = direct_sym_id
            .filter(|&id| self.ctx.binder.get_symbol(id).is_some())
            .map(|id| (id, None))
            .or_else(|| {
                select_external_module_lib_interface(
                    name,
                    self.ctx.actual_lib_file_count,
                    &self.ctx.lib_contexts,
                )
            });
        let Some((sym_id, selected_binder_arc)) = selected else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        let Some(symbol) = selected_binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };

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

        // Early exit: skip expensive type parameter scope setup and heritage merge
        // if no declarations have extends clauses
        let has_any_heritage = decls_with_arenas.iter().any(|&(decl_idx, arena)| {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|hc| !hc.nodes.is_empty())
        });

        if !has_any_heritage {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        }

        // Seed type-parameter scope with the derived interface's generic params so
        // heritage args like `extends IteratorObject<T, ...>` resolve `T` correctly.
        // Without this, lib heritage substitution falls back to `unknown` and loses
        // member types (e.g. `ArrayIterator<T>.next().value` becomes `unknown`).
        let mut scope_restore: Vec<(String, Option<TypeId>)> = Vec::new();
        for param in self.get_type_params_for_symbol(sym_id) {
            let name = self.ctx.types.resolve_atom(param.name).to_string();
            let param_ty = self.ctx.types.type_param(param);
            let prev = self.ctx.type_parameter_scope.insert(name.clone(), param_ty);
            scope_restore.push((name, prev));
        }

        // Collect base type info: name and type argument node indices with their arena.
        // We collect these first to avoid borrow conflicts during resolution.
        let mut bases: Vec<LibHeritageBase<'_>> = Vec::new();

        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
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

                    // Extract the base type name and type arguments
                    let (expr_idx, type_arguments) =
                        if let Some(eta) = arena.get_expr_type_args(type_node) {
                            (eta.expression, eta.type_arguments.as_ref())
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            if let Some(tr) = arena.get_type_ref(type_node) {
                                (tr.type_name, tr.type_arguments.as_ref())
                            } else {
                                (type_idx, None)
                            }
                        } else {
                            (type_idx, None)
                        };

                    if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx) {
                        let type_arg_indices = type_arguments
                            .map(|args| args.nodes.clone())
                            .unwrap_or_default();
                        bases.push(LibHeritageBase {
                            name: base_name.to_string(),
                            expr_idx,
                            type_arg_indices,
                            arena,
                        });
                    }
                }
            }
        }

        let heritage_namespace = name.split_once('.').map(|(namespace, _)| namespace);

        // Now resolve each base type and merge, applying type argument substitution
        for base in &bases {
            let namespace_base_sym = heritage_namespace
                .filter(|_| !base.name.contains('.'))
                .and_then(|namespace| {
                    self.resolve_lib_namespace_export_symbol(namespace, &base.name)
                });
            let mut base_type = self.resolve_scoped_lib_typeof_class_heritage(base, &lib_contexts);
            if base_type.is_none()
                && let (Some(namespace), Some(sym_id)) = (heritage_namespace, namespace_base_sym)
            {
                let cache_name = format!("{namespace}.{}", base.name);
                base_type = self.resolve_lib_interface_type_by_symbol(&cache_name, sym_id);
            }
            if base_type.is_none() {
                base_type = self.resolve_lib_type_by_entity_name(&base.name);
            }

            if let Some(mut base_type) = base_type {
                // If there are type arguments, resolve them and substitute
                if !base.type_arg_indices.is_empty() {
                    let base_sym = namespace_base_sym
                        .or_else(|| self.resolve_lib_symbol_by_entity_name(&base.name));
                    if let Some(base_sym_id) = base_sym {
                        let base_params = self.get_type_params_for_symbol(base_sym_id);
                        if !base_params.is_empty() {
                            let mut type_args = Vec::new();
                            for &arg_idx in &base.type_arg_indices {
                                // Resolve type arguments from the lib arena.
                                // Heritage type args are typically simple type
                                // references (e.g., `string`, `number`).
                                let ty = self.resolve_lib_heritage_type_arg(arg_idx, base.arena);
                                type_args.push(ty);
                            }
                            // Pad/truncate args to match params
                            while type_args.len() < base_params.len() {
                                let param = &base_params[type_args.len()];
                                type_args.push(
                                    param
                                        .default
                                        .or(param.constraint)
                                        .unwrap_or(TypeId::UNKNOWN),
                                );
                            }
                            type_args.truncate(base_params.len());

                            let substitution =
                                crate::query_boundaries::common::TypeSubstitution::from_args(
                                    self.ctx.types,
                                    &base_params,
                                    &type_args,
                                );
                            base_type = crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                base_type,
                                &substitution,
                            );
                        }
                    }
                }
                derived_type = self.merge_interface_types(derived_type, base_type);
            }
        }

        for (name, prev) in scope_restore {
            if let Some(prev_ty) = prev {
                self.ctx.type_parameter_scope.insert(name, prev_ty);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        self.ctx.lib_heritage_in_progress.remove(name);
        self.ctx.leave_recursion();
        derived_type
    }

    /// Resolve a type argument node from a lib arena to a TypeId.
    /// Handles simple keyword types (string, number, etc.), type references
    /// to other lib types, and the derived interface's own type parameters.
    fn resolve_lib_heritage_type_arg(&mut self, node_idx: NodeIndex, arena: &NodeArena) -> TypeId {
        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        // Handle keyword types (string, number, boolean, etc.)
        if let Some(ty) = keyword_syntax_to_type_id(node.kind) {
            return ty;
        }

        // Handle type references (e.g., other interface names or type params)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(name) = entity_name_text_in_arena(arena, type_ref.type_name)
        {
            if let Some(ty) = keyword_name_to_type_id(&name) {
                return ty;
            }
            return self.resolve_heritage_type_arg_by_name(&name);
        }

        // For identifiers, try resolving the name
        if let Some(name) = entity_name_text_in_arena(arena, node_idx) {
            return self.resolve_heritage_type_arg_by_name(&name);
        }

        TypeId::UNKNOWN
    }

    /// Resolve a heritage type argument by name: type-parameter scope → lib type → symbolic param.
    fn resolve_heritage_type_arg_by_name(&mut self, name: &str) -> TypeId {
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
            return type_id;
        }
        if !self.ctx.file_local_type_shadow_for_lib_name(name)
            && let Some(ty) = self.resolve_lib_type_by_name(name)
        {
            return ty;
        }
        // Preserve unresolved lib heritage args as symbolic type params
        // (e.g. `T` in `extends IteratorObject<T, ...>`) instead of
        // collapsing to unknown.
        let atom = self.ctx.types.intern_string(name);
        self.ctx.types.type_param(tsz_solver::TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
        })
    }
}
