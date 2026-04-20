use super::*;

impl<'a> NarrowingContext<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Owned(NarrowingCache::new()),
        }
    }

    /// Create a new context with a shared cache.
    pub fn with_cache(db: &'a dyn QueryDatabase, cache: &'a NarrowingCache) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Borrowed(cache),
        }
    }

    /// Set the `TypeResolver` for this context.
    ///
    /// This enables proper resolution of Lazy types (type aliases) during narrowing.
    /// The resolver should be borrowed from the Checker's `TypeEnvironment`.
    pub fn with_resolver(mut self, resolver: &'a dyn TypeResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Resolve a type to its structural representation.
    ///
    /// Unwraps:
    /// - Lazy types (evaluates them using resolver if available, otherwise falls back to db)
    /// - Application types (evaluates the generic instantiation)
    ///
    /// This ensures that type aliases, interfaces, and generics are resolved
    /// to their actual structural types before performing narrowing operations.
    pub(crate) fn resolve_type(&self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.cache.resolve_cache.borrow().get(&type_id) {
            // A self-mapping cache entry for a Lazy type means a previous resolution
            // attempt failed (TypeEnvironment wasn't populated yet). Re-attempt resolution
            // since the environment may have been populated since then.
            if cached != type_id {
                return cached;
            }
            if let Some(TypeData::Lazy(_)) = self.db.lookup(type_id) {
                // Fall through to re-resolve — don't trust stale self-mapping for Lazy
            } else {
                return cached;
            }
        }

        let result = self.resolve_type_uncached(type_id);
        // Only cache if we actually resolved it — don't cache Lazy → Lazy self-mappings
        // since the TypeEnvironment may be populated later with the real mapping.
        let is_unresolved_lazy =
            result == type_id && matches!(self.db.lookup(type_id), Some(TypeData::Lazy(_)));
        if !is_unresolved_lazy {
            self.cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, result);
        }
        result
    }

    fn resolve_type_uncached(&self, mut type_id: TypeId) -> TypeId {
        // Prevent infinite loops with a fuel counter
        let mut fuel = 100;

        while fuel > 0 {
            fuel -= 1;

            // Single lookup per iteration — dispatch based on TypeData variant
            let data = self.db.lookup(type_id);
            match data {
                // 1. Lazy types (DefId-based)
                Some(TypeData::Lazy(def_id)) => {
                    if let Some(resolver) = self.resolver
                        && let Some(resolved) =
                            resolver.resolve_lazy(def_id, self.db.as_type_database())
                    {
                        type_id = resolved;
                        continue;
                    }
                    type_id = self.db.evaluate_type(type_id);
                    continue;
                }

                // 2. Application types (Generics)
                Some(TypeData::Application(app_id)) => {
                    if let Some(resolver) = self.resolver {
                        let app = self.db.type_application(app_id);
                        if let Some(def_id) = lazy_def_id(self.db, app.base) {
                            let resolved_body =
                                resolver.resolve_lazy(def_id, self.db.as_type_database());
                            let type_params = resolver.get_lazy_type_params(def_id);
                            // A placeholder body of `unknown` / `error` indicates
                            // the Checker registered the DefId but never bound a
                            // real body for it (often happens for cross-file
                            // DefId aliases, e.g. a lib type alias like
                            // `NonNullable` referenced from a namespace-imported
                            // signature). Treat such bodies as unresolved so we
                            // fall through to `db.evaluate_type` instead of
                            // substituting `unknown` into the generic body.
                            let is_placeholder =
                                |t: TypeId| t == TypeId::UNKNOWN || t == TypeId::ERROR;
                            if let (Some(body), Some(params)) = (resolved_body, type_params)
                                && !is_placeholder(body)
                            {
                                // Resolve type args so Lazy aliases become their
                                // structural forms (e.g. Union) for distribution.
                                let resolved_args: Vec<TypeId> =
                                    app.args.iter().map(|&arg| self.resolve_type(arg)).collect();
                                type_id = crate::instantiation::instantiate::instantiate_generic(
                                    self.db.as_type_database(),
                                    body,
                                    &params,
                                    &resolved_args,
                                );
                                continue;
                            }
                        }
                    }
                    type_id = self.db.evaluate_type(type_id);
                    continue;
                }

                // 3. TemplateLiteral types
                Some(TypeData::TemplateLiteral(spans_id)) => {
                    use crate::types::TemplateSpan;
                    let spans = self.db.template_list(spans_id);
                    let mut new_spans = Vec::with_capacity(spans.len());
                    let mut changed = false;
                    for span in spans.iter() {
                        match span {
                            TemplateSpan::Type(inner_id) => {
                                let resolved = self.resolve_type(*inner_id);
                                if resolved != *inner_id {
                                    changed = true;
                                }
                                new_spans.push(TemplateSpan::Type(resolved));
                            }
                            other => new_spans.push(other.clone()),
                        }
                    }
                    let eval_input = if changed {
                        self.db.template_literal(new_spans)
                    } else {
                        type_id
                    };
                    let evaluated = self.db.evaluate_type(eval_input);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // 4. KeyOf types
                Some(TypeData::KeyOf(inner)) => {
                    let resolved_inner = self.resolve_type(inner);
                    if resolved_inner != inner {
                        let new_keyof = self.db.keyof(resolved_inner);
                        type_id = self.db.evaluate_type(new_keyof);
                        continue;
                    }
                    break;
                }

                // 5. IndexAccess types
                Some(TypeData::IndexAccess(obj, idx)) => {
                    let resolved_obj = self.resolve_type(obj);
                    let resolved_idx = if let Some(info) = type_param_info(self.db, idx) {
                        info.constraint.map(|c| self.resolve_type(c)).unwrap_or(idx)
                    } else {
                        self.resolve_type(idx)
                    };
                    if resolved_obj != obj || resolved_idx != idx {
                        let evaluated = self.db.evaluate_index_access(resolved_obj, resolved_idx);
                        if !matches!(self.db.lookup(evaluated), Some(TypeData::IndexAccess(_, _))) {
                            type_id = evaluated;
                            continue;
                        }
                    }
                    let evaluated = self.db.evaluate_type(type_id);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // 6. NoInfer — transparent wrapper
                Some(TypeData::NoInfer(inner)) => {
                    type_id = inner;
                    continue;
                }

                // 7. Intersection types with potentially Lazy members
                Some(TypeData::Intersection(members_id)) => {
                    let members = self.db.type_list(members_id);
                    let mut changed = false;
                    let mut resolved_members = Vec::with_capacity(members.len());
                    for &m in members.iter() {
                        let r = self.resolve_type(m);
                        if r != m {
                            changed = true;
                        }
                        resolved_members.push(r);
                    }
                    if changed {
                        type_id = self.db.intersection(resolved_members);
                        continue;
                    }
                    break;
                }

                // 8. Conditional types — resolve inner Lazy/Application types,
                // then re-evaluate to allow distribution/simplification.
                Some(TypeData::Conditional(cond_id)) => {
                    let cond = self.db.get_conditional(cond_id);
                    let resolved_check = self.resolve_type(cond.check_type);
                    let resolved_extends = self.resolve_type(cond.extends_type);
                    if resolved_check != cond.check_type || resolved_extends != cond.extends_type {
                        let new_cond = self.db.conditional(crate::types::ConditionalType {
                            check_type: resolved_check,
                            extends_type: resolved_extends,
                            true_type: cond.true_type,
                            false_type: cond.false_type,
                            is_distributive: cond.is_distributive,
                        });
                        let evaluated = self.db.evaluate_type(new_cond);
                        if evaluated != new_cond && evaluated != type_id {
                            type_id = evaluated;
                            continue;
                        }
                    }
                    // Try evaluating the original conditional
                    let evaluated = self.db.evaluate_type(type_id);
                    if evaluated != type_id {
                        type_id = evaluated;
                        continue;
                    }
                    break;
                }

                // Structural types (Object, Union, Primitive, etc.) — done
                _ => break,
            }
        }

        type_id
    }

    /// Narrow a type based on a typeof check.
    ///
    /// Example: `typeof x === "string"` narrows `string | number` to `string`
    pub fn narrow_by_typeof(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        let _span =
            span!(Level::TRACE, "narrow_by_typeof", source_type = source_type.0, %typeof_result)
                .entered();

        // TypeScript narrows `any` via typeof only for PRIMITIVE type checks.
        // "object" and "function" are non-primitive and do NOT narrow `any`.
        // `unknown` is always narrowed by all typeof checks.
        if source_type == TypeId::ANY {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                // "object" and "function" do NOT narrow `any`
                _ => source_type,
            };
        }
        if source_type == TypeId::UNKNOWN {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                "object" => self.db.union2(TypeId::OBJECT, TypeId::NULL),
                "function" => self.function_type(),
                _ => source_type,
            };
        }

        let target_type = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "object" => TypeId::OBJECT, // includes null
            "function" => return self.narrow_to_function(source_type),
            _ => return source_type,
        };

        self.narrow_to_type(source_type, target_type)
    }

    /// Narrow a type to include only members assignable to target.
    pub fn narrow_to_type(&self, source_type: TypeId, target_type: TypeId) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_to_type",
            source_type = source_type.0,
            target_type = target_type.0
        )
        .entered();

        // CRITICAL FIX: Resolve Lazy/Ref types to inspect their structure.
        // This fixes the "Missing type resolution" bug where type aliases and
        // generics weren't being narrowed correctly.
        let resolved_source = self.resolve_type(source_type);

        // Gracefully handle resolution failures: if evaluation fails but the input
        // wasn't ERROR, we can't narrow structurally. Return original source to
        // avoid cascading ERRORs through the type system.
        if resolved_source == TypeId::ERROR && source_type != TypeId::ERROR {
            trace!("Source type resolution failed, returning original source");
            return source_type;
        }

        // Resolve target for consistency
        let resolved_target = self.resolve_type(target_type);
        if resolved_target == TypeId::ERROR && target_type != TypeId::ERROR {
            trace!("Target type resolution failed, returning original source");
            return source_type;
        }

        // If source is the target, return it
        if resolved_source == resolved_target {
            trace!("Source type equals target type, returning unchanged");
            return source_type;
        }

        // Special case: unknown can be narrowed to any type through type guards
        // This handles cases like: if (typeof x === "string") where x: unknown
        if resolved_source == TypeId::UNKNOWN {
            trace!("Narrowing unknown to specific type via type guard");
            return target_type;
        }

        // Special case: any can be narrowed to any type through type guards
        // This handles cases like: if (x === null) where x: any
        // CRITICAL: Unlike unknown, any MUST be narrowed to match target type
        if resolved_source == TypeId::ANY {
            trace!("Narrowing any to specific type via type guard");
            return target_type;
        }

        // If source is a union, filter members
        // Use resolved_source for structural inspection
        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            trace!(
                "Narrowing union with {} members to type {}",
                members.len(),
                target_type.0
            );
            let matching: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param(member, target_type) {
                        return Some(narrowed);
                    }
                    // Resolve Application/Lazy types before assignability check.
                    // Without this, generic instantiations like ArrayLike<any>
                    // remain opaque Application types and structural assignability
                    // to object targets (e.g. { length: unknown }) fails.
                    let resolved_member = self.resolve_type(member);
                    if self.is_assignable_to(resolved_member, target_type) {
                        return Some(member);
                    }
                    // Reverse subtype check: target <: member.
                    // Handles narrowing \`string | number\` by \`"hello"\` where
                    // \`"hello" <: string\` so the member should be kept.
                    // Guard: only use the bare is_subtype_of_with_db (which lacks a
                    // TypeResolver) for primitive/literal types. For interface/class
                    // Lazy(DefId) types, the global subtype cache can contain stale
                    // results that cause false positives.
                    if (self.is_js_primitive(target_type) || self.is_js_primitive(member))
                        && crate::relations::subtype::is_subtype_of_with_db(
                            self.db,
                            target_type,
                            member,
                        )
                    {
                        return Some(target_type);
                    }
                    // CRITICAL FIX: instanceof Array matching
                    // When narrowing by `instanceof Array`, if the member is array-like and target
                    // is a Lazy/Application type (which includes Array<T> interface references),
                    // assume it's the global Array and match the member.
                    // This handles: `x: Message | Message[]` with `instanceof Array` should keep `Message[]`.
                    // At runtime, instanceof only checks prototype chain, not generic type arguments.
                    if self.is_array_like(member) {
                        use crate::type_queries;
                        // Check if target is a type reference or generic application (Array<T>)
                        let is_target_lazy_or_app = type_queries::is_type_reference(self.db, resolved_target)
                            || type_queries::is_generic_type(self.db, resolved_target);

                        trace!("Member is array-like: member={}, target={}, is_target_lazy_or_app={}",
                            member.0, resolved_target.0, is_target_lazy_or_app);

                        if is_target_lazy_or_app {
                            trace!("Array member with lazy/app target (likely Array interface), keeping member");
                            return Some(member);
                        }
                    }
                    None
                })
                .collect();

            if matching.is_empty() {
                trace!("No matching members found, returning NEVER");
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                trace!("Found single matching member, returning {}", matching[0].0);
                return matching[0];
            }
            trace!(
                "Found {} matching members, creating new union",
                matching.len()
            );
            return self.db.union(matching);
        }

        // Check if this is a type parameter that needs narrowing
        // Use resolved_source to handle type parameters behind aliases
        if let Some(narrowed) = self.narrow_type_param(resolved_source, target_type) {
            trace!("Narrowed type parameter to {}", narrowed.0);
            return narrowed;
        }

        // Task 13: Handle boolean -> literal narrowing
        // When narrowing boolean to true or false, return the corresponding literal
        if resolved_source == TypeId::BOOLEAN {
            let is_target_true = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                resolved_target == TypeId::BOOLEAN_TRUE
            };

            if is_target_true {
                trace!("Narrowing boolean to true");
                return TypeId::BOOLEAN_TRUE;
            }

            let is_target_false = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                resolved_target == TypeId::BOOLEAN_FALSE
            };

            if is_target_false {
                trace!("Narrowing boolean to false");
                return TypeId::BOOLEAN_FALSE;
            }
        }

        // Check if source is assignable to target using resolved types for comparison
        if self.is_assignable_to(resolved_source, resolved_target) {
            trace!("Source type is assignable to target, returning source");
            source_type
        } else if crate::relations::subtype::is_subtype_of_with_db(
            self.db,
            resolved_target,
            resolved_source,
        ) {
            // CRITICAL FIX: Check if target is a subtype of source (reverse narrowing)
            // This handles cases like narrowing string to "hello" where "hello" is a subtype of string
            // The inference engine uses this to narrow upper bounds by lower bounds
            trace!("Target is subtype of source, returning target");
            target_type
        } else {
            trace!("Source type is not assignable to target, returning NEVER");
            TypeId::NEVER
        }
    }

    /// Check if a literal type is assignable to a target for narrowing purposes.
    ///
    /// Handles union decomposition: if the target is a union, checks each member.
    /// Falls back to `narrow_to_type` to determine if the literal can narrow to the target.
    pub fn literal_assignable_to(&self, literal: TypeId, target: TypeId) -> bool {
        if literal == target || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if let UnionMembersKind::Union(members) = classify_for_union_members(self.db, target) {
            return members
                .iter()
                .any(|&member| self.literal_assignable_to(literal, member));
        }

        self.narrow_to_type(literal, target) != TypeId::NEVER
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        // `any` cannot be narrowed by exclusion — it remains `any` in all branches.
        // Without this guard, the `is_assignable_to(any, X)` check below would always
        // succeed (any is assignable to everything), incorrectly producing `never`.
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        // Note: Do NOT resolve Lazy/Application types here. This function is called
        // recursively from narrow_type_param_excluding, which relies on TypeId identity
        // comparisons (narrowed_constraint == constraint). Resolving Lazy types changes
        // the TypeId, breaking those comparisons and producing incorrect intersections
        // (e.g., T & Date instead of excluding T from T | number).
        //
        // Lazy type resolution for the top-level source is handled in narrow_type()
        // before dispatching to this function.

        if let Some(members) = intersection_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let mut narrowed_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for &member in members.iter() {
                let narrowed = self.narrow_excluding_type(member, excluded_type);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                if narrowed != member {
                    changed = true;
                }
                narrowed_members.push(narrowed);
            }
            if !changed {
                return source_type;
            }
            return self.db.intersection(narrowed_members);
        }

        // If source is a union, filter out matching members
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if intersection_list_id(self.db, member).is_some() {
                        return self
                            .narrow_excluding_type(member, excluded_type)
                            .non_never();
                    }
                    if let Some(narrowed) = self.narrow_type_param_excluding(member, excluded_type)
                    {
                        return narrowed.non_never();
                    }
                    if self.is_assignable_to(member, excluded_type) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            tracing::trace!(
                remaining_count = remaining.len(),
                remaining = ?remaining.iter().map(|t| t.0).collect::<Vec<_>>(),
                "narrow_excluding_type: union filter result"
            );
            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        if let Some(narrowed) = self.narrow_type_param_excluding(source_type, excluded_type) {
            return narrowed;
        }

        // Special case: boolean type (treat as true | false union)
        // Task 13: Fix Boolean Narrowing Logic
        // When excluding true or false from boolean, return the other literal
        // When excluding both true and false from boolean, return never
        if source_type == TypeId::BOOLEAN
            || source_type == TypeId::BOOLEAN_TRUE
            || source_type == TypeId::BOOLEAN_FALSE
        {
            // Check if excluded_type is a boolean literal
            let is_excluding_true = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                excluded_type == TypeId::BOOLEAN_TRUE
            };

            let is_excluding_false = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                excluded_type == TypeId::BOOLEAN_FALSE
            };

            // Handle exclusion from boolean, true, or false
            if source_type == TypeId::BOOLEAN {
                if is_excluding_true {
                    // Excluding true from boolean -> return false
                    return TypeId::BOOLEAN_FALSE;
                } else if is_excluding_false {
                    // Excluding false from boolean -> return true
                    return TypeId::BOOLEAN_TRUE;
                }
                // If excluding BOOLEAN, let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_TRUE {
                if is_excluding_true {
                    // Excluding true from true -> return never
                    return TypeId::NEVER;
                }
                // For other cases (e.g., excluding BOOLEAN from TRUE),
                // let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_FALSE && is_excluding_false {
                // Excluding false from false -> return never
                return TypeId::NEVER;
            }
            // For other cases, let the final is_assignable_to check handle it below
            // CRITICAL: Do NOT return source_type here.
            // Fall through to the standard is_assignable_to check below.
            // This handles edge cases like narrow_excluding_type(TRUE, BOOLEAN) -> NEVER
        }

        // If source is assignable to excluded, return never
        if self.is_assignable_to(source_type, excluded_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Narrow a type by excluding multiple types at once (batched version).
    ///
    /// This is an optimized version of `narrow_excluding_type` for cases like
    /// switch default clauses where we need to exclude many types at once.
    /// It avoids creating intermediate union types and reduces complexity from O(N²) to O(N).
    ///
    /// # Arguments
    /// * `source_type` - The type to narrow (typically a union)
    /// * `excluded_types` - Types to exclude from the source
    ///
    /// # Returns
    /// The narrowed type with all excluded types removed
    pub fn narrow_excluding_types(&self, source_type: TypeId, excluded_types: &[TypeId]) -> TypeId {
        if excluded_types.is_empty() {
            return source_type;
        }

        // For small lists, use sequential narrowing (avoids HashSet overhead)
        if excluded_types.len() <= 4 {
            let mut result = source_type;
            for &excluded in excluded_types {
                result = self.narrow_excluding_type(result, excluded);
                if result == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }
            return result;
        }

        // For larger lists, use HashSet for O(1) lookup
        let excluded_set: rustc_hash::FxHashSet<TypeId> = excluded_types.iter().copied().collect();

        // Handle union source type
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    // Fast path: direct identity check against the set
                    if excluded_set.contains(&member) {
                        return None;
                    }

                    // Handle intersection members
                    if intersection_list_id(self.db, member).is_some() {
                        return self
                            .narrow_excluding_types(member, excluded_types)
                            .non_never();
                    }

                    // Handle type parameters
                    if let Some(narrowed) =
                        self.narrow_type_param_excluding_set(member, &excluded_set)
                    {
                        return narrowed.non_never();
                    }

                    // Slow path: check assignability for complex cases
                    // This handles cases where the member isn't identical to an excluded type
                    // but might still be assignable to one (e.g., literal subtypes)
                    for &excluded in &excluded_set {
                        if self.is_assignable_to(member, excluded) {
                            return None;
                        }
                    }
                    Some(member)
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        // Handle single type (not a union)
        if excluded_set.contains(&source_type) {
            return TypeId::NEVER;
        }

        // Check assignability for single type
        for &excluded in &excluded_set {
            if self.is_assignable_to(source_type, excluded) {
                return TypeId::NEVER;
            }
        }

        source_type
    }

    /// Helper for `narrow_excluding_types` with type parameters
    fn narrow_type_param_excluding_set(
        &self,
        source: TypeId,
        excluded_set: &rustc_hash::FxHashSet<TypeId>,
    ) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint?;
        if constraint == source || constraint == TypeId::UNKNOWN {
            return None;
        }

        // Narrow the constraint by excluding all types in the set
        let excluded_vec: Vec<TypeId> = excluded_set.iter().copied().collect();
        let narrowed_constraint = self.narrow_excluding_types(constraint, &excluded_vec);

        if narrowed_constraint == constraint {
            return None;
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    /// Narrow to function types only.
    fn narrow_to_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let functions: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param_to_function(member) {
                        return narrowed.non_never();
                    }
                    self.is_function_type(member).then_some(member)
                })
                .collect();

            return union_or_single(self.db, functions);
        }

        if let Some(narrowed) = self.narrow_type_param_to_function(source_type) {
            return narrowed;
        }

        if self.is_function_type(source_type) {
            source_type
        } else if source_type == TypeId::OBJECT {
            self.function_type()
        } else if let Some(shape_id) = object_shape_id(self.db, source_type) {
            let shape = self.db.object_shape(shape_id);
            if shape.properties.is_empty() {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else if let Some(shape_id) = object_with_index_shape_id(self.db, source_type) {
            let shape = self.db.object_shape(shape_id);
            if shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
            {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else if index_access_parts(self.db, source_type).is_some() {
            // For indexed access types like T[K], narrow to T[K] & Function
            // This handles cases like: typeof obj[key] === 'function'
            let function_type = self.function_type();
            self.db.intersection2(source_type, function_type)
        } else {
            TypeId::NEVER
        }
    }

    /// Check if a type is a function type.
    /// Uses the visitor pattern from `solver::visitor`.
    fn is_function_type(&self, type_id: TypeId) -> bool {
        is_function_type_through_type_constraints(self.db, type_id)
    }

    /// Narrow a type to exclude function-like members (typeof !== "function").
    pub fn narrow_excluding_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param_excluding_function(member) {
                        return narrowed.non_never();
                    }
                    if self.is_function_type(member) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            return union_or_single(self.db, remaining);
        }

        if let Some(narrowed) = self.narrow_type_param_excluding_function(source_type) {
            return narrowed;
        }

        if self.is_function_type(source_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Check if a type has typeof "object".
    /// Uses the visitor pattern from `solver::visitor`.
    fn is_object_typeof(&self, type_id: TypeId) -> bool {
        is_object_like_type_through_type_constraints(self.db, type_id)
    }

    /// Check if a type represents the global Object interface from lib.d.ts.
    ///
    /// All non-primitive values are instances of Object at runtime. Used by
    /// instanceof false branch narrowing to exclude all non-primitive types
    /// when the constructor is `Object`.
    pub(in crate::narrowing) fn is_object_interface(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::OBJECT {
            return true;
        }
        // Check the query database directly for boxed Object interface.
        // Boxed types are registered on the interner during lib.d.ts processing,
        // bypassing TypeResolver (which may be a different instance).
        // Use as_type_database() to disambiguate from TypeResolver's get_boxed_type.
        let db = self.db.as_type_database();
        if db.get_boxed_type(crate::types::IntrinsicKind::Object) == Some(type_id) {
            return true;
        }
        if let Some(def_id) = lazy_def_id(self.db, type_id)
            && db.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Object)
        {
            return true;
        }
        false
    }

    pub(in crate::narrowing) fn narrow_type_param(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source {
            return None;
        }

        let narrowed_constraint = if constraint == TypeId::UNKNOWN {
            target
        } else {
            self.narrow_to_type(constraint, target)
        };

        if narrowed_constraint == TypeId::NEVER {
            return None;
        }

        // When the target is a conditional type utility Application that uses
        // the source type parameter as its check type (e.g., Extract<T, Function>),
        // return the target directly instead of T & target. Distributive conditional
        // types like Extract<T, U> = T extends U ? T : never are always subtypes
        // of their check parameter T, so the intersection is redundant. More
        // importantly, keeping the intersection prevents proper evaluation of the
        // conditional type during instantiation and call resolution, which can
        // cause false TS2348/TS2349 errors when the narrowed type is later called.
        if self.is_conditional_utility_of_source(source, target) {
            return Some(target);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    /// Check if `target` is a conditional type utility Application (like Extract, Exclude,
    /// `NonNullable`) whose first type argument is `source`.
    ///
    /// Distributive conditional type aliases like `Extract<T, U> = T extends U ? T : never`
    /// always produce a subtype of their check parameter T. When narrowing T by a type
    /// predicate `x is Extract<T, Function>`, we return the target directly instead of
    /// creating an intersection `T & Extract<T, Function>`.
    fn is_conditional_utility_of_source(&self, source: TypeId, target: TypeId) -> bool {
        use crate::types::TypeData;

        // Check if target is Application(Base, [source, ...])
        let Some(TypeData::Application(app_id)) = self.db.lookup(target) else {
            return false;
        };
        let app = self.db.type_application(app_id);

        // First arg must be the source type parameter
        if app.args.is_empty() || app.args[0] != source {
            return false;
        }

        // Source must be a type parameter (this pattern only applies to generic narrowing)
        if !matches!(self.db.lookup(source), Some(TypeData::TypeParameter(_))) {
            return false;
        }

        // Check if the base resolves to a conditional type (distributive).
        let base_body = if let Some(resolver) = self.resolver {
            resolver.resolve_lazy(
                match self.db.lookup(app.base) {
                    Some(TypeData::Lazy(def_id)) => def_id,
                    _ => return false,
                },
                self.db,
            )
        } else {
            let eval = self.db.evaluate_type(app.base);
            if eval != app.base { Some(eval) } else { None }
        };

        if let Some(body) = base_body {
            matches!(self.db.lookup(body), Some(TypeData::Conditional(_)))
        } else {
            // Can't resolve the base — heuristic: if the Application has exactly
            // 2 type args and the base is Lazy, it's likely a utility type like
            // Extract<T, U> or Exclude<T, U>.
            matches!(self.db.lookup(app.base), Some(TypeData::Lazy(_))) && app.args.len() == 2
        }
    }

    fn narrow_type_param_to_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            let function_type = self.function_type();
            return Some(self.db.intersection2(source, function_type));
        }

        let narrowed_constraint = self.narrow_to_function(constraint);
        if narrowed_constraint == TypeId::NEVER {
            return None;
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding(&self, source: TypeId, excluded: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint?;
        if constraint == source || constraint == TypeId::UNKNOWN {
            return None;
        }

        let narrowed_constraint = self.narrow_excluding_type(constraint, excluded);
        if narrowed_constraint == constraint {
            return None;
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            return Some(source);
        }

        let narrowed_constraint = self.narrow_excluding_function(constraint);
        if narrowed_constraint == constraint {
            return Some(source);
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    pub(crate) fn narrow_type_param_excluding_typeof_object(
        &self,
        source: TypeId,
    ) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            return Some(source);
        }

        let narrowed_constraint = self.narrow_excluding_typeof_object(constraint);
        if narrowed_constraint == constraint {
            return Some(source);
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    pub(crate) fn function_type(&self) -> TypeId {
        let rest_array = self.db.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.db.function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type: TypeId::ANY,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    /// Check if a type is a JS primitive that can never pass `instanceof`.
    /// Includes string, number, boolean, bigint, symbol, undefined, null,
    /// void, never, and their literal forms.
    pub(in crate::narrowing) fn is_js_primitive(&self, type_id: TypeId) -> bool {
        matches!(
            type_id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
                | TypeId::NEVER
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) || matches!(self.db.lookup(type_id), Some(TypeData::Literal(_)))
    }

    /// Check if `target` is a distributive conditional type whose result is
    /// always a subtype of `source`.
    ///
    /// This handles the common `Extract<T, U>` pattern where the conditional is
    /// `T extends U ? T : never`. Since the true branch equals the check type
    /// and the false branch is `never`, the result is always `<: T`. When the
    /// check type matches `source`, this means `target <: source`, so the
    /// narrowed type should be `target` directly (not `source & target`).
    ///
    /// This matches tsc's `narrowType` behavior: `isTypeSubtypeOf(candidate, type)`
    /// returns true for `Extract<T, U>` when narrowing type parameter `T`.
    fn is_conditional_subtype_of_source(&self, target: TypeId, source: TypeId) -> bool {
        // Direct match: target is Conditional(check_type, extends, true_type, false_type)
        if let Some(TypeData::Conditional(cond_id)) = self.db.lookup(target) {
            let cond = self.db.get_conditional(cond_id);
            // Pattern: check_type == source, true_type == check_type, false_type == never
            // This is the Extract<T, U> = (T extends U ? T : never) pattern.
            if cond.check_type == source
                && cond.true_type == cond.check_type
                && cond.false_type == TypeId::NEVER
            {
                return true;
            }
            // More general: true_type <: source AND false_type <: source
            // (both branches produce subtypes of source). This covers patterns
            // where the true branch is a narrower type that's still <: source.
            if (cond.true_type == source || cond.true_type == TypeId::NEVER)
                && (cond.false_type == source || cond.false_type == TypeId::NEVER)
            {
                return true;
            }
        }
        // Application wrapping a conditional (e.g., Extract<T, U> as a type alias)
        // Resolve through evaluation and check again.
        let resolved = self.resolve_type(target);
        if resolved != target
            && let Some(TypeData::Conditional(cond_id)) = self.db.lookup(resolved)
        {
            let cond = self.db.get_conditional(cond_id);
            if cond.check_type == source
                && cond.true_type == cond.check_type
                && cond.false_type == TypeId::NEVER
            {
                return true;
            }
        }
        false
    }

    /// Simple assignability check for narrowing purposes.
    pub(in crate::narrowing) fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }

        // never is assignable to everything
        if source == TypeId::NEVER {
            return true;
        }

        // everything is assignable to any/unknown
        if target.is_any_or_unknown() {
            return true;
        }

        // Literal to base type
        if let Some(lit) = literal_value(self.db, source) {
            match (lit, target) {
                (LiteralValue::String(_), t) if t == TypeId::STRING => return true,
                (LiteralValue::Number(_), t) if t == TypeId::NUMBER => return true,
                (LiteralValue::Boolean(_), t) if t == TypeId::BOOLEAN => return true,
                (LiteralValue::BigInt(_), t) if t == TypeId::BIGINT => return true,
                _ => {}
            }
        }

        // object/null for typeof "object"
        if target == TypeId::OBJECT {
            if source == TypeId::NULL {
                return true;
            }
            // Resolve Lazy/Application types (e.g., Record<string, any>) before
            // checking object-likeness. Without this, unevaluated type aliases
            // and generic applications are not recognized as object types and
            // get incorrectly filtered out during typeof "object" narrowing.
            let resolved = self.resolve_type(source);
            if self.is_object_typeof(resolved) {
                return true;
            }
            // If resolve_type couldn't fully evaluate an Application type
            // (e.g., Record<string, any> before its definition is registered),
            // conservatively assume it's an object type. Generic instantiations
            // like Record<K,V>, Map<K,V>, etc. are always object types at runtime.
            // Filtering them out would incorrectly narrow to `never`.
            if matches!(self.db.lookup(resolved), Some(TypeData::Application(_))) {
                return true;
            }
            return false;
        }

        if let Some(members) = intersection_list_id(self.db, source) {
            let members = self.db.type_list(members);
            if members
                .iter()
                .any(|member| self.is_assignable_to(*member, target))
            {
                return true;
            }
        }

        if target == TypeId::STRING && template_literal_id(self.db, source).is_some() {
            return true;
        }

        // Check if source is assignable to any member of a union target
        if let Some(members) = union_list_id(self.db, target) {
            let members = self.db.type_list(members);
            if members
                .iter()
                .any(|&member| self.is_assignable_to(source, member))
            {
                return true;
            }
        }

        // Fallback: use full structural/nominal subtype check.
        // This handles class inheritance (Derived extends Base), interface
        // implementations, and other structural relationships that the
        // fast-path checks above don't cover.
        // CRITICAL: Resolve Lazy(DefId) types before the subtype check.
        // Without resolution, two unrelated interfaces (e.g., Cat and Dog)
        // remain as opaque Lazy types and the SubtypeChecker can't distinguish them.
        let resolved_source = self.resolve_type(source);
        let resolved_target = self.resolve_type(target);
        if resolved_source == resolved_target {
            return true;
        }
        if crate::relations::subtype::is_subtype_of_with_db(
            self.db,
            resolved_source,
            resolved_target,
        ) {
            return true;
        }

        // Structural fallback: when the SubtypeChecker can't determine the
        // relationship (e.g., due to evaluation/caching limitations), do a
        // direct property-level check. This handles cases like
        // `ArrayLike<any>` (ObjectWithIndex) being assignable to
        // `{ length: unknown }` (Object) during type predicate narrowing.
        self.is_structurally_assignable_to_object(resolved_source, resolved_target)
    }

    /// Direct structural check: does `source` have all properties required
    /// by `target` (when target is a plain Object type)?
    fn is_structurally_assignable_to_object(&self, source: TypeId, target: TypeId) -> bool {
        use crate::visitor::{object_shape_id, object_with_index_shape_id};

        // Target must be a plain Object type (not ObjectWithIndex)
        let t_shape_id = match object_shape_id(self.db.as_type_database(), target) {
            Some(id) => id,
            None => return false,
        };
        let t_shape = self.db.object_shape(t_shape_id);
        if t_shape.properties.is_empty() {
            return false; // Empty object, skip
        }

        // Source can be Object or ObjectWithIndex
        let s_shape_id = object_shape_id(self.db.as_type_database(), source)
            .or_else(|| object_with_index_shape_id(self.db.as_type_database(), source));
        let s_shape_id = match s_shape_id {
            Some(id) => id,
            None => return false,
        };
        let s_shape = self.db.object_shape(s_shape_id);

        // Check that every target property exists on the source with
        // compatible type and optionality.
        for t_prop in &t_shape.properties {
            let found = s_shape.properties.iter().any(|sp| {
                sp.name == t_prop.name
                    // Optional source can't satisfy required target
                    && (!sp.optional || t_prop.optional)
                    && crate::relations::subtype::is_subtype_of_with_db(
                        self.db,
                        sp.type_id,
                        t_prop.type_id,
                    )
            });
            if !found {
                return false;
            }
        }
        true
    }

    /// Applies a type guard to narrow a type.
    ///
    /// This is the main entry point for AST-agnostic type narrowing.
    /// The Checker extracts a `TypeGuard` from AST nodes, and the Solver
    /// applies it to compute the narrowed type.
    ///
    /// # Arguments
    /// * `source_type` - The type to narrow
    /// * `guard` - The guard condition (extracted from AST by Checker)
    /// * `sense` - If true, narrow for the "true" branch; if false, narrow for the "false" branch
    ///
    /// # Returns
    /// The narrowed type after applying the guard.
    ///
    /// # Examples
    /// ```text
    /// // typeof x === "string"
    /// let guard = TypeGuard::Typeof(TypeofKind::String);
    /// let narrowed = narrowing.narrow_type(string_or_number, &guard, GuardSense::Positive);
    /// assert_eq!(narrowed, TypeId::STRING);
    ///
    /// // x !== null (negated sense)
    /// let guard = TypeGuard::NullishEquality;
    /// let narrowed = narrowing.narrow_type(string_or_null, &guard, GuardSense::Negative);
    /// // Result should exclude null and undefined
    /// ```
    /// Resolve Lazy/Application types to their concrete form for use in exclusion
    /// narrowing paths. This is needed because `narrow_excluding_type` cannot see
    /// through Lazy(DefId) to access union/intersection members.
    ///
    /// Must NOT be called globally at the `narrow_type` entry point because
    /// Instanceof guards with type parameters would break.
    fn resolve_for_exclusion_narrowing(&self, source_type: TypeId) -> TypeId {
        let resolved = if matches!(
            self.db.lookup(source_type),
            Some(TypeData::Lazy(_) | TypeData::Application(_))
        ) {
            let r = self.resolve_type(source_type);
            if r == TypeId::ERROR && source_type != TypeId::ERROR {
                source_type
            } else {
                r
            }
        } else {
            source_type
        };

        // Conditional types (e.g. from type predicates using Extract<T, U> or
        // similar mapped/conditional patterns) must be evaluated to their
        // concrete result before exclusion narrowing can match them against
        // union members. Without this, the exclusion sees an opaque
        // Conditional and returns the source unchanged, preventing narrowing
        // in the false branch of type predicate guards.
        if let Some(TypeData::Conditional(_)) = self.db.lookup(resolved) {
            // Try resolving through the resolve_type pipeline which now
            // handles Conditional types by resolving inner Lazy types
            // and re-evaluating.
            let further_resolved = self.resolve_type(resolved);
            if further_resolved != resolved && further_resolved != TypeId::ERROR {
                return further_resolved;
            }
        }

        resolved
    }

    pub fn narrow_type(&self, source_type: TypeId, guard: &TypeGuard, sense: GuardSense) -> TypeId {
        let sense = matches!(sense, GuardSense::Positive);

        // For generic IndexAccess types (e.g., `Entries[EntryId]` where EntryId is a
        // type parameter), we must preserve the original deferred form after narrowing.
        // Without this, eagerly resolving to the constraint breaks assignability with
        // the original return type (e.g., false TS2322 in quickinfoTypeAtReturn...).
        let original_generic_index =
            if let Some(TypeData::IndexAccess(obj, idx)) = self.db.lookup(source_type) {
                let is_generic = crate::type_queries::contains_type_parameters_db(self.db, obj)
                    || crate::type_queries::contains_type_parameters_db(self.db, idx);
                if is_generic { Some(source_type) } else { None }
            } else {
                None
            };

        // Resolve IndexAccess types (e.g., `A[K]`) to their concrete form before
        // narrowing, so that opaque generic index access types can be decomposed
        // for guard-based narrowing (e.g., excluding null from `number | null`).
        let resolved_source = if matches!(
            self.db.lookup(source_type),
            Some(TypeData::IndexAccess(_, _))
        ) {
            self.resolve_type(source_type)
        } else {
            source_type
        };

        let narrowed = self.narrow_type_inner(resolved_source, guard, sense);

        // For generic IndexAccess, wrap the result to preserve assignability.
        if let Some(original) = original_generic_index {
            if narrowed == resolved_source || narrowed == original {
                return original;
            }
            if narrowed != TypeId::NEVER {
                return self.db.intersection2(original, narrowed);
            }
        }

        narrowed
    }

    fn narrow_type_inner(&self, source_type: TypeId, guard: &TypeGuard, sense: bool) -> TypeId {
        match guard {
            TypeGuard::Typeof(typeof_kind) => {
                let type_name = typeof_kind.as_str();
                if sense {
                    self.narrow_by_typeof(source_type, type_name)
                } else {
                    // TypeScript does NOT narrow `any` in the false branch of typeof.
                    // The true branch narrows `any` to the primitive type, but the
                    // false branch keeps `any` unchanged.
                    if source_type == TypeId::ANY {
                        return source_type;
                    }
                    // Negation: exclude typeof type — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    self.narrow_by_typeof_negation(resolved, type_name)
                }
            }

            TypeGuard::Instanceof(instance_type, _is_explicit_global) => {
                // TypeScript narrows `any` via instanceof for specific constructors
                // (e.g. Error, Date) but NOT for Function or Object. Handle this
                // in the sense-specific branches below.
                if source_type == TypeId::ANY && !sense {
                    // False branch: `any` stays `any` (can't exclude from `any`)
                    return source_type;
                }

                if sense {
                    // Positive branch: `any` narrows to instance type unless
                    // the instance type is Function or Object.
                    if source_type == TypeId::ANY {
                        // Resolve Lazy types before checking Function/Object
                        let resolved_instance = self.resolve_type(*instance_type);
                        if self.is_object_interface(resolved_instance)
                            || crate::type_queries::is_function_interface_structural(
                                self.db,
                                resolved_instance,
                            )
                        {
                            return TypeId::ANY;
                        }
                        return *instance_type;
                    }
                    // Positive: x instanceof Class
                    // Special case: `unknown` instanceof X narrows to X (or object if X unknown)
                    // This must be handled here in the solver, not in the checker.
                    if source_type == TypeId::UNKNOWN {
                        return *instance_type;
                    }

                    // When an empty object `{}` (e.g., from truthiness-narrowed `unknown`) is
                    // narrowed by `instanceof Object`, we return `TypeId::OBJECT` (the intrinsic
                    // non-primitive type) instead of the Object interface. This ensures that
                    // the result is not considered an "empty object" for TS2638 purposes.
                    //
                    // TSC emits TS2638 "may represent a primitive value" for truthiness-narrowed
                    // `unknown` used with `in`, but NOT after `instanceof Object` because the
                    // instanceof check confirms the value is a non-primitive object.
                    let resolved_instance = self.resolve_type(*instance_type);
                    if crate::type_queries::is_empty_object_type(self.db, source_type)
                        && self.is_object_interface(resolved_instance)
                    {
                        return TypeId::OBJECT;
                    }

                    // CRITICAL: The payload is already the Instance Type (extracted by Checker)
                    // Use narrow_by_instance_type for instanceof-specific semantics:
                    // type parameters with matching constraints are kept, but anonymous
                    // object types that happen to be structurally compatible are excluded.
                    // Primitive types are filtered out since they can never pass instanceof.
                    let narrowed = self.narrow_by_instance_type(source_type, *instance_type);

                    if narrowed != TypeId::NEVER || source_type == TypeId::NEVER {
                        return narrowed;
                    }

                    // Fallback 1: If standard narrowing returns NEVER but source wasn't NEVER,
                    // it might be an interface vs class check (which is allowed in TS).
                    // Only create intersection if the types don't have conflicting properties.
                    if self.are_instanceof_types_overlapping(source_type, *instance_type) {
                        let intersection = self.db.intersection2(source_type, *instance_type);
                        if intersection != TypeId::NEVER {
                            return intersection;
                        }
                    } else {
                        // Types have conflicting properties — intersection is uninhabitable.
                        return TypeId::NEVER;
                    }

                    // Fallback 2: If even intersection construction fails,
                    // narrow to object-like types. On the true branch of instanceof,
                    // we know the value must be some kind of object.
                    self.narrow_to_objectish(source_type)
                } else {
                    // Negative: !(x instanceof Class)
                    // Keep primitives (they can never pass instanceof) and exclude
                    // non-primitive types assignable to the instance type.
                    // For `instanceof Object`, this correctly excludes all non-primitives
                    // since every non-primitive is an Object instance at runtime.
                    self.narrow_by_instanceof_false(source_type, *instance_type)
                }
            }

            TypeGuard::LiteralEquality(literal_type) => {
                if sense {
                    // Equality: narrow to the literal type
                    self.narrow_to_type(source_type, *literal_type)
                } else {
                    // Inequality: exclude the literal type — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    self.narrow_excluding_type(resolved, *literal_type)
                }
            }

            TypeGuard::NullishEquality => {
                if sense {
                    // Equality with null: narrow to null | undefined
                    self.db.union2(TypeId::NULL, TypeId::UNDEFINED)
                } else {
                    // Inequality: exclude null and undefined — resolve Lazy types first
                    let resolved = self.resolve_for_exclusion_narrowing(source_type);
                    let without_null = self.narrow_excluding_type(resolved, TypeId::NULL);
                    self.narrow_excluding_type(without_null, TypeId::UNDEFINED)
                }
            }

            TypeGuard::Truthy => {
                if sense {
                    // Truthy: remove null and undefined (TypeScript doesn't narrow other falsy values)
                    self.narrow_by_truthiness(source_type)
                } else {
                    // Falsy: narrow to the falsy component(s)
                    // This handles cases like: if (!x) where x: string → "" in false branch
                    self.narrow_to_falsy(source_type)
                }
            }

            TypeGuard::Discriminant {
                property_path,
                value_type,
            } => {
                // Use narrow_by_discriminant_for_type which handles type parameters
                // by narrowing the constraint and returning T & NarrowedConstraint
                self.narrow_by_discriminant_for_type(source_type, property_path, *value_type, sense)
            }

            TypeGuard::InProperty(property_name) => {
                if sense {
                    // Positive: "prop" in x - narrow to types that have the property
                    self.narrow_by_property_presence(source_type, *property_name, true)
                } else {
                    // Negative: !("prop" in x) - narrow to types that don't have the property
                    self.narrow_by_property_presence(source_type, *property_name, false)
                }
            }

            TypeGuard::Predicate { type_id, asserts } => {
                match type_id {
                    Some(target_type) => {
                        // Type guard with specific type: is T or asserts T
                        if sense {
                            // True branch: narrow source to the predicate type.
                            // Following TSC's narrowType logic:
                            // 1. For unions: filter members using narrow_to_type
                            // 2. For non-unions:
                            //    a. source <: target → return source
                            //    b. target <: source → return target
                            //    c. otherwise → return source & target
                            //
                            // Following TSC's narrowType logic which uses
                            // isTypeSubtypeOf (not isTypeAssignableTo) to decide
                            // whether source is already specific enough.
                            //
                            // If source is a strict subtype of the target, return
                            // source (it's already more specific). If target is a
                            // strict subtype of source, return target (narrowing
                            // down). Otherwise, return the intersection.
                            //
                            // narrow_to_type uses assignability internally, which is
                            // too loose for type predicates (e.g. {} is assignable to
                            // Record<string,unknown> but not a subtype).
                            let resolved_source = self.resolve_type(source_type);

                            if resolved_source == self.resolve_type(*target_type) {
                                source_type
                            } else if resolved_source == TypeId::UNKNOWN
                                || resolved_source == TypeId::ANY
                            {
                                *target_type
                            } else if union_list_id(self.db, resolved_source).is_some() {
                                // For unions: filter members, fall back to
                                // intersection if nothing matches.
                                let narrowed = self.narrow_to_type(source_type, *target_type);
                                if narrowed == TypeId::NEVER && source_type != TypeId::NEVER {
                                    self.db.intersection2(source_type, *target_type)
                                } else {
                                    narrowed
                                }
                            } else if crate::type_param_info(self.db, resolved_source).is_some()
                                && crate::visitors::visitor_predicates::contains_type_parameters(
                                    self.db,
                                    *target_type,
                                )
                            {
                                // When the source is a bare type parameter (T)
                                // AND the predicate type itself references type
                                // parameters (e.g., `Extract<T, Function>` =
                                // `T extends Function ? T : never`), the
                                // predicate type is already a refinement of T.
                                // Creating `T & Extract<T, U>` is redundant and
                                // prevents the solver from recognising the
                                // result as callable after instantiation (fixes
                                // TS2348 false positive in conditionalTypes2).
                                //
                                // When the predicate is a concrete type like
                                // `Pet` (no type params), we MUST keep the
                                // intersection `TPet & Pet` to preserve the type
                                // parameter identity (narrowingConstrainedTypeParameter).
                                *target_type
                            } else {
                                // Non-union source: following tsc's narrowType logic:
                                //   1. target <: source → return target (narrowing down)
                                //   2. source <: target → return source (already specific)
                                //   3. otherwise → intersection
                                //
                                // Check if target is a distributive conditional type
                                // whose result is always <: source. This covers
                                // Extract<T, U> = (T extends U ? T : never) where the
                                // true branch is the check type and false branch is never.
                                // The result is always a subset of the check type T, so
                                // if source IS that check type, return target directly.
                                if self.is_conditional_subtype_of_source(*target_type, source_type)
                                {
                                    return *target_type;
                                }
                                // Then use narrow_to_type. If it returns the source
                                // unchanged (assignable but possibly losing structural
                                // info) or NEVER (no overlap), fall back to an
                                // intersection to preserve the target's structure.
                                let narrowed = self.narrow_to_type(source_type, *target_type);
                                if narrowed == source_type && narrowed != *target_type {
                                    // Source was unchanged — intersect to preserve
                                    // target-side structure such as index signatures.
                                    self.db.intersection2(source_type, *target_type)
                                } else if narrowed == TypeId::NEVER && source_type != TypeId::NEVER
                                {
                                    self.db.intersection2(source_type, *target_type)
                                } else {
                                    narrowed
                                }
                            }
                        } else if *asserts {
                            // CRITICAL: For assertion functions, the false branch is unreachable
                            // (the function throws if the assertion fails), so we don't narrow
                            source_type
                        } else {
                            // False branch for regular type guards: exclude the target type.
                            // Resolve Lazy/Application types first so exclusion can see
                            // through opaque wrappers (e.g. Readonly<Record<K,V>>).
                            let resolved_source = self.resolve_for_exclusion_narrowing(source_type);
                            let resolved_target =
                                self.resolve_for_exclusion_narrowing(*target_type);
                            self.narrow_excluding_type(resolved_source, resolved_target)
                        }
                    }
                    None => {
                        // Truthiness assertion: asserts x
                        // Behaves like TypeGuard::Truthy (narrows to truthy in true branch)
                        if *asserts {
                            self.narrow_by_truthiness(source_type)
                        } else {
                            source_type
                        }
                    }
                }
            }

            TypeGuard::Array => {
                if sense {
                    // Positive: Array.isArray(x) - narrow to array-like types
                    self.narrow_to_array(source_type)
                } else {
                    // Negative: !Array.isArray(x) - exclude array-like types
                    self.narrow_excluding_array(source_type)
                }
            }

            TypeGuard::ArrayElementPredicate { element_type } => {
                trace!(
                    ?element_type,
                    ?sense,
                    "Applying ArrayElementPredicate guard"
                );
                if sense {
                    // True branch: narrow array element type
                    let result = self.narrow_array_element_type(source_type, *element_type);
                    trace!(?result, "ArrayElementPredicate narrowing result");
                    result
                } else {
                    // False branch: we don't narrow (arr.every could be false for various reasons)
                    trace!("ArrayElementPredicate false branch, no narrowing");
                    source_type
                }
            }

            TypeGuard::Constructor(instance_type) => {
                if sense {
                    self.narrow_by_constructor(source_type, *instance_type)
                } else {
                    self.narrow_by_constructor_false(source_type, *instance_type)
                }
            }
        }
    }
}
