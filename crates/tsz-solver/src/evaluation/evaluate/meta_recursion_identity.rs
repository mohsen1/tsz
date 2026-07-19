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
    TypeParamJsdocComment { file: Atom, pos: u32 },
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
        let stack_len = self.meta_recursion_identity_stack.len();
        if let Some(fallback) =
            self.enter_meta_rereduce_recursion_identity(type_id, deferred_fallback)
        {
            return fallback;
        }
        let result = body(self);
        self.meta_recursion_identity_stack.truncate(stack_len);
        result
    }

    pub(in crate::evaluation) fn with_optional_meta_rereduce_recursion_identity(
        &mut self,
        type_id: TypeId,
        deferred_fallback: TypeId,
        body: impl FnOnce(&mut Self) -> Option<TypeId>,
    ) -> Option<TypeId> {
        let stack_len = self.meta_recursion_identity_stack.len();
        if let Some(fallback) =
            self.enter_meta_rereduce_recursion_identity(type_id, deferred_fallback)
        {
            return Some(fallback);
        }
        let result = body(self);
        self.meta_recursion_identity_stack.truncate(stack_len);
        result
    }

    pub(in crate::evaluation) fn enter_meta_rereduce_recursion_identity(
        &mut self,
        type_id: TypeId,
        deferred_fallback: TypeId,
    ) -> Option<TypeId> {
        if self.meta_rereduce_identity_count_for_type(type_id) + 1
            >= META_REREDUCE_RECURSION_IDENTITY_MAX_DEPTH
        {
            self.record_request_limit_event(TerminationKind::IterationExceeded);
            return Some(deferred_fallback);
        }

        self.meta_recursion_identity_stack
            .push(self.meta_rereduce_recursion_identity(type_id));
        None
    }

    pub(in crate::evaluation) fn meta_rereduce_recursion_identity_would_exceed_with_seen(
        &self,
        type_id: TypeId,
        seen: &[TypeId],
    ) -> bool {
        self.meta_rereduce_identity_count_for_type(type_id)
            + seen
                .iter()
                .filter(|&&entry| self.same_meta_rereduce_recursion_identity(entry, type_id))
                .count()
            + 1
            >= META_REREDUCE_RECURSION_IDENTITY_MAX_DEPTH
    }

    pub(in crate::evaluation) fn same_meta_rereduce_recursion_identity(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        self.same_rereduce_recursion_identity(
            self.meta_rereduce_recursion_identity(left),
            self.meta_rereduce_recursion_identity(right),
        )
    }

    fn meta_rereduce_identity_count_for_type(&self, type_id: TypeId) -> usize {
        let identity = self.meta_rereduce_recursion_identity(type_id);
        self.meta_recursion_identity_stack
            .iter()
            .filter(|&&entry| self.same_rereduce_recursion_identity(entry, identity))
            .count()
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
            Some(TypeData::Conditional(cond_id)) => self
                .conditional_guard_recursion_identity(cond_id)
                .unwrap_or(MetaRecursionIdentity::Conditional(cond_id)),
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
            TypeParamOrigin::JsdocCommentScoped { file, pos } => {
                MetaRecursionIdentity::TypeParamJsdocComment { file, pos }
            }
            TypeParamOrigin::InferPlaceholder { id } => MetaRecursionIdentity::InferPlaceholder(id),
            TypeParamOrigin::InferSource { id, .. } => MetaRecursionIdentity::InferSource(id),
            // An overload-renamed param already has a program-unique TypeId, so
            // its recursion identity is its type id — same as a plain `User`.
            TypeParamOrigin::User | TypeParamOrigin::OverloadRenamed { .. } => {
                MetaRecursionIdentity::Type(type_id)
            }
        }
    }

    fn conditional_guard_recursion_identity(
        &self,
        cond_id: ConditionalTypeId,
    ) -> Option<MetaRecursionIdentity> {
        let cond = self.interner.get_conditional(cond_id);
        if self.type_contains_infer(cond.check_type) || self.type_contains_infer(cond.extends_type)
        {
            return None;
        }

        let check_identity = self.direct_named_meta_recursion_identity(cond.check_type)?;
        let extends_identity = self.direct_named_meta_recursion_identity(cond.extends_type)?;
        self.same_rereduce_recursion_identity(check_identity, extends_identity)
            .then_some(check_identity)
    }

    fn direct_named_meta_recursion_identity(
        &self,
        type_id: TypeId,
    ) -> Option<MetaRecursionIdentity> {
        match self.interner.lookup(type_id)? {
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.direct_named_meta_recursion_identity(app.base)
            }
            TypeData::Lazy(def_id) => Some(MetaRecursionIdentity::Def(
                self.resolver.canonical_def_id(def_id),
            )),
            TypeData::TypeQuery(symbol) => self
                .resolver
                .symbol_to_def_id(symbol)
                .map(|def_id| MetaRecursionIdentity::Def(self.resolver.canonical_def_id(def_id)))
                .or(Some(MetaRecursionIdentity::TypeQuery(symbol))),
            _ => None,
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
