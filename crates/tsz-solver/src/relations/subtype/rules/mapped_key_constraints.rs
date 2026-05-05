//! Shared mapped-type key-space relationship helpers.

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{MappedType, TypeId};
use crate::visitor::{keyof_inner_type, type_param_info};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(super) fn mapped_name_types_compatible(
        &mut self,
        source_mapped: &MappedType,
        target_mapped: &MappedType,
    ) -> bool {
        let (Some(source_name), Some(target_name)) =
            (source_mapped.name_type, target_mapped.name_type)
        else {
            return source_mapped.name_type == target_mapped.name_type;
        };

        let source_param = self.interner.type_param(source_mapped.type_param);
        let target_param = self.interner.type_param(target_mapped.type_param);
        let equiv_start = self.type_param_equivalences.len();
        self.type_param_equivalences
            .push((source_param, target_param));
        let compatible = self.check_subtype(source_name, target_name).is_true()
            && self.check_subtype(target_name, source_name).is_true();
        self.type_param_equivalences.truncate(equiv_start);
        compatible
    }

    pub(super) fn mapped_key_constraint_covers(
        &mut self,
        source_constraint: TypeId,
        target_constraint: TypeId,
    ) -> bool {
        if source_constraint == target_constraint {
            return true;
        }
        let source_eval = self.evaluate_type(source_constraint);
        let target_eval = self.evaluate_type(target_constraint);
        if source_eval != source_constraint || target_eval != target_constraint {
            return self.mapped_key_constraint_covers(source_eval, target_eval);
        }
        if let Some(target_param) = type_param_info(self.interner, target_constraint)
            && let Some(target_bound) = target_param.constraint
        {
            return self.mapped_key_constraint_covers(source_constraint, target_bound);
        }
        if type_param_info(self.interner, source_constraint).is_some() {
            return false;
        }
        if let (Some(source_obj), Some(target_obj)) = (
            keyof_inner_type(self.interner, source_constraint),
            keyof_inner_type(self.interner, target_constraint),
        ) {
            return self.check_subtype(source_obj, target_obj).is_true();
        }
        self.check_subtype(target_constraint, source_constraint)
            .is_true()
    }
}
