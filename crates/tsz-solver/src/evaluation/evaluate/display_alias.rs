use crate::def::{DefId, DefKind};
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashSet;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(in crate::evaluation) fn should_record_application_alias(
        &self,
        evaluated: TypeId,
        application: TypeId,
        skip_type_alias_repaint: bool,
        keep_existing_conditional_branch_alias: bool,
    ) -> bool {
        !skip_type_alias_repaint
            && !keep_existing_conditional_branch_alias
            && !self.suppress_spread_tuple_alias_repaint(evaluated, application)
    }

    pub(in crate::evaluation) fn should_store_structural_display_alias(
        &self,
        evaluated: TypeId,
        application: TypeId,
        evaluated_is_mapped: bool,
    ) -> bool {
        (evaluated_is_mapped || self.is_recursive_type_alias_application(application))
            && !self.suppress_spread_tuple_alias_repaint(evaluated, application)
    }

    pub(in crate::evaluation) fn is_concrete_application_display_branch(
        &self,
        branch: TypeId,
        evaluated: TypeId,
    ) -> bool {
        matches!(self.interner.lookup(branch), Some(TypeData::Application(_)))
            && Self::is_displayable_conditional_branch_result(self.interner, evaluated)
            && !crate::type_queries::contains_generic_type_parameters_db(self.interner, branch)
            && !self.suppress_spread_tuple_alias_repaint(evaluated, branch)
    }

    /// Classify an `Application`'s type-alias body for tuple display-alias
    /// purposes.
    ///
    /// * `Some(true)`  — the alias body resolves to a fixed tuple literal, e.g.
    ///   `Pair<A, B> = [A, B]`.
    /// * `Some(false)` — the alias body resolves to something that yields a
    ///   tuple through spreading or branch resolution: a variadic tuple
    ///   (`[T, ...A]`, `[...A, ...B]`) or a recursive tuple builder.
    /// * `None`        — the caller keeps its existing behaviour.
    fn application_alias_body_is_fixed_tuple(&self, application: TypeId) -> Option<bool> {
        let TypeData::Application(app_id) = self.interner.lookup(application)? else {
            return None;
        };
        let app = self.interner.type_application(app_id);
        let def_id = self.resolve_application_def_id(app.base)?;
        let body = self.resolver.resolve_lazy(def_id, self.interner)?;
        let inner = crate::type_queries::data::unwrap_readonly(self.interner, body);
        Some(
            matches!(self.interner.lookup(inner), Some(TypeData::Tuple(_)))
                && !crate::type_queries::data::is_variadic_tuple(self.interner, inner),
        )
    }

    fn is_recursive_type_alias_application(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base) else {
            return false;
        };
        if self.resolver.get_def_kind(def_id) != Some(DefKind::TypeAlias) {
            return false;
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            return false;
        };
        let mut visited = FxHashSet::default();
        self.type_reaches_alias_def(body, def_id, &mut visited)
    }

    fn type_reaches_alias_def(
        &self,
        type_id: TypeId,
        target_def_id: DefId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id))
                if self.resolver.defs_are_equivalent(def_id, target_def_id) =>
            {
                return true;
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                    && self.resolver.defs_are_equivalent(def_id, target_def_id)
                {
                    return true;
                }
            }
            _ => {}
        }

        let mut found = false;
        crate::visitor::for_each_child_by_id(self.interner, type_id, |child| {
            if !found {
                found = self.type_reaches_alias_def(child, target_def_id, visited);
            }
        });
        found
    }

    /// True when a display-alias repaint of `evaluated` to the named
    /// `application` form must be skipped.
    ///
    /// `tsc` keeps an alias symbol on a tuple result only when the alias body
    /// is a fixed tuple literal. Spread tuple aliases and recursive aliases
    /// that build tuples by spreading produce a fresh tuple, so diagnostics
    /// print the resolved tuple structurally.
    pub(in crate::evaluation) fn suppress_spread_tuple_alias_repaint(
        &self,
        evaluated: TypeId,
        application: TypeId,
    ) -> bool {
        matches!(self.interner.lookup(evaluated), Some(TypeData::Tuple(_)))
            && self.application_alias_body_is_fixed_tuple(application) == Some(false)
    }
}
