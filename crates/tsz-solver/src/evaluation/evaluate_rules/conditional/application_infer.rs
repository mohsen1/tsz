use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::type_queries::get_application_base;
use crate::types::{ConditionalType, TypeData, TypeId};
use rustc_hash::FxHashMap;

use super::super::super::evaluate::TypeEvaluator;
use super::super::infer_pattern::InferPatternVisited;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Try to match conditional types at the Application level before structural expansion.
    ///
    /// When both `check_type` and `extends_type` are Applications with the same base type
    /// (e.g., `Promise<string>` vs `Promise<infer U>`), we can match type arguments
    /// directly without expanding the interface structure. This is critical for complex
    /// generic interfaces like Promise, Map, Set where structural expansion makes the
    /// infer pattern matching fail.
    pub(in crate::evaluation::evaluate_rules::conditional) fn try_application_infer_match(
        &mut self,
        cond: &ConditionalType,
    ) -> Option<TypeId> {
        // Only proceed if extends_type is an Application containing infer.
        // Keep extends_type as-is (unevaluated) so match_infer_pattern can handle
        // it at the Application level. This is critical for complex generic interfaces
        // like Promise, Map, Set where structural expansion loses the ability to
        // match type arguments directly.
        let Some(TypeData::Application(pattern_app_id)) = self.interner().lookup(cond.extends_type)
        else {
            return None;
        };
        let pattern_base = self.interner().type_application(pattern_app_id).base;

        let contains_infer =
            if let Some(contains_infer) = self.cached_contains_infer(cond.extends_type) {
                contains_infer
            } else {
                let contains_infer = self.type_contains_infer(cond.extends_type);
                self.cache_contains_infer(cond.extends_type, contains_infer);
                contains_infer
            };
        if !contains_infer {
            return None;
        }

        // Recover an Application form for `check_type` whose base matches
        // `pattern_base`. Three shapes need recovery:
        //   1. raw type isn't an Application (e.g. `S[K]` inside a per-key
        //      conditional) — evaluate may yield one;
        //   2. raw type evaluates to a structural Object/Callable — the
        //      `display_alias` map records a back-reference to the original
        //      Application;
        //   3. raw type IS an Application but its base differs from the
        //      pattern's (e.g. `Exclude<X<T> | undefined, undefined>` wraps
        //      `X<T>`) — evaluate through the wrapper so the
        //      Application-vs-Application match has a same-base source.
        let mut check_type = cond.check_type;
        if get_application_base(self.interner(), check_type) != Some(pattern_base) {
            let evaluated = self.evaluate(check_type);
            if get_application_base(self.interner(), evaluated) == Some(pattern_base) {
                check_type = evaluated;
            } else if let Some(origin) = self.try_recover_application_from_display_alias(evaluated)
                && get_application_base(self.interner(), origin) == Some(pattern_base)
            {
                check_type = origin;
            }
        }

        // Skip for special types.
        if check_type == TypeId::ANY || check_type == TypeId::NEVER {
            return None;
        }
        if matches!(
            self.interner().lookup(check_type),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }

        let direct_application_bases_match =
            get_application_base(self.interner(), check_type) == Some(pattern_base);
        if direct_application_bases_match {
            // Try infer pattern matching with unevaluated same-base applications.
            // Positional argument binding is only sound once the source and pattern
            // share the same generic base. Different-base applications such as
            // `ReturnType<F>` vs `Promise<infer T>` must fall through to the alias
            // reducer below so `ReturnType` can expose its return application first.
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            let mut bindings = FxHashMap::default();
            let mut visited = InferPatternVisited::default();
            let matched = self.match_infer_pattern(
                check_type,
                cond.extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            );
            if matched && !bindings.is_empty() {
                let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                return Some(self.evaluate(substituted_true));
            }
            if self.application_infer_bases_match(check_type, cond.extends_type, &mut checker) {
                return Some(self.evaluate(cond.false_type));
            }
        }

        // Last-chance recovery: reduce the source through generic-alias bodies
        // whose alias body is a conditional that yields an Application form
        // matching the pattern's base. Handles `Application(ReturnType, [F])
        // extends Application(Promise, [infer T])` by simulating ReturnType's
        // body conditional to discover its `Application(Promise, [...])`
        // substituted true-branch, which the structural fallback cannot
        // recover from the fully expanded structural object.
        //
        // Only worth attempting when the raw source is itself an `Application`
        // (potentially reducible by alias peeling) or has a display-alias
        // back-reference to one (recorded for parametric structural bodies).
        // For intrinsics, type parameters, unions, and other shapes the
        // reducer would just do one no-op lookup before returning None.
        for candidate in [cond.check_type, check_type] {
            if Self::is_alias_reducible_candidate(self.interner(), candidate)
                && let Some(reduced) = self.reduce_alias_body_to_application_form(candidate)
                && reduced != candidate
                && reduced != check_type
            {
                let mut checker = self.conditional_subtype_checker();
                checker.allow_bivariant_rest = true;
                let mut bindings = FxHashMap::default();
                let mut visited = InferPatternVisited::default();
                let matched = self.match_infer_pattern(
                    reduced,
                    cond.extends_type,
                    &mut bindings,
                    &mut visited,
                    &mut checker,
                );
                if matched && !bindings.is_empty() {
                    let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                    return Some(self.evaluate(substituted_true));
                }
            }
        }

        if let Some(alias) = self.try_recover_application_from_display_alias(check_type)
            && alias != check_type
        {
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            let mut bindings = FxHashMap::default();
            let mut visited = InferPatternVisited::default();
            let matched = self.match_infer_pattern(
                alias,
                cond.extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            );
            if matched && !bindings.is_empty() {
                let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                return Some(self.evaluate(substituted_true));
            }
        }

        None
    }

    fn application_infer_bases_match(
        &self,
        check_type: TypeId,
        extends_type: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let (
            Some(TypeData::Application(check_app_id)),
            Some(TypeData::Application(pattern_app_id)),
        ) = (
            self.interner().lookup(check_type),
            self.interner().lookup(extends_type),
        )
        else {
            return false;
        };
        let check_app = self.interner().type_application(check_app_id);
        let pattern_app = self.interner().type_application(pattern_app_id);
        check_app.args.len() == pattern_app.args.len()
            && (check_app.base == pattern_app.base
                || (checker.is_subtype_of(check_app.base, pattern_app.base)
                    && checker.is_subtype_of(pattern_app.base, check_app.base)))
    }
}
