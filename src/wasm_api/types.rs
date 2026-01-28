//! TypeScript Type and Symbol APIs
//!
//! Provides `TsType`, `TsSymbol`, and `TsSignature` structs.

use wasm_bindgen::prelude::*;

use crate::solver::TypeId;

/// TypeScript Type - represents a type in the type system
///
/// Types are identified by handles (TypeId) and have:
/// - flags (TypeFlags bits)
/// - optional symbol
/// - various type-specific properties
#[wasm_bindgen]
pub struct TsType {
    /// Type handle (TypeId)
    handle: u32,
    /// Type flags
    flags: u32,
}

#[wasm_bindgen]
impl TsType {
    /// Create a new type wrapper
    #[wasm_bindgen(constructor)]
    pub fn new(handle: u32, flags: u32) -> TsType {
        TsType { handle, flags }
    }

    /// Get the type handle
    #[wasm_bindgen(getter)]
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// Get type flags
    #[wasm_bindgen(getter)]
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Check if this is `any` type
    #[wasm_bindgen(js_name = isAny)]
    pub fn is_any(&self) -> bool {
        self.handle == TypeId::ANY.0
    }

    /// Check if this is `unknown` type
    #[wasm_bindgen(js_name = isUnknown)]
    pub fn is_unknown(&self) -> bool {
        self.handle == TypeId::UNKNOWN.0
    }

    /// Check if this is `string` type
    #[wasm_bindgen(js_name = isString)]
    pub fn is_string(&self) -> bool {
        self.handle == TypeId::STRING.0
    }

    /// Check if this is `number` type
    #[wasm_bindgen(js_name = isNumber)]
    pub fn is_number(&self) -> bool {
        self.handle == TypeId::NUMBER.0
    }

    /// Check if this is `boolean` type
    #[wasm_bindgen(js_name = isBoolean)]
    pub fn is_boolean(&self) -> bool {
        self.handle == TypeId::BOOLEAN.0
    }

    /// Check if this is `void` type
    #[wasm_bindgen(js_name = isVoid)]
    pub fn is_void(&self) -> bool {
        self.handle == TypeId::VOID.0
    }

    /// Check if this is `undefined` type
    #[wasm_bindgen(js_name = isUndefined)]
    pub fn is_undefined(&self) -> bool {
        self.handle == TypeId::UNDEFINED.0
    }

    /// Check if this is `null` type
    #[wasm_bindgen(js_name = isNull)]
    pub fn is_null(&self) -> bool {
        self.handle == TypeId::NULL.0
    }

    /// Check if this is `never` type
    #[wasm_bindgen(js_name = isNever)]
    pub fn is_never(&self) -> bool {
        self.handle == TypeId::NEVER.0
    }

    /// Check if this is a union type
    #[wasm_bindgen(js_name = isUnion)]
    pub fn is_union(&self) -> bool {
        (self.flags & (1 << 20)) != 0 // TypeFlags.Union
    }

    /// Check if this is an intersection type
    #[wasm_bindgen(js_name = isIntersection)]
    pub fn is_intersection(&self) -> bool {
        (self.flags & (1 << 21)) != 0 // TypeFlags.Intersection
    }

    /// Check if this is a union or intersection
    #[wasm_bindgen(js_name = isUnionOrIntersection)]
    pub fn is_union_or_intersection(&self) -> bool {
        self.is_union() || self.is_intersection()
    }

    /// Check if this is a literal type
    #[wasm_bindgen(js_name = isLiteral)]
    pub fn is_literal(&self) -> bool {
        let literal_flags = (1 << 7) | (1 << 8) | (1 << 9) | (1 << 11); // String/Number/Boolean/BigInt Literal
        (self.flags & literal_flags) != 0
    }

    /// Check if this is a string literal type
    #[wasm_bindgen(js_name = isStringLiteral)]
    pub fn is_string_literal(&self) -> bool {
        (self.flags & (1 << 7)) != 0 // TypeFlags.StringLiteral
    }

    /// Check if this is a number literal type
    #[wasm_bindgen(js_name = isNumberLiteral)]
    pub fn is_number_literal(&self) -> bool {
        (self.flags & (1 << 8)) != 0 // TypeFlags.NumberLiteral
    }

    /// Check if this is a type parameter
    #[wasm_bindgen(js_name = isTypeParameter)]
    pub fn is_type_parameter(&self) -> bool {
        (self.flags & (1 << 18)) != 0 // TypeFlags.TypeParameter
    }

    /// Check if this is an object type
    #[wasm_bindgen(js_name = isObject)]
    pub fn is_object(&self) -> bool {
        (self.flags & (1 << 19)) != 0 // TypeFlags.Object
    }

    /// Check if this type is a class or interface
    #[wasm_bindgen(js_name = isClassOrInterface)]
    pub fn is_class_or_interface(&self) -> bool {
        self.is_object() // Simplified - would check ObjectFlags
    }
}

/// TypeScript Symbol - represents a named entity
///
/// Symbols have:
/// - name (escaped name for identifiers)
/// - flags (SymbolFlags bits)
/// - declarations (AST nodes where declared)
/// - value declaration (primary declaration)
#[wasm_bindgen]
pub struct TsSymbol {
    /// Symbol handle
    handle: u32,
    /// Symbol flags
    flags: u32,
    /// Symbol name
    name: String,
}

#[wasm_bindgen]
impl TsSymbol {
    /// Create a new symbol wrapper
    #[wasm_bindgen(constructor)]
    pub fn new(handle: u32, flags: u32, name: String) -> TsSymbol {
        TsSymbol {
            handle,
            flags,
            name,
        }
    }

    /// Get the symbol handle
    #[wasm_bindgen(getter)]
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// Get symbol flags
    #[wasm_bindgen(getter)]
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Get symbol name
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Get escaped name (same as name for most identifiers)
    #[wasm_bindgen(getter, js_name = escapedName)]
    pub fn escaped_name(&self) -> String {
        self.name.clone()
    }

    /// Check if this is a variable symbol
    #[wasm_bindgen(js_name = isVariable)]
    pub fn is_variable(&self) -> bool {
        (self.flags & 0b11) != 0 // FunctionScopedVariable | BlockScopedVariable
    }

    /// Check if this is a property symbol
    #[wasm_bindgen(js_name = isProperty)]
    pub fn is_property(&self) -> bool {
        (self.flags & (1 << 2)) != 0 // SymbolFlags.Property
    }

    /// Check if this is a function symbol
    #[wasm_bindgen(js_name = isFunction)]
    pub fn is_function(&self) -> bool {
        (self.flags & (1 << 4)) != 0 // SymbolFlags.Function
    }

    /// Check if this is a class symbol
    #[wasm_bindgen(js_name = isClass)]
    pub fn is_class(&self) -> bool {
        (self.flags & (1 << 5)) != 0 // SymbolFlags.Class
    }

    /// Check if this is an interface symbol
    #[wasm_bindgen(js_name = isInterface)]
    pub fn is_interface(&self) -> bool {
        (self.flags & (1 << 6)) != 0 // SymbolFlags.Interface
    }

    /// Check if this is an enum symbol
    #[wasm_bindgen(js_name = isEnum)]
    pub fn is_enum(&self) -> bool {
        (self.flags & ((1 << 7) | (1 << 8))) != 0 // ConstEnum | RegularEnum
    }

    /// Check if this is a method symbol
    #[wasm_bindgen(js_name = isMethod)]
    pub fn is_method(&self) -> bool {
        (self.flags & (1 << 13)) != 0 // SymbolFlags.Method
    }

    /// Check if this is a type parameter symbol
    #[wasm_bindgen(js_name = isTypeParameter)]
    pub fn is_type_parameter(&self) -> bool {
        (self.flags & (1 << 18)) != 0 // SymbolFlags.TypeParameter
    }

    /// Check if this is a type alias symbol
    #[wasm_bindgen(js_name = isTypeAlias)]
    pub fn is_type_alias(&self) -> bool {
        (self.flags & (1 << 19)) != 0 // SymbolFlags.TypeAlias
    }

    /// Check if this is an alias (import) symbol
    #[wasm_bindgen(js_name = isAlias)]
    pub fn is_alias(&self) -> bool {
        (self.flags & (1 << 21)) != 0 // SymbolFlags.Alias
    }

    /// Check if this symbol is optional
    #[wasm_bindgen(js_name = isOptional)]
    pub fn is_optional(&self) -> bool {
        (self.flags & (1 << 24)) != 0 // SymbolFlags.Optional
    }
}

/// TypeScript Signature - represents a call/construct signature
///
/// Signatures have:
/// - parameters
/// - return type
/// - type parameters (for generic signatures)
#[wasm_bindgen]
pub struct TsSignature {
    /// Signature handle
    handle: u32,
    /// Declaration node handle (if any)
    declaration_handle: Option<u32>,
}

#[wasm_bindgen]
impl TsSignature {
    /// Create a new signature wrapper
    #[wasm_bindgen(constructor)]
    pub fn new(handle: u32) -> TsSignature {
        TsSignature {
            handle,
            declaration_handle: None,
        }
    }

    /// Get the signature handle
    #[wasm_bindgen(getter)]
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// Get declaration node handle
    #[wasm_bindgen(js_name = getDeclarationHandle)]
    pub fn get_declaration_handle(&self) -> Option<u32> {
        self.declaration_handle
    }

    /// Get type parameter handles
    #[wasm_bindgen(js_name = getTypeParameterHandles)]
    pub fn get_type_parameter_handles(&self) -> Vec<u32> {
        Vec::new() // Would query from signature data
    }

    /// Get parameter symbol handles
    #[wasm_bindgen(js_name = getParameterHandles)]
    pub fn get_parameter_handles(&self) -> Vec<u32> {
        Vec::new() // Would query from signature data
    }
}

/// Create the `any` type
#[wasm_bindgen(js_name = createAnyType)]
pub fn create_any_type() -> TsType {
    TsType::new(TypeId::ANY.0, 1) // TypeFlags.Any
}

/// Create the `unknown` type
#[wasm_bindgen(js_name = createUnknownType)]
pub fn create_unknown_type() -> TsType {
    TsType::new(TypeId::UNKNOWN.0, 2) // TypeFlags.Unknown
}

/// Create the `string` type
#[wasm_bindgen(js_name = createStringType)]
pub fn create_string_type() -> TsType {
    TsType::new(TypeId::STRING.0, 4) // TypeFlags.String
}

/// Create the `number` type
#[wasm_bindgen(js_name = createNumberType)]
pub fn create_number_type() -> TsType {
    TsType::new(TypeId::NUMBER.0, 8) // TypeFlags.Number
}

/// Create the `boolean` type
#[wasm_bindgen(js_name = createBooleanType)]
pub fn create_boolean_type() -> TsType {
    TsType::new(TypeId::BOOLEAN.0, 16) // TypeFlags.Boolean
}

/// Create the `void` type
#[wasm_bindgen(js_name = createVoidType)]
pub fn create_void_type() -> TsType {
    TsType::new(TypeId::VOID.0, 16384) // TypeFlags.Void
}

/// Create the `undefined` type
#[wasm_bindgen(js_name = createUndefinedType)]
pub fn create_undefined_type() -> TsType {
    TsType::new(TypeId::UNDEFINED.0, 32768) // TypeFlags.Undefined
}

/// Create the `null` type
#[wasm_bindgen(js_name = createNullType)]
pub fn create_null_type() -> TsType {
    TsType::new(TypeId::NULL.0, 65536) // TypeFlags.Null
}

/// Create the `never` type
#[wasm_bindgen(js_name = createNeverType)]
pub fn create_never_type() -> TsType {
    TsType::new(TypeId::NEVER.0, 131072) // TypeFlags.Never
}
