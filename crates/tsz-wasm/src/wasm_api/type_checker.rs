//! TypeScript `TypeChecker` API
//!
//! Provides the `TsTypeChecker` struct which implements TypeScript's `TypeChecker` interface.

use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;

use tsz::lib_loader::LibFile;
use tsz::parallel::MergedProgram;
use tsz_solver::construction::TypeInterner;
use tsz_solver::ts_type_flags::{is_nullable_type, type_id_ts_flags};
use tsz_solver::{
    TypeFormatter, TypeId, is_array_type, is_intersection_type, is_tuple_type, is_type_parameter,
    is_union_type,
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

    /// Check if type is nullable (includes `null` or `undefined`).
    ///
    /// Mirrors `TypeChecker.isNullableType`: returns true for the `null` and
    /// `undefined` intrinsics directly, and for any union that contains them.
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

    /// Construct a checker from an interner alone. Test-only entrypoint that
    /// lets unit tests build user-defined types via the interner and exercise
    /// the predicate methods without standing up a full program.
    #[cfg(test)]
    pub(crate) fn from_interner_for_test(interner: Arc<TypeInterner>) -> Self {
        Self { interner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TupleElement;
    use tsz_solver::ts_type_flags::flags as type_flags;

    fn checker() -> (Arc<TypeInterner>, TsTypeChecker) {
        let interner = Arc::new(TypeInterner::new());
        let checker = TsTypeChecker::from_interner_for_test(Arc::clone(&interner));
        (interner, checker)
    }

    fn tuple_elem(type_id: TypeId) -> TupleElement {
        TupleElement {
            type_id,
            name: None,
            optional: false,
            rest: false,
        }
    }

    fn make_type_param(interner: &TypeInterner, name: &str) -> tsz_solver::TypeParamInfo {
        tsz_solver::TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
        }
    }

    #[test]
    fn predicates_match_type_data_for_unions_and_intersections() {
        let (interner, checker) = checker();

        let union_id = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(checker.is_union_type(union_id.0));
        assert!(!checker.is_intersection_type(union_id.0));
        assert!(!checker.is_array_type(union_id.0));
        assert!(!checker.is_tuple_type(union_id.0));
        assert!(!checker.is_type_parameter(union_id.0));

        // `intersect_types_raw2` skips most normalization, but it still
        // collapses pairs that the solver knows are vacuous (e.g., disjoint
        // primitives -> never). Two type-parameter references survive raw
        // intersection unchanged, so we use those.
        let tp_a = interner.type_param(make_type_param(&interner, "A"));
        let tp_b = interner.type_param(make_type_param(&interner, "B"));
        let raw_intersection = interner.intersect_types_raw2(tp_a, tp_b);
        assert!(checker.is_intersection_type(raw_intersection.0));
        assert!(!checker.is_union_type(raw_intersection.0));
    }

    #[test]
    fn predicates_recognize_arrays_and_tuples() {
        let (interner, checker) = checker();

        let array_id = interner.array(TypeId::NUMBER);
        assert!(checker.is_array_type(array_id.0));
        assert!(!checker.is_tuple_type(array_id.0));
        assert!(!checker.is_union_type(array_id.0));

        let tuple_id = interner.tuple(vec![tuple_elem(TypeId::STRING), tuple_elem(TypeId::NUMBER)]);
        assert!(checker.is_tuple_type(tuple_id.0));
        assert!(!checker.is_array_type(tuple_id.0));
    }

    #[test]
    fn intrinsic_predicates_return_false() {
        let (_interner, checker) = checker();

        for &intrinsic in &[
            TypeId::ANY,
            TypeId::UNKNOWN,
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            TypeId::VOID,
            TypeId::NEVER,
        ] {
            assert!(!checker.is_union_type(intrinsic.0));
            assert!(!checker.is_intersection_type(intrinsic.0));
            assert!(!checker.is_array_type(intrinsic.0));
            assert!(!checker.is_tuple_type(intrinsic.0));
            assert!(!checker.is_type_parameter(intrinsic.0));
        }
    }

    #[test]
    fn is_nullable_type_covers_unions_with_null_or_undefined() {
        let (interner, checker) = checker();

        assert!(checker.is_nullable_type(TypeId::NULL.0));
        assert!(checker.is_nullable_type(TypeId::UNDEFINED.0));
        assert!(!checker.is_nullable_type(TypeId::STRING.0));

        let nullable_string = interner.union2(TypeId::STRING, TypeId::NULL);
        assert!(checker.is_nullable_type(nullable_string.0));

        let optional_string = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        assert!(checker.is_nullable_type(optional_string.0));

        let plain_union = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(!checker.is_nullable_type(plain_union.0));
    }

    #[test]
    fn type_flags_match_typescript_constants() {
        let (interner, checker) = checker();

        assert_eq!(checker.get_type_flags(TypeId::ANY.0), type_flags::ANY);
        assert_eq!(
            checker.get_type_flags(TypeId::UNKNOWN.0),
            type_flags::UNKNOWN
        );
        assert_eq!(checker.get_type_flags(TypeId::STRING.0), type_flags::STRING);
        assert_eq!(checker.get_type_flags(TypeId::NUMBER.0), type_flags::NUMBER);
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN.0),
            type_flags::BOOLEAN
        );
        assert_eq!(
            checker.get_type_flags(TypeId::BIGINT.0),
            type_flags::BIG_INT
        );
        assert_eq!(
            checker.get_type_flags(TypeId::SYMBOL.0),
            type_flags::ES_SYMBOL
        );
        assert_eq!(checker.get_type_flags(TypeId::VOID.0), type_flags::VOID);
        assert_eq!(
            checker.get_type_flags(TypeId::UNDEFINED.0),
            type_flags::UNDEFINED
        );
        assert_eq!(checker.get_type_flags(TypeId::NULL.0), type_flags::NULL);
        assert_eq!(checker.get_type_flags(TypeId::NEVER.0), type_flags::NEVER);
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN_TRUE.0),
            type_flags::BOOLEAN_LITERAL | type_flags::BOOLEAN
        );
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN_FALSE.0),
            type_flags::BOOLEAN_LITERAL | type_flags::BOOLEAN
        );

        let str_lit = interner.literal_string("hello");
        assert_eq!(
            checker.get_type_flags(str_lit.0),
            type_flags::STRING_LITERAL
        );

        let num_lit = interner.literal_number(42.0);
        assert_eq!(
            checker.get_type_flags(num_lit.0),
            type_flags::NUMBER_LITERAL
        );

        let union_id = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(checker.get_type_flags(union_id.0), type_flags::UNION);

        let array_id = interner.array(TypeId::NUMBER);
        assert_eq!(checker.get_type_flags(array_id.0), type_flags::OBJECT);

        let tuple_id = interner.tuple(vec![tuple_elem(TypeId::NUMBER)]);
        assert_eq!(checker.get_type_flags(tuple_id.0), type_flags::OBJECT);
    }
}
