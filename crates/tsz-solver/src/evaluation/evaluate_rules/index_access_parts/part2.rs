impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Returns `true` when both `left` and `right` are `KeyOf(X)` with the same inner `X`.
    /// Purely structural — no evaluation — so safe for recursive/conditional inner types.
    ///
    /// Type-parameter inners are compared by identity (`name` `Atom`), not by raw
    /// `TypeId`. Nested generic instantiation can produce two distinct interned
    /// `TypeParameter` `TypeId`s for the *same* logical parameter (e.g. when
    /// `Record<keyof T, V>` is expanded as the argument of an outer homomorphic
    /// mapped type like `Partial<…>`). Both `keyof T` occurrences denote the same
    /// key space, so a raw-`TypeId` comparison would spuriously reject
    /// `{ [P in keyof T]?: V }[K]` for `K extends keyof T`.
    fn keyof_same_inner(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        let Some(TypeData::KeyOf(l_inner)) = db.lookup(left) else {
            return false;
        };
        let Some(TypeData::KeyOf(r_inner)) = db.lookup(right) else {
            return false;
        };
        if l_inner == r_inner {
            return true;
        }
        match (db.lookup(l_inner), db.lookup(r_inner)) {
            (Some(TypeData::TypeParameter(l_tp)), Some(TypeData::TypeParameter(r_tp))) => {
                l_tp.name == r_tp.name
            }
            _ => false,
        }
    }

    fn constraints_semantically_match(&mut self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }

        // `keyof T` denotes the same key space regardless of which interned
        // `TypeParameter` `TypeId` represents `T`. Nested generic instantiation can
        // alias the same logical `T` to two distinct `TypeId`s, so compare the
        // `KeyOf` inners by type-parameter identity before falling back to
        // evaluation. This is the homomorphic-mapped read counterpart to the
        // same-name handling already used for the mapped iteration variable.
        if Self::keyof_same_inner(self.interner(), left, right) {
            return true;
        }

        let evaluated_left = self.evaluate(left);
        let evaluated_right = self.evaluate(right);
        if evaluated_left == evaluated_right || left == evaluated_right || evaluated_left == right {
            return true;
        }
        Self::keyof_same_inner(self.interner(), evaluated_left, evaluated_right)
    }

    fn index_type_overlaps_optional_props(
        &mut self,
        index_type: TypeId,
        optional_props: &[tsz_common::Atom],
    ) -> bool {
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
        {
            return optional_props.contains(&name);
        }

        if let Some(members) = union_list_id(self.interner(), index_type) {
            return self
                .interner()
                .type_list(members)
                .iter()
                .any(|&member| self.index_type_overlaps_optional_props(member, optional_props));
        }

        // Intrinsics never match TypeParameter/KeyOf/Intersection — skip lookup.
        if index_type.is_intrinsic() {
            return false;
        }
        match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.is_some_and(|constraint| {
                self.index_type_overlaps_optional_props(constraint, optional_props)
            }),
            Some(TypeData::KeyOf(inner)) => {
                let evaluated = self.evaluate(self.interner().keyof(inner));
                evaluated != index_type
                    && self.index_type_overlaps_optional_props(evaluated, optional_props)
            }
            Some(TypeData::Intersection(list_id)) => self
                .interner()
                .type_list(list_id)
                .iter()
                .any(|&member| self.index_type_overlaps_optional_props(member, optional_props)),
            _ => false,
        }
    }

    fn index_type_can_hit_optional_property(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        let evaluated_object = self.evaluate(object_type);
        let optional_props: Vec<_> = match self.interner().lookup(evaluated_object) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner()
                .object_shape(shape_id)
                .properties
                .iter()
                .filter(|prop| prop.optional)
                .map(|prop| prop.name)
                .collect(),
            Some(TypeData::Callable(shape_id)) => self
                .interner()
                .callable_shape(shape_id)
                .properties
                .iter()
                .filter(|prop| prop.optional)
                .map(|prop| prop.name)
                .collect(),
            _ => return false,
        };

        !optional_props.is_empty()
            && self.index_type_overlaps_optional_props(index_type, &optional_props)
    }

    fn apply_mapped_optional_read_semantics(
        &mut self,
        object_type: TypeId,
        mapped: &MappedType,
        index_type: TypeId,
        value_type: TypeId,
    ) -> TypeId {
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add))
            || (mapped.optional_modifier.is_none()
                && self.index_type_can_hit_optional_property(object_type, index_type))
        {
            return self.interner().union2(value_type, TypeId::UNDEFINED);
        }

        value_type
    }

    fn homomorphic_mapped_source_for_index_read(&mut self, mapped: &MappedType) -> Option<TypeId> {
        let Some(TypeData::IndexAccess(source, idx)) = self.interner().lookup(mapped.template)
        else {
            return None;
        };
        let Some(TypeData::TypeParameter(param)) = self.interner().lookup(idx) else {
            return None;
        };
        if param.name != mapped.type_param.name {
            return None;
        }

        if let Some(keyof_source) = keyof_inner_type(self.interner(), mapped.constraint) {
            return (source == keyof_source).then_some(source);
        }

        if matches!(
            self.interner().lookup(source),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }
        (self.evaluate(self.interner().keyof(source)) == mapped.constraint).then_some(source)
    }

    fn constrained_index_type(&mut self, index_type: TypeId) -> Option<TypeId> {
        if index_type.is_intrinsic() {
            return None;
        }
        match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.and_then(|constraint| {
                let evaluated = self.evaluate(constraint);
                (evaluated != index_type).then_some(evaluated)
            }),
            Some(TypeData::KeyOf(inner)) => {
                let evaluated = self.evaluate(self.interner().keyof(inner));
                (evaluated != index_type).then_some(evaluated)
            }
            Some(TypeData::Intersection(list_id)) => {
                let members: Vec<_> = self.interner().type_list(list_id).iter().copied().collect();
                let resolved: Vec<_> = members
                    .into_iter()
                    .filter_map(|member| {
                        self.constrained_index_type(member)
                            .filter(|resolved| *resolved != member)
                    })
                    .collect();
                match resolved.as_slice() {
                    [] => None,
                    [only] => Some(*only),
                    _ => Some(self.interner().intersection(resolved)),
                }
            }
            _ => None,
        }
    }

    fn evaluate_object_index_from_constraint(
        &mut self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> Option<TypeId> {
        let constrained = self.constrained_index_type(index_type)?;
        let result = self.evaluate_object_index(props, constrained);
        (result != TypeId::UNDEFINED
            || !crate::type_queries::is_generic_type(self.interner(), constrained))
        .then_some(result)
    }

    fn evaluate_object_with_index_from_constraint(
        &mut self,
        shape: &ObjectShape,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let constrained = self.constrained_index_type(index_type)?;
        let result = self.evaluate_object_with_index(shape, constrained);
        (result != TypeId::UNDEFINED
            || !crate::type_queries::is_generic_type(self.interner(), constrained))
        .then_some(result)
    }

    /// Pre-evaluation check for mapped type + type parameter index access.
    ///
    /// When the object is a mapped type like `{ [P in C]: Template<P> }` and the
    /// index is a type parameter `K extends C`, substitute K into the template
    /// to produce `Template<K>`. This must happen before `evaluate(object_type)`
    /// because evaluation expands mapped types with concrete constraints into
    /// Object types, losing the template relationship.
    fn try_mapped_type_param_substitution(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        // Intrinsics are never Mapped or TypeParameter — bail before lookups.
        if object_type.is_intrinsic() || index_type.is_intrinsic() {
            return None;
        }
        let (mapped_object_type, mapped) = self.mapped_substitution_target(object_type)?;

        // Skip if there's a name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        let generic_covering_index = {
            let mut visitor = IndexAccessVisitor {
                evaluator: self,
                object_type,
                index_type,
            };
            visitor.generic_index_covering_mapped_constraint(mapped.constraint)
        };

        // The constraint may be unevaluated; the index itself can still evaluate
        // to the mapped constraint while its own constraint stays deferred.
        let constraint_matches = match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.is_some_and(|index_constraint| {
                self.constraints_semantically_match(index_constraint, mapped.constraint)
                    || self.constraints_semantically_match(index_type, mapped.constraint)
            }),
            // When the index is `keyof T` used directly as the index type, check
            // structurally whether both index and constraint are `KeyOf` of the same
            // inner type — no evaluation, so recursive/conditional types are safe.
            // Example: `{ [K in keyof T]: V }[keyof T]` → both are `KeyOf(T)` → V.
            _ => {
                generic_covering_index.is_some()
                    || Self::keyof_same_inner(self.interner(), index_type, mapped.constraint)
            }
        };

        if !constraint_matches {
            return None;
        }

        // `{ [K in Keys]: F<K> }[Keys]` is a per-key union, not `F<Keys>`.
        // The normal mapped visitor already applies this rule after object
        // evaluation; mirror it here so the pre-evaluation substitution path
        // preserves correlated mapped/indexed access behavior.
        if index_type == mapped.constraint {
            let mut visitor = IndexAccessVisitor {
                evaluator: self,
                object_type,
                index_type,
            };
            if visitor.index_is_symbolic_key_space(mapped.constraint) {
                if let Some(per_key_result) =
                    super::mapped_template_index::try_evaluate_mapped_template_per_concrete_key(
                        visitor.evaluator,
                        &mapped,
                    )
                {
                    return Some(per_key_result);
                }
                return Some(visitor.instantiate_mapped_template_with_constraint_param(&mapped));
            }
        }

        // Substitute K into the mapped template
        let substitution_index = generic_covering_index.unwrap_or(index_type);
        let subst = TypeSubstitution::single(mapped.type_param.name, substitution_index);

        let value_type = self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));
        let value_type = if matches!(mapped.optional_modifier, Some(MappedModifier::Remove))
            && !self.interner().exact_optional_property_types()
            && let Some(source) = self.homomorphic_mapped_source_for_index_read(&mapped)
            && self.index_type_can_hit_optional_property(source, substitution_index)
        {
            crate::narrowing::utils::remove_undefined(self.interner(), value_type)
        } else {
            value_type
        };

        Some(self.apply_mapped_optional_read_semantics(
            mapped_object_type,
            &mapped,
            substitution_index,
            value_type,
        ))
    }

    fn mapped_substitution_target(&self, object_type: TypeId) -> Option<(TypeId, MappedType)> {
        let mapped_type = |mapped_id: MappedTypeId| {
            let mapped = self.interner().get_mapped(mapped_id);
            (object_type, mapped)
        };

        match self.interner().lookup(object_type)? {
            TypeData::Mapped(mapped_id) => Some(mapped_type(mapped_id)),
            TypeData::Lazy(def_id) => {
                let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;
                match self.interner().lookup(resolved)? {
                    TypeData::Mapped(mapped_id) => {
                        Some((resolved, self.interner().get_mapped(mapped_id)))
                    }
                    _ => None,
                }
            }
            TypeData::TypeQuery(sym_ref) => {
                let def_id = self.resolver().symbol_to_def_id(sym_ref)?;
                let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;
                match self.interner().lookup(resolved)? {
                    TypeData::Mapped(mapped_id) => {
                        Some((resolved, self.interner().get_mapped(mapped_id)))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn try_mapped_application_type_param_substitution(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        if object_type.is_intrinsic() {
            return None;
        }

        let application_type = if matches!(
            self.interner().lookup(object_type),
            Some(TypeData::Application(_))
        ) {
            object_type
        } else {
            self.interner()
                .get_display_alias(object_type)
                .filter(|&alias| {
                    matches!(
                        self.interner().lookup(alias),
                        Some(TypeData::Application(_))
                    )
                })?
        };

        let instantiated = self.instantiate_mapped_application_preserving_meta(application_type)?;
        if instantiated == application_type {
            return None;
        }

        if !matches!(
            self.interner().lookup(instantiated),
            Some(TypeData::Mapped(_))
        ) {
            return None;
        }

        self.try_mapped_type_param_substitution(instantiated, index_type)
    }

    fn instantiate_mapped_application_preserving_meta(
        &mut self,
        application_type: TypeId,
    ) -> Option<TypeId> {
        let app_id = match self.interner().lookup(application_type)? {
            TypeData::Application(app_id) => app_id,
            _ => return None,
        };
        let app = self.interner().type_application(app_id);
        let def_id = match self.interner().lookup(app.base)? {
            TypeData::Lazy(def_id) => def_id,
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref)?,
            _ => return None,
        };
        let type_params = self.resolver().get_lazy_type_params(def_id)?;
        let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;
        if !matches!(self.interner().lookup(resolved), Some(TypeData::Mapped(_))) {
            return None;
        }

        let expanded_args = self.expand_type_args(&app.args);
        let mut substitution = TypeSubstitution::new();
        for (param, &arg) in type_params.iter().zip(expanded_args.iter()) {
            substitution.insert(param.name, arg);
        }

        Some(instantiate_type_preserving_meta_cached(
            self.interner(),
            self.query_db(),
            resolved,
            &substitution,
        ))
    }

    /// Helper to recursively evaluate an index access while respecting depth limits.
    /// Creates an `IndexAccess` type and evaluates it through the main `evaluate()` method.
    pub(crate) fn recurse_index_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        let index_access = self.interner().index_access(object_type, index_type);
        self.evaluate(index_access)
    }

    /// Evaluate an index access type: T[K]
    ///
    /// This resolves property access on object types.
    pub fn evaluate_index_access(&mut self, object_type: TypeId, index_type: TypeId) -> TypeId {
        // Pre-evaluation check: if the object is a mapped type and the index is a type
        // parameter whose constraint matches the mapped constraint, substitute K into
        // the mapped template directly. This MUST happen before evaluate(object_type)
        // because evaluation expands mapped types with concrete constraints into Object
        // types, losing the template relationship. Without this, `MappedType[K]` where
        // K extends the mapped constraint would produce a deferred IndexAccess(Object, K)
        // that resolves to a union of concrete types instead of a single generic type.
        // Example: `{ [P in "one"|"two"]: (option: T & {kind:P}) => string }[K]`
        // should produce `(option: T & {kind:K}) => string`, not a union of functions.
        if let Some(mapped_result) =
            self.try_mapped_type_param_substitution(object_type, index_type)
        {
            return mapped_result;
        }
        if let Some(mapped_result) =
            self.try_mapped_application_type_param_substitution(object_type, index_type)
        {
            return mapped_result;
        }

        let evaluated_object = self.evaluate(object_type);
        let evaluated_index = self.evaluate(index_type);
        if evaluated_object != object_type || evaluated_index != index_type {
            // Use recurse_index_access to respect depth limits
            return self.recurse_index_access(evaluated_object, evaluated_index);
        }
        // Match tsc: index access involving `any` produces `any`.
        // (e.g. `any[string]` is `any`, not an error)
        if evaluated_object == TypeId::ANY || evaluated_index == TypeId::ANY {
            return TypeId::ANY;
        }

        // Error type propagation: if the object or index type is ERROR (e.g., from
        // a failed module import), return ERROR to suppress cascading diagnostics.
        // Without this, `Out[T]` where `Out` comes from a missing module would
        // produce false TS2322 errors instead of silently propagating the error.
        if evaluated_object == TypeId::ERROR || evaluated_index == TypeId::ERROR {
            return TypeId::ERROR;
        }

        // `T[never]` and `T[keyof T]` where the key set is empty index over no
        // properties, so they evaluate to `never`. Keep this narrower than all
        // indexes that simplify to `never`, because some mapped/utility-type
        // paths rely on the existing concrete lookup fallback behavior.
        let is_empty_index_access = evaluated_index == TypeId::NEVER
            && (index_type == TypeId::NEVER
                || matches!(
                    self.interner().lookup(index_type),
                    Some(TypeData::KeyOf(inner))
                        if inner == object_type
                            || inner == evaluated_object
                            || self.evaluate(inner) == object_type
                            || self.evaluate(inner) == evaluated_object
                ));
        if is_empty_index_access {
            return TypeId::NEVER;
        }

        // Rule #38: Distribute over index union at the top level (Cartesian product expansion)
        // T[A | B] -> T[A] | T[B]
        // This must happen before checking the object type to ensure full cross-product expansion
        // when both object and index are unions: (X | Y)[A | B] -> X[A] | X[B] | Y[A] | Y[B]
        if let Some(members_id) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members_id);
            // Limit to prevent OOM with large unions
            if members.len() > MAX_UNION_INDEX_SIZE {
                self.mark_depth_exceeded();
                return TypeId::ERROR;
            }
            let mut results = Vec::new();
            for &member in members.iter() {
                if self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                let result = self.recurse_index_access(object_type, member);
                if result == TypeId::ERROR && self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        let interner = self.interner();
        let mut visitor = IndexAccessVisitor {
            evaluator: self,
            object_type,
            index_type,
        };
        if let Some(result) = visitor.visit_type(interner, object_type) {
            return self.evaluate_index_access_result(result);
        }

        // For other types, keep as IndexAccess (deferred)
        self.interner().index_access(object_type, index_type)
    }

    fn evaluate_index_access_result(&mut self, result: TypeId) -> TypeId {
        if result.is_intrinsic() {
            return result;
        }

        match self.interner().lookup(result) {
            Some(TypeData::Application(_)) => {
                let evaluated = self.evaluate(result);
                self.interner()
                    .store_display_alias_preferring_application(evaluated, result);
                evaluated
            }
            Some(
                TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::Mapped(_)
                | TypeData::KeyOf(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::ReadonlyType(_)
                | TypeData::TypeQuery(_)
                | TypeData::Lazy(_),
            ) => self.evaluate(result),
            _ => result,
        }
    }

    /// Evaluate property access on an object type
    pub(crate) fn evaluate_object_index(
        &self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> TypeId {
        // If index is a literal string or unique symbol, look up the property directly
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
        {
            for prop in props {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            // Property not found
            return TypeId::UNDEFINED;
        }

        // If index is a union of literals, return union of property types
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_index(props, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // A plain object type has no index signatures, so indexing it by the bare
        // `string`, `number`, or `symbol` primitive matches no key and no applicable
        // index signature. tsc reports TS2536/TS2537 and resolves the access to the
        // error type (which relations treat as bidirectionally assignable like `any`),
        // suppressing downstream `TS2322`/`TS2344` cascades. Non-primitive indices
        // (e.g. an unresolved generic type parameter) must still fall through to
        // `undefined` so `visit_object` can defer their evaluation.
        if matches!(index_type, TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL) {
            return TypeId::ERROR;
        }

        TypeId::UNDEFINED
    }

    /// Evaluate property access on an object type with index signatures.
    pub(crate) fn evaluate_object_with_index(
        &self,
        shape: &ObjectShape,
        index_type: TypeId,
    ) -> TypeId {
        let string_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL);
        let symbol_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type == TypeId::SYMBOL);

        // If index is a union, evaluate each member
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_with_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal string or unique symbol, look up the property first,
        // then fallback to string index.
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
        {
            let is_symbol_key = matches!(
                self.interner().lookup(index_type),
                Some(TypeData::UniqueSymbol(_))
            );
            for prop in &shape.properties {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            if utils::is_numeric_property_name(self.interner(), name)
                && let Some(number_index) = shape.number_index.as_ref()
            {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if is_symbol_key && let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            // Symbol-keyed properties must not fall through to string index signatures.
            if !is_symbol_key
                && let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // If index is a literal number, prefer number index, then string index.
        if literal_number(self.interner(), index_type).is_some() {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // Bare `string`/`number`/`symbol` indices that match no applicable index
        // signature are a TS2536/TS2537 failure: tsc resolves the access to the error
        // type rather than the union of all member value types, so downstream checks
        // are suppressed. A numeric index still falls back to a string index signature
        // (numeric keys are string keys).
        if index_type == TypeId::STRING {
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::NUMBER {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::SYMBOL {
            if let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            return TypeId::ERROR;
        }

        // Template literal types (e.g., `foo${string}`), string intrinsic types
        // (e.g., Lowercase<T>), and intersections containing string (e.g., string & { brand: any })
        // are all subtypes of string. When the object has a string index signature,
        // these index types should resolve to the string index signature's value type,
        // just like TypeId::STRING does.
        if let Some(string_index) = string_index
            && string_index_signature_applies(self, string_index, index_type)
        {
            return self.add_undefined_if_unchecked(string_index.value_type);
        }

        TypeId::UNDEFINED
    }

    /// Evaluate index access on a callable type (class constructor / `typeof ClassName`).
    ///
    /// Callable types have static properties and index signatures, analogous to
    /// `ObjectWithIndex`. This resolves type-level indexed access like
    /// `(typeof B)["foo"]` or `(typeof B)[number]`.
    pub(crate) fn evaluate_callable_index(
        &self,
        shape: &CallableShape,
        index_type: TypeId,
    ) -> TypeId {
        let string_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL);
        let symbol_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type == TypeId::SYMBOL);

        // If index is a union, evaluate each member
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_callable_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal string or unique symbol, look up properties first,
        // then fallback to index sigs.
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
        {
            let is_symbol_key = matches!(
                self.interner().lookup(index_type),
                Some(TypeData::UniqueSymbol(_))
            );
            for prop in &shape.properties {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            if utils::is_numeric_property_name(self.interner(), name)
                && let Some(number_index) = shape.number_index.as_ref()
            {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if is_symbol_key && let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            // Symbol-keyed properties must NOT fall through to string index signatures
            if !is_symbol_key
                && let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // If index is a literal number, prefer number index, then string index.
        if literal_number(self.interner(), index_type).is_some() {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // Bare `string`/`number`/`symbol` indices that match no applicable index
        // signature are a TS2536/TS2537 failure: tsc resolves the access to the error
        // type rather than the union of all member value types, so downstream checks
        // are suppressed. A numeric index still falls back to a string index signature
        // (numeric keys are string keys).
        if index_type == TypeId::STRING {
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::NUMBER {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::SYMBOL {
            if let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            return TypeId::ERROR;
        }

        // String-like index types (template literals, string intrinsics, branded strings)
        // should use the string index signature when available.
        if let Some(string_index) = string_index
            && string_index_signature_applies(self, string_index, index_type)
        {
            return self.add_undefined_if_unchecked(string_index.value_type);
        }

        TypeId::UNDEFINED
    }

    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        crate::utils::optional_property_type(self.interner(), prop)
    }

    pub(crate) fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access() || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        super::index_access_keys::rest_element_type(self.interner(), type_id)
    }

    /// Evaluate index access on a tuple type
    pub(crate) fn evaluate_tuple_index(
        &self,
        elements: &[TupleElement],
        index_type: TypeId,
    ) -> TypeId {
        super::index_access_keys::evaluate_tuple_index(
            self.interner(),
            elements,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }

    pub(crate) fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        super::index_access_keys::evaluate_array_index(
            self.interner(),
            elem,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }
}
