//! Type Arena for the type checker.
//!
//! This module contains the TypeArena which manages type allocation
//! and provides singleton caching for intrinsic types.

use super::types::{
    ArrayTypeInfo, ConditionalType, EnumTypeInfo, FunctionType, IndexInfo, IndexType,
    IndexedAccessType, IntersectionType, IntrinsicType, LiteralType, LiteralValue, MappedType,
    MappedTypeModifier, ObjectType, Signature, TemplateLiteralType, ThisTypeMarker, TupleTypeInfo,
    Type, TypeId, TypeParameter, UnionType, UniqueSymbolType, element_flags, object_flags,
    type_flags,
};
use crate::binder::{SymbolId, SymbolTable};
use crate::parser::NodeIndex;
use serde::Serialize;

/// Arena allocator for types with singleton caching.
#[derive(Debug, Serialize)]
pub struct TypeArena {
    types: Vec<Type>,
    // Singleton intrinsic types
    pub any_type: TypeId,
    pub unknown_type: TypeId,
    pub string_type: TypeId,
    pub number_type: TypeId,
    pub boolean_type: TypeId,
    pub big_int_type: TypeId,
    pub es_symbol_type: TypeId,
    pub void_type: TypeId,
    pub undefined_type: TypeId,
    pub null_type: TypeId,
    pub never_type: TypeId,
    pub object_type: TypeId,
    // Global object types
    pub regexp_type: TypeId,
    // Literal singletons
    pub true_type: TypeId,
    pub false_type: TypeId,
}

impl TypeArena {
    pub fn new() -> Self {
        let mut arena = TypeArena {
            types: Vec::new(),
            any_type: TypeId::NONE,
            unknown_type: TypeId::NONE,
            string_type: TypeId::NONE,
            number_type: TypeId::NONE,
            boolean_type: TypeId::NONE,
            big_int_type: TypeId::NONE,
            es_symbol_type: TypeId::NONE,
            void_type: TypeId::NONE,
            undefined_type: TypeId::NONE,
            null_type: TypeId::NONE,
            never_type: TypeId::NONE,
            object_type: TypeId::NONE,
            regexp_type: TypeId::NONE,
            true_type: TypeId::NONE,
            false_type: TypeId::NONE,
        };

        // Pre-allocate singleton intrinsic types
        arena.any_type = arena.create_intrinsic(type_flags::ANY, "any");
        arena.unknown_type = arena.create_intrinsic(type_flags::UNKNOWN, "unknown");
        arena.string_type = arena.create_intrinsic(type_flags::STRING, "string");
        arena.number_type = arena.create_intrinsic(type_flags::NUMBER, "number");
        arena.boolean_type = arena.create_intrinsic(type_flags::BOOLEAN, "boolean");
        arena.big_int_type = arena.create_intrinsic(type_flags::BIG_INT, "bigint");
        arena.es_symbol_type = arena.create_intrinsic(type_flags::ES_SYMBOL, "symbol");
        arena.void_type = arena.create_intrinsic(type_flags::VOID, "void");
        arena.undefined_type = arena.create_intrinsic(type_flags::UNDEFINED, "undefined");
        arena.null_type = arena.create_intrinsic(type_flags::NULL, "null");
        arena.never_type = arena.create_intrinsic(type_flags::NEVER, "never");
        arena.object_type = arena.create_intrinsic(type_flags::NON_PRIMITIVE, "object");

        // Global object type singletons
        arena.regexp_type = arena.create_intrinsic(type_flags::OBJECT, "RegExp");

        // Boolean literal singletons
        arena.true_type = arena.create_boolean_literal(true);
        arena.false_type = arena.create_boolean_literal(false);

        arena
    }

    /// Create an intrinsic type.
    fn create_intrinsic(&mut self, flags: u32, name: &str) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(Type::Intrinsic(IntrinsicType {
            flags,
            intrinsic_name: name.to_string(),
        }));
        id
    }

    /// Create a boolean literal type.
    fn create_boolean_literal(&mut self, value: bool) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(Type::Literal(LiteralType {
            flags: type_flags::BOOLEAN_LITERAL,
            value: LiteralValue::Boolean(value),
            fresh_type: id,
            regular_type: id,
        }));
        id
    }

    /// Allocate a new type.
    pub fn alloc(&mut self, typ: Type) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(typ);
        id
    }

    /// Get a type by ID.
    pub fn get(&self, id: TypeId) -> Option<&Type> {
        if id.is_none() {
            None
        } else {
            self.types.get(id.0 as usize)
        }
    }

    /// Get a mutable type by ID.
    pub fn get_mut(&mut self, id: TypeId) -> Option<&mut Type> {
        if id.is_none() {
            None
        } else {
            self.types.get_mut(id.0 as usize)
        }
    }

    /// Get the number of types.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Create a string literal type.
    pub fn create_string_literal(&mut self, value: String) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(Type::Literal(LiteralType {
            flags: type_flags::STRING_LITERAL,
            value: LiteralValue::String(value),
            fresh_type: id,
            regular_type: id,
        }));
        id
    }

    /// Create a number literal type.
    pub fn create_number_literal(&mut self, value: f64) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(Type::Literal(LiteralType {
            flags: type_flags::NUMBER_LITERAL,
            value: LiteralValue::Number(value),
            fresh_type: id,
            regular_type: id,
        }));
        id
    }

    /// Create a bigint literal type.
    pub fn create_bigint_literal(&mut self, value: String) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(Type::Literal(LiteralType {
            flags: type_flags::BIG_INT_LITERAL,
            value: LiteralValue::BigInt(value),
            fresh_type: id,
            regular_type: id,
        }));
        id
    }

    /// Create a union type with simplification.
    /// - Flattens nested unions
    /// - Removes `never` (X | never = X)
    /// - Returns `any` if any constituent is `any`
    /// - Removes duplicates
    /// - Returns single type if only one remains
    pub fn create_union(&mut self, types: Vec<TypeId>) -> TypeId {
        // First, flatten nested unions and collect constituent types
        let mut flattened: Vec<TypeId> = Vec::new();
        for type_id in types {
            if let Some(typ) = self.get(type_id) {
                let flags = typ.flags();

                // If any type is any, the whole union is any
                if (flags & type_flags::ANY) != 0 {
                    return self.any_type;
                }

                // Skip never (X | never = X)
                if (flags & type_flags::NEVER) != 0 {
                    continue;
                }

                // Flatten nested unions
                if let Type::Union(ut) = typ {
                    let nested_types = ut.types.clone();
                    for nested_id in nested_types {
                        // Check nested for any/never too
                        if let Some(nested_typ) = self.get(nested_id) {
                            let nested_flags = nested_typ.flags();
                            if (nested_flags & type_flags::ANY) != 0 {
                                return self.any_type;
                            }
                            if (nested_flags & type_flags::NEVER) != 0 {
                                continue;
                            }
                        }
                        if !flattened.contains(&nested_id) {
                            flattened.push(nested_id);
                        }
                    }
                } else {
                    // Add if not already present (dedup)
                    if !flattened.contains(&type_id) {
                        flattened.push(type_id);
                    }
                }
            } else {
                // Type not found, add as-is
                if !flattened.contains(&type_id) {
                    flattened.push(type_id);
                }
            }
        }

        // If empty after filtering, return never (identity for union)
        if flattened.is_empty() {
            return self.never_type;
        }

        // If only one type, return it directly
        if flattened.len() == 1 {
            return flattened[0];
        }

        self.alloc(Type::Union(Box::new(UnionType::new(flattened))))
    }

    /// Create an intersection type with simplification.
    /// - Flattens nested intersections
    /// - Returns `never` if any constituent is `never`
    /// - Removes `unknown` (X & unknown = X)
    /// - Removes duplicates
    /// - Returns single type if only one remains
    pub fn create_intersection(&mut self, types: Vec<TypeId>) -> TypeId {
        // First, flatten nested intersections and collect constituent types
        let mut flattened: Vec<TypeId> = Vec::new();
        for type_id in types {
            if let Some(typ) = self.get(type_id) {
                let flags = typ.flags();

                // If any type is never, the whole intersection is never
                if (flags & type_flags::NEVER) != 0 {
                    return self.never_type;
                }

                // Skip unknown (X & unknown = X)
                if (flags & type_flags::UNKNOWN) != 0 {
                    continue;
                }

                // Flatten nested intersections
                if let Type::Intersection(it) = typ {
                    let nested_types = it.types.clone();
                    for nested_id in nested_types {
                        // Check nested for never/unknown too
                        if let Some(nested_typ) = self.get(nested_id) {
                            let nested_flags = nested_typ.flags();
                            if (nested_flags & type_flags::NEVER) != 0 {
                                return self.never_type;
                            }
                            if (nested_flags & type_flags::UNKNOWN) != 0 {
                                continue;
                            }
                        }
                        if !flattened.contains(&nested_id) {
                            flattened.push(nested_id);
                        }
                    }
                } else {
                    // Add if not already present (dedup)
                    if !flattened.contains(&type_id) {
                        flattened.push(type_id);
                    }
                }
            } else {
                // Type not found, add as-is
                if !flattened.contains(&type_id) {
                    flattened.push(type_id);
                }
            }
        }

        // If empty after filtering, return unknown (identity for intersection)
        if flattened.is_empty() {
            return self.unknown_type;
        }

        // If only one type, return it directly
        if flattened.len() == 1 {
            return flattened[0];
        }

        self.alloc(Type::Intersection(Box::new(IntersectionType::new(
            flattened,
        ))))
    }

    /// Create an array type (T[] or Array<T>).
    pub fn create_array_type(&mut self, element_type: TypeId, is_readonly: bool) -> TypeId {
        self.alloc(Type::Array(Box::new(ArrayTypeInfo {
            flags: type_flags::OBJECT,
            element_type,
            is_readonly,
        })))
    }

    /// Create a tuple type ([T, U, V]).
    pub fn create_tuple_type(
        &mut self,
        element_types: Vec<TypeId>,
        has_optional_elements: bool,
        has_rest_element: bool,
        is_readonly: bool,
    ) -> TypeId {
        // Generate default element flags: all required except last if has_rest_element
        let len = element_types.len();
        let element_flags: Vec<u32> = (0..len)
            .map(|i| {
                if has_rest_element && i == len - 1 {
                    element_flags::REST
                } else {
                    element_flags::REQUIRED
                }
            })
            .collect();

        self.alloc(Type::Tuple(Box::new(TupleTypeInfo {
            flags: type_flags::OBJECT,
            element_types,
            element_flags,
            element_names: None,
            has_optional_elements,
            has_rest_element,
            is_readonly,
        })))
    }

    /// Create a tuple type with explicit element flags (for variadic tuples).
    pub fn create_variadic_tuple_type(
        &mut self,
        element_types: Vec<TypeId>,
        element_flags: Vec<u32>,
        element_names: Option<Vec<Option<String>>>,
        is_readonly: bool,
    ) -> TypeId {
        let has_optional = element_flags
            .iter()
            .any(|&f| f & element_flags::OPTIONAL != 0);
        let has_rest = element_flags
            .iter()
            .any(|&f| f & (element_flags::REST | element_flags::VARIADIC) != 0);

        self.alloc(Type::Tuple(Box::new(TupleTypeInfo {
            flags: type_flags::OBJECT,
            element_types,
            element_flags,
            element_names,
            has_optional_elements: has_optional,
            has_rest_element: has_rest,
            is_readonly,
        })))
    }

    /// Create a named tuple type ([x: T, y: U]).
    pub fn create_named_tuple_type(
        &mut self,
        element_types: Vec<TypeId>,
        element_names: Vec<Option<String>>,
        has_optional_elements: bool,
        has_rest_element: bool,
        is_readonly: bool,
    ) -> TypeId {
        // Generate default element flags: all required except last if has_rest_element
        let len = element_types.len();
        let element_flags: Vec<u32> = (0..len)
            .map(|i| {
                if has_rest_element && i == len - 1 {
                    element_flags::REST
                } else {
                    element_flags::REQUIRED
                }
            })
            .collect();

        self.alloc(Type::Tuple(Box::new(TupleTypeInfo {
            flags: type_flags::OBJECT,
            element_types,
            element_flags,
            element_names: Some(element_names),
            has_optional_elements,
            has_rest_element,
            is_readonly,
        })))
    }

    /// Create an enum type.
    pub fn create_enum_type(&mut self, name: String, members: Vec<(String, TypeId)>) -> TypeId {
        self.alloc(Type::Enum(Box::new(EnumTypeInfo {
            flags: type_flags::ENUM,
            name,
            members,
        })))
    }

    /// Create a ThisType<T> marker type.
    /// ThisType<T> specifies the type of 'this' within object literal methods.
    pub fn create_this_type(&mut self, constraint: TypeId) -> TypeId {
        self.alloc(Type::ThisType(Box::new(ThisTypeMarker {
            flags: type_flags::OBJECT,
            constraint,
        })))
    }

    /// Create a unique symbol type.
    /// Each unique symbol declaration (const x: unique symbol = Symbol()) creates a distinct type.
    pub fn create_unique_symbol_type(&mut self, symbol: SymbolId, name: String) -> TypeId {
        self.alloc(Type::UniqueSymbol(Box::new(UniqueSymbolType {
            flags: type_flags::UNIQUE_ES_SYMBOL,
            symbol,
            name,
        })))
    }

    /// Create a conditional type (T extends U ? X : Y).
    /// If is_distributive is true, the conditional will distribute over union types
    /// when the check type is instantiated.
    pub fn create_conditional_type(
        &mut self,
        check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) -> TypeId {
        // A conditional type is distributive when check_type is a naked type parameter
        let is_distributive = self.is_naked_type_parameter(check_type);

        self.alloc(Type::Conditional(Box::new(ConditionalType {
            flags: type_flags::CONDITIONAL,
            check_type,
            extends_type,
            true_type,
            false_type,
            is_distributive,
            infer_type_parameters: Vec::new(),
        })))
    }

    /// Check if a type is a "naked" type parameter (just T, not keyof T or T[]).
    fn is_naked_type_parameter(&self, type_id: TypeId) -> bool {
        if let Some(Type::TypeParameter(_)) = self.get(type_id) {
            true
        } else {
            false
        }
    }

    /// Create a template literal type (`hello ${T}`).
    /// If all substitution types are string literals, evaluates to a single string literal.
    pub fn create_template_literal_type(
        &mut self,
        texts: Vec<String>,
        types: Vec<TypeId>,
    ) -> TypeId {
        // If no substitutions, just return a string literal of the first text
        if types.is_empty() {
            if texts.is_empty() {
                return self.create_string_literal(String::new());
            }
            return self.create_string_literal(texts[0].clone());
        }

        // Try to evaluate: if all substitution types are string literals, concat them
        let mut all_concrete = true;
        let mut string_values: Vec<Option<String>> = Vec::new();

        for type_id in &types {
            if let Some(typ) = self.get(*type_id) {
                match typ {
                    Type::Literal(LiteralType {
                        value: LiteralValue::String(s),
                        ..
                    }) => {
                        string_values.push(Some(s.clone()));
                    }
                    Type::Literal(LiteralType {
                        value: LiteralValue::Number(n),
                        ..
                    }) => {
                        // Numbers can be stringified
                        string_values.push(Some(n.to_string()));
                    }
                    Type::Literal(LiteralType {
                        value: LiteralValue::Boolean(b),
                        ..
                    }) => {
                        // Booleans can be stringified
                        string_values.push(Some(b.to_string()));
                    }
                    Type::Literal(LiteralType {
                        value: LiteralValue::BigInt(b),
                        ..
                    }) => {
                        // BigInt can be stringified
                        string_values.push(Some(b.clone()));
                    }
                    _ => {
                        all_concrete = false;
                        break;
                    }
                }
            } else {
                all_concrete = false;
                break;
            }
        }

        if all_concrete && string_values.len() == types.len() {
            // All types are concrete literals, evaluate the template
            let mut result = String::new();
            for (i, text) in texts.iter().enumerate() {
                result.push_str(text);
                if i < string_values.len() {
                    if let Some(s) = &string_values[i] {
                        result.push_str(s);
                    }
                }
            }
            return self.create_string_literal(result);
        }

        // Not all concrete, return unevaluated template literal type
        self.alloc(Type::TemplateLiteral(Box::new(TemplateLiteralType {
            flags: type_flags::TEMPLATE_LITERAL,
            texts,
            types,
        })))
    }

    /// Create a mapped type ({ [K in keyof T]: T[K] }).
    pub fn create_mapped_type(
        &mut self,
        declaration: NodeIndex,
        type_parameter: TypeId,
        constraint_type: TypeId,
        name_type: TypeId,
        template_type: TypeId,
    ) -> TypeId {
        self.create_mapped_type_with_modifiers(
            declaration,
            type_parameter,
            constraint_type,
            name_type,
            template_type,
            MappedTypeModifier::None,
            MappedTypeModifier::None,
        )
    }

    /// Create a mapped type with explicit readonly and optional modifiers.
    pub fn create_mapped_type_with_modifiers(
        &mut self,
        declaration: NodeIndex,
        type_parameter: TypeId,
        constraint_type: TypeId,
        name_type: TypeId,
        template_type: TypeId,
        readonly_modifier: MappedTypeModifier,
        optional_modifier: MappedTypeModifier,
    ) -> TypeId {
        self.alloc(Type::Mapped(Box::new(MappedType {
            flags: type_flags::OBJECT,
            object_flags: object_flags::MAPPED,
            declaration,
            type_parameter,
            constraint_type,
            name_type,
            template_type,
            readonly_modifier,
            optional_modifier,
        })))
    }

    /// Create an index type (keyof T).
    pub fn create_index_type(&mut self, source_type: TypeId) -> TypeId {
        self.alloc(Type::Index(Box::new(IndexType {
            flags: type_flags::INDEX,
            source_type,
        })))
    }

    /// Create an indexed access type (T[K]).
    pub fn create_indexed_access_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        self.alloc(Type::IndexedAccess(Box::new(IndexedAccessType {
            flags: type_flags::INDEXED_ACCESS,
            object_type,
            index_type,
            constraint: TypeId::NONE,
        })))
    }

    /// Create a function type.
    pub fn create_function_type(
        &mut self,
        declaration: NodeIndex,
        parameter_types: Vec<TypeId>,
        parameter_names: Vec<String>,
        return_type: TypeId,
        min_argument_count: u32,
        has_rest_parameter: bool,
    ) -> TypeId {
        self.alloc(Type::Function(Box::new(FunctionType {
            flags: type_flags::OBJECT,
            object_flags: object_flags::ANONYMOUS,
            declaration,
            parameter_types,
            parameter_names,
            return_type,
            type_parameters: Vec::new(),
            min_argument_count,
            has_rest_parameter,
            this_type: None,
        })))
    }

    /// Create a function type with type parameters.
    pub fn create_function_type_with_type_params(
        &mut self,
        declaration: NodeIndex,
        parameter_types: Vec<TypeId>,
        parameter_names: Vec<String>,
        return_type: TypeId,
        type_parameters: Vec<TypeId>,
        min_argument_count: u32,
        has_rest_parameter: bool,
    ) -> TypeId {
        self.alloc(Type::Function(Box::new(FunctionType {
            flags: type_flags::OBJECT,
            object_flags: object_flags::ANONYMOUS,
            declaration,
            parameter_types,
            parameter_names,
            return_type,
            type_parameters,
            min_argument_count,
            has_rest_parameter,
            this_type: None,
        })))
    }

    /// Create a function type with type parameters and explicit `this` type.
    pub fn create_function_type_with_this(
        &mut self,
        declaration: NodeIndex,
        parameter_types: Vec<TypeId>,
        parameter_names: Vec<String>,
        return_type: TypeId,
        type_parameters: Vec<TypeId>,
        min_argument_count: u32,
        has_rest_parameter: bool,
        this_type: Option<TypeId>,
    ) -> TypeId {
        self.alloc(Type::Function(Box::new(FunctionType {
            flags: type_flags::OBJECT,
            object_flags: object_flags::ANONYMOUS,
            declaration,
            parameter_types,
            parameter_names,
            return_type,
            type_parameters,
            min_argument_count,
            has_rest_parameter,
            this_type,
        })))
    }

    /// Create a type parameter.
    pub fn create_type_parameter(
        &mut self,
        symbol: SymbolId,
        constraint: TypeId,
        default: TypeId,
    ) -> TypeId {
        self.alloc(Type::TypeParameter(Box::new(TypeParameter {
            flags: type_flags::TYPE_PARAMETER,
            symbol,
            constraint,
            default,
            target: TypeId::NONE,
            is_this_type: false,
            is_const: false,
        })))
    }

    /// Create a const type parameter (TS 5.0+: `function foo<const T>()`).
    pub fn create_const_type_parameter(
        &mut self,
        symbol: SymbolId,
        constraint: TypeId,
        default: TypeId,
    ) -> TypeId {
        self.alloc(Type::TypeParameter(Box::new(TypeParameter {
            flags: type_flags::TYPE_PARAMETER,
            symbol,
            constraint,
            default,
            target: TypeId::NONE,
            is_this_type: false,
            is_const: true,
        })))
    }

    /// Create an object type with properties.
    pub fn create_object_type(&mut self, properties: Vec<SymbolId>) -> TypeId {
        let mut obj = ObjectType::new(object_flags::ANONYMOUS, SymbolId::NONE);
        obj.properties = properties;
        self.alloc(Type::Object(Box::new(obj)))
    }

    /// Create an anonymous object type with properties and members table.
    pub fn create_object_type_with_members(
        &mut self,
        properties: Vec<SymbolId>,
        members: SymbolTable,
    ) -> TypeId {
        let mut obj = ObjectType::new(object_flags::ANONYMOUS, SymbolId::NONE);
        obj.properties = properties;
        obj.members = members;
        self.alloc(Type::Object(Box::new(obj)))
    }

    /// Create a fresh object literal type with properties and members table.
    /// Fresh object literal types are subject to excess property checks.
    pub fn create_fresh_object_literal_type(
        &mut self,
        properties: Vec<SymbolId>,
        members: SymbolTable,
    ) -> TypeId {
        let mut obj = ObjectType::new(
            object_flags::ANONYMOUS | object_flags::OBJECT_LITERAL | object_flags::FRESH_LITERAL,
            SymbolId::NONE,
        );
        obj.properties = properties;
        obj.members = members;
        self.alloc(Type::Object(Box::new(obj)))
    }

    /// Create a class type with properties and construct signatures.
    pub fn create_class_type(
        &mut self,
        properties: Vec<SymbolId>,
        construct_signatures: Vec<Signature>,
        call_signatures: Vec<Signature>,
    ) -> TypeId {
        let mut obj = ObjectType::new(object_flags::CLASS, SymbolId::NONE);
        obj.properties = properties;
        obj.construct_signatures = construct_signatures;
        obj.call_signatures = call_signatures;
        self.alloc(Type::Object(Box::new(obj)))
    }

    /// Create an interface type with properties, signatures, and index infos.
    pub fn create_interface_type(
        &mut self,
        properties: Vec<SymbolId>,
        construct_signatures: Vec<Signature>,
        call_signatures: Vec<Signature>,
        index_infos: Vec<IndexInfo>,
    ) -> TypeId {
        let mut obj = ObjectType::new(object_flags::INTERFACE, SymbolId::NONE);
        obj.properties = properties;
        obj.construct_signatures = construct_signatures;
        obj.call_signatures = call_signatures;
        obj.index_infos = index_infos;
        self.alloc(Type::Object(Box::new(obj)))
    }

    /// Create a union type from a list of types.
    pub fn create_union_type(&mut self, types: Vec<TypeId>) -> TypeId {
        // Filter duplicates and flatten nested unions
        let mut flattened = Vec::new();
        for t in types {
            if let Some(Type::Union(u)) = self.get(t) {
                for &ut in &u.types {
                    if !flattened.contains(&ut) {
                        flattened.push(ut);
                    }
                }
            } else if !flattened.contains(&t) {
                flattened.push(t);
            }
        }

        // Handle edge cases
        if flattened.is_empty() {
            return self.never_type;
        }
        if flattened.len() == 1 {
            return flattened[0];
        }

        self.alloc(Type::Union(Box::new(UnionType::new(flattened))))
    }
}

impl Default for TypeArena {
    fn default() -> Self {
        Self::new()
    }
}
