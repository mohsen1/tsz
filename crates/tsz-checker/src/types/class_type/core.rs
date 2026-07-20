//! Core implementation for class instance type resolution.
//!
//! `get_class_instance_type_inner` is a thin orchestrator: it performs the
//! setup guards (cycle/fuel detection, type-parameter push), constructs a
//! [`ClassInstanceBuilder`](super::instance::ClassInstanceBuilder), then drives
//! the named phase helpers in `super::instance` in their original order. The
//! phase bodies are pure code motion out of this function; the early-return and
//! resolution-set cleanup semantics are preserved exactly.

use super::helpers::exceeds_class_inheritance_depth_limit;
use super::instance::{ClassInstanceBuilder, ClassInstanceFlags, RestoreEnclosingClass};
use super::walk_state::ClassInstanceWalkState;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Inner implementation of class instance type resolution with cycle detection.
    ///
    /// This function builds the complete instance type by:
    /// 1. Collecting all instance members (properties, methods, accessors)
    /// 2. Processing constructor parameter properties
    /// 3. Handling index signatures
    /// 4. Merging base class members
    /// 5. Adding private brand for nominal typing if needed
    /// 6. Inheriting Object prototype members
    pub(super) fn get_class_instance_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        walk_state: &mut ClassInstanceWalkState,
        apply_module_augmentations: bool,
    ) -> TypeId {
        let current_sym = self.class_declaration_symbol(class_idx);

        // Try to insert into global class_instance_resolution_set for recursion prevention.
        let did_insert_into_global_set = if let Some(sym_id) = current_sym {
            if self.ctx.class_instance_resolution_set.insert(sym_id) {
                true // We inserted it
            } else {
                // Symbol already being resolved — break recursion without diagnostic
                return TypeId::ERROR;
            }
        } else {
            false
        };

        // Check for cycles using both symbol ID (for same-file cycles)
        // and node index (for cross-file cycles with @Filename annotations)
        if let Some(sym_id) = current_sym
            && !walk_state.enter_symbol(sym_id)
        {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Circular reference detected via symbol
        }
        if !walk_state.enter_node(class_idx) {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Circular reference detected via node index
        }
        if exceeds_class_inheritance_depth_limit(walk_state.node_depth()) {
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR;
        }

        // Check fuel to prevent timeout on pathological inheritance hierarchies
        if !self.ctx.consume_fuel() {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Fuel exhausted - prevent infinite loop
        }

        // Track this class as in-progress on the cross-arena class-instance
        // stack, keyed by its stable `(owner file, declaration node)`. A
        // mutually-recursive cross-file member reference that reaches this class
        // through a delegation defers to a lazy self-reference instead of
        // re-delegating into the depth cap and dropping members (false TS2339).
        // Pushed here (not only at the delegation site) so a class first built
        // locally in its declaring file is covered before any delegation. The
        // guard pops on every exit path below, including panic unwind.
        let _cross_arena_class_instance_guard = Self::enter_cross_arena_class_instance(
            u32::try_from(self.ctx.current_file_idx).ok(),
            class_idx,
        );

        // Class member types can reference class type parameters (e.g. `class Box<T> { value: T }`).
        // Keep class type parameters in scope while constructing the instance type.
        let (class_type_params, class_type_param_updates) =
            self.push_effective_class_type_parameters(class_idx, class);
        let class_type_param_ids = self
            .exact_type_parameter_ids_in_scope(&class_type_params)
            .unwrap_or_default();

        // PERF: Pre-size maps based on member count to avoid rehashing
        let member_count = class.members.nodes.len();
        let mut flags = ClassInstanceFlags::default();
        flags.set_did_insert_into_global_set(did_insert_into_global_set);
        let mut builder = ClassInstanceBuilder {
            current_sym,
            flags,
            class_type_params,
            class_type_param_ids,
            class_type_param_updates,
            member_count,
            properties: FxHashMap::with_capacity_and_hasher(member_count, Default::default()),
            methods: FxHashMap::with_capacity_and_hasher(member_count / 2, Default::default()),
            accessors: FxHashMap::with_capacity_and_hasher(4, Default::default()),
            string_index: None,
            number_index: None,
            symbol_index: None,
            merged_interface_type_for_class: None,
            prescan_this_type: None,
            deferred_methods: Vec::with_capacity(member_count / 2),
            deferred_accessors: Vec::with_capacity(4),
            restore_enclosing_class: RestoreEnclosingClass::Skip,
        };

        // Class construction can type-check property initializers before any
        // method/accessor has been deferred. Install the exact class binder
        // identities before those phases when an instance arrow initializer can
        // capture `this`; a nested generic callable must not hide them in the
        // active type-parameter scope. The structural gate preserves the old
        // no-scan fast path for non-generic classes and generic classes without
        // initialized instance fields.
        let installed_early_enclosing = self.class_instance_needs_early_enclosing(class, &builder);
        if installed_early_enclosing {
            self.class_instance_setup_enclosing(class, class_idx, &mut builder, true);
        }

        // Phase 0: Pre-scan annotated properties to push a partial `this`.
        self.class_instance_phase0_prescan_this(class_idx, class, &mut builder);

        // Phase 1: Process all non-method members; methods/accessors are deferred.
        self.class_instance_phase1_non_method_members(class, class_idx, &mut builder);

        // Preserve the former deferred-body setup path for classes that did not
        // need an early enclosing-class snapshot.
        if !installed_early_enclosing
            && (!builder.deferred_methods.is_empty() || !builder.deferred_accessors.is_empty())
        {
            self.class_instance_setup_enclosing(class, class_idx, &mut builder, false);
        }

        // Pop the prescan `this` at the original phase boundary. Phase 2 pushes
        // its own partial type when deferred bodies exist.
        self.class_instance_finish_prescan_this(&builder);

        // Phase 2: Process deferred methods under a partial `this`.
        self.class_instance_phase2_deferred_methods(class, class_idx, &mut builder);

        // Process deferred accessors, then restore `enclosing_class`.
        self.class_instance_process_deferred_accessors(class_idx, &mut builder);

        // Convert accessors/methods to properties and add the private brand.
        self.class_instance_finalize_members(class_idx, &mut builder);

        // Merge base class members. A detected cycle/forward reference performs
        // the resolution-set cleanup inline and short-circuits the whole call.
        if let Some(early) = self.class_instance_merge_base_members(class, walk_state, &mut builder)
        {
            return early;
        }

        // Merge interface declarations (class/interface merging).
        self.class_instance_merge_interface_decls(apply_module_augmentations, &mut builder);

        // Build the final instance type, run the interface-merge/augmentation
        // pass, perform cleanup, and register the result.
        self.class_instance_build_final_type(
            class_idx,
            apply_module_augmentations,
            walk_state,
            builder,
        )
    }
}
