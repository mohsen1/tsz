//! Exact-type substitution used by distributive conditional evaluation.
//!
//! The key invariant: substitution must rewrite every occurrence of the
//! source type, even when the same hash-consed node is reachable through
//! multiple paths in the type tree. A simple visit-once `seen` set
//! conflates "currently being processed" with "already substituted",
//! which causes the second occurrence of a shared node to be returned
//! unchanged. We therefore memoize per-node substitutions and use a
//! self-mapping placeholder to handle true cycles.

use crate::caches::db::TypeDatabase;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    ConditionalType, FunctionShape, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId,
};
use rustc_hash::FxHashMap;

use super::super::evaluate::TypeEvaluator;

/// Free-function form of [`TypeEvaluator::substitute_exact_type`] that walks
/// the type graph using only a [`TypeDatabase`]. Crate-private so the
/// instantiation layer can do per-element source rebinding without depending
/// on a `TypeResolver`.
pub(crate) fn substitute_exact_type_db(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    from: TypeId,
    to: TypeId,
    memo: &mut FxHashMap<TypeId, TypeId>,
) -> TypeId {
    if type_id == from {
        return to;
    }
    if type_id.is_intrinsic() {
        return type_id;
    }
    if let Some(&cached) = memo.get(&type_id) {
        return cached;
    }
    memo.insert(type_id, type_id);

    let result = match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            let base = substitute_exact_type_db(db, app.base, from, to, memo);
            let mut changed = base != app.base;
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| {
                    let substituted = substitute_exact_type_db(db, arg, from, to, memo);
                    changed |= substituted != arg;
                    substituted
                })
                .collect();
            if changed {
                db.application(base, args)
            } else {
                type_id
            }
        }
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mut changed = false;
            let members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let substituted = substitute_exact_type_db(db, member, from, to, memo);
                    changed |= substituted != member;
                    substituted
                })
                .collect();
            if changed { db.union(members) } else { type_id }
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mut changed = false;
            let members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let substituted = substitute_exact_type_db(db, member, from, to, memo);
                    changed |= substituted != member;
                    substituted
                })
                .collect();
            if changed {
                db.intersection(members)
            } else {
                type_id
            }
        }
        Some(TypeData::Array(element)) => {
            let substituted = substitute_exact_type_db(db, element, from, to, memo);
            if substituted != element {
                db.array(substituted)
            } else {
                type_id
            }
        }
        Some(TypeData::Tuple(elements_id)) => {
            let elements = db.tuple_list(elements_id);
            let mut changed = false;
            let elements: Vec<_> = elements
                .iter()
                .map(|element| {
                    let type_id = substitute_exact_type_db(db, element.type_id, from, to, memo);
                    changed |= type_id != element.type_id;
                    TupleElement {
                        type_id,
                        ..*element
                    }
                })
                .collect();
            if changed { db.tuple(elements) } else { type_id }
        }
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            let mut changed = false;
            let params = shape
                .params
                .iter()
                .map(|param| {
                    let type_id = substitute_exact_type_db(db, param.type_id, from, to, memo);
                    changed |= type_id != param.type_id;
                    ParamInfo { type_id, ..*param }
                })
                .collect();
            let this_type = shape.this_type.map(|this_type| {
                let substituted = substitute_exact_type_db(db, this_type, from, to, memo);
                changed |= substituted != this_type;
                substituted
            });
            let return_type = substitute_exact_type_db(db, shape.return_type, from, to, memo);
            changed |= return_type != shape.return_type;
            let type_predicate = shape.type_predicate.map(|mut predicate| {
                if let Some(predicate_type) = predicate.type_id {
                    let substituted = substitute_exact_type_db(db, predicate_type, from, to, memo);
                    changed |= substituted != predicate_type;
                    predicate.type_id = Some(substituted);
                }
                predicate
            });
            if changed {
                db.function(FunctionShape {
                    type_params: shape.type_params.clone(),
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            } else {
                type_id
            }
        }
        Some(TypeData::IndexAccess(object_type, index_type)) => {
            let substituted_object = substitute_exact_type_db(db, object_type, from, to, memo);
            let substituted_index = substitute_exact_type_db(db, index_type, from, to, memo);
            if substituted_object != object_type || substituted_index != index_type {
                db.index_access(substituted_object, substituted_index)
            } else {
                type_id
            }
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.get_conditional(cond_id);
            let check_type = substitute_exact_type_db(db, cond.check_type, from, to, memo);
            let extends_type = substitute_exact_type_db(db, cond.extends_type, from, to, memo);
            let true_type = substitute_exact_type_db(db, cond.true_type, from, to, memo);
            let false_type = substitute_exact_type_db(db, cond.false_type, from, to, memo);
            if check_type != cond.check_type
                || extends_type != cond.extends_type
                || true_type != cond.true_type
                || false_type != cond.false_type
            {
                db.conditional(ConditionalType {
                    check_type,
                    extends_type,
                    true_type,
                    false_type,
                    is_distributive: cond.is_distributive,
                })
            } else {
                type_id
            }
        }
        Some(TypeData::TemplateLiteral(template_id)) => {
            let spans = db.template_list(template_id);
            let mut changed = false;
            let spans: Vec<_> = spans
                .iter()
                .map(|span| match span {
                    TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                    TemplateSpan::Type(span_type) => {
                        let substituted = substitute_exact_type_db(db, *span_type, from, to, memo);
                        changed |= substituted != *span_type;
                        TemplateSpan::Type(substituted)
                    }
                })
                .collect();
            if changed {
                db.template_literal(spans)
            } else {
                type_id
            }
        }
        Some(TypeData::KeyOf(inner)) => {
            let substituted = substitute_exact_type_db(db, inner, from, to, memo);
            if substituted != inner {
                db.keyof(substituted)
            } else {
                type_id
            }
        }
        Some(TypeData::ReadonlyType(inner)) => {
            let substituted = substitute_exact_type_db(db, inner, from, to, memo);
            if substituted != inner {
                db.readonly_type(substituted)
            } else {
                type_id
            }
        }
        Some(TypeData::NoInfer(inner)) => {
            let substituted = substitute_exact_type_db(db, inner, from, to, memo);
            if substituted != inner {
                db.no_infer(substituted)
            } else {
                type_id
            }
        }
        Some(TypeData::StringIntrinsic { kind, type_arg }) => {
            let substituted = substitute_exact_type_db(db, type_arg, from, to, memo);
            if substituted != type_arg {
                db.string_intrinsic(kind, substituted)
            } else {
                type_id
            }
        }
        // Object/Function/Mapped/etc. — substitution does not reach into
        // these in the original method either; preserve behaviour.
        _ => type_id,
    };

    memo.insert(type_id, result);
    result
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Substitute every occurrence of `from` with `to` inside `type_id`.
    ///
    /// `memo` maps each visited node to its substituted result. On entry we
    /// insert `type_id -> type_id` as a cycle guard; if a recursive call
    /// re-enters the same node before we finish, the cached self-mapping is
    /// returned (matching the previous `seen`-set behavior). Once processing
    /// completes we overwrite the placeholder with the real substituted
    /// result, so later non-reentrant visits to the same hash-consed node
    /// reuse the substituted value instead of seeing the original.
    pub(crate) fn substitute_exact_type(
        &mut self,
        type_id: TypeId,
        from: TypeId,
        to: TypeId,
        memo: &mut FxHashMap<TypeId, TypeId>,
    ) -> TypeId {
        substitute_exact_type_db(self.interner(), type_id, from, to, memo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;
    use crate::types::TypeParamInfo;

    /// Regression: `substitute_exact_type` must substitute every occurrence
    /// of `from`, including hash-consed nodes that appear via multiple paths.
    /// Previously the visit-once `seen` set caused later occurrences of a
    /// shared node to be returned unchanged.
    #[test]
    fn test_substitute_exact_type_handles_shared_hash_consed_nodes() {
        let interner = TypeInterner::new();

        // Type parameter `T`.
        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });

        // Two named base types `Bar` and `Foo` (modeled as `Lazy(DefId)`).
        let bar = interner.lazy(DefId(101));
        let foo = interner.lazy(DefId(102));

        // Inner `Bar<T>` — the interner is hash-consed, so referencing this
        // structure twice yields the *same* TypeId.
        let bar_of_t = interner.application(bar, vec![t_param]);
        let bar_of_t_again = interner.application(bar, vec![t_param]);
        assert_eq!(
            bar_of_t, bar_of_t_again,
            "interner should return the same TypeId for structurally identical Application types"
        );

        // Outer `Foo<Bar<T>, Bar<T>>` — both args are the same shared node.
        let outer = interner.application(foo, vec![bar_of_t, bar_of_t]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(outer, t_param, TypeId::STRING, &mut memo);

        // Expected: `Foo<Bar<string>, Bar<string>>`.
        let expected_inner = interner.application(bar, vec![TypeId::STRING]);
        let expected = interner.application(foo, vec![expected_inner, expected_inner]);
        assert_eq!(
            result, expected,
            "shared hash-consed node should be substituted on every occurrence"
        );

        // Sanity: pre-fix output would have been `Foo<Bar<string>, Bar<T>>`.
        let buggy_outer = interner.application(foo, vec![expected_inner, bar_of_t]);
        assert_ne!(
            result, buggy_outer,
            "second occurrence of shared node was left unsubstituted (pre-fix bug)"
        );
    }

    #[test]
    fn test_substitute_exact_type_reuses_memo_without_corrupting_shared_node() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });

        let bar = interner.lazy(DefId(201));
        let baz = interner.lazy(DefId(202));
        let foo = interner.lazy(DefId(203));

        let bar_of_t = interner.application(bar, vec![t_param]);
        let baz_of_bar_t = interner.application(baz, vec![bar_of_t]);
        let outer = interner.application(foo, vec![bar_of_t, bar_of_t, baz_of_bar_t]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(outer, t_param, TypeId::STRING, &mut memo);

        let bar_of_string = interner.application(bar, vec![TypeId::STRING]);
        let baz_of_bar_string = interner.application(baz, vec![bar_of_string]);
        let expected =
            interner.application(foo, vec![bar_of_string, bar_of_string, baz_of_bar_string]);
        assert_eq!(
            result, expected,
            "third visit to a shared node must reuse the substituted memo value"
        );

        let corrupted = interner.application(foo, vec![bar_of_string, bar_of_string, baz_of_bar_t]);
        assert_ne!(
            result, corrupted,
            "memo lookup was corrupted back to the original unsubstituted node"
        );
    }

    #[test]
    fn test_substitute_exact_type_reaches_index_access_and_template_spans() {
        let interner = TypeInterner::new();

        let k_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let obj = interner.lazy(DefId(301));
        let indexed = interner.index_access(obj, k_param);
        let dot = interner.intern_string(".");
        let template = interner.template_literal(vec![
            TemplateSpan::Type(k_param),
            TemplateSpan::Text(dot),
            TemplateSpan::Type(indexed),
        ]);
        let branch = interner.union(vec![indexed, template]);
        let meta = interner.literal_string("meta");

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, k_param, meta, &mut memo);

        let expected_indexed = interner.index_access(obj, meta);
        let expected_template = interner.template_literal(vec![
            TemplateSpan::Type(meta),
            TemplateSpan::Text(dot),
            TemplateSpan::Type(expected_indexed),
        ]);
        let expected = interner.union(vec![expected_indexed, expected_template]);
        assert_eq!(
            result, expected,
            "distributive branch substitution must update T[K] and template-literal K spans"
        );
    }
}
