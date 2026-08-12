//! Hoisting and exact rewriting for nonlocal generic type parameters.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::type_param_info;
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};

use super::HoistedTypeParams;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(super) fn hoist_matching_nonlocal_type_params(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> Option<HoistedTypeParams> {
        let mut source_params = Vec::new();
        let mut collect_from = |type_id: TypeId| {
            for ty in crate::visitor::collect_all_types(self.interner, type_id) {
                let Some(info) = type_param_info(self.interner, ty) else {
                    continue;
                };
                if !source_params
                    .iter()
                    .any(|existing: &TypeParamInfo| existing.is_same_binder(info))
                {
                    source_params.push(info);
                }
            }
        };

        for param in &source.params {
            collect_from(param.type_id);
        }
        if let Some(this_type) = source.this_type {
            collect_from(this_type);
        }
        collect_from(source.return_type);
        if let Some(predicate) = &source.type_predicate
            && let Some(predicate_type) = predicate.type_id
        {
            collect_from(predicate_type);
        }

        if source_params.len() != target.type_params.len() {
            return None;
        }

        let mut target_to_source = TypeSubstitution::for_signature_domain(&target.type_params);
        let mut hoisted = Vec::with_capacity(target.type_params.len());
        let mut replacements = Vec::new();
        for target_tp in &target.type_params {
            let source_tp = source_params
                .iter()
                .copied()
                // Hoisting pairs a free source binder with a separately-declared
                // target quantifier. Their declaration origins are intentionally
                // different, so this is alpha-pairing by the declared name rather
                // than an occurrence-ownership check.
                .find(|source_tp| source_tp.name == target_tp.name)?;
            target_to_source.insert(target_tp.name, self.interner.type_param(source_tp));
            hoisted.push(
                if source_tp.constraint.is_none() && target_tp.constraint.is_some() {
                    *target_tp
                } else {
                    source_tp
                },
            );
        }

        for ((source_tp, hoisted_tp), target_tp) in source_params
            .iter()
            .zip(hoisted.iter())
            .zip(target.type_params.iter())
        {
            let source_constraint = source_tp.constraint.unwrap_or(TypeId::UNKNOWN);
            let target_constraint = target_tp.constraint.map_or(TypeId::UNKNOWN, |constraint| {
                instantiate_type(self.interner, constraint, &target_to_source)
            });
            let constraints_match = self
                .check_subtype(source_constraint, target_constraint)
                .is_true()
                && self
                    .check_subtype(target_constraint, source_constraint)
                    .is_true();
            if constraints_match {
                continue;
            }

            if source_tp.constraint.is_none()
                && target_tp.constraint.is_some()
                && target_constraint != TypeId::UNKNOWN
            {
                replacements.push((target_constraint, self.interner.type_param(*hoisted_tp)));
                continue;
            }

            return None;
        }

        Some((hoisted, replacements))
    }

    pub(super) fn replace_function_type_exact(
        &mut self,
        shape: &FunctionShape,
        from: TypeId,
        to: TypeId,
    ) -> FunctionShape {
        FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape
                .params
                .iter()
                .map(|param| ParamInfo {
                    type_id: self.replace_type_exact(param.type_id, from, to),
                    ..*param
                })
                .collect(),
            this_type: shape
                .this_type
                .map(|this_type| self.replace_type_exact(this_type, from, to)),
            return_type: self.replace_type_exact(shape.return_type, from, to),
            type_predicate: shape
                .type_predicate
                .as_ref()
                .map(|predicate| TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target,
                    type_id: predicate
                        .type_id
                        .map(|ty| self.replace_type_exact(ty, from, to)),
                    parameter_index: predicate.parameter_index,
                }),
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        }
    }

    fn replace_type_exact(&mut self, type_id: TypeId, from: TypeId, to: TypeId) -> TypeId {
        if type_id == from {
            return to;
        }
        let Some(type_data) = self.interner.lookup(type_id) else {
            return type_id;
        };
        match type_data {
            TypeData::Array(elem) => {
                let replaced = self.replace_type_exact(elem, from, to);
                if replaced == elem {
                    type_id
                } else {
                    self.interner.array(replaced)
                }
            }
            TypeData::Tuple(list_id) => {
                let elements = self.interner.tuple_list(list_id);
                let mut changed = false;
                let replaced = elements
                    .iter()
                    .map(|elem| {
                        let replaced_type = self.replace_type_exact(elem.type_id, from, to);
                        changed |= replaced_type != elem.type_id;
                        TupleElement {
                            type_id: replaced_type,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.interner.tuple(replaced)
                } else {
                    type_id
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let replaced = self.replace_function_type_exact(&shape, from, to);
                if *shape == replaced {
                    type_id
                } else {
                    self.interner.function(replaced)
                }
            }
            _ => type_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::types::TypeParamOrigin;

    fn scoped_param(name: tsz_common::Atom, file: tsz_common::Atom, node: u32) -> TypeParamInfo {
        TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node },
        }
    }

    fn function(type_params: Vec<TypeParamInfo>, value_type: TypeId) -> FunctionShape {
        FunctionShape {
            type_params,
            params: vec![ParamInfo::unnamed(value_type)],
            this_type: None,
            return_type: value_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }
    }

    #[test]
    fn hoisting_alpha_pairs_distinct_scoped_declarations_by_name() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nonlocal-hoist.ts");
        let name = interner.intern_string("T");
        let source_param = scoped_param(name, file, 1);
        let target_param = scoped_param(name, file, 2);
        let source = function(vec![], interner.fresh_type_param(source_param));
        let target = function(vec![target_param], interner.fresh_type_param(target_param));
        let mut checker = SubtypeChecker::new(&interner);

        let (hoisted, replacements) = checker
            .hoist_matching_nonlocal_type_params(&source, &target)
            .expect("distinct declarations with the same name alpha-pair");

        assert_eq!(hoisted, vec![source_param]);
        assert!(replacements.is_empty());
    }

    #[test]
    fn hoisting_does_not_pair_differently_named_declarations() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nonlocal-hoist-negative.ts");
        let source_param = scoped_param(interner.intern_string("T"), file, 1);
        let target_param = scoped_param(interner.intern_string("U"), file, 2);
        let source = function(vec![], interner.fresh_type_param(source_param));
        let target = function(vec![target_param], interner.fresh_type_param(target_param));
        let mut checker = SubtypeChecker::new(&interner);

        assert!(
            checker
                .hoist_matching_nonlocal_type_params(&source, &target)
                .is_none()
        );
    }
}
