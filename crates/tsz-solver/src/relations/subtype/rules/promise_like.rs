//! PromiseLike-specific subtype helpers.
//!
//! Kept out of `generics.rs` so the generic application rules stay under the
//! solver file-size ceiling.

use super::super::{SubtypeChecker, TypeResolver};
use crate::types::{CallSignature, ParamInfo, TypeData, TypeId};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    fn call_signatures_for_type(&self, type_id: TypeId) -> Vec<CallSignature> {
        if type_id.is_intrinsic() {
            return Vec::new();
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                vec![CallSignature {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: shape.this_type,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_method: shape.is_method,
                }]
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape.call_signatures.to_vec()
            }
            Some(TypeData::Union(list_id)) | Some(TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .flat_map(|&member| self.call_signatures_for_type(member))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn promise_like_then_property_type(
        &mut self,
        query_db: &dyn crate::caches::db::QueryDatabase,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let evaluator = crate::operations::property::PropertyAccessEvaluator::new(query_db);
        evaluator
            .resolve_property_access(type_id, "then")
            .success_type()
            .or_else(|| {
                let evaluated = self.evaluate_type(type_id);
                (evaluated != type_id).then(|| {
                    evaluator
                        .resolve_property_access(evaluated, "then")
                        .success_type()
                })?
            })
    }

    fn return_type_is_promise_like(
        &mut self,
        query_db: &dyn crate::caches::db::QueryDatabase,
        type_id: TypeId,
    ) -> bool {
        crate::type_queries::is_promise_like(query_db, type_id)
            || crate::type_queries::is_promise_like(query_db, self.evaluate_type(type_id))
    }

    fn callback_accepts_promise_value(
        &mut self,
        callback_param: &ParamInfo,
        value_arg: TypeId,
    ) -> bool {
        if callback_param.type_id == TypeId::ANY {
            return true;
        }

        self.call_signatures_for_type(callback_param.type_id)
            .into_iter()
            .any(|callback_sig| {
                callback_sig.params.first().is_some_and(|value_param| {
                    value_param.type_id == TypeId::ANY
                        || self.check_subtype(value_arg, value_param.type_id).is_true()
                })
            })
    }

    pub(super) fn application_has_promise_like_then_contract(
        &mut self,
        query_db: &dyn crate::caches::db::QueryDatabase,
        application_type: TypeId,
        value_arg: TypeId,
    ) -> bool {
        let Some(then_type) = self.promise_like_then_property_type(query_db, application_type)
        else {
            return false;
        };

        self.call_signatures_for_type(then_type)
            .into_iter()
            .any(|then_sig| {
                self.return_type_is_promise_like(query_db, then_sig.return_type)
                    && then_sig.params.first().is_some_and(|callback_param| {
                        self.callback_accepts_promise_value(callback_param, value_arg)
                    })
            })
    }
}
