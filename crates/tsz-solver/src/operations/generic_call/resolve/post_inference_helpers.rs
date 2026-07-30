//! Small shape probes used after generic-call candidate collection.

use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Returns `true` when `ty` is or structurally contains a `TypeParameter`
    /// that does not belong to the current generic call (i.e. is absent from
    /// `var_map`).
    pub(super) fn type_contains_any_foreign_type_param(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(_)) => !var_map.contains_key(&ty),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .any(|&member| self.type_contains_any_foreign_type_param(member, var_map)),
            Some(TypeData::IndexAccess(object, index)) => {
                self.type_contains_any_foreign_type_param(object, var_map)
                    || self.type_contains_any_foreign_type_param(index, var_map)
            }
            Some(TypeData::Array(element)) => {
                self.type_contains_any_foreign_type_param(element, var_map)
            }
            Some(TypeData::Application(app_id)) => {
                let application = self.interner.type_application(app_id);
                self.type_contains_any_foreign_type_param(application.base, var_map)
                    || application.args.iter().any(|&argument| {
                        self.type_contains_any_foreign_type_param(argument, var_map)
                    })
            }
            _ => false,
        }
    }

    pub(super) fn application_expands_to_conditional_alias_for_return_display(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        if !matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Application(_))
        ) {
            return false;
        }
        self.checker
            .expand_type_alias_application(type_id)
            .is_some_and(|expanded| {
                matches!(
                    self.interner.lookup(expanded),
                    Some(TypeData::Conditional(_))
                )
            })
    }
}
