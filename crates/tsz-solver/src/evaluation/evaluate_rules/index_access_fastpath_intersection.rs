//! Large-union fast-path and intersection receiver helpers for index access.
//!
//! Split from `index_access.rs` (arch size ratchet): the literal-key
//! fast path over very large unions, and the intersection-receiver
//! property-collection/`this`-property lookup paths, including the
//! unresolved-member deferral guard for `(A & B)[K]` (#15676).

use crate::objects::PropertyCollectionResult;
use crate::relations::subtype::TypeResolver;
use crate::types::{ObjectShape, TypeData, TypeId, TypeListId};
use crate::visitor::literal_number;

use super::index_access::IndexAccessVisitor;

impl<'a, 'b, R: TypeResolver> IndexAccessVisitor<'a, 'b, R> {
    pub(super) fn can_fast_path_large_union_index(&self) -> bool {
        crate::type_queries::get_literal_property_name(self.evaluator.interner(), self.index_type)
            .is_some()
            || literal_number(self.evaluator.interner(), self.index_type).is_some()
            || matches!(self.index_type, TypeId::STRING | TypeId::NUMBER)
    }

    pub(super) fn try_fast_index_large_union_member(&mut self, member: TypeId) -> Option<TypeId> {
        // Intrinsics are never Object/ObjectWithIndex/Array/Tuple — skip lookup.
        if member.is_intrinsic() {
            return None;
        }
        match self.evaluator.interner().lookup(member) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.evaluator.interner().object_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_object_index(&shape.properties, self.index_type),
                )
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.evaluator.interner().object_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_object_with_index(&shape, self.index_type),
                )
            }
            Some(TypeData::Array(element_type)) => Some(
                self.evaluator
                    .evaluate_array_index(element_type, self.index_type),
            ),
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.evaluator.interner().tuple_list(list_id);
                Some(
                    self.evaluator
                        .evaluate_tuple_index(&elements, self.index_type),
                )
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.evaluator.interner().callable_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_callable_index(&shape, self.index_type),
                )
            }
            Some(TypeData::ReadonlyType(inner_type)) => {
                self.try_fast_index_large_union_member(inner_type)
            }
            Some(TypeData::Lazy(def_id)) => {
                let resolved = self
                    .evaluator
                    .resolver()
                    .resolve_lazy(def_id, self.evaluator.interner())?;
                if resolved == member {
                    None
                } else {
                    self.try_fast_index_large_union_member(resolved)
                }
            }
            _ => None,
        }
    }

    /// Bind a `this`-typed property value to the receiver it was read from.
    ///
    /// When an indexed access reads a property whose declared type references the
    /// polymorphic `this` (e.g. `interface I { return: this["args"] }`), `tsc`
    /// binds `this` to the receiver object the property is read from — its
    /// `getTypeWithThisArgument`. Without this binding the read leaks an
    /// unreduced `this[...]` that is neither assignable to nor identical with its
    /// reduced form, breaking both the assignability gateway and `Equal`-style
    /// identity checks. `self.object_type` is the concrete object/intersection
    /// shape the property was looked up on, i.e. exactly that receiver.
    ///
    /// Shares the `getTypeWithThisArgument` rebinding policy with the
    /// infer-candidate path via [`TypeEvaluator::bind_member_this`]
    /// (the `== UNDEFINED` short-circuit is subsumed by its intrinsic guard).
    pub(super) fn bind_property_this(&mut self, result: TypeId) -> TypeId {
        self.evaluator.bind_member_this(result, self.object_type)
    }

    /// True when a per-member indexed access came back as the member's own
    /// deferred `member[K]` because the member is a semantic reference
    /// (`Lazy`/`Application`/`TypeQuery`) the active resolver could not expand —
    /// the cross-file registration window, or the instantiation-time
    /// `NoopResolver` eager-evaluation pass.
    ///
    /// Intersection distribution must not keep such a member-wise deferral:
    /// `(A & B)["a"]` skips constituents lacking the key, while the standalone
    /// member access `B["a"]` later resolves to `undefined` on its own and
    /// poisons the intersection to `never` (false TS2322, #15676). The caller
    /// defers the WHOLE access instead so a resolver-backed pass redoes the
    /// intersection-aware lookup.
    ///
    /// A member that partially resolves to a *different* unresolved ref
    /// (`obj != member`) is not fingerprinted here; that pass already tripped
    /// `mark_unresolved_def_seen`, so the recompute-via-taint backstop owns it.
    pub(super) fn member_access_stuck_on_unresolved_ref(
        &self,
        member: TypeId,
        result: TypeId,
    ) -> bool {
        matches!(
            self.evaluator.interner().lookup(member),
            Some(TypeData::Lazy(_) | TypeData::Application(_) | TypeData::TypeQuery(_))
        ) && crate::index_access_parts(self.evaluator.interner(), result)
            .is_some_and(|(obj, idx)| obj == member && idx == self.index_type)
    }

    /// Merge an intersection receiver's full property set into one object and
    /// index that, so a property read observes every constituent at once. Used
    /// for both generic-index distribution fallback and `this`-typed concrete
    /// reads. Returns `None` (caller falls through) only when the collected
    /// properties do not form an object.
    pub(super) fn index_intersection_via_collected_properties(&mut self) -> Option<TypeId> {
        match crate::objects::collect_properties_cached(
            self.object_type,
            self.evaluator.interner(),
            self.evaluator.resolver(),
            self.evaluator.query_db(),
        ) {
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
                symbol_index,
            } => {
                let merged =
                    if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
                        let shape = ObjectShape {
                            flags: crate::types::ObjectFlags::empty(),
                            properties,
                            string_index,
                            number_index,
                            symbol_index,
                            symbol: None,
                        };
                        self.evaluator.interner().object_with_index(shape)
                    } else {
                        self.evaluator.interner().object(properties)
                    };
                Some(self.evaluator.recurse_index_access(merged, self.index_type))
            }
            PropertyCollectionResult::Any => Some(TypeId::ANY),
            PropertyCollectionResult::NonObject => None,
        }
    }

    /// Resolve a concrete (literal-named) index against an intersection when at
    /// least one constituent declares the named property with a `this`-typed
    /// value. Each constituent's own property type is read raw, its `this` is
    /// rebound to the whole receiver intersection (`self.object_type`), and the
    /// contributions are intersected — mirroring tsc's `getTypeWithThisArgument`
    /// over the intersection. Returns `None` (caller falls through to plain
    /// distribution) when no constituent's matching property references `this`.
    pub(super) fn try_index_intersection_this_property(
        &mut self,
        list_id: TypeListId,
    ) -> Option<TypeId> {
        let name = self
            .evaluator
            .literal_property_lookup_atom(self.index_type)?;
        let members = self.evaluator.interner().type_list(list_id);

        let mut parts = Vec::new();
        let mut saw_this = false;
        for &member in members.iter() {
            let Some(raw) = self.member_own_property_type(member, name) else {
                continue;
            };
            if crate::contains_this_type(self.evaluator.interner(), raw) {
                saw_this = true;
                let bound = crate::instantiation::instantiate::substitute_this_type(
                    self.evaluator.interner(),
                    raw,
                    self.object_type,
                );
                parts.push(self.evaluator.evaluate(bound));
            } else {
                parts.push(raw);
            }
        }

        (saw_this && !parts.is_empty())
            .then(|| crate::utils::intersection_or_single(self.evaluator.interner(), parts))
    }

    /// The raw (pre-`this`-binding) type of `member`'s own property `name`, if
    /// `member` resolves to an object/object-with-index shape that declares it.
    pub(super) fn member_own_property_type(
        &mut self,
        member: TypeId,
        name: tsz_common::Atom,
    ) -> Option<TypeId> {
        let resolved = self.evaluator.evaluate(member);
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.evaluator.interner().lookup(resolved)
        else {
            return None;
        };
        let shape = self.evaluator.interner().object_shape(shape_id);
        shape
            .properties
            .iter()
            .find(|prop| prop.name == name)
            .map(|prop| self.evaluator.optional_property_type(prop))
    }

    pub(super) fn try_fast_index_large_union(&mut self, members: &[TypeId]) -> Option<TypeId> {
        if !self.can_fast_path_large_union_index() {
            return None;
        }

        let mut results = Vec::with_capacity(members.len());
        for &member in members {
            let result = self.try_fast_index_large_union_member(member)?;
            if result != TypeId::UNDEFINED || self.evaluator.no_unchecked_indexed_access() {
                results.push(result);
            }
        }

        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(self.evaluator.interner().union(results))
        }
    }
}
