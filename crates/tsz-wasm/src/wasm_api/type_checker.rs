//! TypeScript `TypeChecker` API
//!
//! Provides the `TsTypeChecker` struct which implements TypeScript's `TypeChecker` interface.

use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;

use tsz::lib_loader::LibFile;
use tsz::parallel::MergedProgram;
use tsz_solver::{
    TypeFormatter, TypeId, TypeInterner, is_array_type, is_intersection_type, is_nullable_type,
    is_tuple_type, is_type_parameter, is_union_type, type_id_ts_flags,
};

use super::enums::SignatureKind;
use super::program::TsCompilerOptions;

/// TypeScript `TypeChecker` - provides type information
///
/// The type checker is the primary interface for querying type information
/// from a program. It provides methods like:
/// - `getTypeAtLocation(node)` - Get the type of an AST node
/// - `getSymbolAtLocation(node)` - Get the symbol for an identifier
/// - `typeToString(type)` - Format a type for display
/// - `isTypeAssignableTo(source, target)` - Check assignability
#[wasm_bindgen]
pub struct TsTypeChecker {
    /// Shared type interner.
    interner: Arc<TypeInterner>,
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
    /// Handle (ID) of the symbol, or `u32::MAX` if none
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

        let interner = &*self.interner;
        let mut formatter = TypeFormatter::new(interner);
        formatter.format(type_id).into_owned()
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
    ///
    /// Returns a bitmask matching TypeScript's public `TypeFlags` enum so JS
    /// callers can reason about the structural family of a type even when it
    /// is not one of the well-known intrinsic ids.
    #[wasm_bindgen(js_name = getTypeFlags)]
    pub fn get_type_flags(&self, type_handle: u32) -> u32 {
        type_id_ts_flags(&*self.interner, TypeId(type_handle))
    }

    /// Get symbol flags
    #[wasm_bindgen(js_name = getSymbolFlags)]
    pub fn get_symbol_flags(&self, _symbol_handle: u32) -> u32 {
        0
    }

    // === Type predicates ===

    /// Check if type is a union type
    #[wasm_bindgen(js_name = isUnionType)]
    pub fn is_union_type(&self, type_handle: u32) -> bool {
        is_union_type(&*self.interner, TypeId(type_handle))
    }

    /// Check if type is an intersection type
    #[wasm_bindgen(js_name = isIntersectionType)]
    pub fn is_intersection_type(&self, type_handle: u32) -> bool {
        is_intersection_type(&*self.interner, TypeId(type_handle))
    }

    /// Check if type is a type parameter
    #[wasm_bindgen(js_name = isTypeParameter)]
    pub fn is_type_parameter(&self, type_handle: u32) -> bool {
        is_type_parameter(&*self.interner, TypeId(type_handle))
    }

    /// Check if type is an array type
    #[wasm_bindgen(js_name = isArrayType)]
    pub fn is_array_type(&self, type_handle: u32) -> bool {
        is_array_type(&*self.interner, TypeId(type_handle))
    }

    /// Check if type is a tuple type
    #[wasm_bindgen(js_name = isTupleType)]
    pub fn is_tuple_type(&self, type_handle: u32) -> bool {
        is_tuple_type(&*self.interner, TypeId(type_handle))
    }

    /// Check if type is nullable (includes null or undefined, including
    /// unions whose members include null or undefined).
    #[wasm_bindgen(js_name = isNullableType)]
    pub fn is_nullable_type(&self, type_handle: u32) -> bool {
        is_nullable_type(&*self.interner, TypeId(type_handle))
    }
}

/// Macro for intrinsic type ID getters on `TsTypeChecker`.
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
        interner: Arc<TypeInterner>,
        _options: &TsCompilerOptions,
        _lib_files: &[Arc<LibFile>],
    ) -> Self {
        Self { interner }
    }

    /// Test-only constructor that wraps a bare `TypeInterner`.
    ///
    /// Production code goes through `Self::new` (which threads the merged
    /// program / lib files); this helper exists so unit tests can build a
    /// minimal checker from synthesized types without standing up a full
    /// program.
    #[cfg(test)]
    pub(crate) fn from_interner_for_test(interner: Arc<TypeInterner>) -> Self {
        Self { interner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TypeParamInfo;
    use tsz_solver::ts_type_flags::flags as tf;

    fn checker() -> (TsTypeChecker, Arc<TypeInterner>) {
        let interner = Arc::new(TypeInterner::new());
        let checker = TsTypeChecker::from_interner_for_test(interner.clone());
        (checker, interner)
    }

    // === Predicate parity (regression for #4742) ===

    #[test]
    fn is_union_type_returns_true_for_union_and_false_for_intrinsics() {
        let (checker, db) = checker();
        let union = db.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(checker.is_union_type(union.0));
        assert!(!checker.is_union_type(TypeId::STRING.0));
        assert!(!checker.is_union_type(TypeId::ANY.0));
    }

    #[test]
    fn is_intersection_type_returns_true_for_raw_intersection() {
        let (checker, db) = checker();
        // Two distinct type parameters intersect to a `TypeData::Intersection`
        // even after dedup/normalization, since they have different `TypeId`s
        // and no disjointness rule applies.
        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let u = db.type_param(TypeParamInfo {
            name: db.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let intersection = db.intersect_types_raw2(t, u);
        assert!(checker.is_intersection_type(intersection.0));
        assert!(!checker.is_intersection_type(t.0));
        assert!(!checker.is_intersection_type(TypeId::STRING.0));
    }

    #[test]
    fn is_array_type_returns_true_only_for_array_type() {
        let (checker, db) = checker();
        let array = db.array(TypeId::STRING);
        assert!(checker.is_array_type(array.0));
        assert!(!checker.is_array_type(TypeId::STRING.0));
        assert!(!checker.is_array_type(TypeId::ANY.0));
    }

    #[test]
    fn is_tuple_type_returns_true_for_tuple_and_readonly_tuple() {
        let (checker, db) = checker();
        let tuple = db.tuple(vec![tsz_solver::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert!(checker.is_tuple_type(tuple.0));
        let readonly_tuple = db.readonly_type(tuple);
        assert!(checker.is_tuple_type(readonly_tuple.0));
        assert!(!checker.is_tuple_type(TypeId::STRING.0));
    }

    #[test]
    fn is_type_parameter_recognizes_both_regular_and_alternate_names() {
        // Anti-hardcoding: the predicate must hold for any user-chosen
        // type-parameter name, so exercise two distinct names.
        let (checker, db) = checker();
        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let k = db.type_param(TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        });
        assert!(checker.is_type_parameter(t.0));
        assert!(checker.is_type_parameter(k.0));
        assert!(!checker.is_type_parameter(TypeId::STRING.0));
    }

    // === isNullableType: union-aware (regression for #4742) ===

    #[test]
    fn is_nullable_type_handles_bare_null_and_undefined() {
        let (checker, _) = checker();
        assert!(checker.is_nullable_type(TypeId::NULL.0));
        assert!(checker.is_nullable_type(TypeId::UNDEFINED.0));
        assert!(!checker.is_nullable_type(TypeId::STRING.0));
        assert!(!checker.is_nullable_type(TypeId::ANY.0));
    }

    #[test]
    fn is_nullable_type_recognizes_union_with_null_or_undefined() {
        let (checker, db) = checker();
        let str_or_null = db.union2(TypeId::STRING, TypeId::NULL);
        let num_or_undef = db.union2(TypeId::NUMBER, TypeId::UNDEFINED);
        let str_or_num = db.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(checker.is_nullable_type(str_or_null.0));
        assert!(checker.is_nullable_type(num_or_undef.0));
        assert!(!checker.is_nullable_type(str_or_num.0));
    }

    // === getTypeFlags: structural mapping (regression for #4742) ===

    #[test]
    fn get_type_flags_maps_intrinsics_to_typescript_bits() {
        let (checker, _) = checker();
        assert_eq!(checker.get_type_flags(TypeId::ANY.0), tf::ANY);
        assert_eq!(checker.get_type_flags(TypeId::UNKNOWN.0), tf::UNKNOWN);
        assert_eq!(checker.get_type_flags(TypeId::STRING.0), tf::STRING);
        assert_eq!(checker.get_type_flags(TypeId::NUMBER.0), tf::NUMBER);
        assert_eq!(checker.get_type_flags(TypeId::BOOLEAN.0), tf::BOOLEAN);
        assert_eq!(checker.get_type_flags(TypeId::VOID.0), tf::VOID);
        assert_eq!(checker.get_type_flags(TypeId::UNDEFINED.0), tf::UNDEFINED);
        assert_eq!(checker.get_type_flags(TypeId::NULL.0), tf::NULL);
        assert_eq!(checker.get_type_flags(TypeId::NEVER.0), tf::NEVER);
        assert_eq!(checker.get_type_flags(TypeId::BIGINT.0), tf::BIG_INT);
        assert_eq!(checker.get_type_flags(TypeId::SYMBOL.0), tf::ES_SYMBOL);
        // `object` (lowercase) is NonPrimitive in tsserver, distinct from `Object`.
        assert_eq!(checker.get_type_flags(TypeId::OBJECT.0), tf::NON_PRIMITIVE);
    }

    #[test]
    fn get_type_flags_for_boolean_literal_carries_both_bits() {
        let (checker, _) = checker();
        let true_flags = checker.get_type_flags(TypeId::BOOLEAN_TRUE.0);
        let false_flags = checker.get_type_flags(TypeId::BOOLEAN_FALSE.0);
        assert_eq!(true_flags, tf::BOOLEAN_LITERAL | tf::BOOLEAN);
        assert_eq!(false_flags, tf::BOOLEAN_LITERAL | tf::BOOLEAN);
    }

    #[test]
    fn get_type_flags_maps_literal_types_to_literal_bits() {
        let (checker, db) = checker();
        let str_lit = db.literal_string("hello");
        let num_lit = db.literal_number(42.0);
        let bigint_lit = db.literal_bigint("123");
        assert_eq!(checker.get_type_flags(str_lit.0), tf::STRING_LITERAL);
        assert_eq!(checker.get_type_flags(num_lit.0), tf::NUMBER_LITERAL);
        assert_eq!(checker.get_type_flags(bigint_lit.0), tf::BIG_INT_LITERAL);
    }

    #[test]
    fn get_type_flags_maps_structural_types_to_structural_bits() {
        let (checker, db) = checker();
        let union = db.union2(TypeId::STRING, TypeId::NUMBER);
        let array = db.array(TypeId::STRING);
        let tuple = db.tuple(vec![tsz_solver::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        let tparam = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let keyof = db.keyof(TypeId::STRING);
        let index_access = db.index_access(TypeId::STRING, TypeId::NUMBER);

        assert_eq!(checker.get_type_flags(union.0), tf::UNION);
        assert_eq!(checker.get_type_flags(array.0), tf::OBJECT);
        assert_eq!(checker.get_type_flags(tuple.0), tf::OBJECT);
        assert_eq!(checker.get_type_flags(tparam.0), tf::TYPE_PARAMETER);
        assert_eq!(checker.get_type_flags(keyof.0), tf::INDEX);
        assert_eq!(checker.get_type_flags(index_access.0), tf::INDEXED_ACCESS);
    }

    #[test]
    fn get_type_flags_unknown_handle_returns_zero() {
        let (checker, _) = checker();
        // A handle past the intrinsic block that hasn't been interned
        // returns zero rather than panicking.
        assert_eq!(checker.get_type_flags(u32::MAX), 0);
    }
}
