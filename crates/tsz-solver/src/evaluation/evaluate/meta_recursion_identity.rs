//! Recursion-identity containment for evaluator meta-operations.

use super::TypeEvaluator;
use crate::def::DefId;
use crate::evaluation::result::TerminationKind;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    ConditionalTypeId, ObjectShapeId, SymbolRef, TupleListId, TypeData, TypeId, TypeParamInfo,
    TypeParamOrigin,
};
use tsz_common::interner::Atom;

const META_REREDUCE_RECURSION_IDENTITY_MAX_DEPTH: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MetaRecursionIdentity {
    Def(DefId),
    Symbol(tsz_binder::SymbolId),
    Type(TypeId),
    Object(ObjectShapeId),
    Tuple(TupleListId),
    Conditional(ConditionalTypeId),
    TypeQuery(SymbolRef),
    TypeParamDecl { file: Atom, node: u32 },
    InferPlaceholder(u64),
    InferSource(u64),
    BoundParameter(u32),
    Recursive(u32),
}

fn leftmost_index_access_object(
    interner: &dyn crate::construction::TypeDatabase,
    mut type_id: TypeId,
) -> TypeId {
    while let Some(TypeData::IndexAccess(object_type, _)) = interner.lookup(type_id) {
        type_id = object_type;
    }
    type_id
}

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    // tsc bounds productive meta-operation growth by recursion identity. For
    // evaluator-side `IndexAccess`/`KeyOf`, the stable identity is the terminal
    // definition under transparent wrappers; when evaluation grows the wrapper
    // chain but keeps that identity, preserve the deferred form.
    pub(in crate::evaluation) fn same_meta_recursion_identity(
        &self,
        before: TypeId,
        after: TypeId,
    ) -> bool {
        let Some(before_def) = self.meta_recursion_identity(before) else {
            return false;
        };
        let Some(after_def) = self.meta_recursion_identity(after) else {
            return false;
        };
        self.same_def_recursion_identity(before_def, after_def)
    }

    fn meta_recursion_identity(&self, type_id: TypeId) -> Option<DefId> {
        let mut current = type_id;
        for _ in 0..32 {
            match self.interner.lookup(current)? {
                TypeData::Lazy(def_id) => return Some(def_id),
                TypeData::TypeQuery(symbol_ref) => {
                    return self.resolver.symbol_to_def_id(symbol_ref);
                }
                TypeData::Application(app_id) => {
                    current = self.interner.type_application(app_id).base;
                }
                TypeData::IndexAccess(object, _) | TypeData::KeyOf(object) => {
                    current = object;
                }
                TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                    current = inner;
                }
                _ => return None,
            }
        }
        None
    }

    /// Run an eager `keyof` / indexed-access re-reduction under a tsc-style
    /// recursion identity. If this would enter the fifth nested re-reduction
    /// with the same origin, keep the caller's deferred meta-type instead of
    /// forcing another expansion.
    pub(in crate::evaluation) fn with_meta_rereduce_recursion_identity(
        &mut self,
        type_id: TypeId,
        deferred_fallback: TypeId,
        body: impl FnOnce(&mut Self) -> TypeId,
    ) -> TypeId {
        let identity = self.meta_rereduce_recursion_identity(type_id);
        let existing_count = self
            .meta_recursion_identity_stack
            .iter()
            .filter(|&&entry| self.same_rereduce_recursion_identity(entry, identity))
            .count();
        if existing_count + 1 >= META_REREDUCE_RECURSION_IDENTITY_MAX_DEPTH {
            self.record_request_limit_event(TerminationKind::IterationExceeded);
            return deferred_fallback;
        }

        let stack_len = self.meta_recursion_identity_stack.len();
        self.meta_recursion_identity_stack.push(identity);
        let result = body(self);
        debug_assert_eq!(
            self.meta_recursion_identity_stack.get(stack_len).copied(),
            Some(identity)
        );
        self.meta_recursion_identity_stack.truncate(stack_len);
        result
    }

    fn same_def_recursion_identity(&self, left: DefId, right: DefId) -> bool {
        let canonical_left = self.resolver.canonical_def_id(left);
        let canonical_right = self.resolver.canonical_def_id(right);

        left == right
            || canonical_left == canonical_right
            || self.resolver.defs_are_equivalent(left, right)
            || self
                .resolver
                .defs_are_equivalent(canonical_left, canonical_right)
    }

    fn same_rereduce_recursion_identity(
        &self,
        left: MetaRecursionIdentity,
        right: MetaRecursionIdentity,
    ) -> bool {
        match (left, right) {
            (MetaRecursionIdentity::Def(left), MetaRecursionIdentity::Def(right)) => {
                self.same_def_recursion_identity(left, right)
            }
            _ => left == right,
        }
    }

    fn meta_rereduce_recursion_identity(&self, type_id: TypeId) -> MetaRecursionIdentity {
        if type_id.is_intrinsic() {
            return MetaRecursionIdentity::Type(type_id);
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id)) => {
                MetaRecursionIdentity::Def(self.resolver.canonical_def_id(def_id))
            }
            Some(TypeData::TypeQuery(symbol)) => self
                .resolver
                .symbol_to_def_id(symbol)
                .map_or(MetaRecursionIdentity::TypeQuery(symbol), |def_id| {
                    MetaRecursionIdentity::Def(self.resolver.canonical_def_id(def_id))
                }),
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                self.interner.object_shape(shape_id).symbol.map_or(
                    MetaRecursionIdentity::Object(shape_id),
                    MetaRecursionIdentity::Symbol,
                )
            }
            Some(TypeData::Tuple(tuple_id)) => MetaRecursionIdentity::Tuple(tuple_id),
            Some(TypeData::Conditional(cond_id)) => MetaRecursionIdentity::Conditional(cond_id),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                Self::type_param_rereduce_recursion_identity(type_id, info)
            }
            Some(TypeData::BoundParameter(index)) => MetaRecursionIdentity::BoundParameter(index),
            Some(TypeData::Recursive(index)) => MetaRecursionIdentity::Recursive(index),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.meta_rereduce_recursion_identity(app.base)
            }
            Some(TypeData::IndexAccess(object_type, _)) => self.meta_rereduce_recursion_identity(
                leftmost_index_access_object(self.interner, object_type),
            ),
            Some(TypeData::KeyOf(operand)) => self.meta_rereduce_recursion_identity(operand),
            _ => MetaRecursionIdentity::Type(type_id),
        }
    }

    const fn type_param_rereduce_recursion_identity(
        type_id: TypeId,
        info: TypeParamInfo,
    ) -> MetaRecursionIdentity {
        match info.origin {
            TypeParamOrigin::DeclScoped { file, node } => {
                MetaRecursionIdentity::TypeParamDecl { file, node }
            }
            TypeParamOrigin::InferPlaceholder { id } => MetaRecursionIdentity::InferPlaceholder(id),
            TypeParamOrigin::InferSource { id, .. } => MetaRecursionIdentity::InferSource(id),
            TypeParamOrigin::User => MetaRecursionIdentity::Type(type_id),
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_meta_rereduce_recursion_identity_for_test(
        &mut self,
        type_id: TypeId,
        count: usize,
    ) {
        let identity = self.meta_rereduce_recursion_identity(type_id);
        self.meta_recursion_identity_stack
            .extend(std::iter::repeat_n(identity, count));
    }

    pub(in crate::evaluation) fn defer_same_identity_meta_recursion(&mut self) {
        self.mark_silent_depth_bailed();
        self.note_request_termination(TerminationKind::CrossEvalCycle);
    }
}
