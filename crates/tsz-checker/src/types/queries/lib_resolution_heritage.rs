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

/// Maximum nesting for `merge_lib_interface_heritage`, tracked by the
/// thread-local [`LIB_HERITAGE_MERGE_DEPTH`] counter below.
///
/// Real lib heritage chains are shallow (the deepest, the DOM diamond, stays
/// well under 20) and same-name re-entry is already blocked by the
/// `lib_heritage_in_progress` name guard, so this bound is reached only by a
/// pathologically deep distinct-name chain. It exists purely as OS-stack
/// defense; normal lib types never approach it, so their inherited heritage
/// members always materialize regardless of surrounding checker recursion.
const LIB_HERITAGE_MERGE_MAX_DEPTH: u32 = 50;

thread_local! {
    /// Depth of the active `merge_lib_interface_heritage` call stack on this
    /// thread. A thread-local (mirroring `LIB_RESOLUTION_DEPTH`) rather than a
    /// `CheckerContext` field so it survives the fresh / cross-arena child
    /// contexts the heritage merge can hop — which reset per-context counters —
    /// and so its nesting is decoupled from the global `recursion_depth` budget
    /// that unrelated deep recursion exhausts (issue #13942). Balanced
    /// enter/leave keep it self-clearing back to `0` at each top-level
    /// resolution.
    static LIB_HERITAGE_MERGE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII frame for one `merge_lib_interface_heritage` call. Increments
/// [`LIB_HERITAGE_MERGE_DEPTH`] on entry and decrements it on drop, so the
/// counter self-balances even if the merge unwinds.
struct LibHeritageMergeFrame {
    /// Whether this frame is the outermost lib-heritage merge on the thread
    /// (it entered at depth `0`).
    is_outermost: bool,
}

impl LibHeritageMergeFrame {
    /// Enter a frame, or return `None` when the dedicated lib-heritage budget is
    /// exhausted so the caller bails to a heritage-thin body.
    fn enter() -> Option<Self> {
        LIB_HERITAGE_MERGE_DEPTH.with(|depth| {
            let cur = depth.get();
            (cur < LIB_HERITAGE_MERGE_MAX_DEPTH).then(|| {
                depth.set(cur + 1);
                Self {
                    is_outermost: cur == 0,
                }
            })
        })
    }
}

impl Drop for LibHeritageMergeFrame {
    fn drop(&mut self) {
        LIB_HERITAGE_MERGE_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

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

/// Find a global (script, non-external-module) lib interface symbol directly in
/// the loaded lib contexts by name.
///
/// The primary symbol resolution path (`resolve_lib_symbol_by_entity_name`)
/// reads the active checker's binder, which carries the standard-lib globals
/// merged in via `merge_lib_contexts_into_binder`. A transient cross-arena
/// delegation child (`delegate_cross_arena_class_instance_type`) runs against
/// the imported file's binder, which never received that merge, so a global
/// lib base reached through a heritage clause (e.g. a generic class
/// `extends Response`, where `Response extends Body`) fails to resolve there.
/// `select_external_module_lib_interface` only covers module-scoped lib
/// declarations, so global script libs (`lib.dom.d.ts`, `lib.es5.d.ts`) need
/// this context-independent fallback to keep the lib base's own transitive
/// heritage from being dropped. Mirrors the same `lib_ctx.binder.file_locals`
/// scan `resolve_lib_type_with_params` uses for the lib base's OWN members, so
/// the two resolutions agree on the same lib symbol.
fn select_global_lib_interface(
    name: &str,
    actual_lib_file_count: usize,
    lib_contexts: &[crate::context::LibContext],
) -> Option<(SymbolId, Option<Arc<BinderState>>)> {
    lib_contexts
        .iter()
        .take(actual_lib_file_count)
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
    /// Merge heritage and report whether the result is incomplete because a
    /// heritage base was dropped while it was itself mid-resolution (directly,
    /// or transitively through a base that was already incomplete). The caller
    /// uses the flag to avoid caching the incomplete derived type (#12299).
    pub(crate) fn merge_lib_interface_heritage(
        &mut self,
        derived_type: TypeId,
        name: &str,
    ) -> (TypeId, bool) {
        // Guard against infinite recursion in recursive generic hierarchies
        // (e.g., interface B<T extends B<T,S>> extends A<B<T,S>, B<T,S>>).
        //
        // A bail here returns `derived_type` with its heritage loop NOT yet run,
        // i.e. an own-members-only, heritage-THIN body. Report it as incomplete so
        // the caller refuses to cache it (the #12299 taint contract): a thin body
        // would otherwise be cached with inherited members un-substituted — a base
        // interface's raw type parameter (`Iterator<T>.next(): IteratorResult<T>`)
        // then leaks through a concrete `Map`/`Set` iteration into the checker as a
        // bare `T` (false TS2488/TS2345, issue #13652).
        //
        // This uses a DEDICATED `LibHeritageMergeFrame` budget, not the global
        // `recursion_depth` (`enter_recursion`) counter. The lib heritage graph is
        // shallow and same-name re-entry is already blocked by the
        // `lib_heritage_in_progress` name guard below, so sharing the global
        // counter only let *unrelated* deep recursion exhaust the budget and force
        // a spurious thin body that dropped a derived iterator's inherited `next`
        // (the `SetIterator`/`MapIterator`/`IterableIterator` false-positive family,
        // issue #13942). The dedicated counter reflects actual lib-heritage nesting,
        // so normal lib types always materialize their full heritage regardless of
        // surrounding checker recursion. O(1) at the bail site; no identifier/
        // file-name predicate.
        let Some(frame) = LibHeritageMergeFrame::enter() else {
            return (derived_type, true);
        };

        // Outermost lib-heritage entry: give the bounded, name-cycle-guarded
        // subtree a fresh global `recursion_depth` budget so the structural member
        // merge (`merge_interface_types_with_mode`, which also consults that global
        // counter) is not starved by *unrelated* surrounding recursion — the second
        // half of the #13942 fix. OS-stack safety stays with the #14111
        // `with_stack_guard` breaker; cycle-safety with `lib_heritage_in_progress`
        // and the resolution fuel. Nested lib-heritage calls keep the live budget.
        let saved_recursion_depth = frame.is_outermost.then(|| {
            std::mem::replace(
                &mut *self.ctx.recursion_depth.borrow_mut(),
                tsz_solver::recursion::DepthCounter::with_profile(
                    tsz_solver::recursion::RecursionProfile::CheckerRecursion,
                ),
            )
        });

        let result = self.merge_lib_interface_heritage_inner(derived_type, name);

        if let Some(saved) = saved_recursion_depth {
            *self.ctx.recursion_depth.borrow_mut() = saved;
        }
        // Drop the frame only now: it must outlive the inner call so nested
        // lib-heritage merges observe the correct nesting depth.
        drop(frame);
        result
    }

    /// Inner body of [`Self::merge_lib_interface_heritage`]; the public wrapper
    /// owns the dedicated lib-heritage depth counter and the outermost
    /// global-`recursion_depth` insulation.
    fn merge_lib_interface_heritage_inner(
        &mut self,
        mut derived_type: TypeId,
        name: &str,
    ) -> (TypeId, bool) {
        // Name-based cycle guard: prevent re-entrant heritage merging for the same
        // interface name. This breaks the resolve_lib_type_by_name ↔ merge_lib_interface_heritage
        // mutual recursion that occurs through deep heritage chains
        // (e.g., Array → ReadonlyArray → Iterable → ...), especially when child
        // CheckerStates are created for cross-arena type param resolution.
        //
        // Unlike the depth guard above, this bail must report `incomplete = false`:
        // re-entry for the SAME name means an OUTER resolution of that exact name is
        // already on the stack and WILL complete the heritage merge and cache the
        // full body. The name guard is precisely the designed terminal that lets the
        // outer call finish; its inner partial is meant to be discarded by the outer,
        // not promoted to a global `Incomplete` mark — doing so removes the outer's
        // in-progress cache entry and drops inherited members for self-referential
        // lib interfaces (e.g. the DOM heritage graph; regresses the
        // declarationFileForHtml* conformance rows).
        if !self.ctx.lib_heritage_in_progress.insert(name.to_string()) {
            return (derived_type, false);
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
            })
            .or_else(|| {
                // A transient cross-arena delegation child checks against an
                // imported file's binder, which lacks the merged standard-lib
                // globals, so the binder-based resolution above returns None for
                // a global lib base (e.g. a class `extends Response`). Fall back
                // to a direct lib-context scan so the lib base's own transitive
                // heritage (`Response extends Body`) is still merged instead of
                // dropped.
                //
                // Scope this to a cross-arena child's resolution of a class's
                // directly-named `extends` base (and its transitive lib heritage,
                // resolved while still in that scope). Same-arena checkers either
                // have merged globals in their active binder or should keep the
                // existing resolution behavior.
                //
                // Without the heritage-base gate the fallback also fires when a
                // delegation child lowers the base's member signatures, eagerly
                // pulling an entire global graph it does not own (e.g. the DOM
                // graph reached through `React.Component`'s members). Re-merging
                // those base members the top-level checker already resolves
                // produces duplicated intersections and false JSX-inference
                // diagnostics; the member types resolve correctly in the
                // top-level checker whose binder carries the globals.
                if !Self::is_in_cross_arena_delegation()
                    || !Self::is_resolving_class_heritage_base()
                {
                    return None;
                }
                select_global_lib_interface(
                    name,
                    self.ctx.actual_lib_file_count,
                    &self.ctx.lib_contexts,
                )
            });
        let Some((sym_id, selected_binder_arc)) = selected else {
            self.ctx.lib_heritage_in_progress.remove(name);
            return (derived_type, false);
        };
        let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
        let Some(symbol) = selected_binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            return (derived_type, false);
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
            return (derived_type, false);
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

        // Now resolve each base type and merge, applying type argument substitution.
        //
        // Under `TSZ_LAZY_LIB_HERITAGE` (#13933 Slice 1) a resolved base is
        // recorded as a `Lazy(BaseDef)` edge collected in `base_edges` instead of
        // having its members flattened into `derived_type`; after the loop those
        // edges are wrapped as `Intersection(own, base_edge…)`. Inherited members
        // then resolve on demand through the descent-safe consumer path.
        let lazy_heritage = crate::state_checking::lazy_lib_member::lazy_lib_heritage_enabled();
        let mut base_edges: Vec<TypeId> = Vec::new();
        let mut incomplete = false;
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

            // A base that resolved to `None` only because it is itself mid-resolution
            // (its `resolve_lib_type_by_name` is on the stack) must not be silently
            // dropped — that loses every inherited member and the gap gets cached
            // (e.g. `Element extends Node` resolved while `Node` is in-progress, the
            // DOM diamond in #12299). Distinguish that from a genuinely-missing base
            // (a typo `extends Foo`), which is correctly dropped. Likewise, a base
            // that resolved to an already-incomplete type taints this type too.
            match base_type {
                None if self.lib_name_resolution_in_progress(&base.name) => incomplete = true,
                Some(_) if self.lib_name_heritage_incomplete(&base.name) => incomplete = true,
                _ => {}
            }

            if let Some(mut base_type) = base_type {
                // Lazy heritage edge: represent this base as `Lazy(BaseDef)`
                // (or `Application(Lazy(BaseDef), args)` for a generic base)
                // rather than flattening its members. Only for non-namespaced
                // bases resolvable to a lib symbol; namespaced/`typeof`-class
                // bases keep the eager merge. The eagerly resolved `base_type`
                // above is retained for the #12299 incomplete/drain contract
                // (Slice 1 keeps eager resolution; Slice 3 drops it).
                if lazy_heritage
                    && namespace_base_sym.is_none()
                    && let Some(edge) = self.lazy_heritage_base_edge(base)
                {
                    base_edges.push(edge);
                    continue;
                }
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
                derived_type = self.merge_interface_types_heritage(derived_type, base_type);
            }
        }

        for (name, prev) in scope_restore {
            if let Some(prev_ty) = prev {
                self.ctx.type_parameter_scope.insert(name, prev_ty);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        // Flag-on: combine the own object with the collected lazy base edges into
        // `Intersection(own, base_edge…)`. Own members come first so descent-based
        // property collection reads them ahead of inherited base members.
        if lazy_heritage && !base_edges.is_empty() {
            let factory = self.ctx.types.factory();
            let mut members = Vec::with_capacity(base_edges.len() + 1);
            members.push(derived_type);
            members.extend(base_edges);
            derived_type = factory.intersection(members);
        }

        self.ctx.lib_heritage_in_progress.remove(name);
        (derived_type, incomplete)
    }

    /// Build the lazy heritage edge for `base` under `TSZ_LAZY_LIB_HERITAGE`:
    /// `Lazy(BaseDef)` for a non-generic base, or
    /// `Application(Lazy(BaseDef), instantiated_args)` for a generic one. The
    /// `Lazy` handle is the base's canonical, name-verified lib `DefId`
    /// ([`crate::context::CheckerContext::lib_def_id_verified`], which avoids the
    /// cross-lib-binder raw-id collision that a bare `get_or_create_def_id` can
    /// hit), so descent resolves it to the byte-identical materialized body.
    ///
    /// Returns `None` when the base name does not resolve to a lib symbol, so the
    /// caller falls back to the eager member-merge (a genuinely-missing base or a
    /// `typeof`-class base is unchanged).
    fn lazy_heritage_base_edge(&mut self, base: &LibHeritageBase<'_>) -> Option<TypeId> {
        let base_sym = self.resolve_lib_symbol_by_entity_name(&base.name)?;
        let def_id = self.ctx.lib_def_id_verified(&base.name, base_sym);
        let base_lazy = self.ctx.types.lazy(def_id);

        if base.type_arg_indices.is_empty() {
            return Some(base_lazy);
        }
        let base_params = self.get_type_params_for_symbol(base_sym);
        if base_params.is_empty() {
            // No declared params: the base is effectively non-generic; the type
            // arguments (if any) have no slots to fill, so the bare `Lazy` head is
            // the correct edge.
            return Some(base_lazy);
        }

        // Resolve the heritage type arguments in the derived interface's seeded
        // type-parameter scope (so `extends IteratorObject<T, …>` carries the
        // instantiated `T`), padding/truncating to the base's arity. Descent
        // applies the substitution via `expand_application_with_resolver`
        // (collect.rs), preserving base type-param substitution (#13652).
        let mut type_args = Vec::with_capacity(base_params.len());
        for &arg_idx in &base.type_arg_indices {
            type_args.push(self.resolve_lib_heritage_type_arg(arg_idx, base.arena));
        }
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

        Some(self.ctx.types.factory().application(base_lazy, type_args))
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
            origin: tsz_solver::TypeParamOrigin::User,
        })
    }
}
