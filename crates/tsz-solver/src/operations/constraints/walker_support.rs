//! Support helpers for the core constraint walker.

use crate::def::DefId;
use crate::inference::infer::InferenceContext;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{MappedType, TypeData, TypeId, TypeListId};
use rustc_hash::FxHashMap;
use std::sync::Arc;

impl<C: AssignabilityChecker> CallEvaluator<'_, C> {
    /// Constrain a mapped type's parameters from a source whose key space is
    /// empty: the intrinsic `object` type, or an empty object literal `{}`.
    ///
    /// `keyof <keyless source>` is `never`, so the mapped key space collapses to
    /// `never`. For a non-homomorphic mapped type (`{ [P in K]: T }`, whose
    /// constraint is a plain key set rather than `keyof <param>`) the value space
    /// is empty as well, so the template type parameter is inferred as `never`.
    /// This matches tsc's `inferToMappedType`, which infers `[never, never]` for
    /// both `rec(object)` and `rec({})` against
    /// `<K extends PropertyKey, T>(r: Record<K, T>)`. Homomorphic mapped types
    /// (constraint `keyof <param>`) keep only the key-space collapse, preserving
    /// their reverse-mapped candidate handling.
    pub(super) fn constrain_empty_keyspace_mapped(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        mapped: &MappedType,
        priority: crate::types::InferencePriority,
    ) {
        self.constrain_types(ctx, var_map, TypeId::NEVER, mapped.constraint, priority);
        if self
            .find_keyof_inference_target(mapped.constraint, var_map)
            .is_none()
        {
            let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NEVER);
            let instantiated_template = instantiate_type(self.interner, mapped.template, &subst);
            self.constrain_types(
                ctx,
                var_map,
                TypeId::NEVER,
                instantiated_template,
                crate::types::InferencePriority::MappedType,
            );
        }
    }

    /// Normalize a union's members for inference partitioning by peeling
    /// transparent identity-alias applications (`type Some<X> = X`).
    pub(super) fn normalize_union_members_for_inference(
        &mut self,
        members: TypeListId,
    ) -> Arc<[TypeId]> {
        let original = self.interner.type_list(members);
        if !original
            .iter()
            .any(|&member| self.lazy_alias_application_def_id(member).is_some())
        {
            return original;
        }
        let mut out = Vec::with_capacity(original.len());
        for &member in original.iter() {
            let Some(peeled) = self.try_peel_identity_alias_application(member) else {
                out.push(member);
                continue;
            };
            if let Some(TypeData::Union(inner)) = self.interner.lookup(peeled) {
                out.extend(self.interner.type_list(inner).iter().copied());
            } else {
                out.push(peeled);
            }
        }
        out.into()
    }

    fn lazy_alias_application_def_id(&self, member: TypeId) -> Option<DefId> {
        let TypeData::Application(app_id) = self.interner.lookup(member)? else {
            return None;
        };
        let base = self.interner.type_application(app_id).base;
        if base.is_intrinsic() {
            return None;
        }
        match self.interner.lookup(base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            _ => None,
        }
    }

    /// If `member` applies a transparent identity alias, expand it to the
    /// forwarded type argument. Aliases that add structure stay opaque so
    /// structural union inference can still match against the wrapper.
    fn try_peel_identity_alias_application(&mut self, member: TypeId) -> Option<TypeId> {
        let def_id = self.lazy_alias_application_def_id(member)?;
        let is_identity = {
            let resolver = self.checker.type_resolver()?;
            let type_params = resolver.get_lazy_type_params(def_id)?;
            let body = resolver.resolve_lazy(def_id, self.interner)?;
            match self.interner.lookup(body)? {
                TypeData::TypeParameter(body_param) => {
                    type_params.iter().any(|p| p.name == body_param.name)
                }
                _ => false,
            }
        };
        if !is_identity {
            return None;
        }
        let peeled = self.checker.expand_type_alias_application(member)?;
        (peeled != member).then_some(peeled)
    }

    pub(super) fn constraint_is_nullable_union(&self, constraint: TypeId) -> bool {
        let Some(TypeData::Union(members)) = self.interner.lookup(constraint) else {
            return false;
        };
        self.interner
            .type_list(members)
            .iter()
            .any(|&member| member.is_nullable())
    }
}
