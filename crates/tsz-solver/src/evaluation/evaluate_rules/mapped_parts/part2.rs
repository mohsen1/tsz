impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Extract mapped keys from a type (for mapped type iteration).
    pub(super) fn extract_mapped_keys(&mut self, type_id: TypeId) -> Option<MappedKeys> {
        let key = self.interner().lookup(type_id)?;

        let mut keys = MappedKeys {
            keys: Vec::new(),
            has_string: false,
            has_number: false,
            template_literals: Vec::new(),
            symbol_keys: Vec::new(),
        };

        match key {
            // NEW: Handle KeyOf types directly if evaluate_keyof deferred
            // This fixes Bug #1: Key Remapping with conditionals
            TypeData::KeyOf(operand) => {
                tracing::trace!(
                    operand = operand.0,
                    operand_lookup = ?self.interner().lookup(operand),
                    "extract_mapped_keys: handling KeyOf type"
                );
                // NORTH STAR: Use collect_properties to extract keys from KeyOf operand.
                // This handles interfaces, classes, intersections, and type parameters.
                let prop_result = crate::objects::collect_properties_cached(
                    operand,
                    self.interner(),
                    self.resolver(),
                    self.query_db(),
                );
                tracing::trace!(
                    operand = operand.0,
                    prop_result = ?std::mem::discriminant(&prop_result),
                    "extract_mapped_keys: collect_properties result"
                );
                match prop_result {
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                    } => {
                        self.collect_props_into_keys(&mut keys, properties);
                        keys.has_string = string_index.is_some();
                        keys.has_number = number_index.is_some();
                        tracing::trace!(
                            keys = ?keys.keys.iter().map(|k| k.name).collect::<Vec<_>>(),
                            has_string = keys.has_string,
                            has_number = keys.has_number,
                            symbol_keys_len = keys.symbol_keys.len(),
                            "extract_mapped_keys: extracted keys from KeyOf"
                        );
                        Some(keys)
                    }
                    PropertyCollectionResult::Any => {
                        keys.has_string = true;
                        keys.has_number = true;
                        tracing::trace!("extract_mapped_keys: KeyOf is Any type");
                        Some(keys)
                    }
                    PropertyCollectionResult::NonObject => {
                        // The operand might be an unevaluated Application or other
                        // deferred type (e.g., `PartialProperties<T, K>` as a type alias
                        // Application). Evaluate it first, then retry collect_properties.
                        let evaluated = self.evaluate(operand);
                        if evaluated != operand {
                            let retry_result = crate::objects::collect_properties_cached(
                                evaluated,
                                self.interner(),
                                self.resolver(),
                                self.query_db(),
                            );
                            match retry_result {
                                PropertyCollectionResult::Properties {
                                    properties,
                                    string_index,
                                    number_index,
                                } => {
                                    self.collect_props_into_keys(&mut keys, properties);
                                    keys.has_string = string_index.is_some();
                                    keys.has_number = number_index.is_some();
                                    tracing::trace!(
                                        keys = ?keys.keys.iter().map(|k| k.name).collect::<Vec<_>>(),
                                        "extract_mapped_keys: extracted keys from evaluated KeyOf operand"
                                    );
                                    return Some(keys);
                                }
                                PropertyCollectionResult::Any => {
                                    keys.has_string = true;
                                    keys.has_number = true;
                                    return Some(keys);
                                }
                                PropertyCollectionResult::NonObject => {}
                            }
                        }
                        tracing::trace!("extract_mapped_keys: KeyOf operand is not an object");
                        None
                    }
                }
            }
            TypeData::Literal(LiteralValue::String(s)) => {
                keys.keys.push(MappedKey {
                    name: s,
                    key_literal: type_id,
                });
                Some(keys)
            }
            TypeData::TemplateLiteral(_) => {
                let evaluated = self.evaluate(type_id);
                if evaluated != type_id {
                    return self.extract_mapped_keys(evaluated);
                }
                // Infinite set — can't expand to concrete keys; emit as index signature.
                keys.template_literals.push(type_id);
                Some(keys)
            }
            // Numeric literals become string property names (e.g., enum value 0 → "0").
            // This handles the case where a single-member enum is used as a mapped type
            // constraint: `Record<E, any>` where `enum E { A = 0 }` produces constraint
            // Enum(_, Literal(Number(0))) → key "0".
            TypeData::Literal(LiteralValue::Number(_)) => {
                keys.keys.push(
                    self.mapped_key_from_literal(type_id)
                        .expect("matched LiteralValue::Number"),
                );
                Some(keys)
            }
            // `AB[K]` in mapped constraints: resolve to the union of property
            // value types for index keys compatible with K, then recurse.
            TypeData::IndexAccess(object_type, index_type) => {
                // If index access can be simplified, recurse into the result.
                let evaluated = self.evaluate(type_id);
                if evaluated != type_id {
                    return self.extract_mapped_keys(evaluated);
                }

                let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
                if let Some(db) = self.query_db() {
                    checker = checker.with_query_db(db);
                }

                match crate::objects::collect_properties_cached(
                    object_type,
                    self.interner(),
                    self.resolver(),
                    self.query_db(),
                ) {
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                    } => {
                        let mut members = Vec::new();

                        // Match literal property keys against the index constraint.
                        for prop in properties {
                            let prop_key = self.interner().literal_string(
                                self.interner().resolve_atom_ref(prop.name).as_ref(),
                            );
                            if checker.is_assignable_to(prop_key, index_type) {
                                members.push(prop.type_id);
                            }
                        }

                        // Index signatures are only used as a fallback if they are
                        // directly addressed by the index constraint.
                        if let Some(string_sig) = string_index
                            && checker.is_assignable_to(string_sig.key_type, index_type)
                        {
                            members.push(string_sig.value_type);
                        }
                        if let Some(number_sig) = number_index
                            && checker.is_assignable_to(number_sig.key_type, index_type)
                        {
                            members.push(number_sig.value_type);
                        }

                        if members.is_empty() {
                            return None;
                        }

                        let value_union = if members.len() == 1 {
                            members[0]
                        } else {
                            self.interner().union(members)
                        };

                        self.extract_mapped_keys(value_union)
                    }
                    PropertyCollectionResult::Any | PropertyCollectionResult::NonObject => None,
                }
            }
            TypeData::Union(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        keys.has_string = true;
                        continue;
                    }
                    if member == TypeId::NUMBER {
                        keys.has_number = true;
                        continue;
                    }
                    if member == TypeId::SYMBOL {
                        // Generic `symbol` type — no concrete index to track.
                        continue;
                    }
                    if matches!(
                        self.interner().lookup(member),
                        Some(TypeData::UniqueSymbol(_))
                    ) {
                        keys.symbol_keys.push(member);
                        continue;
                    }
                    if let Some(key) = self.mapped_key_from_literal(member) {
                        keys.keys.push(key);
                    } else {
                        // Recursively extract keys from non-literal union members.
                        // Handles enum types (TypeData::Enum), lazy refs (TypeData::Lazy),
                        // and nested unions (e.g., `A | B` where A, B are enum types).
                        let inner_keys = self.extract_mapped_keys(member)?;
                        keys.keys.extend(inner_keys.keys);
                        keys.has_string |= inner_keys.has_string;
                        keys.has_number |= inner_keys.has_number;
                        keys.template_literals.extend(inner_keys.template_literals);
                        keys.symbol_keys.extend(inner_keys.symbol_keys);
                    }
                }
                if !keys.has_string
                    && !keys.has_number
                    && keys.keys.is_empty()
                    && keys.template_literals.is_empty()
                    && keys.symbol_keys.is_empty()
                {
                    return None;
                }
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::String) => {
                keys.has_string = true;
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::Number) => {
                keys.has_number = true;
                Some(keys)
            }
            TypeData::Intrinsic(IntrinsicKind::Never) => {
                // Mapped over `never` yields an empty object.
                Some(keys)
            }
            TypeData::UniqueSymbol(_) => {
                keys.symbol_keys.push(type_id);
                Some(keys)
            }
            TypeData::Enum(_def_id, members) => {
                // Enum used as mapped type constraint: extract keys from member union.
                // For `enum E { A, B }`, members is the union `0 | 1`, and the keys
                // are the enum values. Recursively extract from the members type.
                self.extract_mapped_keys(members)
            }
            TypeData::Intersection(members) => {
                // Intersection of key sets: compute the intersection of extracted keys
                // from each member. This handles constraints like `keyof T & keyof U`
                // that remain as Intersection after evaluate_keyof_or_constraint.
                let member_list = self.interner().type_list(members);
                let mut member_keys: Vec<MappedKeys> = Vec::with_capacity(member_list.len());
                for &member in member_list.iter() {
                    // Empty object brands (e.g., the `{}` in `string & {}`) are
                    // identity elements for key iteration — `{}` represents
                    // "any non-nullish value" and imposes no key constraint.
                    // Skip them so the intersection inherits keys from the
                    // remaining members, matching tsc's mapped-type expansion
                    // for branded primitives.
                    if crate::visitors::visitor_predicates::is_empty_object_type(
                        self.interner(),
                        member,
                    ) {
                        continue;
                    }
                    let mk = self.extract_mapped_keys(member)?;
                    member_keys.push(mk);
                }
                if member_keys.is_empty() {
                    return None;
                }
                // Start with the first member's keys and intersect with the rest.
                let mut result = member_keys.remove(0);
                for other in &member_keys {
                    // For string/number index: intersection means both must have it.
                    result.has_string = result.has_string && other.has_string;
                    result.has_number = result.has_number && other.has_number;
                    // For string literals: keep only those present in both sets.
                    // If one side has `has_string` (string index signature), all
                    // literals from the other side are kept (since string encompasses them).
                    if other.has_string {
                        // Other side accepts all strings, so keep result's literals.
                    } else if result.has_string {
                        // Result side accepts all strings, take other's literals.
                        result.keys = other.keys.clone();
                        result.has_string = false; // Narrowed to specific literals.
                    } else {
                        // Both have specific literals: keep only the intersection (by atom).
                        let other_set: rustc_hash::FxHashSet<Atom> =
                            other.keys.iter().map(|k| k.name).collect();
                        result.keys.retain(|k| other_set.contains(&k.name));
                    }
                    // Template literals: union (not intersection) — keep all constraints.
                    for tl in &other.template_literals {
                        if !result.template_literals.contains(tl) {
                            result.template_literals.push(*tl);
                        }
                    }
                    // Symbol keys: keep only symbols present in every member's key set.
                    if !result.symbol_keys.is_empty() {
                        if other.symbol_keys.is_empty() {
                            result.symbol_keys.clear();
                        } else {
                            result.symbol_keys.retain(|k| other.symbol_keys.contains(k));
                        }
                    }
                }
                // Intersection may be empty — still return Some to produce an empty object
                // rather than deferring.
                Some(result)
            }
            TypeData::Lazy(def_id) => {
                // Lazy type reference (e.g., type alias `AB = A | B`): resolve and recurse.
                if let Some(resolved) = self.resolver().resolve_lazy(def_id, self.interner())
                    && resolved != type_id
                {
                    return self.extract_mapped_keys(resolved);
                }
                None
            }
            TypeData::TypeQuery(sym_ref) => {
                // `typeof sym` can be a concrete unique-symbol key. Resolve the
                // value-space query before deciding whether mapped iteration has
                // a concrete property key; otherwise tuple element unions like
                // `typeof tuple[number]` drop symbol elements as if they were
                // unconstrained `symbol`.
                let resolved = self
                    .resolver()
                    .resolve_type_query(sym_ref, self.interner())
                    .unwrap_or_else(|| self.evaluate(type_id));
                if resolved != type_id {
                    self.extract_mapped_keys(resolved)
                } else {
                    None
                }
            }
            // Can't extract literals from other types
            _ => None,
        }
    }

    /// A mapped type is homomorphic if:
    /// 1. The constraint is `keyof T` for some type T
    /// 2. The template is `T[K]` where T is the same type and K is the iteration parameter
    ///
    /// Also handles the post-instantiation case where the `keyof T` constraint was
    /// eagerly evaluated to a union of string literals during `instantiate_type`.
    /// In that case, we verify that `template = obj[P]` and `keyof obj == constraint`.
    fn homomorphic_mapped_source(&mut self, mapped: &MappedType) -> Option<TypeId> {
        // Method 1: Constraint is explicitly `keyof T` (pre-evaluation form)
        if let Some(source_from_constraint) = self.extract_source_from_keyof(mapped.constraint) {
            // Check if template is an IndexAccess type T[K]
            return match self.interner().lookup(mapped.template) {
                Some(TypeData::IndexAccess(obj, idx)) => {
                    if obj != source_from_constraint {
                        return None;
                    }
                    match self.interner().lookup(idx) {
                        Some(TypeData::TypeParameter(param)) => {
                            if param.name == mapped.type_param.name {
                                Some(source_from_constraint)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
        }

        // Method 2: Post-instantiation form where `keyof T` was eagerly evaluated
        // to a union of string literals. The template still has the original structure
        // `T[P]` with the concrete object. Verify by computing `keyof obj` and
        // comparing with the constraint.
        // Key remapping does not change the source used for property modifier
        // preservation. Array/tuple shape preservation is guarded separately.
        if let Some(TypeData::IndexAccess(obj, idx)) = self.interner().lookup(mapped.template)
            && let Some(TypeData::TypeParameter(param)) = self.interner().lookup(idx)
            && param.name == mapped.type_param.name
        {
            // Don't match if obj is still a type parameter (not yet instantiated)
            if matches!(
                self.interner().lookup(obj),
                Some(TypeData::TypeParameter(_))
            ) {
                return None;
            }
            let expected_keys = self.evaluate_keyof(obj);
            // Exact match: safe for all as-clauses including non-identity remapping.
            // For a renaming as-clause like `as Uppercase<K>`, the constraint is still
            // the original keyof obj (before renaming), so the exact match holds.
            if expected_keys == mapped.constraint {
                return Some(obj);
            }
            // Subset check: only safe for identity/no-name mappings. A non-identity
            // as-clause could produce a proper subset for unrelated reasons (e.g.,
            // filtering), making it unsafe to infer source from a subset constraint.
            if crate::type_queries::mapped::is_identity_name_mapping(self.interner(), mapped) {
                // Subset match handles Pick/Omit where constraint is a filtered subset
                // of `keyof T` (e.g., `Exclude<keyof T, K>` evaluates to a subset of keys).
                let evaluated_constraint = self.evaluate_keyof_or_constraint(mapped.constraint);
                if let (Some(constraint_keys), Some(expected_key_set)) = (
                    self.extract_mapped_keys(evaluated_constraint),
                    self.extract_mapped_keys(expected_keys),
                ) {
                    // Only do subset check for pure string literal keys (no string/number index)
                    if !constraint_keys.has_string
                        && !constraint_keys.has_number
                        && !constraint_keys.keys.is_empty()
                    {
                        let expected_set: rustc_hash::FxHashSet<Atom> =
                            expected_key_set.keys.iter().map(|k| k.name).collect();
                        let is_subset = constraint_keys
                            .keys
                            .iter()
                            .all(|k| expected_set.contains(&k.name));
                        if is_subset {
                            return Some(obj);
                        }
                    }
                }
            }
        }

        None
    }

    fn post_instantiation_mapped_template_source(&mut self, mapped: &MappedType) -> Option<TypeId> {
        let source = self.extract_template_index_source(mapped.template, mapped.type_param.name)?;
        if matches!(
            self.interner().lookup(source),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }

        let expected_keys = self.evaluate_keyof(source);
        if expected_keys == mapped.constraint {
            return Some(source);
        }

        let evaluated_constraint = self.evaluate_keyof_or_constraint(mapped.constraint);
        if let (Some(constraint_keys), Some(expected_key_set)) = (
            self.extract_mapped_keys(evaluated_constraint),
            self.extract_mapped_keys(expected_keys),
        ) && !constraint_keys.has_string
            && !constraint_keys.has_number
            && !constraint_keys.keys.is_empty()
        {
            let expected_set: rustc_hash::FxHashSet<Atom> =
                expected_key_set.keys.iter().map(|k| k.name).collect();
            if constraint_keys
                .keys
                .iter()
                .all(|k| expected_set.contains(&k.name))
            {
                return Some(source);
            }
        }

        None
    }

    fn extract_template_index_source(
        &mut self,
        template: TypeId,
        iter_name: Atom,
    ) -> Option<TypeId> {
        match self.interner().lookup(template) {
            Some(TypeData::IndexAccess(obj, idx)) => match self.interner().lookup(idx) {
                Some(TypeData::TypeParameter(param)) if param.name == iter_name => Some(obj),
                _ => None,
            },
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                let members = self.interner().type_list(list_id);
                members
                    .iter()
                    .find_map(|&member| self.extract_template_index_source(member, iter_name))
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                self.extract_template_index_source(cond.true_type, iter_name)
                    .or_else(|| self.extract_template_index_source(cond.false_type, iter_name))
            }
            _ => None,
        }
    }

    /// Extract the source type T from a `keyof T` constraint.
    /// Handles aliased constraints like `type Keys<T> = keyof T`,
    /// and intersection constraints like `keyof T & keyof U` (returns first keyof source).
    fn extract_source_from_keyof(&mut self, constraint: TypeId) -> Option<TypeId> {
        match self.interner().lookup(constraint) {
            Some(TypeData::KeyOf(source)) => Some(source),
            // Handle aliased constraints (Application)
            Some(TypeData::Application(_)) => {
                // Evaluate to resolve the alias
                let evaluated = self.evaluate(constraint);
                // Recursively check the evaluated type
                if evaluated != constraint {
                    self.extract_source_from_keyof(evaluated)
                } else {
                    None
                }
            }
            // Handle intersection constraints like `keyof T & keyof U`.
            // Return the first keyof source found (for property lookup/modifier preservation).
            Some(TypeData::Intersection(members)) => {
                let member_list = self.interner().type_list(members);
                for &member in member_list.iter() {
                    if let Some(source) = self.extract_source_from_keyof(member) {
                        return Some(source);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
