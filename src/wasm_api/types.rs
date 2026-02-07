//! TypeScript Type and Symbol APIs
//!
//! Provides `TsType`, `TsSymbol`, and `TsSignature` structs.

use wasm_bindgen::prelude::*;

use crate::solver::TypeId;

/// Macro for handle-based type identity checks on TsType.
/// Each entry generates a `pub fn` that returns `self.handle == TypeId::X.0`.
macro_rules! define_type_handle_checks {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $type_id:expr);* $(;)?) => {
        #[wasm_bindgen]
        impl TsType {
            $(
                $(#[doc = $doc])*
                #[wasm_bindgen(js_name = $js_name)]
                pub fn $rust_name(&self) -> bool {
                    self.handle == $type_id.0
                }
            )*
        }
    };
}

/// Macro for flag-based checks on TsType.
/// Each entry generates a `pub fn` that returns `(self.flags & mask) != 0`.
macro_rules! define_type_flag_checks {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $mask:expr);* $(;)?) => {
        #[wasm_bindgen]
        impl TsType {
            $(
                $(#[doc = $doc])*
                #[wasm_bindgen(js_name = $js_name)]
                pub fn $rust_name(&self) -> bool {
                    (self.flags & ($mask)) != 0
                }
            )*
        }
    };
}

/// Macro for flag-based checks on TsSymbol.
/// Each entry generates a `pub fn` that returns `(self.flags & mask) != 0`.
macro_rules! define_symbol_flag_checks {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $mask:expr);* $(;)?) => {
        #[wasm_bindgen]
        impl TsSymbol {
            $(
                $(#[doc = $doc])*
                #[wasm_bindgen(js_name = $js_name)]
                pub fn $rust_name(&self) -> bool {
                    (self.flags & ($mask)) != 0
                }
            )*
        }
    };
}

/// Macro for `create_*_type` factory functions.
macro_rules! define_type_creators {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $type_id:expr, $flags:expr);* $(;)?) => {
        $(
            $(#[doc = $doc])*
            #[wasm_bindgen(js_name = $js_name)]
            pub fn $rust_name() -> TsType {
                TsType::new($type_id.0, $flags)
            }
        )*
    };
}

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

    /// Check if this type is a class or interface
    #[wasm_bindgen(js_name = isClassOrInterface)]
    pub fn is_class_or_interface(&self) -> bool {
        self.is_object() // Simplified - would check ObjectFlags
    }
}

define_type_handle_checks! {
    /// Check if this is `any` type
    "isAny", is_any => TypeId::ANY;
    /// Check if this is `unknown` type
    "isUnknown", is_unknown => TypeId::UNKNOWN;
    /// Check if this is `string` type
    "isString", is_string => TypeId::STRING;
    /// Check if this is `number` type
    "isNumber", is_number => TypeId::NUMBER;
    /// Check if this is `boolean` type
    "isBoolean", is_boolean => TypeId::BOOLEAN;
    /// Check if this is `void` type
    "isVoid", is_void => TypeId::VOID;
    /// Check if this is `undefined` type
    "isUndefined", is_undefined => TypeId::UNDEFINED;
    /// Check if this is `null` type
    "isNull", is_null => TypeId::NULL;
    /// Check if this is `never` type
    "isNever", is_never => TypeId::NEVER;
}

define_type_flag_checks! {
    /// Check if this is a union type
    "isUnion", is_union => 1 << 20;           // TypeFlags.Union
    /// Check if this is an intersection type
    "isIntersection", is_intersection => 1 << 21; // TypeFlags.Intersection
    /// Check if this is a string literal type
    "isStringLiteral", is_string_literal => 1 << 7;  // TypeFlags.StringLiteral
    /// Check if this is a number literal type
    "isNumberLiteral", is_number_literal => 1 << 8;  // TypeFlags.NumberLiteral
    /// Check if this is a type parameter
    "isTypeParameter", is_type_parameter => 1 << 18; // TypeFlags.TypeParameter
    /// Check if this is an object type
    "isObject", is_object => 1 << 19;         // TypeFlags.Object
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
}

define_symbol_flag_checks! {
    /// Check if this is a variable symbol
    "isVariable", is_variable => 0b11;                  // FunctionScopedVariable | BlockScopedVariable
    /// Check if this is a property symbol
    "isProperty", is_property => 1 << 2;                // SymbolFlags.Property
    /// Check if this is a function symbol
    "isFunction", is_function => 1 << 4;                // SymbolFlags.Function
    /// Check if this is a class symbol
    "isClass", is_class => 1 << 5;                      // SymbolFlags.Class
    /// Check if this is an interface symbol
    "isInterface", is_interface => 1 << 6;              // SymbolFlags.Interface
    /// Check if this is an enum symbol
    "isEnum", is_enum => (1 << 7) | (1 << 8);          // ConstEnum | RegularEnum
    /// Check if this is a method symbol
    "isMethod", is_method => 1 << 13;                   // SymbolFlags.Method
    /// Check if this is a type parameter symbol
    "isTypeParameter", is_type_parameter => 1 << 18;   // SymbolFlags.TypeParameter
    /// Check if this is a type alias symbol
    "isTypeAlias", is_type_alias => 1 << 19;            // SymbolFlags.TypeAlias
    /// Check if this is an alias (import) symbol
    "isAlias", is_alias => 1 << 21;                     // SymbolFlags.Alias
    /// Check if this symbol is optional
    "isOptional", is_optional => 1 << 24;               // SymbolFlags.Optional
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

define_type_creators! {
    /// Create the `any` type
    "createAnyType", create_any_type => TypeId::ANY, 1;             // TypeFlags.Any
    /// Create the `unknown` type
    "createUnknownType", create_unknown_type => TypeId::UNKNOWN, 2; // TypeFlags.Unknown
    /// Create the `string` type
    "createStringType", create_string_type => TypeId::STRING, 4;    // TypeFlags.String
    /// Create the `number` type
    "createNumberType", create_number_type => TypeId::NUMBER, 8;    // TypeFlags.Number
    /// Create the `boolean` type
    "createBooleanType", create_boolean_type => TypeId::BOOLEAN, 16; // TypeFlags.Boolean
    /// Create the `void` type
    "createVoidType", create_void_type => TypeId::VOID, 16384;     // TypeFlags.Void
    /// Create the `undefined` type
    "createUndefinedType", create_undefined_type => TypeId::UNDEFINED, 32768; // TypeFlags.Undefined
    /// Create the `null` type
    "createNullType", create_null_type => TypeId::NULL, 65536;     // TypeFlags.Null
    /// Create the `never` type
    "createNeverType", create_never_type => TypeId::NEVER, 131072; // TypeFlags.Never
}
