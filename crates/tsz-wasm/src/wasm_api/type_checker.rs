//! TypeScript TypeChecker API
//!
//! Provides the `TsTypeChecker` struct which implements TypeScript's TypeChecker interface.

use std::sync::Arc;
use wasm_bindgen::prelude::*;

use tsz::checker::context::CheckerOptions;
use tsz::lib_loader::LibFile;
use tsz::parallel::MergedProgram;
use tsz::solver::{TypeFormatter, TypeId, TypeInterner};

use super::enums::SignatureKind;
use super::program::TsCompilerOptions;

/// TypeScript TypeChecker - provides type information
///
/// The type checker is the primary interface for querying type information
/// from a program. It provides methods like:
/// - `getTypeAtLocation(node)` - Get the type of an AST node
/// - `getSymbolAtLocation(node)` - Get the symbol for an identifier
/// - `typeToString(type)` - Format a type for display
/// - `isTypeAssignableTo(source, target)` - Check assignability
#[wasm_bindgen]
pub struct TsTypeChecker {
    /// Reference to program's merged state
    /// Note: We use indices/IDs rather than holding full references
    /// to avoid complex lifetime issues with wasm-bindgen
    #[allow(dead_code)]
    program_id: u32,
    /// Type interner pointer (borrowed from program)
    /// SAFETY: The TsTypeChecker is always created from TsProgram and
    /// must not outlive the program that created it
    interner_ptr: *const TypeInterner,
    /// Checker options
    #[allow(dead_code)]
    options: CheckerOptions,
    /// Lib file names for global type resolution
    #[allow(dead_code)]
    lib_file_names: Vec<String>,
}

#[wasm_bindgen]
impl TsTypeChecker {
    /// Get the type at a specific AST node location
    ///
    /// # Arguments
    /// * `node_handle` - Handle (index) of the AST node
    ///
    /// # Returns
    /// Handle (ID) of the type, or 0 for error type
    #[wasm_bindgen(js_name = getTypeAtLocation)]
    pub fn get_type_at_location(&self, _node_handle: u32) -> u32 {
        // In a full implementation, we'd:
        // 1. Look up the node from the handle
        // 2. Run type checking on demand
        // 3. Return the type ID

        // For now, return a placeholder
        TypeId::ANY.0
    }

    /// Get the symbol at a specific AST node location
    ///
    /// # Arguments
    /// * `node_handle` - Handle (index) of the AST node
    ///
    /// # Returns
    /// Handle (ID) of the symbol, or u32::MAX if none
    #[wasm_bindgen(js_name = getSymbolAtLocation)]
    pub fn get_symbol_at_location(&self, _node_handle: u32) -> u32 {
        // In a full implementation, we'd look up the symbol
        u32::MAX
    }

    /// Get the declared type of a symbol
    #[wasm_bindgen(js_name = getDeclaredTypeOfSymbol)]
    pub fn get_declared_type_of_symbol(&self, _symbol_handle: u32) -> u32 {
        TypeId::ANY.0
    }

    /// Get the type of a symbol
    #[wasm_bindgen(js_name = getTypeOfSymbol)]
    pub fn get_type_of_symbol(&self, _symbol_handle: u32) -> u32 {
        TypeId::ANY.0
    }

    /// Format a type as a string
    #[wasm_bindgen(js_name = typeToString)]
    pub fn type_to_string(&self, type_handle: u32) -> String {
        let type_id = TypeId(type_handle);

        // SAFETY: interner_ptr is valid for the lifetime of TsTypeChecker
        // which is tied to the TsProgram that created it
        if self.interner_ptr.is_null() {
            return self.format_basic_type(type_id);
        }

        #[allow(unsafe_code)]
        let interner = unsafe { &*self.interner_ptr };
        let mut formatter = TypeFormatter::new(interner);
        formatter.format(type_id)
    }

    /// Fallback type formatting for basic/intrinsic types
    fn format_basic_type(&self, type_id: TypeId) -> String {
        match type_id {
            t if t == TypeId::ANY => "any".to_string(),
            t if t == TypeId::UNKNOWN => "unknown".to_string(),
            t if t == TypeId::STRING => "string".to_string(),
            t if t == TypeId::NUMBER => "number".to_string(),
            t if t == TypeId::BOOLEAN => "boolean".to_string(),
            t if t == TypeId::VOID => "void".to_string(),
            t if t == TypeId::UNDEFINED => "undefined".to_string(),
            t if t == TypeId::NULL => "null".to_string(),
            t if t == TypeId::NEVER => "never".to_string(),
            t if t == TypeId::OBJECT => "object".to_string(),
            t if t == TypeId::SYMBOL => "symbol".to_string(),
            t if t == TypeId::BIGINT => "bigint".to_string(),
            _ => format!("Type({})", type_id.0),
        }
    }

    /// Format a symbol as a string
    #[wasm_bindgen(js_name = symbolToString)]
    pub fn symbol_to_string(&self, _symbol_handle: u32) -> String {
        "symbol".to_string()
    }

    /// Get the fully qualified name of a symbol
    #[wasm_bindgen(js_name = getFullyQualifiedName)]
    pub fn get_fully_qualified_name(&self, _symbol_handle: u32) -> String {
        "".to_string()
    }

    /// Check if source type is assignable to target type
    #[wasm_bindgen(js_name = isTypeAssignableTo)]
    pub fn is_type_assignable_to(&self, source: u32, target: u32) -> bool {
        // In a full implementation, check assignability
        source == target || target == TypeId::ANY.0
    }

    /// Get properties of a type
    ///
    /// Returns handles to symbol objects
    #[wasm_bindgen(js_name = getPropertiesOfType)]
    pub fn get_properties_of_type(&self, _type_handle: u32) -> Vec<u32> {
        Vec::new()
    }

    /// Get a specific property of a type by name
    #[wasm_bindgen(js_name = getPropertyOfType)]
    pub fn get_property_of_type(&self, _type_handle: u32, _property_name: &str) -> Option<u32> {
        None
    }

    /// Get call signatures of a type
    #[wasm_bindgen(js_name = getSignaturesOfType)]
    pub fn get_signatures_of_type(&self, _type_handle: u32, _kind: SignatureKind) -> Vec<u32> {
        Vec::new()
    }

    /// Get return type of a signature
    #[wasm_bindgen(js_name = getReturnTypeOfSignature)]
    pub fn get_return_type_of_signature(&self, _signature_handle: u32) -> u32 {
        TypeId::ANY.0
    }

    /// Get base types (for classes/interfaces)
    #[wasm_bindgen(js_name = getBaseTypes)]
    pub fn get_base_types(&self, _type_handle: u32) -> Vec<u32> {
        Vec::new()
    }

    /// Get the apparent type (handles widening, etc.)
    #[wasm_bindgen(js_name = getApparentType)]
    pub fn get_apparent_type(&self, type_handle: u32) -> u32 {
        type_handle
    }

    /// Get type flags
    #[wasm_bindgen(js_name = getTypeFlags)]
    pub fn get_type_flags(&self, type_handle: u32) -> u32 {
        // Return appropriate flags based on type
        match TypeId(type_handle) {
            t if t == TypeId::ANY => 1,           // TypeFlags.Any
            t if t == TypeId::UNKNOWN => 2,       // TypeFlags.Unknown
            t if t == TypeId::STRING => 4,        // TypeFlags.String
            t if t == TypeId::NUMBER => 8,        // TypeFlags.Number
            t if t == TypeId::BOOLEAN => 16,      // TypeFlags.Boolean
            t if t == TypeId::VOID => 16384,      // TypeFlags.Void
            t if t == TypeId::UNDEFINED => 32768, // TypeFlags.Undefined
            t if t == TypeId::NULL => 65536,      // TypeFlags.Null
            t if t == TypeId::NEVER => 131072,    // TypeFlags.Never
            _ => 0,
        }
    }

    /// Get symbol flags
    #[wasm_bindgen(js_name = getSymbolFlags)]
    pub fn get_symbol_flags(&self, _symbol_handle: u32) -> u32 {
        0
    }

    // === Type predicates ===

    /// Check if type is a union type
    #[wasm_bindgen(js_name = isUnionType)]
    pub fn is_union_type(&self, _type_handle: u32) -> bool {
        false // Would check TypeFlags.Union
    }

    /// Check if type is an intersection type
    #[wasm_bindgen(js_name = isIntersectionType)]
    pub fn is_intersection_type(&self, _type_handle: u32) -> bool {
        false // Would check TypeFlags.Intersection
    }

    /// Check if type is a type parameter
    #[wasm_bindgen(js_name = isTypeParameter)]
    pub fn is_type_parameter(&self, _type_handle: u32) -> bool {
        false // Would check TypeFlags.TypeParameter
    }

    /// Check if type is an array type
    #[wasm_bindgen(js_name = isArrayType)]
    pub fn is_array_type(&self, _type_handle: u32) -> bool {
        false
    }

    /// Check if type is a tuple type
    #[wasm_bindgen(js_name = isTupleType)]
    pub fn is_tuple_type(&self, _type_handle: u32) -> bool {
        false
    }

    /// Check if type is nullable (includes null or undefined)
    #[wasm_bindgen(js_name = isNullableType)]
    pub fn is_nullable_type(&self, type_handle: u32) -> bool {
        let id = TypeId(type_handle);
        id == TypeId::NULL || id == TypeId::UNDEFINED
    }
}

/// Macro for intrinsic type ID getters on TsTypeChecker.
/// Each entry generates a `pub fn` that returns `TypeId::X.0`.
macro_rules! define_checker_type_getters {
    ($($(#[doc = $doc:expr])* $js_name:literal, $rust_name:ident => $type_id:expr);* $(;)?) => {
        #[wasm_bindgen]
        impl TsTypeChecker {
            $(
                $(#[doc = $doc])*
                #[wasm_bindgen(js_name = $js_name)]
                pub fn $rust_name(&self) -> u32 {
                    $type_id.0
                }
            )*
        }
    };
}

define_checker_type_getters! {
    /// Get the `any` type
    "getAnyType", get_any_type => TypeId::ANY;
    /// Get the `unknown` type
    "getUnknownType", get_unknown_type => TypeId::UNKNOWN;
    /// Get the `string` type
    "getStringType", get_string_type => TypeId::STRING;
    /// Get the `number` type
    "getNumberType", get_number_type => TypeId::NUMBER;
    /// Get the `boolean` type
    "getBooleanType", get_boolean_type => TypeId::BOOLEAN;
    /// Get the `void` type
    "getVoidType", get_void_type => TypeId::VOID;
    /// Get the `undefined` type
    "getUndefinedType", get_undefined_type => TypeId::UNDEFINED;
    /// Get the `null` type
    "getNullType", get_null_type => TypeId::NULL;
    /// Get the `never` type
    "getNeverType", get_never_type => TypeId::NEVER;
    /// Get the `true` literal type
    "getTrueType", get_true_type => TypeId::BOOLEAN_TRUE;
    /// Get the `false` literal type
    "getFalseType", get_false_type => TypeId::BOOLEAN_FALSE;
}

impl TsTypeChecker {
    /// Create a new type checker for a program
    pub(crate) fn new(
        _merged: &MergedProgram,
        interner: &TypeInterner,
        options: &TsCompilerOptions,
        lib_files: &[Arc<LibFile>],
    ) -> Self {
        TsTypeChecker {
            program_id: 0,
            interner_ptr: interner as *const TypeInterner,
            options: options.to_checker_options(),
            lib_file_names: lib_files.iter().map(|f| f.file_name.clone()).collect(),
        }
    }
}
