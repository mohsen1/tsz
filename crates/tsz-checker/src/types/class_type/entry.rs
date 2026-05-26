//! Public entrypoints for class instance type construction.

use super::helpers::in_progress_class_instance_result;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
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
                self.ctx.class_instance_type_cache.get(&class_idx).copied(),
            ) {
                return result;
            }

            if let Some(&cached) = self.ctx.class_instance_type_cache.get(&class_idx) {
                return cached;
            }
        } else {
            if is_in_resolution_set {
                return self
                    .ctx
                    .class_instance_type_cache
                    .get(&class_idx)
                    .copied()
                    .unwrap_or(TypeId::ERROR);
            }
            if let Some(&cached) = self.ctx.class_instance_type_cache.get(&class_idx) {
                return cached;
            }
        }

        let mut visited = FxHashSet::default();
        let mut visited_nodes = FxHashSet::default();
        let result = self.get_class_instance_type_inner(
            class_idx,
            class,
            &mut visited,
            &mut visited_nodes,
            apply_module_augmentations,
        );

        if apply_module_augmentations {
            self.ctx.class_instance_type_cache.insert(class_idx, result);
        }

        result
    }
}
