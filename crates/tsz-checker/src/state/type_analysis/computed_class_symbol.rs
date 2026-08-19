//! Class symbol type computation for `compute_type_of_symbol`.
//!
//! Split from `computed_helpers_binding.rs` to keep that file under the
//! architecture size ceiling. Owns `compute_class_symbol_type` (local and
//! cross-arena class declaration resolution for a `CLASS`-flagged symbol)
//! and its function-merge helper.

use crate::query_boundaries::construct_signatures::callable_with_call_signatures_and_erased_metadata;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn compute_class_symbol_type(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
        value_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // `NodeIndex` is only meaningful together with its arena. Cross-file class
        // symbols carry declaration indices from the owner arena; the same raw index
        // can name an unrelated class in the requester. Accept a current-arena class
        // only when declaration provenance is local and either the symbol's stable
        // owner is this file or the current binder maps the node back to this exact
        // symbol. The owner alternative preserves export/self-import wrappers whose
        // related symbol identity differs; the O(1) provenance check still rejects a
        // foreign declaration collision. Foreign classes fall through to the
        // owner-arena search below.
        let symbol_is_owned_by_current_file =
            self.ctx.resolve_symbol_file_index_stable(sym_id) == Some(self.ctx.current_file_idx);
        let is_local_class_declaration = |decl_idx: NodeIndex| {
            if decl_idx.is_none()
                || !self
                    .ctx
                    .declaration_is_local_to_current_arena(sym_id, decl_idx)
            {
                return false;
            }
            let Some(class) = self
                .ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_class(node))
            else {
                return false;
            };
            symbol_is_owned_by_current_file
                || self
                    .ctx
                    .binder
                    .get_node_symbol(decl_idx)
                    // For `export default class C {}` the class NODE's symbol
                    // is the default-export binding, not the `CLASS`-flagged
                    // symbol the class's own name binds to, so a direct
                    // `== sym_id` comparison can never accept it. Map the node
                    // symbol through the shared self-reference rule (#17629)
                    // so the alternative recognizes the class symbol whenever
                    // the stable owner index is unavailable (#17743). An
                    // unrelated local class at a colliding `decl_idx` binds
                    // its name to a different symbol and stays rejected.
                    .is_some_and(|node_sym| {
                        node_sym == sym_id
                            || self.class_self_reference_symbol(class, node_sym) == sym_id
                    })
        };
        let decl_idx = if is_local_class_declaration(value_decl) {
            value_decl
        } else {
            declarations
                .iter()
                .find(|&&decl_idx| is_local_class_declaration(decl_idx))
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_some()
            && let Some(node) = self.ctx.arena.get(decl_idx)
            && let Some(class) = self.ctx.arena.get_class(node)
        {
            // Build instance type FIRST so that the constructor type's construct
            // signatures can use the real instance type instead of a rough
            // approximation. This ensures that static methods like
            // `static getInstance() { return new C(); }` infer the correct
            // return type when the class is a class expression.
            let instance_type = self.get_class_instance_type(decl_idx, class);
            // Guard: don't overwrite a valid cached instance type with a degraded
            // value (ERROR/ANY). This happens when compute_type_of_symbol is called
            // re-entrantly from within get_class_instance_type_inner (e.g., during
            // prescan of a method whose return type references the same class).
            // The re-entrant get_class_instance_type hits the in-progress guard and
            // returns ERROR/ANY, which would corrupt the previously-cached correct type.
            if (instance_type == TypeId::ANY || instance_type == TypeId::ERROR)
                && let Some(existing) = self.ctx.symbol_instance_types.get(&sym_id)
                && existing != TypeId::ANY
                && existing != TypeId::ERROR
            {
                // Keep the existing valid type; skip the degraded overwrite.
                let ctor_type = self.get_class_constructor_type(decl_idx, class);
                self.ctx.symbol_types.insert(sym_id, ctor_type);

                let ctor_type = if flags & symbol_flags::FUNCTION != 0 {
                    self.merge_function_call_signatures_into_class(ctor_type, declarations)
                } else {
                    ctor_type
                };

                if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                    let merged = self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                    return (merged, Vec::new());
                }
                return (ctor_type, Vec::new());
            }
            self.ctx.symbol_instance_types.insert(sym_id, instance_type);

            let ctor_type = self.get_class_constructor_type(decl_idx, class);
            // Guard against constructor type cache corruption.
            //
            // When an outer `get_class_constructor_type(C)` is already in
            // progress (typical for self-referential static member types
            // like `static instance: C<string>[]`), a nested
            // `get_type_of_symbol(C)` can re-enter this function. The
            // nested call's `get_class_constructor_type(C)` hits the
            // in-progress guard and returns a cycle-fallback: the current
            // `symbol_types[C]`, which at this point is the `Lazy(DefId)`
            // placeholder that `get_type_of_symbol_inner` installed. That
            // Lazy resolves to the class's INSTANCE type (not the
            // constructor type). Storing it in `symbol_types[C]` would
            // corrupt later value-position lookups of `C` (e.g.
            // `C.instance` in an instance method body → false TS2339).
            //
            // Detect the degenerate case (ctor_type is a Lazy pointing at
            // the class's own DefId) and skip the cache overwrite so that
            // the outer resolution, once it completes, keeps providing the
            // correct constructor type on the next lookup.
            let is_degenerate_lazy = {
                use crate::query_boundaries::common as common_query;
                let lazy_def =
                    common_query::lazy_def_id(self.ctx.types.as_type_database(), ctor_type);
                let own_def = self.ctx.get_existing_def_id(sym_id);
                lazy_def.is_some() && lazy_def == own_def
            };
            if !is_degenerate_lazy {
                self.ctx.symbol_types.insert(sym_id, ctor_type);
            }

            let ctor_type = if flags & symbol_flags::FUNCTION != 0 {
                self.merge_function_call_signatures_into_class(ctor_type, declarations)
            } else {
                ctor_type
            };

            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                let merged = self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                return (merged, Vec::new());
            }
            return (ctor_type, Vec::new());
        }

        // Cross-file fallback: the class declaration might be in a different file's
        // arena. This happens when a constructor function in one JS file merges with
        // a class declaration in another JS file (SALSA mode). The merged symbol has
        // CLASS flag but the class node is only accessible in the declaring file's arena.
        // Search all available arenas for the class node, then build the class type
        // using a child checker with the correct arena. We directly call
        // get_class_instance_type/get_class_constructor_type instead of
        // get_type_of_symbol to avoid SymbolId collisions across binders.
        // Only attempt cross-file fallback when the current arena is among the
        // user file arenas. This prevents lib delegation child checkers (whose
        // arena is a lib arena, not in all_arenas) from incorrectly picking up
        // class nodes from user files due to NodeIndex collisions.
        let current_arena_is_user_file = self.ctx.all_arenas.as_ref().is_some_and(|arenas| {
            arenas
                .iter()
                .any(|a| std::ptr::eq(a.as_ref(), self.ctx.arena))
        });
        let sym_name = if current_arena_is_user_file {
            self.get_symbol_globally(sym_id)
                .map(|s| s.escaped_name.clone())
        } else {
            None
        };
        if let Some(all_arenas) = self.ctx.all_arenas.clone()
            && let Some(ref sym_name) = sym_name
        {
            for (file_idx, arena) in all_arenas.iter().enumerate() {
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
                // Find a class declaration in this arena, verifying the name matches
                // to prevent NodeIndex collisions across arenas.
                let cross_decl_idx = declarations
                    .iter()
                    .chain(std::iter::once(&value_decl))
                    .find(|&&d| {
                        d.is_some()
                            && arena
                                .get(d)
                                .and_then(|n| arena.get_class(n))
                                .and_then(|class| {
                                    let name_node = arena.get(class.name)?;
                                    let ident = arena.get_identifier(name_node)?;
                                    Some(ident.escaped_text == *sym_name)
                                })
                                .unwrap_or(false)
                    })
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                if cross_decl_idx.is_none() {
                    continue;
                }
                // Fast path: if a parallel worker already resolved this
                // (sym_id, file_idx) pair, the canonical SYMBOL_TYPE bucket
                // has the class type. Short-circuit before building a child
                // checker — but first populate `symbol_instance_types` from
                // the parallel CLASS_INSTANCE_TYPE bucket. Without this,
                // TYPE-position references (e.g. `let x: MyClass`) fall
                // through to `class_instance_type_with_params_from_symbol`,
                // which only searches the *current* arena and returns None
                // for cross-file classes — so the constructor type leaks
                // into the instance position.
                // Only short-circuit when the INSTANCE side is also
                // recoverable; see `class_instance_recoverable` (#13185).
                if let Some((cached_type, cached_params)) = self
                    .ctx
                    .cached_cross_file_symbol_type(sym_id, file_idx as u32)
                    && self.ctx.class_instance_recoverable(sym_id, file_idx as u32)
                {
                    return (cached_type, cached_params.as_ref().clone());
                }
                // Found class in another file's arena. Create a child checker
                // with that arena and directly compute the class type.
                let Some(cross_arena_guard) = Self::enter_cross_arena_delegation() else {
                    return (TypeId::ERROR, Vec::new());
                };
                if !self.ctx.enter_recursion() {
                    return (TypeId::ERROR, Vec::new());
                }

                let delegate_binder = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .unwrap_or(self.ctx.binder);
                let delegate_file_name = arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_else(|| self.ctx.file_name.clone());

                let mut checker = CheckerState::delegate_for_arena(
                    arena.as_ref(),
                    delegate_binder,
                    delegate_file_name,
                    self,
                    tsz_common::perf_counters::CheckerCreationReason::BindingHelpers,
                );
                checker.ctx.current_file_idx = file_idx;
                for &id in &self.ctx.class_instance_resolution_set {
                    checker.ctx.class_instance_resolution_set.insert(id);
                }
                for &id in &self.ctx.class_constructor_resolution_set {
                    checker.ctx.class_constructor_resolution_set.insert(id);
                }

                // Directly compute the class type using the cross-arena class node.
                // `get_class_instance_type`/`get_class_constructor_type` take
                // `&mut self`, so a borrow of the arena cannot be held across them.
                // Clone the class data once (binding it by value) instead of
                // re-fetching and re-`expect`ing the same arena chain per phase.
                let cross_class = checker
                    .ctx
                    .arena
                    .get(cross_decl_idx)
                    .and_then(|n| checker.ctx.arena.get_class(n))
                    .cloned();
                let (result, cross_instance_type) = if let Some(class) = cross_class {
                    // Phase 1: compute instance type
                    let instance_type = checker.get_class_instance_type(cross_decl_idx, &class);
                    // Phase 2: compute constructor type (same cloned class)
                    let ctor_type = checker.get_class_constructor_type(cross_decl_idx, &class);
                    (ctor_type, Some(instance_type))
                } else {
                    (TypeId::UNKNOWN, None)
                };

                // Collect child data before dropping (checker borrows from self)
                let child_instance_types: Vec<(SymbolId, TypeId)> =
                    checker.ctx.symbol_instance_types.iter().collect();
                drop(checker);

                // Now safe to mutate self
                if let Some(inst) = cross_instance_type
                    && inst != TypeId::ANY
                    && inst != TypeId::ERROR
                {
                    self.ctx.symbol_instance_types.insert(sym_id, inst);
                    // Publish the instance side next to the SYMBOL bucket
                    // entry so sibling checkers can recover it; declaration-
                    // file classes have no other ClassInstance writer
                    // (#13185).
                    self.ctx.cache_cross_file_class_instance_type(
                        sym_id,
                        file_idx as u32,
                        inst,
                        Vec::new(),
                    );
                }
                for (k, v) in child_instance_types {
                    self.ctx.symbol_instance_types.entry_or_insert(k, v);
                }

                drop(cross_arena_guard);
                self.ctx.leave_recursion();

                if result != TypeId::UNKNOWN && result != TypeId::ERROR {
                    return (result, Vec::new());
                }
            }
        }

        (TypeId::UNKNOWN, Vec::new())
    }

    /// Merge function call signatures into a class constructor type.
    fn merge_function_call_signatures_into_class(
        &mut self,
        ctor_type: TypeId,
        declarations: &[NodeIndex],
    ) -> TypeId {
        use crate::query_boundaries::state::type_analysis::{
            call_signatures_for_type, callable_shape_for_type,
        };

        let mut call_signatures = Vec::new();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };
            if func.body.is_none() {
                call_signatures.push(self.call_signature_from_function(func, decl_idx));
            }
        }

        if call_signatures.is_empty() {
            for &decl_idx in declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if self.ctx.arena.get_function(node).is_some() {
                    let func_type = self.get_type_of_function(decl_idx);
                    if let Some(signatures) = call_signatures_for_type(self.ctx.types, func_type) {
                        call_signatures = signatures;
                    }
                    break;
                }
            }
        }

        if call_signatures.is_empty() {
            return ctor_type;
        }

        let Some(shape) = callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };

        callable_with_call_signatures_and_erased_metadata(self.ctx.types, &shape, call_signatures)
    }
}
