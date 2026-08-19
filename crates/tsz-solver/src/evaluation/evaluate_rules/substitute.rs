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
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, ObjectShape,
    ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
};
use rustc_hash::FxHashMap;

use super::super::evaluate::TypeEvaluator;

/// Rewrite `from -> to` inside every property's read and write type, keeping
/// all declaration-site metadata (optionality, readonly, visibility, symbol,
/// declaration order) intact. Returns the rebuilt list and whether any type
/// changed so callers can preserve hash-consed identity on a no-op.
fn substitute_properties_db(
    db: &dyn TypeDatabase,
    properties: &[PropertyInfo],
    from: TypeId,
    to: TypeId,
    memo: &mut FxHashMap<TypeId, TypeId>,
) -> (Vec<PropertyInfo>, bool) {
    let mut changed = false;
    let rebuilt = properties
        .iter()
        .map(|prop| {
            let type_id = substitute_exact_type_db(db, prop.type_id, from, to, memo);
            // Getter-only / plain properties share one type; reuse the result
            // instead of re-walking the same node for the write slot.
            let write_type = if prop.write_type == prop.type_id {
                type_id
            } else {
                substitute_exact_type_db(db, prop.write_type, from, to, memo)
            };
            changed |= type_id != prop.type_id || write_type != prop.write_type;
            PropertyInfo {
                type_id,
                write_type,
                ..prop.clone()
            }
        })
        .collect();
    (rebuilt, changed)
}

/// Rewrite `from -> to` inside an index signature's value type, preserving the
/// key type and `readonly`/cosmetic metadata. Returns the rebuilt signature and
/// whether the value type changed.
fn substitute_index_signature_db(
    db: &dyn TypeDatabase,
    idx: &IndexSignature,
    from: TypeId,
    to: TypeId,
    memo: &mut FxHashMap<TypeId, TypeId>,
) -> (IndexSignature, bool) {
    let value_type = substitute_exact_type_db(db, idx.value_type, from, to, memo);
    let changed = value_type != idx.value_type;
    (IndexSignature { value_type, ..*idx }, changed)
}

/// Rewrite `from -> to` inside a single call/construct signature's parameter
/// types, this-type, return type, and type-predicate payload. Returns the
/// rebuilt signature and whether any type changed.
fn substitute_call_signature_db(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
    from: TypeId,
    to: TypeId,
    memo: &mut FxHashMap<TypeId, TypeId>,
) -> (CallSignature, bool) {
    let mut changed = false;
    let params = sig
        .params
        .iter()
        .map(|param| {
            let type_id = substitute_exact_type_db(db, param.type_id, from, to, memo);
            changed |= type_id != param.type_id;
            ParamInfo { type_id, ..*param }
        })
        .collect();
    let this_type = sig.this_type.map(|this| {
        let substituted = substitute_exact_type_db(db, this, from, to, memo);
        changed |= substituted != this;
        substituted
    });
    let return_type = substitute_exact_type_db(db, sig.return_type, from, to, memo);
    changed |= return_type != sig.return_type;
    let type_predicate = sig.type_predicate.map(|mut predicate| {
        if let Some(predicate_type) = predicate.type_id {
            let substituted = substitute_exact_type_db(db, predicate_type, from, to, memo);
            changed |= substituted != predicate_type;
            predicate.type_id = Some(substituted);
        }
        predicate
    });
    let rebuilt = CallSignature {
        type_params: sig.type_params.clone(),
        params,
        this_type,
        return_type,
        type_predicate,
        is_method: sig.is_method,
        declaration_group: sig.declaration_group,
    };
    (rebuilt, changed)
}

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
        // Callable object types in distributive-conditional branches — e.g.
        // `T extends string ? { (arg: T): T } : never` — carry the distribution
        // variable in call/construct signature parameter/return types and in
        // any attached properties.  Without this arm every union member that
        // hits this branch returns the same (unsubstituted) Callable TypeId,
        // collapsing the distribution into a single widened shape instead of a
        // per-member union.
        Some(TypeData::Callable(cs_id)) => {
            let shape = db.callable_shape(cs_id);
            let mut changed = false;
            let call_signatures: Vec<CallSignature> = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let (rebuilt, sig_changed) =
                        substitute_call_signature_db(db, sig, from, to, memo);
                    changed |= sig_changed;
                    rebuilt
                })
                .collect();
            let construct_signatures: Vec<CallSignature> = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let (rebuilt, sig_changed) =
                        substitute_call_signature_db(db, sig, from, to, memo);
                    changed |= sig_changed;
                    rebuilt
                })
                .collect();
            let (properties, props_changed) =
                substitute_properties_db(db, &shape.properties, from, to, memo);
            changed |= props_changed;
            let string_index = shape.string_index.as_ref().map(|idx| {
                let (sig, sig_changed) = substitute_index_signature_db(db, idx, from, to, memo);
                changed |= sig_changed;
                sig
            });
            let number_index = shape.number_index.as_ref().map(|idx| {
                let (sig, sig_changed) = substitute_index_signature_db(db, idx, from, to, memo);
                changed |= sig_changed;
                sig
            });
            if changed {
                db.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index,
                    number_index,
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                })
            } else {
                type_id
            }
        }
        // Object literals reached as a distributive-conditional branch carry
        // the distribution variable in their property/index value types — e.g.
        // `T extends ... ? { kind: 'x'; value: T } : ...`. When the check side
        // is a deferred union the per-member rewrite happens here (not at
        // instantiation time), so the variable must be substituted inside the
        // shape; otherwise every union member collapses to one widened object.
        Some(TypeData::Object(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let (properties, changed) =
                substitute_properties_db(db, &shape.properties, from, to, memo);
            if changed {
                db.object_with_flags_and_symbol(properties, shape.flags, shape.symbol)
            } else {
                type_id
            }
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let (properties, mut changed) =
                substitute_properties_db(db, &shape.properties, from, to, memo);
            let string_index = shape.string_index.as_ref().map(|idx| {
                let (sig, sig_changed) = substitute_index_signature_db(db, idx, from, to, memo);
                changed |= sig_changed;
                sig
            });
            let number_index = shape.number_index.as_ref().map(|idx| {
                let (sig, sig_changed) = substitute_index_signature_db(db, idx, from, to, memo);
                changed |= sig_changed;
                sig
            });
            let symbol_index = shape.symbol_index.as_ref().map(|idx| {
                let (sig, sig_changed) = substitute_index_signature_db(db, idx, from, to, memo);
                changed |= sig_changed;
                sig
            });
            if changed {
                db.object_with_index(ObjectShape {
                    flags: shape.flags,
                    properties,
                    string_index,
                    number_index,
                    symbol_index,
                    symbol: shape.symbol,
                })
            } else {
                type_id
            }
        }
        // Mapped, TypeParameter, Lazy, Recursive, Enum, etc. — substitution
        // does not reach into these structural leaf or deferred nodes.  Mapped
        // types own their own substitution pass; Lazy/Recursive/TypeParameter
        // are already handled by the `type_id == from` guard above when they
        // ARE the target variable.
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
            origin: crate::types::TypeParamOrigin::User,
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
            origin: crate::types::TypeParamOrigin::User,
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
            origin: crate::types::TypeParamOrigin::User,
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

    /// Regression for the distributive-conditional-over-deferred-union family
    /// (issue #10864): when a distributive conditional's check side is a
    /// deferred union the per-member rewrite runs through
    /// `substitute_exact_type`. The true branch is frequently an object literal
    /// (`{ value: T }`, `{ kind; value: T }`), so substitution must reach into
    /// object property read/write types — otherwise every union member collapses
    /// to one widened object and the conditional becomes over-constrained.
    #[test]
    fn test_substitute_exact_type_reaches_object_property_types() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let value_atom = interner.intern_string("value");
        let nested_atom = interner.intern_string("inner");

        // `{ value: { inner: T } }` — the distribution variable is two object
        // levels deep, so the rewrite must recurse structurally.
        let inner = interner.object(vec![PropertyInfo::new(nested_atom, t_param)]);
        let branch = interner.object(vec![PropertyInfo::new(value_atom, inner)]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, t_param, TypeId::NUMBER, &mut memo);

        let expected_inner = interner.object(vec![PropertyInfo::new(nested_atom, TypeId::NUMBER)]);
        let expected = interner.object(vec![PropertyInfo::new(value_atom, expected_inner)]);
        assert_eq!(
            result, expected,
            "object-valued distributive branch must substitute the variable inside property types"
        );
        assert_ne!(
            result, branch,
            "pre-fix behaviour left object property types unsubstituted, widening the branch"
        );
    }

    /// Callable branch types carry the distribution variable in their call
    /// signatures. When a distributive conditional's true/false branch is a
    /// type literal with call signatures (`{ (arg: T): T }`), the solver
    /// represents it as `TypeData::Callable`. Without the `Callable` arm in
    /// `substitute_exact_type_db` every union member would collapse to the
    /// same hash-consed Callable (T still free) instead of a per-member union.
    #[test]
    fn test_substitute_exact_type_reaches_callable_call_signature() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let arg_atom = interner.intern_string("arg");

        // `{ (arg: T): T }` — call signature with T in param and return.
        let callable = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo::required(arg_atom, t_param)],
                this_type: None,
                return_type: t_param,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(callable, t_param, TypeId::STRING, &mut memo);

        // The substituted Callable should have `(arg: string): string`.
        let expected = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo::required(arg_atom, TypeId::STRING)],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        assert_eq!(
            result, expected,
            "Callable call-signature param/return types must be substituted"
        );
        assert_ne!(
            result, callable,
            "pre-fix: Callable was returned unchanged with T still free"
        );
    }

    /// A no-op substitution (the variable does not occur in the object) must
    /// return the original hash-consed `TypeId` so identity-based caches and
    /// display aliases are preserved.
    #[test]
    fn test_substitute_exact_type_object_no_match_preserves_identity() {
        let interner = TypeInterner::new();

        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let value_atom = interner.intern_string("value");
        let branch = interner.object(vec![PropertyInfo::new(value_atom, TypeId::STRING)]);

        let mut evaluator =
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::new(&interner);
        let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
        let result = evaluator.substitute_exact_type(branch, t_param, TypeId::NUMBER, &mut memo);

        assert_eq!(
            result, branch,
            "object without the substituted variable must keep its original TypeId"
        );
    }
}
