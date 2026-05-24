use super::*;

impl<'a> NarrowingContext<'a> {
    /// Helper for `narrow_excluding_types` with type parameters
    pub(super) fn narrow_type_param_excluding_set(
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

    /// Unwrap `TypeData::Enum(D, inner)` so narrowing-to runs on the inner literal
    /// union and rewraps the result with the appropriate enum nominal identity.
    ///
    /// Structural rule (matches tsc's `getBaseTypeOfEnumType` narrowing model):
    /// a whole-enum value `Enum(parent_def, lit_union)` is treated, for control
    /// flow, as the union of its member-typed values
    /// `Enum(member_i_def, lit_i)`. When the target is the same nominal enum or
    /// one of its members, narrowing runs on the inner literal union, and the
    /// remaining literals are remapped back to the corresponding member-typed
    /// values so equality narrowing yields `Enum(E.A_def, lit_A)` and
    /// exclusion narrowing yields the union of the remaining member types.
    ///
    /// Returns `None` for non-enum sources so callers fall through.
    pub(super) fn narrow_enum_to_type(
        &self,
        original_source: TypeId,
        resolved_source: TypeId,
        target_type: TypeId,
    ) -> Option<TypeId> {
        let (enum_def, inner) = crate::visitor::enum_components(self.db, resolved_source)?;

        // Unwrap target when it lives in the same enum domain as the source
        // (same nominal enum, or any registered member of the source enum).
        let effective_target = match crate::visitor::enum_components(self.db, target_type) {
            Some((target_def, target_inner))
                if self.enum_def_in_source_domain(enum_def, target_def) =>
            {
                target_inner
            }
            _ => target_type,
        };

        let narrowed_inner = self.narrow_to_type(inner, effective_target);

        if narrowed_inner == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }
        if narrowed_inner == inner {
            return Some(original_source);
        }
        Some(self.wrap_enum_narrowed(enum_def, narrowed_inner))
    }

    /// Unwrap `TypeData::Enum(D, inner)` so exclusion runs on the inner literal
    /// union and rewraps the result with the appropriate enum nominal identity.
    /// Excluded values of the same nominal enum or any of its registered
    /// members are normalised to their inner literal so identity-based union
    /// filtering can drop them. The result is remapped back to the union of
    /// the remaining member-typed values, matching tsc's
    /// `getBaseTypeOfEnumType` narrowing model.
    /// Returns `None` for non-enum sources so callers fall through.
    pub(super) fn narrow_enum_excluding_types(
        &self,
        source_type: TypeId,
        excluded_types: &[TypeId],
    ) -> Option<TypeId> {
        let (enum_def, inner) = crate::visitor::enum_components(self.db, source_type)?;

        let normalized: Vec<TypeId> = excluded_types
            .iter()
            .map(
                |&excluded| match crate::visitor::enum_components(self.db, excluded) {
                    Some((excluded_def, excluded_inner))
                        if self.enum_def_in_source_domain(enum_def, excluded_def) =>
                    {
                        excluded_inner
                    }
                    _ => excluded,
                },
            )
            .collect();

        let narrowed_inner = self.narrow_excluding_types(inner, &normalized);

        if narrowed_inner == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }
        if narrowed_inner == inner {
            return Some(source_type);
        }
        Some(self.wrap_enum_narrowed(enum_def, narrowed_inner))
    }

    /// Whether `other_def` names the same nominal enum as `source_enum_def`,
    /// or a registered member of it.
    fn enum_def_in_source_domain(&self, source_enum_def: DefId, other_def: DefId) -> bool {
        if self.class_defs_equivalent_for_narrowing(source_enum_def, other_def) {
            return true;
        }
        let Some(resolver) = self.resolver else {
            return false;
        };
        let Some(parent) = resolver.get_enum_parent_def_id(other_def) else {
            return false;
        };
        self.class_defs_equivalent_for_narrowing(source_enum_def, parent)
    }

    /// Wrap a narrowed inner literal (or union of literals) back into the
    /// appropriate enum nominal. When `parent_def` is a whole-enum with
    /// registered members and every literal in `narrowed_inner` matches a
    /// member's value, returns the union of `Enum(member_def_i, lit_i)`.
    /// Otherwise falls back to wrapping `narrowed_inner` with `parent_def`.
    fn wrap_enum_narrowed(&self, parent_def: DefId, narrowed_inner: TypeId) -> TypeId {
        let fallback = || self.db.enum_type(parent_def, narrowed_inner);
        let Some(resolver) = self.resolver else {
            return fallback();
        };
        let member_defs = resolver.get_enum_member_def_ids(parent_def);
        if member_defs.is_empty() {
            // Either `parent_def` is itself a member (no children registered)
            // or the resolver has no member list for this enum.
            return fallback();
        }

        let lit_to_member: Vec<(TypeId, DefId)> = member_defs
            .iter()
            .filter_map(|&member_def| {
                let member_type = resolver.resolve_lazy(member_def, self.db.as_type_database())?;
                let (_, lit) = crate::visitor::enum_components(self.db, member_type)?;
                Some((lit, member_def))
            })
            .collect();

        let literals: Vec<TypeId> = match union_list_id(self.db, narrowed_inner) {
            Some(union_id) => self.db.type_list(union_id).to_vec(),
            None => vec![narrowed_inner],
        };

        let mut parts: Vec<TypeId> = Vec::with_capacity(literals.len());
        for lit in literals {
            let Some(&(_, member_def)) = lit_to_member.iter().find(|(l, _)| *l == lit) else {
                // At least one literal does not correspond to a registered
                // member (e.g., when the inner has been further narrowed to a
                // subtype of a member's value). Preserve the parent's nominal.
                return fallback();
            };
            parts.push(self.db.enum_type(member_def, lit));
        }

        union_or_single(self.db, parts)
    }

    /// Narrow to function types only.
    pub(super) fn narrow_to_function(&self, source_type: TypeId) -> TypeId {
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

    pub(super) fn narrow_type_param_excluding(
        &self,
        source: TypeId,
        excluded: TypeId,
    ) -> Option<TypeId> {
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

    /// Whether equality narrowing must keep a wide `symbol` rather than
    /// collapsing it to a `unique symbol` value. Mirrors tsc's
    /// `replacePrimitivesWithLiterals`, which replaces only wide
    /// `string`/`number`/`bigint` with literal subtypes and never narrows a
    /// wide `symbol` to a `unique symbol` (so `x: symbol` stays `symbol` after
    /// `x === uniqueSym`).
    pub(super) fn keeps_wide_symbol_over_unique(&self, source: TypeId, target: TypeId) -> bool {
        source == TypeId::SYMBOL && crate::type_queries::is_unique_symbol_type(self.db, target)
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
    pub(super) fn is_conditional_subtype_of_source(&self, target: TypeId, source: TypeId) -> bool {
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

        if self.is_class_subtype_for_narrowing(source, target) {
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

        if self.is_subtype_for_narrowing(source, target) {
            return true;
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
        if self.is_subtype_for_narrowing(resolved_source, resolved_target) {
            return true;
        }

        // Structural fallback: when the SubtypeChecker can't determine the
        // relationship (e.g., due to evaluation/caching limitations), do a
        // direct property-level check. This handles cases like
        // `ArrayLike<any>` (ObjectWithIndex) being assignable to
        // `{ length: unknown }` (Object) during type predicate narrowing.
        self.is_structurally_assignable_to_object(resolved_source, resolved_target)
    }

    pub(super) fn is_subtype_for_narrowing(&self, source: TypeId, target: TypeId) -> bool {
        if let Some(resolver) = self.resolver {
            let mut checker = SubtypeChecker::with_resolver(self.db.as_type_database(), &resolver)
                .with_query_db(self.db);
            checker.is_subtype_of(source, target)
        } else {
            crate::relations::subtype::is_subtype_of_with_db(self.db, source, target)
        }
    }

    fn is_class_subtype_for_narrowing(&self, source: TypeId, target: TypeId) -> bool {
        let Some(source_def) = self.class_def_id_for_narrowing(source) else {
            return false;
        };
        let Some(target_def) = self.class_def_id_for_narrowing(target) else {
            return false;
        };

        if self.class_defs_equivalent_for_narrowing(source_def, target_def) {
            return true;
        }

        let Some(resolver) = self.resolver else {
            return false;
        };
        let mut current = source_def;
        let mut fuel = 50;
        while fuel > 0 {
            fuel -= 1;
            let Some(parent) = resolver.get_class_extends(current) else {
                return false;
            };
            if self.class_defs_equivalent_for_narrowing(parent, target_def) {
                return true;
            }
            current = parent;
        }
        false
    }

    fn class_def_id_for_narrowing(&self, type_id: TypeId) -> Option<DefId> {
        let resolver = self.resolver?;

        if let Some(def_id) = lazy_def_id(self.db, type_id)
            && let Some(crate::def::DefKind::Class) = resolver.get_def_kind(def_id)
        {
            return Some(def_id);
        }

        if let Some(app_id) = application_id(self.db, type_id) {
            let app = self.db.type_application(app_id);
            if let Some(def_id) = lazy_def_id(self.db, app.base)
                && let Some(crate::def::DefKind::Class) = resolver.get_def_kind(def_id)
            {
                return Some(def_id);
            }
        }

        resolver.class_def_for_instance_type(type_id)
    }

    fn class_defs_equivalent_for_narrowing(&self, left: DefId, right: DefId) -> bool {
        if left == right {
            return true;
        }
        self.resolver
            .map(|resolver| resolver.defs_are_equivalent(left, right))
            .unwrap_or(false)
    }

    pub(super) fn remove_redundant_intersection_members(&self, members: &mut Vec<TypeId>) {
        if members.len() <= 1 {
            return;
        }

        let snapshot = members.clone();
        members.retain(|member| {
            let Some(intersection_id) = intersection_list_id(self.db, *member) else {
                return true;
            };
            let intersection_members = self.db.type_list(intersection_id);
            !snapshot.iter().any(|other| {
                other != member && intersection_members.iter().any(|part| part == other)
            })
        });
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
    pub(super) fn resolve_for_exclusion_narrowing(&self, source_type: TypeId) -> TypeId {
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
}
