impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Distribute a conditional type over a union.
    /// (A | B) extends U ? X : Y -> (A extends U ? X : Y) | (B extends U ? X : Y)
    pub(crate) fn distribute_conditional(
        &mut self,
        members: &[TypeId],
        original_check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) -> TypeId {
        // Limit distribution to prevent OOM with large unions
        const MAX_DISTRIBUTION_SIZE: usize = 100;
        if members.len() > MAX_DISTRIBUTION_SIZE {
            self.mark_depth_exceeded();
            return TypeId::ERROR;
        }

        let mut results: SmallVec<[TypeId; 8]> = SmallVec::with_capacity(members.len());
        // PERF: Track whether all results are identical. If every branch
        // produces the same TypeId (common for `T extends X ? never : T`
        // patterns where all members pass/fail uniformly), we can skip the
        // union construction entirely.
        let mut all_same = true;
        let mut first_result = TypeId::NONE;

        // PERF: Pre-allocate the substitution memo outside the loop.
        // Reusing the same HashMap (with clear() between uses) avoids
        // O(members.len()) allocations for large union distributions.
        let mut memo = FxHashMap::default();

        for &member in members {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Substitute the specific member if true_type or false_type references the original check_type
            // This handles cases like: NonNullable<T> = T extends null ? never : T
            // When T = A | B, we need (A extends null ? never : A) | (B extends null ? never : B)
            memo.clear();
            let substituted_extends_type =
                self.substitute_exact_type(extends_type, original_check_type, member, &mut memo);
            memo.clear();
            let substituted_true_type =
                self.substitute_exact_type(true_type, original_check_type, member, &mut memo);
            memo.clear();
            let substituted_false_type =
                self.substitute_exact_type(false_type, original_check_type, member, &mut memo);

            // Create conditional for this union member
            let member_cond = ConditionalType {
                check_type: member,
                extends_type: substituted_extends_type,
                true_type: substituted_true_type,
                false_type: substituted_false_type,
                is_distributive: false,
            };

            // Recursively evaluate via evaluate() to respect depth limits
            let cond_type = self.interner().conditional(member_cond);
            let result = self.evaluate(cond_type);
            // Check if evaluation hit depth limit
            if result == TypeId::ERROR && self.is_depth_exceeded() {
                return TypeId::ERROR;
            }
            if all_same {
                if first_result == TypeId::NONE {
                    first_result = result;
                } else if result != first_result {
                    all_same = false;
                }
            }
            results.push(result);
        }

        // PERF: If all branches produced the same type, return it directly
        // without constructing a union.
        if all_same && first_result != TypeId::NONE {
            return first_result;
        }

        // Combine results into a union
        self.interner().union_from_slice(&results)
    }

    /// Try to match conditional types at the Application level before structural expansion.
    ///
    /// When both `check_type` and `extends_type` are Applications with the same base type
    /// (e.g., `Promise<string>` vs `Promise<infer U>`), we can match type arguments
    /// directly without expanding the interface structure. This is critical for complex
    /// generic interfaces like Promise, Map, Set where structural expansion makes the
    /// infer pattern matching fail.
    fn try_application_infer_match(&mut self, cond: &ConditionalType) -> Option<TypeId> {
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

        // Skip for special types
        if check_type == TypeId::ANY || check_type == TypeId::NEVER {
            return None;
        }
        if matches!(
            self.interner().lookup(check_type),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }

        // Try infer pattern matching with unevaluated types.
        // match_infer_pattern handles Application vs Application matching
        // by comparing base types and recursing on type arguments.
        let mut checker = self.conditional_subtype_checker();
        checker.allow_bivariant_rest = true;
        let mut bindings = FxHashMap::default();
        let mut visited = FxHashSet::default();
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
                let mut visited = FxHashSet::default();
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
            let mut visited = FxHashSet::default();
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

    /// Check whether a type is an **intersection** of type parameters/Lazy refs.
    ///
    /// TSC defers conditional types when the check type is a naked type parameter.
    /// An intersection like `T & U` is NOT a naked type parameter (so Step 2 misses it),
    /// but the subtype relationship `T & U extends X` IS genuinely indeterminate until
    /// T and U are instantiated. This helper detects that case.
    ///
    /// We intentionally limit this to Intersection types. Other compound types like
    /// `keyof T`, `T[K]`, or `Lowercase<T>` are evaluated eagerly by TSC through
    /// constraint resolution and should NOT be deferred at this stage.
    fn type_is_compound_generic(&self, type_id: TypeId) -> bool {
        // Check for compound types containing unresolved type parameter references.
        // We intentionally skip the `contains_type_parameters` visitor here because
        // it catches KeyOf(TypeParam), StringIntrinsic(_, TypeParam), etc., which
        // TSC evaluates eagerly via constraint resolution (not deferral).
        //
        // We handle two compound forms that TSC considers "generic" and defers:
        // - Intersections like `T & U` with type-parameter-like members
        // - IndexAccess like `T[K]` where object or index is generic
        //   (TSC's `isGenericType` returns true for IndexedAccessType with
        //   generic components, causing conditional type deferral)
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner().lookup(type_id) {
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner().type_list(list_id);
                members.iter().any(|&m| {
                    matches!(
                        self.interner().lookup(m),
                        Some(TypeData::Recursive(_) | TypeData::TypeParameter(_))
                    )
                })
            }
            Some(TypeData::IndexAccess(obj, idx)) => {
                // IndexAccess types like T[K] where T or K is an unresolved type
                // parameter are genuinely indeterminate and must be deferred.
                // Example: Extract<M[K], ArrayLike<any>> stays deferred because
                // M[K] could resolve to anything once M and K are instantiated.
                // Named concrete types (Lazy(DefId)) resolve eagerly and do NOT
                // trigger deferral — Interface["prop"] is always evaluatable.
                Self::is_generic_ref(self.interner(), obj)
                    || Self::is_generic_ref(self.interner(), idx)
            }
            _ => false,
        }
    }

    fn type_is_generic_tuple(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Tuple(list_id)) = self.interner().lookup(type_id) else {
            return false;
        };
        let elements = self.interner().tuple_list(list_id);
        elements
            .iter()
            .any(|element| Self::is_generic_ref(self.interner(), element.type_id))
    }

    fn type_contains_never(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NEVER || type_id.is_intrinsic() {
            return type_id == TypeId::NEVER;
        }
        match self.interner().lookup(type_id) {
            Some(TypeData::Tuple(list_id)) => self
                .interner()
                .tuple_list(list_id)
                .iter()
                .any(|element| self.type_contains_never(element.type_id)),
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner()
                .type_list(list_id)
                .iter()
                .any(|&member| self.type_contains_never(member)),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                self.type_contains_never(inner)
            }
            _ => false,
        }
    }

    fn type_has_nested_generic_tuple(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Tuple(list_id)) = self.interner().lookup(type_id) else {
            return false;
        };
        self.interner().tuple_list(list_id).iter().any(|element| {
            matches!(self.interner().lookup(element.type_id), Some(TypeData::Tuple(inner_id)) if self
                .interner()
                .tuple_list(inner_id)
                .iter()
                .any(|inner| Self::is_generic_ref(self.interner(), inner.type_id)))
        })
    }

    fn is_generic_ref(db: &dyn crate::construction::TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match db.lookup(type_id) {
            // Lazy(DefId) is a reference to a concrete named type (interface, class, type
            // alias). It is always resolvable — evaluate(Lazy(D)) yields the body of D,
            // which is structural and concrete. Only true unknowns (TypeParameter, Infer)
            // and self-recursive placeholders (Recursive) should trigger deferral.
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::Recursive(_)) => true,
            Some(TypeData::IndexAccess(obj, idx)) => {
                Self::is_generic_ref(db, obj) || Self::is_generic_ref(db, idx)
            }
            _ => false,
        }
    }
}
