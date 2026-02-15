//! keyof operator evaluation.
//!
//! Handles TypeScript's keyof operator: `keyof T`

use crate::TypeDatabase;
use crate::subtype::TypeResolver;
use crate::types::*;
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
    /// Helper to recursively evaluate keyof while respecting depth limits.
    /// Creates a KeyOf type and evaluates it through the main evaluate() method.
    fn recurse_keyof(&mut self, operand: TypeId) -> TypeId {
        let keyof = self.interner().keyof(operand);
        self.evaluate(keyof)
    }

    /// Evaluate keyof T - extract the keys of an object type
    pub fn evaluate_keyof(&mut self, operand: TypeId) -> TypeId {
        // CRITICAL: Handle TemplateLiteral BEFORE Union to avoid incorrect intersection.
        // Template literals that expand to unions should return apparent keys of string,
        // not the intersection of individual literal keys.
        if let Some(TypeData::TemplateLiteral(_)) = self.interner().lookup(operand) {
            return self.apparent_primitive_keyof(IntrinsicKind::String);
        }

        // CRITICAL: Handle Union types BEFORE general evaluation to avoid union simplification.
        // keyof (A | B) = keyof A & keyof B (distributive contravariance)
        // If we call evaluate(operand) first, unions get simplified and we lose members.
        // See test_keyof_union_string_index_and_literal_narrows
        if let Some(TypeData::Union(members)) = self.interner().lookup(operand) {
            let member_list = self.interner().type_list(members);

            // Recursively compute keyof for each member (this resolves Lazy/Ref/etc.)
            let mut key_types: Vec<TypeId> = Vec::with_capacity(member_list.len());
            for &member in member_list.iter() {
                key_types.push(self.recurse_keyof(member));
            }

            // keyof (A | B) = keyof A & keyof B - compute intersection of all key sets
            // Prefer explicit key-set intersection to avoid opaque literal intersections
            if let Some(intersection) = self.intersect_keyof_sets(&key_types) {
                intersection
            } else {
                // Fallback: use general intersection
                self.interner().intersection(key_types)
            }
        } else {
            // For non-union types, evaluate normally
            let evaluated_operand = self.evaluate(operand);

            let key = match self.interner().lookup(evaluated_operand) {
                Some(k) => k,
                None => return TypeId::NEVER,
            };

            match key {
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
                        } else {
                            self.recurse_keyof(constraint)
                        }
                    } else {
                        self.interner().keyof(operand)
                    }
                }
                TypeData::Object(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    if shape.properties.is_empty() {
                        return TypeId::NEVER;
                    }
                    let key_types: Vec<TypeId> = shape
                        .properties
                        .iter()
                        .map(|p| self.interner().literal_string_atom(p.name))
                        .collect();
                    self.interner().union(key_types)
                }
                TypeData::ObjectWithIndex(shape_id) => {
                    let shape = self.interner().object_shape(shape_id);
                    let mut key_types: Vec<TypeId> = shape
                        .properties
                        .iter()
                        .map(|p| self.interner().literal_string_atom(p.name))
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
                // For other types (type parameters, etc.), keep as KeyOf (deferred)
                _ => self.interner().keyof(operand),
            }
        }
    }

    /// Compute keyof for an intersection type: keyof (A & B) = keyof A | keyof B
    pub(crate) fn keyof_intersection(&mut self, members: TypeListId, _operand: TypeId) -> TypeId {
        let members = self.interner().type_list(members);
        // Use recurse_keyof to respect depth limits
        // Use loop instead of closure to allow mutable self access
        let mut key_sets: Vec<TypeId> = Vec::with_capacity(members.len());
        for &m in members.iter() {
            key_sets.push(self.recurse_keyof(m));
        }
        self.interner().union(key_sets)
    }

    /// Get the keyof keys for an array type (includes all array methods and number index).
    pub(crate) fn array_keyof_keys(&self) -> Vec<TypeId> {
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
                    Some(TypeData::Array(_)) => return None,
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
