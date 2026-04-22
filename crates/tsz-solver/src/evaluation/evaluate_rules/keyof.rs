//! keyof operator evaluation.
//!
//! Handles TypeScript's keyof operator: `keyof T`

use crate::TypeDatabase;
use crate::objects::{PropertyCollectionResult, collect_properties};
use crate::relations::subtype::TypeResolver;
use crate::type_queries::narrow_keyof_intersection_member_by_literal_discriminants;
use crate::types::{
    IntrinsicKind, LiteralValue, MappedType, MappedTypeId, TupleElement, TypeData, TypeId,
    TypeListId,
};
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;

use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID, TypeEvaluator,
};

/// Tracks the types of keys found during keyof evaluation.
pub(crate) struct KeyofKeySet {
    pub has_string: bool,
    pub has_number: bool,
    pub has_symbol: bool,
    pub string_literals: FxHashSet<Atom>,
}

impl KeyofKeySet {
    pub fn new() -> Self {
        Self {
            has_string: false,
            has_number: false,
            has_symbol: false,
            string_literals: FxHashSet::default(),
        }
    }

    pub fn insert_type(&mut self, interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let Some(key) = interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeData::Union(members) => {
                let members = interner.type_list(members);
                members
                    .iter()
                    .all(|&member| self.insert_type(interner, member))
            }
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::String => {
                    self.has_string = true;
                    true
                }
                IntrinsicKind::Number => {
                    self.has_number = true;
                    true
                }
                IntrinsicKind::Symbol => {
                    self.has_symbol = true;
                    true
                }
                IntrinsicKind::Never => true,
                _ => false,
            },
            TypeData::Literal(LiteralValue::String(atom)) => {
                self.string_literals.insert(atom);
                true
            }
            _ => false,
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn property_name_atom_to_key_type(&self, name: Atom) -> TypeId {
        let name_text = self.interner().resolve_atom_ref(name);
        if let Some(symbol_ref) = name_text.strip_prefix("__unique_")
            && let Ok(id) = symbol_ref.parse::<u32>()
        {
            return self.interner().unique_symbol(crate::types::SymbolRef(id));
        }
        self.interner().literal_string_atom(name)
    }

    fn push_remapped_key_type(&mut self, key_types: &mut Vec<TypeId>, remapped_key: TypeId) {
        if remapped_key == TypeId::STRING {
            key_types.push(TypeId::STRING);
            key_types.push(TypeId::NUMBER);
        } else {
            key_types.push(remapped_key);
        }
    }

    fn collect_keyof_for_remapped_mapped_type(
        &mut self,
        mapped_id: MappedTypeId,
        mapped: &MappedType,
    ) -> Option<TypeId> {
        let name_type = mapped.name_type?;
        let mut key_types = Vec::new();

        let constraint_source = crate::keyof_inner_type(self.interner(), mapped.constraint)
            .or_else(|| {
                let evaluated = self.evaluate(mapped.constraint);
                (evaluated != mapped.constraint)
                    .then(|| crate::keyof_inner_type(self.interner(), evaluated))
                    .flatten()
            });

        if let Some(source) = constraint_source {
            let resolved_source = self.evaluate(source);
            match collect_properties(resolved_source, self.interner(), self.resolver()) {
                PropertyCollectionResult::Properties {
                    properties,
                    string_index,
                    number_index,
                } => {
                    for prop in properties {
                        let source_key = self.property_name_atom_to_key_type(prop.name);
                        match self.remap_key_type_for_mapped(mapped, source_key) {
                            Ok(Some(remapped_key)) => {
                                self.push_remapped_key_type(&mut key_types, remapped_key);
                            }
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    }
                    if string_index.is_some() {
                        match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                            Ok(Some(remapped_key)) => {
                                self.push_remapped_key_type(&mut key_types, remapped_key);
                            }
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    } else if number_index.is_some() {
                        match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                            Ok(Some(remapped_key)) => key_types.push(remapped_key),
                            Ok(None) => {}
                            Err(()) => return None,
                        }
                    }
                }
                PropertyCollectionResult::Any => {
                    match self.remap_key_type_for_mapped(mapped, TypeId::STRING) {
                        Ok(Some(remapped_key)) => {
                            self.push_remapped_key_type(&mut key_types, remapped_key);
                        }
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                    match self.remap_key_type_for_mapped(mapped, TypeId::NUMBER) {
                        Ok(Some(remapped_key)) => key_types.push(remapped_key),
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                    match self.remap_key_type_for_mapped(mapped, TypeId::SYMBOL) {
                        Ok(Some(remapped_key)) => key_types.push(remapped_key),
                        Ok(None) => {}
                        Err(()) => return None,
                    }
                }
                PropertyCollectionResult::NonObject => {}
            }
        } else if let Some(names) =
            crate::type_queries::collect_finite_mapped_property_names(self.interner(), mapped_id)
        {
            let mut sorted_names: Vec<_> = names.into_iter().collect();
            sorted_names.sort_by_key(|atom| atom.0);
            for name in sorted_names {
                key_types.push(self.property_name_atom_to_key_type(name));
            }
        }

        if crate::type_queries::contains_type_parameters_db(self.interner(), mapped.constraint) {
            key_types.push(name_type);
        }

        if key_types.is_empty() {
            None
        } else if key_types.len() == 1 {
            Some(key_types[0])
        } else {
            Some(self.interner().union(key_types))
        }
    }

    fn should_include_keyof_property(&self, prop: &crate::PropertyInfo) -> bool {
        prop.visibility == crate::Visibility::Public
            && !self
                .interner()
                .resolve_atom_ref(prop.name)
                .starts_with("__private_brand_")
    }

    /// Helper to recursively evaluate keyof while respecting depth limits.
    /// Creates a `KeyOf` type and evaluates it through the main `evaluate()` method.
    fn recurse_keyof(&mut self, operand: TypeId) -> TypeId {
        let keyof = self.interner().keyof(operand);
        self.evaluate(keyof)
    }

    /// Evaluate keyof T - extract the keys of an object type
    pub fn evaluate_keyof(&mut self, operand: TypeId) -> TypeId {
        // PERF: Single lookup for both TemplateLiteral and Union checks.
        // CRITICAL ordering: TemplateLiteral BEFORE Union to avoid incorrect intersection,
        // and Union BEFORE general evaluation to avoid union simplification.
        match self.interner().lookup(operand) {
            Some(TypeData::TemplateLiteral(_)) => {
                return self.apparent_primitive_keyof(IntrinsicKind::String);
            }
            Some(TypeData::Union(_members)) => {
                let narrowed_operand = crate::type_queries::prune_impossible_object_union_members(
                    self.interner(),
                    operand,
                );
                let Some(TypeData::Union(members)) = self.interner().lookup(narrowed_operand)
                else {
                    return self.recurse_keyof(narrowed_operand);
                };
                let member_list = self.interner().type_list(members);

                // Recursively compute keyof for each member (this resolves Lazy/Ref/etc.)
                let mut key_types: Vec<TypeId> = Vec::with_capacity(member_list.len());
                for &member in member_list.iter() {
                    key_types.push(self.recurse_keyof(member));
                }

                // keyof (A | B) = keyof A & keyof B - compute intersection of all key sets
                // Prefer explicit key-set intersection to avoid opaque literal intersections
                return if let Some(intersection) = self.intersect_keyof_sets(&key_types) {
                    intersection
                } else {
                    // Fallback: use general intersection
                    self.interner().intersection(key_types)
                };
            }
            _ => {}
        }

        {
            // For non-union types, evaluate normally
            let evaluated_operand = self.evaluate(operand);

            let key = match self.interner().lookup(evaluated_operand) {
                Some(k) => k,
                None => return TypeId::NEVER,
            };

            match key {
                TypeData::Union(_members) => {
                    let narrowed_operand =
                        crate::type_queries::prune_impossible_object_union_members(
                            self.interner(),
                            evaluated_operand,
                        );
                    let Some(TypeData::Union(members)) = self.interner().lookup(narrowed_operand)
                    else {
                        return self.recurse_keyof(narrowed_operand);
                    };
                    let member_list = self.interner().type_list(members);
                    let mut key_types: Vec<TypeId> = Vec::with_capacity(member_list.len());
                    for &member in member_list.iter() {
                        key_types.push(self.recurse_keyof(member));
                    }
                    if let Some(intersection) = self.intersect_keyof_sets(&key_types) {
                        intersection
                    } else {
                        self.interner().intersection(key_types)
                    }
                }
                TypeData::Mapped(mapped_id) => {
                    let mapped = self.interner().get_mapped(mapped_id);
                    if mapped.name_type.is_some() {
                        self.collect_keyof_for_remapped_mapped_type(mapped_id, &mapped)
                            .unwrap_or_else(|| {
                                self.evaluate(mapped.name_type.unwrap_or(TypeId::ERROR))
                            })
                    } else {
                        self.evaluate(mapped.constraint)
                    }
                }
                TypeData::ReadonlyType(inner) => self.recurse_keyof(inner),
                TypeData::TypeQuery(sym) => {
                    // Resolve typeof query before computing keyof
                    let resolved = self.resolver().resolve_symbol_ref(sym, self.interner());
                    if let Some(resolved) = resolved {
                        self.recurse_keyof(resolved)
                    } else {
                        TypeId::ERROR
                    }
                }
                TypeData::TypeParameter(param) | TypeData::Infer(param) => {
                    if let Some(constraint) = param.constraint {
                        if constraint == evaluated_operand {
                            self.interner().keyof(operand)
                        } else if matches!(
                            self.interner().lookup(constraint),
                            Some(TypeData::TypeParameter(_))
                        ) {
                            // When the constraint is itself a type parameter
                            // (e.g., B extends A where A is generic), do NOT
                            // recurse. keyof B ≠ keyof A even though B extends A,
                            // because B may have additional keys. Collapsing
                            // keyof B → keyof A breaks subtype comparisons.
                            self.interner().keyof(operand)
                        } else {
                            // Evaluate keyof of the constraint. If the result is
                            // non-informative (NEVER — e.g. `object`, `unknown`),
                            // keep keyof as deferred to preserve the type parameter
                            // connection. This is critical for for-in loops where
                            // `keyof T` must remain abstract for mapped type index
                            // access patterns to work correctly.
                            let constraint_keyof = self.recurse_keyof(constraint);
                            if constraint_keyof == TypeId::NEVER {
                                self.interner().keyof(operand)
                            } else {
                                constraint_keyof
                            }
                        }
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                TypeData::Object(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    let key_types: Vec<TypeId> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .map(|p| self.property_name_atom_to_key_type(p.name))
                        .collect();
                    if key_types.is_empty() {
                        return TypeId::NEVER;
                    }
                    self.interner().union(key_types)
                }
                TypeData::ObjectWithIndex(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    let mut key_types: Vec<TypeId> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .map(|p| self.property_name_atom_to_key_type(p.name))
                        .collect();

                    if shape.string_index.is_some() {
                        key_types.push(TypeId::STRING);
                        key_types.push(TypeId::NUMBER);
                    } else if shape.number_index.is_some() {
                        key_types.push(TypeId::NUMBER);
                    }

                    if key_types.is_empty() {
                        TypeId::NEVER
                    } else {
                        self.interner().union(key_types)
                    }
                }
                TypeData::Callable(shape_id) => {
                    let shape = self.interner().callable_shape(shape_id);
                    let mut key_types: Vec<TypeId> = shape
                        .properties
                        .iter()
                        .filter(|p| self.should_include_keyof_property(p))
                        .map(|p| self.property_name_atom_to_key_type(p.name))
                        .collect();

                    if shape.string_index.is_some() {
                        key_types.push(TypeId::STRING);
                        key_types.push(TypeId::NUMBER);
                    } else if shape.number_index.is_some() {
                        key_types.push(TypeId::NUMBER);
                    }

                    if key_types.is_empty() {
                        TypeId::NEVER
                    } else {
                        self.interner().union(key_types)
                    }
                }
                TypeData::Array(_) => self.interner().union(self.array_keyof_keys()),
                TypeData::Tuple(elements) => {
                    let elements = self.interner().tuple_list(elements);
                    let mut key_types: Vec<TypeId> = Vec::new();
                    self.append_tuple_indices(&elements, 0, &mut key_types);
                    let mut array_keys = self.array_keyof_keys();
                    key_types.append(&mut array_keys);
                    if key_types.is_empty() {
                        return TypeId::NEVER;
                    }
                    self.interner().union(key_types)
                }
                TypeData::Intrinsic(kind) => match kind {
                    IntrinsicKind::Any => {
                        // keyof any = string | number | symbol
                        self.interner()
                            .union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
                    }
                    IntrinsicKind::Unknown => {
                        // keyof unknown = never
                        TypeId::NEVER
                    }
                    IntrinsicKind::Never
                    | IntrinsicKind::Void
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Object
                    | IntrinsicKind::Function => TypeId::NEVER,
                    IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol => self.apparent_primitive_keyof(kind),
                },
                TypeData::Literal(literal) => {
                    if let Some(kind) = self.apparent_literal_kind(&literal) {
                        self.apparent_primitive_keyof(kind)
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                TypeData::TemplateLiteral(_) => {
                    self.apparent_primitive_keyof(IntrinsicKind::String)
                }
                // NOTE: Union is handled at the top of this function to avoid union simplification
                TypeData::Intersection(members) => {
                    // keyof (A & B) = keyof A | keyof B (covariance)
                    self.keyof_intersection(members, operand)
                }
                // CRITICAL: Handle Lazy (type aliases) by attempting resolution via resolver
                TypeData::Lazy(def_id) => {
                    match self.resolver().resolve_lazy(def_id, self.interner()) {
                        Some(resolved) => {
                            // Recursively compute keyof of the resolved type
                            self.recurse_keyof(resolved)
                        }
                        None => {
                            // Keep as deferred KeyOf if resolution fails
                            self.interner().keyof(operand)
                        }
                    }
                }
                // CRITICAL: Handle Application (generic types) by evaluating them first
                TypeData::Application(_app_id) => {
                    // Evaluate the application to get the instantiated type
                    let evaluated = self.evaluate(evaluated_operand);
                    // Then compute keyof of the evaluated result
                    self.recurse_keyof(evaluated)
                }
                // ThisType: resolve to the concrete class type via the resolver,
                // then compute keyof on the resolved type.
                TypeData::ThisType => {
                    if let Some(concrete_this) = self.resolver().resolve_this_type(self.interner())
                    {
                        self.recurse_keyof(concrete_this)
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                // Enum types: resolve to the namespace object type for keyof
                // typeof Enum gives { Up: E.Up, Down: E.Down }, keyof gives "Up" | "Down"
                TypeData::Enum(def_id, _member_type) => {
                    if let Some(ns_type) = self.resolver().get_enum_namespace_type(def_id) {
                        self.recurse_keyof(ns_type)
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                // For other types (type parameters, etc.), keep as KeyOf (deferred)
                _ => self.interner().keyof(operand),
            }
        }
    }

    /// Compute keyof for an intersection type: keyof (A & B) = keyof A | keyof B
    pub(crate) fn keyof_intersection(&mut self, members: TypeListId, _operand: TypeId) -> TypeId {
        let members = self.interner().type_list(members).to_vec();
        // Use recurse_keyof to respect depth limits
        // Use loop instead of closure to allow mutable self access
        let mut key_sets: Vec<TypeId> = Vec::with_capacity(members.len());
        for (member_idx, &member) in members.iter().enumerate() {
            let narrowed_member = narrow_keyof_intersection_member_by_literal_discriminants(
                self.interner(),
                member,
                &members,
                member_idx,
            );
            key_sets.push(self.recurse_keyof(narrowed_member));
        }
        self.interner().union(key_sets)
    }

    /// Get the keyof keys for an array type (includes all array methods and number index).
    pub(crate) fn array_keyof_keys(&self) -> Vec<TypeId> {
        let array_base = self
            .interner()
            .get_array_display_base_type()
            .or_else(|| self.resolver().get_array_base_type());
        if let Some(array_base) = array_base {
            let base_props = crate::type_queries::collect_homomorphic_source_property_infos(
                self.interner(),
                array_base,
            );
            if !base_props.is_empty() {
                let mut keys = Vec::with_capacity(base_props.len() + 1);
                keys.push(TypeId::NUMBER);
                for prop in base_props {
                    keys.push(self.property_name_atom_to_key_type(prop.name));
                }
                return keys;
            }
        }

        let mut keys = Vec::new();
        keys.push(TypeId::NUMBER);
        keys.push(self.interner().literal_string("length"));
        for &name in ARRAY_METHODS_RETURN_ANY {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_BOOLEAN {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_NUMBER {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_VOID {
            keys.push(self.interner().literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_STRING {
            keys.push(self.interner().literal_string(name));
        }
        keys
    }

    /// Append tuple indices as string literal keys to the output vector.
    /// Returns the next index to use, or None if a rest element prevents fixed indexing.
    pub(crate) fn append_tuple_indices(
        &self,
        elements: &[TupleElement],
        base: usize,
        out: &mut Vec<TypeId>,
    ) -> Option<usize> {
        let mut index = base;

        for element in elements {
            if element.rest {
                match self.interner().lookup(element.type_id) {
                    Some(TypeData::Tuple(rest_elements)) => {
                        let rest_elements = self.interner().tuple_list(rest_elements);
                        match self.append_tuple_indices(&rest_elements, index, out) {
                            Some(next) => {
                                index = next;
                                continue;
                            }
                            None => return None,
                        }
                    }
                    _ => return None,
                }
            }
            out.push(self.interner().literal_string(&index.to_string()));
            index += 1;
        }

        Some(index)
    }

    /// Compute the intersection of multiple keyof key sets.
    /// Returns None if the intersection cannot be computed (e.g., non-literal keys).
    pub(crate) fn intersect_keyof_sets(&self, key_sets: &[TypeId]) -> Option<TypeId> {
        let mut parsed_sets = Vec::with_capacity(key_sets.len());
        for &key_set in key_sets {
            let mut parsed = KeyofKeySet::new();
            if !parsed.insert_type(self.interner(), key_set) {
                return None;
            }
            parsed_sets.push(parsed);
        }

        let mut all_string = true;
        let mut string_possible = true;
        let mut common_literals: Option<FxHashSet<Atom>> = None;
        let mut all_number = true;
        let mut all_symbol = true;

        for set in &parsed_sets {
            if set.has_string {
                // string index signatures don't restrict literal key overlap
            } else {
                all_string = false;
                if set.string_literals.is_empty() {
                    string_possible = false;
                } else {
                    common_literals = Some(match common_literals {
                        Some(mut existing) => {
                            existing.retain(|atom| set.string_literals.contains(atom));
                            existing
                        }
                        None => set.string_literals.clone(),
                    });
                }
            }

            if !set.has_number {
                all_number = false;
            }
            if !set.has_symbol {
                all_symbol = false;
            }
        }

        let mut result_keys = Vec::new();
        if string_possible {
            if all_string {
                result_keys.push(TypeId::STRING);
            } else if let Some(common) = common_literals {
                for atom in common {
                    result_keys.push(self.interner().literal_string_atom(atom));
                }
            }
        }
        if all_number {
            result_keys.push(TypeId::NUMBER);
        }
        if all_symbol {
            result_keys.push(TypeId::SYMBOL);
        }

        if result_keys.is_empty() {
            Some(TypeId::NEVER)
        } else if result_keys.len() == 1 {
            Some(result_keys[0])
        } else {
            Some(self.interner().union(result_keys))
        }
    }
}
