//! Public entrypoints for class instance type construction.

use super::helpers::in_progress_class_instance_result;
use super::walk_state::ClassInstanceWalkState;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the instance type of a class declaration.
    ///
    /// This is the type that instances of the class will have. It includes:
    /// - Instance properties and methods
    /// - Inherited members from base classes
    /// - Index signatures
    /// - Private brand property for nominal typing (if class has private/protected members)
    ///
    /// # Arguments
    /// * `class_idx` - The `NodeIndex` of the class declaration
    /// * `class` - The parsed class data
    ///
    /// # Returns
    /// The `TypeId` representing the instance type of the class
    pub(crate) fn get_class_instance_type(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        self.get_class_instance_type_with_mode(class_idx, class, true)
    }

    pub(crate) fn get_class_instance_type_without_module_augmentations(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        self.get_class_instance_type_with_mode(class_idx, class, false)
    }

    fn get_class_instance_type_with_mode(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        apply_module_augmentations: bool,
    ) -> TypeId {
        let current_sym = self.class_declaration_symbol(class_idx);
        let is_in_resolution_set = current_sym
            .is_some_and(|sym_id| self.ctx.class_instance_resolution_set.contains(&sym_id));

        if apply_module_augmentations {
            if let Some(result) = in_progress_class_instance_result(
                is_in_resolution_set,
                self.ctx
                    .class_instance_type_cache
                    .borrow()
                    .get(&class_idx)
                    .copied(),
            ) {
                // Serving a mid-resolution partial: taint in-flight
                // evaluations so they do not persist results derived from it
                // (issue #16055).
                self.ctx.note_provisional_class_value();
                return result;
            }

            if let Some(cached) = self
                .ctx
                .class_instance_type_cache
                .borrow()
                .get(&class_idx)
                .copied()
            {
                return cached;
            }
        } else {
            if is_in_resolution_set {
                // Mid-resolution partial serve; see the sibling branch above
                // (issue #16055).
                self.ctx.note_provisional_class_value();
                return self
                    .ctx
                    .class_instance_type_cache
                    .borrow()
                    .get(&class_idx)
                    .copied()
                    .unwrap_or(TypeId::ERROR);
            }
            if let Some(cached) = self
                .ctx
                .class_instance_type_cache
                .borrow()
                .get(&class_idx)
                .copied()
            {
                return cached;
            }
        }

        // Self-reference deferral (cache-miss only): a *fresh* instance-type
        // build for this class has been requested. If it was triggered from
        // within the resolution of one of this class's OWN arrow/function-valued
        // property initializers — that node is still in flight on
        // `node_resolution_stack` in an enclosing frame, e.g. an arrow-property
        // whose return annotation references the enclosing class
        // (`m = (): C => ...`), reached through return-type name validation or
        // constructor-type building — then
        // building now would re-enter that member and read its transient
        // `ERROR` placeholder from the `get_type_of_node` cycle guard, baking an
        // unsound `ERROR`/`any` member type into the cached instance (the bug
        // behind silent assignability false-negatives on such properties).
        // Mirror `tsc`, which represents `C` inside `C`'s own member signatures
        // as a deferred reference rather than its resolved members: return a
        // valid instance type from an outer, already-completed build when one
        // exists, otherwise a lazy self-reference — WITHOUT caching, so the real
        // instance is still built later, once the member is no longer in flight.
        // A build already in the resolution set is the in-progress self-build,
        // handled by the cache/`ERROR` paths above.
        if let Some(sym_id) = current_sym
            && !is_in_resolution_set
            && self.class_build_reenters_in_flight_member(sym_id)
        {
            let self_sym = self.class_self_reference_symbol(class, sym_id);
            if let Some(existing) = self.ctx.symbol_instance_types.get(&self_sym)
                && !existing.is_any_unknown_or_error()
            {
                return existing;
            }
            super::helpers::note_class_self_reference_deferral();
            return self.ctx.create_lazy_type_ref(self_sym);
        }

        let mut walk_state = ClassInstanceWalkState::default();
        let result = self.get_class_instance_type_inner(
            class_idx,
            class,
            &mut walk_state,
            apply_module_augmentations,
        );
        if apply_module_augmentations {
            self.ctx
                .class_instance_type_cache
                .borrow_mut()
                .insert(class_idx, result);
            // Keep the enclosing fast path synchronized with completed class
            // construction. A build re-entered from this class's own property
            // initializer is excluded: its result can contain the
            // node-resolution cycle sentinel for that initializer, while the
            // enclosing snapshot remains the stable receiver for later members.
            let reentered_from_own_initializer = current_sym.is_some_and(|class_sym| {
                self.class_build_reenters_in_flight_property_initializer(class_sym)
            });
            if !reentered_from_own_initializer
                && let Some(info) = self
                    .ctx
                    .enclosing_class
                    .as_mut()
                    .filter(|info| info.class_idx == class_idx)
            {
                info.cached_instance_this_type = Some(result);
            }
        }

        result
    }
}
