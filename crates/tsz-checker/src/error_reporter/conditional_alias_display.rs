//! Conditional/indexed alias application display helpers.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn reduced_alias_app_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let display_alias = self.ctx.types.get_display_alias(ty);
        for candidate in [Some(ty), display_alias].into_iter().flatten() {
            if let Some(display) = self.reduced_alias_app_candidate_display(candidate) {
                return Some(display);
            }
        }
        None
    }

    /// Source/argument-position entry for the reduced conditional/indexed-access
    /// application display. It defers a *non-generic* type alias whose body is
    /// such an application (`type RO = DeepReadonly<Config>`) to the established
    /// non-generic computed-body path in `format_type_for_assignability_message`:
    /// that path drops the wrapping alias symbol and renders the resolved
    /// structural object, whereas `reduced_alias_app_display` would re-emit the
    /// bare alias name (`RO`) because the application-alias skip does not strip a
    /// non-generic alias. Direct generic applications (`Classify<"x">`) and the
    /// concrete shapes they reduce to (which carry an *application* display alias,
    /// not a Lazy non-generic reference) still flow through.
    pub(in crate::error_reporter) fn reduced_generic_application_source_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        if let Some(def_id) = crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && def.type_params.is_empty()
        {
            return None;
        }
        self.reduced_alias_app_display(ty)
    }

    fn reduced_alias_app_candidate_display(&mut self, candidate: TypeId) -> Option<String> {
        if !crate::query_boundaries::diagnostics::alias_application_body_reduces_through_conditional_or_indexed(
            self.ctx.types,
            &self.ctx.definition_store,
            candidate,
        ) {
            return None;
        }

        // `tsc` only drops the `aliasSymbol` once the application is instantiated
        // with *concrete* arguments. An application whose arguments still mention
        // a free type parameter (`ReturnType<T[M]>`) keeps its alias spelling even
        // when the conditional reduces to a concrete type (e.g. `unknown` once the
        // constraint is applied) — tsc renders `ReturnType<T[M]>`, not `unknown`.
        // Decline before evaluating so the deferred application keeps its name.
        if crate::query_boundaries::diagnostics::contains_type_parameters(self.ctx.types, candidate)
        {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(candidate);
        // `tsc` renders a reduced conditional/indexed-access application
        // structurally, but a *union* result's member ordering follows `tsc`'s
        // global lazy creation order, which tsz does not reproduce (it sorts
        // unions canonically). Expanding a union-reducing application would only
        // trade the alias-name surface for a member-order divergence, so keep the
        // alias name for unions and leave them to the separate union-ordering
        // work. Single object/tuple/array/primitive reductions have no such
        // ambiguity and match `tsc` exactly.
        if crate::query_boundaries::diagnostics::is_union_type(self.ctx.types, evaluated) {
            return None;
        }
        // When the reduced shape is registered against a *non-generic* type alias
        // (`type RO = DeepReadonly<Config>` — the wrapping alias survives onto the
        // resolved object), the application-alias skip cannot drop it and would
        // re-emit the bare alias name (`RO`). `tsc` renders such a non-generic
        // alias structurally, but that is already owned by the non-generic
        // computed-body path in `format_type_for_assignability_message`, so defer
        // to it rather than leak the alias here. `registered_non_generic_type_alias_def`
        // resolves the name through the same registration the formatter consults,
        // so a direct generic application (which registers only a generic — or no —
        // alias) still flows through to its structure (`Classify<"x">` → `{ s: "x"; }`).
        if self
            .registered_non_generic_type_alias_def(evaluated)
            .is_some()
        {
            return None;
        }
        (self.should_use_evaluated_assignability_display(candidate, evaluated)
            || crate::query_boundaries::diagnostics::evaluated_alias_application_has_concrete_display(
                self.ctx.types,
                candidate,
                evaluated,
            ))
        .then(|| self.format_type_for_assignability_message_skip_application_alias(evaluated))
    }
}
