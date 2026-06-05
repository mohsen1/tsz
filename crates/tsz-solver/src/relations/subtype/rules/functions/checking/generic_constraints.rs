//! Generic type-parameter constraint helpers for function subtype checks.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{TypeData, TypeId, TypeParamInfo};

use super::super::super::super::{SubtypeChecker, TypeResolver};

/// Result of comparing one source/target type-parameter constraint pair while
/// relating two generic signatures of the same arity (see
/// [`SubtypeChecker::classify_generic_tp_constraint`]).
pub(super) struct GenericTpConstraintRelation {
    /// The source bound is strictly narrower than the target bound, so the source
    /// type parameter cannot be freely alpha-renamed onto the target marker.
    pub(super) source_is_stricter: bool,
    /// The source constraint merely wraps the target's recursive constraint in
    /// extra application layers (e.g. `Array<Array<T>>` vs `Array<T>`); the extra
    /// wrapping is not treated as genuinely stricter. Only computed (and only
    /// meaningful) when `source_is_stricter` is set.
    pub(super) wraps_recursive: bool,
    /// The two constraints are mutually assignable. Only computed (and only
    /// meaningful) when the caller requests the bidirectional check via
    /// `need_bidirectional`, i.e. for mapped/indexed contexts; otherwise `false`.
    pub(super) constraints_mutually_assignable: bool,
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Whether the type parameter `tp_id` appears *bare* anywhere inside
    /// `type_id` -- that is, as a directly-observable value type rather than only
    /// as a type argument of a generic application.
    ///
    /// `Box<T>` does NOT count `T` as bare (it is a type argument whose effect is
    /// mediated by `Box`'s declared variance), but `T`, `T[]`, `T | null`, and
    /// `(arg: T) => void` all do (the parameter is observed directly). Used to
    /// decide whether a generic target signature's type parameter may be erased
    /// to its constraint when relating a non-generic implementation against it:
    /// bare parameters must stay opaque because the caller controls them, while
    /// application-only parameters reduce to their constraint like tsc's
    /// `getBaseSignature`.
    pub(super) fn type_param_appears_bare(&self, type_id: TypeId, tp_id: TypeId) -> bool {
        if type_id == tp_id {
            return true;
        }
        // Cheap pruning: if the parameter does not occur at all, it is not bare.
        if !crate::visitor::collect_all_types(self.interner, type_id).contains(&tp_id) {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if self.type_param_appears_bare(app.base, tp_id) {
                    return true;
                }
                // A direct type argument equal to `tp_id` is mediated by the
                // application's variance and is not a bare occurrence; deeper
                // structure inside an argument still counts.
                app.args
                    .iter()
                    .any(|&arg| arg != tp_id && self.type_param_appears_bare(arg, tp_id))
            }
            Some(TypeData::Array(elem)) => self.type_param_appears_bare(elem, tp_id),
            Some(TypeData::ReadonlyType(inner)) => self.type_param_appears_bare(inner, tp_id),
            Some(TypeData::Union(list)) | Some(TypeData::Intersection(list)) => self
                .interner
                .type_list(list)
                .iter()
                .any(|&m| self.type_param_appears_bare(m, tp_id)),
            Some(TypeData::Tuple(list)) => self
                .interner
                .tuple_list(list)
                .iter()
                .any(|e| self.type_param_appears_bare(e.type_id, tp_id)),
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|p| self.type_param_appears_bare(p.type_id, tp_id))
                    || shape
                        .this_type
                        .is_some_and(|t| self.type_param_appears_bare(t, tp_id))
                    || self.type_param_appears_bare(shape.return_type, tp_id)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| {
                        sig.params
                            .iter()
                            .any(|p| self.type_param_appears_bare(p.type_id, tp_id))
                            || sig
                                .this_type
                                .is_some_and(|t| self.type_param_appears_bare(t, tp_id))
                            || self.type_param_appears_bare(sig.return_type, tp_id)
                    })
            }
            // The parameter occurs (per the pruning check above) inside a variant
            // we do not structurally decompose here. Treat it conservatively as a
            // bare occurrence so the parameter stays opaque rather than being
            // erased -- this preserves existing relation behavior for those shapes.
            _ => true,
        }
    }

    /// Walk a chain of single-argument generic applications (e.g. `Array<Array<T>>`)
    /// looking for `tp_name` at the leaf, returning the shared application base and
    /// the nesting depth. Used to recognise when a source constraint *wraps* the
    /// target's recursive constraint one or more levels deeper, in which case the
    /// extra wrapping does not make the source genuinely stricter.
    fn recursive_application_depth_for_tp(
        &self,
        mut type_id: TypeId,
        tp_name: tsz_common::interner::Atom,
    ) -> Option<(TypeId, usize)> {
        let mut base = None;
        let mut depth = 0;
        loop {
            match self.interner.lookup(type_id) {
                Some(TypeData::Application(app_id)) => {
                    let app = self.interner.type_application(app_id);
                    if app.args.len() != 1 {
                        return None;
                    }
                    if base.is_some_and(|base| base != app.base) {
                        return None;
                    }
                    base = Some(app.base);
                    depth += 1;
                    type_id = app.args[0];
                }
                Some(TypeData::TypeParameter(info) | TypeData::Infer(info))
                    if info.name == tp_name =>
                {
                    return base.map(|base| (base, depth));
                }
                _ => return None,
            }
        }
    }

    /// Classify how a source type parameter's constraint relates to the
    /// corresponding target type parameter's constraint when comparing two
    /// generic signatures of the same arity.
    ///
    /// `target_to_source_substitution` maps the target's type-parameter names onto
    /// the source's type-parameter identities so the two constraints are expressed
    /// in a common vocabulary before comparison.
    ///
    /// `need_bidirectional` requests the (more expensive) mutual-assignability
    /// check used by mapped/indexed contexts. This is a hot path, so the
    /// `check_subtype` queries and recursive-application walks are only performed
    /// when their results can actually affect a decision.
    pub(super) fn classify_generic_tp_constraint(
        &mut self,
        source_tp: &TypeParamInfo,
        target_tp: &TypeParamInfo,
        target_to_source_substitution: &TypeSubstitution,
        need_bidirectional: bool,
    ) -> GenericTpConstraintRelation {
        let source_has_constraint = source_tp.constraint.is_some();
        let target_has_constraint = target_tp.constraint.is_some();
        let source_constraint = source_tp.constraint.unwrap_or(TypeId::UNKNOWN);
        let target_constraint = target_tp.constraint.map_or(TypeId::UNKNOWN, |constraint| {
            instantiate_type(self.interner, constraint, target_to_source_substitution)
        });

        // `target <= source` decides strictness when both sides are constrained,
        // and feeds the mutual-assignability check; only compute it when one of
        // those is reachable.
        let need_target_to_source =
            (source_has_constraint && target_has_constraint) || need_bidirectional;
        let target_to_source = need_target_to_source
            && self
                .check_subtype(target_constraint, source_constraint)
                .is_true();

        // Source is stricter when it imposes a narrower bound than the target:
        // - source constrained, target not -> always stricter;
        // - target constrained, source not -> source is looser (OK);
        // - both constrained -> stricter iff the target bound is not assignable to
        //   the source bound (so the source bound is the narrower one).
        let source_is_stricter = if source_has_constraint && !target_has_constraint {
            true
        } else if !source_has_constraint && target_has_constraint {
            false
        } else if source_has_constraint && target_has_constraint {
            !target_to_source
        } else {
            false
        };

        // The recursive-wrap exception only matters when the source would
        // otherwise be rejected as stricter.
        let wraps_recursive = source_is_stricter && {
            let source_recursive_depth =
                self.recursive_application_depth_for_tp(source_constraint, source_tp.name);
            let target_recursive_depth =
                self.recursive_application_depth_for_tp(target_constraint, source_tp.name);
            source_recursive_depth
                .zip(target_recursive_depth)
                .is_some_and(
                    |((source_base, source_depth), (target_base, target_depth))| {
                        source_base == target_base && source_depth > target_depth
                    },
                )
        };

        let constraints_mutually_assignable = need_bidirectional
            && target_to_source
            && self
                .check_subtype(source_constraint, target_constraint)
                .is_true();

        GenericTpConstraintRelation {
            source_is_stricter,
            wraps_recursive,
            constraints_mutually_assignable,
        }
    }
}
