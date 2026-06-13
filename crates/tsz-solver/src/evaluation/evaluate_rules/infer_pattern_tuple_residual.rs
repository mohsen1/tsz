//! Tuple-rest residual helpers for conditional `infer` pattern matching.

use crate::relations::subtype::TypeResolver;
use crate::types::{TupleElement, TypeData, TypeId};

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(super) fn reify_application_over_tuple_index_residual(
        &self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let Some(TypeData::Application(app_id)) = self.interner().lookup(type_id) else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        if app.args.len() != 1 {
            return None;
        }

        let Some(TypeData::IndexAccess(object, index)) = self.interner().lookup(app.args[0]) else {
            return None;
        };
        if index != TypeId::NUMBER {
            return None;
        }

        let object = crate::type_queries::data::unwrap_readonly(self.interner(), object);
        let Some(TypeData::Tuple(list_id)) = self.interner().lookup(object) else {
            return None;
        };

        // Rest inference lost slot correlation when mapped recursion collapsed to
        // `F<Tuple[number]>`; reify it as `[F<T0>, ...F<Rest>]` for this tuple.
        let elements = self.interner().tuple_list(list_id);
        let mut fixed_prefix_len = 0usize;
        let reified = elements
            .iter()
            .map(|element| {
                let type_id = self.interner().application(
                    app.base,
                    vec![self.evaluated_tuple_rest_argument(*element, fixed_prefix_len)],
                );
                if !element.rest {
                    fixed_prefix_len = fixed_prefix_len.saturating_add(1);
                }
                TupleElement {
                    type_id,
                    name: element.name,
                    optional: element.optional,
                    rest: element.rest,
                }
            })
            .collect();
        Some(self.interner().tuple(reified))
    }

    fn evaluated_tuple_rest_argument(
        &self,
        element: TupleElement,
        fixed_prefix_len: usize,
    ) -> TypeId {
        if !element.rest {
            return element.type_id;
        }

        let evaluated = self.evaluate_for_infer_match(element.type_id);
        if evaluated == element.type_id {
            return element.type_id;
        }
        let tuple_like = crate::type_queries::data::unwrap_readonly(self.interner(), evaluated);
        if matches!(self.interner().lookup(tuple_like), Some(TypeData::Tuple(_))) {
            // The evaluated rest application may include fixed slots already
            // emitted by the outer tuple reconstruction.
            self.drop_reified_rest_prefix(evaluated, fixed_prefix_len)
                .unwrap_or(evaluated)
        } else {
            element.type_id
        }
    }

    fn drop_reified_rest_prefix(&self, type_id: TypeId, fixed_prefix_len: usize) -> Option<TypeId> {
        if fixed_prefix_len == 0 {
            return None;
        }
        let tuple_like = crate::type_queries::data::unwrap_readonly(self.interner(), type_id);
        let Some(TypeData::Tuple(list_id)) = self.interner().lookup(tuple_like) else {
            return None;
        };
        let elements = self.interner().tuple_list(list_id);
        if elements.len() <= fixed_prefix_len {
            return None;
        }
        Some(self.interner().tuple(elements[fixed_prefix_len..].to_vec()))
    }
}
