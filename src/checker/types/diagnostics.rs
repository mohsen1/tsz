//! Diagnostic codes and message templates for the type checker.
//!
//! Message templates match TypeScript's diagnosticMessages.json exactly.

use serde::Serialize;

// =============================================================================
// Diagnostic Types
// =============================================================================

/// Diagnostic category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DiagnosticCategory {
    Warning = 0,
    Error = 1,
    Suggestion = 2,
    Message = 3,
}

/// Related information for a diagnostic (e.g., "see also" locations).
#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticRelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
}

/// A type-checking diagnostic message with optional related information.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    /// Related information spans (e.g., where a type was declared)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(file: String, start: u32, length: u32, message: String, code: u32) -> Self {
        Diagnostic {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        }
    }

    /// Add related information to this diagnostic.
    pub fn with_related(mut self, file: String, start: u32, length: u32, message: String) -> Self {
        self.related_information.push(DiagnosticRelatedInformation {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Message,
            code: 0,
        });
        self
    }
}

/// Format a diagnostic message by replacing {0}, {1}, etc. with arguments.
pub fn format_message(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{}}}", i), arg);
    }
    result
}

/// Diagnostic message templates matching TypeScript exactly.
/// Use format_message() to fill in placeholders.
pub mod diagnostic_messages {
    // Basic type errors
    pub const TYPE_NOT_ASSIGNABLE: &str = "Type '{0}' is not assignable to type '{1}'.";
    pub const CANNOT_FIND_NAME: &str = "Cannot find name '{0}'.";
    pub const DUPLICATE_IDENTIFIER: &str = "Duplicate identifier '{0}'.";
    pub const MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS: &str =
        "Multiple constructor implementations are not allowed.";
    pub const PROPERTY_DOES_NOT_EXIST: &str = "Property '{0}' does not exist on type '{1}'.";
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE: &str = "Type '{0}' is not an array type.";
    pub const TYPE_IS_NOT_AN_ARRAY_OR_STRING: &str =
        "Type '{0}' is not an array type or a string type.";
    pub const PROPERTY_MISSING: &str = "Property '{0}' is missing in type '{1}'.";
    pub const PROPERTY_MISSING_BUT_REQUIRED: &str =
        "Property '{0}' is missing in type '{1}' but required in type '{2}'.";
    pub const TYPES_OF_PROPERTY_INCOMPATIBLE: &str = "Types of property '{0}' are incompatible.";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE: &str =
        "'{0}' only refers to a type, but is being used as a value here.";
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB: &str = "'{0}' only refers to a type, but is being used as a value here. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.";
    pub const ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE: &str =
        "'{0}' refers to a value, but is being used as a type here. Did you mean 'typeof {0}'?";
    pub const LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS: &str =
        "Left side of comma operator is unused and has no side effects.";

    // Arithmetic operator errors
    pub const LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER: &str = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
    pub const RIGHT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER: &str = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.";
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPES: &str =
        "Operator '{0}' cannot be applied to types '{1}' and '{2}'.";

    // Function/call errors
    pub const EXPECTED_ARGUMENTS: &str = "Expected {0} arguments, but got {1}.";
    pub const EXPECTED_AT_LEAST_ARGUMENTS: &str = "Expected at least {0} arguments, but got {1}.";
    pub const ARGUMENT_NOT_ASSIGNABLE: &str =
        "Argument of type '{0}' is not assignable to parameter of type '{1}'.";
    pub const CANNOT_INVOKE_EXPRESSION_LACKING_CALL_SIGNATURE: &str = "Cannot invoke an expression whose type lacks a call signature. Type '{0}' has no compatible call signatures.";
    pub const CANNOT_INVOKE_EXPRESSION: &str = "This expression is not callable.";
    pub const NO_OVERLOAD_MATCHES: &str = "No overload matches this call.";
    pub const OVERLOAD_SIGNATURE: &str = "Overload {0} of {1}, '{2}', gave the following error.";

    // Object literal errors
    pub const EXCESS_PROPERTY: &str =
        "Object literal may only specify known properties, and '{0}' does not exist in type '{1}'.";
    pub const OBJECT_LITERAL_DUPLICATE_PROPERTY: &str =
        "An object literal cannot have multiple properties with the same name '{0}'.";

    // Null/undefined errors
    pub const OBJECT_POSSIBLY_UNDEFINED: &str = "Object is possibly 'undefined'.";
    pub const OBJECT_POSSIBLY_NULL: &str = "Object is possibly 'null'.";
    pub const OBJECT_POSSIBLY_NULL_OR_UNDEFINED: &str = "Object is possibly 'null' or 'undefined'.";
    pub const OBJECT_IS_OF_TYPE_UNKNOWN: &str = "Object is of type 'unknown'.";

    // Class errors
    pub const CLASS_INCORRECTLY_IMPLEMENTS: &str =
        "Class '{0}' incorrectly implements interface '{1}'.";
    pub const CLASS_INCORRECTLY_EXTENDS: &str = "Class '{0}' incorrectly extends base class '{1}'.";
    pub const TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE: &str =
        "Type '{0}' is not a constructor function type.";
    pub const PROPERTY_HAS_NO_INITIALIZER: &str =
        "Property '{0}' has no initializer and is not definitely assigned in the constructor.";
    pub const PROPERTY_USED_BEFORE_BEING_ASSIGNED: &str =
        "Property '{0}' is used before being assigned in the constructor.";
    pub const CANNOT_ASSIGN_READONLY: &str =
        "Cannot assign to '{0}' because it is a read-only property.";
    pub const CANNOT_ASSIGN_PRIVATE_METHOD: &str =
        "Cannot assign to private method '{0}'. Private methods are not writable.";
    pub const MEMBER_NOT_ACCESSIBLE: &str =
        "Property '{0}' is {1} and only accessible within class '{2}'.";
    pub const PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT: &str =
        "Private identifiers are not allowed in ambient contexts.";
    pub const THIS_IMPLICITLY_HAS_TYPE_ANY: &str =
        "'this' implicitly has type 'any' because it does not have a type annotation.";

    // Super keyword errors
    /// TS2335: 'super' can only be referenced in a derived class.
    pub const SUPER_ONLY_IN_DERIVED_CLASS: &str =
        "'super' can only be referenced in a derived class.";
    /// TS2336: 'super' property access is permitted only in a constructor, member function, or member accessor of a derived class.
    pub const SUPER_PROPERTY_ACCESS_INVALID_CONTEXT: &str = "'super' property access is permitted only in a constructor, member function, or member accessor of a derived class.";
    /// TS2337: Super calls are not permitted outside constructors or in nested functions inside constructors.
    pub const SUPER_CALL_NOT_IN_CONSTRUCTOR: &str = "Super calls are not permitted outside constructors or in nested functions inside constructors.";
    /// TS2376: A 'super' call must be the first statement in the constructor to refer to 'super' or 'this' when a derived class contains initialized properties, parameter properties, or private identifiers.
    pub const SUPER_MUST_BE_CALLED_BEFORE_THIS: &str = "A 'super' call must be the first statement in the constructor to refer to 'super' or 'this' when a derived class contains initialized properties, parameter properties, or private identifiers.";
    /// TS17011: 'super' cannot be referenced in a static property initializer.
    pub const SUPER_IN_STATIC_PROPERTY_INITIALIZER: &str =
        "'super' cannot be referenced in a static property initializer.";

    // Interface errors
    pub const INTERFACE_INCORRECTLY_EXTENDS: &str =
        "Interface '{0}' incorrectly extends interface '{1}'.";
    pub const TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF: &str =
        "Type alias '{0}' circularly references itself.";

    // Enum errors
    pub const ENUM_MEMBER_MUST_HAVE_INITIALIZER: &str = "Enum member must have initializer.";
    pub const CONST_ENUM_MEMBER_INITIALIZER: &str =
        "In 'const' enum declarations member initializer must be constant expression.";

    // Variable errors
    pub const CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE: &str =
        "Cannot redeclare block-scoped variable '{0}'.";
    pub const VARIABLE_USED_BEFORE_ASSIGNED: &str = "Variable '{0}' is used before being assigned.";

    // Definite assignment errors
    pub const PROPERTY_NO_INITIALIZER_NO_DEFINITE_ASSIGNMENT: &str =
        "Property '{0}' has no initializer and is not definitely assigned in the constructor.";

    // Switch exhaustiveness / control flow
    pub const NOT_EXHAUSTIVE: &str = "Not all code paths return a value.";
    pub const NOT_ALL_CODE_PATHS_RETURN: &str = "Not all code paths return a value.";
    pub const SWITCH_NOT_EXHAUSTIVE: &str =
        "Switch is not exhaustive. Did you forget to handle '{0}'?";
    pub const FUNCTION_LACKS_ENDING_RETURN_STATEMENT: &str =
        "Function lacks ending return statement and return type does not include 'undefined'.";
    pub const ASYNC_FUNCTION_RETURNS_PROMISE: &str = "Async function return type must be Promise.";
    pub const ASYNC_FUNCTION_REQUIRES_PROMISE_CONSTRUCTOR: &str = "An async function or method in ES5/ES3 requires the 'Promise' constructor. \
         Make sure you have a declaration for the 'Promise' constructor or include 'ES2015' in your `--lib` option.";
    pub const ASYNC_FUNCTION_MUST_RETURN_PROMISE: &str = "An async function or method must return a 'Promise'. \
         Make sure you have a declaration for 'Promise' or include 'ES2015' in your `--lib` option.";
    pub const UNREACHABLE_CODE_DETECTED: &str = "Unreachable code detected.";

    // Generic/type parameter errors
    pub const TYPE_NOT_SATISFY_CONSTRAINT: &str =
        "Type '{0}' does not satisfy the constraint '{1}'.";
    pub const GENERIC_TYPE_REQUIRES_ARGS: &str =
        "Generic type '{0}' requires {1} type argument(s).";
    pub const TYPE_IS_NOT_GENERIC: &str = "Type '{0}' is not generic.";
    pub const TYPE_INSTANTIATION_EXCESSIVELY_DEEP: &str =
        "Type instantiation is excessively deep and possibly infinite.";

    // Module/ambient errors
    pub const AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME: &str =
        "Ambient module declaration cannot specify relative module name.";
    pub const MODULE_HAS_NO_EXPORTED_MEMBER: &str = "Module '{0}' has no exported member '{1}'.";
    pub const CANNOT_FIND_MODULE: &str =
        "Cannot find module '{0}' or its corresponding type declarations.";
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION: &str =
        "Invalid module name in augmentation, module '{0}' cannot be found.";

    // Implicit any errors
    pub const VARIABLE_IMPLICIT_ANY: &str = "Variable '{0}' implicitly has an '{1}' type.";
    pub const PARAMETER_IMPLICIT_ANY: &str = "Parameter '{0}' implicitly has an '{1}' type.";
    pub const MEMBER_IMPLICIT_ANY: &str = "Member '{0}' implicitly has an '{1}' type.";
    pub const IMPLICIT_ANY_RETURN: &str =
        "'{0}', which lacks return-type annotation, implicitly has an '{1}' return type.";
    pub const IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION: &str = "Function expression, which lacks return-type annotation, implicitly has an '{0}' return type.";
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN: &str = "Cannot find name '{0}'. Did you mean '{1}'?";
    /// TS2583: Cannot find name - suggest changing target library
    pub const CANNOT_FIND_NAME_CHANGE_LIB: &str = "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.";
    pub const AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION: &str =
        "An 'await' expression is only allowed within an async function.";
    pub const AWAIT_IN_PARAMETER_DEFAULT: &str =
        "'await' expressions cannot be used in a parameter default value.";

    // Parameter ordering errors
    pub const REQUIRED_PARAMETER_AFTER_OPTIONAL: &str =
        "A required parameter cannot follow an optional parameter.";

    // Scanner/parser errors
    pub const NUMERIC_SEPARATORS_NOT_ALLOWED_HERE: &str =
        "Numeric separators are not allowed here.";
    pub const MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED: &str =
        "Multiple consecutive numeric separators are not permitted.";

    // Iterator/iterable errors
    /// TS2488: Type must have Symbol.iterator
    pub const TYPE_MUST_HAVE_SYMBOL_ITERATOR: &str =
        "Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.";
    /// TS2504: Type must have Symbol.asyncIterator
    pub const TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR: &str =
        "Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.";
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's diagnosticMessages.json
pub mod diagnostic_codes {
    // =========================================================================
    // Scanner/Parser errors (1xxx)
    // =========================================================================
    pub const UNTERMINATED_STRING_LITERAL: u32 = 1002;
    pub const UNTERMINATED_TEMPLATE_LITERAL: u32 = 1160;
    pub const IDENTIFIER_EXPECTED: u32 = 1003;
    pub const TOKEN_EXPECTED: u32 = 1005; // '{0}' expected.
    pub const TRAILING_COMMA_NOT_ALLOWED: u32 = 1009;
    pub const UNEXPECTED_TOKEN: u32 = 1012;
    pub const REST_PARAMETER_MUST_BE_LAST: u32 = 1014;
    pub const PARAMETER_CANNOT_HAVE_INITIALIZER: u32 = 1015;
    pub const REQUIRED_PARAMETER_AFTER_OPTIONAL: u32 = 1016; // A required parameter cannot follow an optional parameter.
    pub const ASYNC_MODIFIER_IN_AMBIENT_CONTEXT: u32 = 1040; // 'async' modifier cannot be used in an ambient context.
    pub const ASYNC_MODIFIER_CANNOT_BE_USED_HERE: u32 = 1042; // 'async' modifier cannot be used here.
    pub const SETTER_MUST_HAVE_EXACTLY_ONE_PARAMETER: u32 = 1049;
    pub const SETTER_PARAMETER_CANNOT_HAVE_INITIALIZER: u32 = 1052; // A 'set' accessor parameter cannot have an initializer.
    pub const SETTER_CANNOT_HAVE_REST_PARAMETER: u32 = 1053; // A 'set' accessor cannot have rest parameter.
    pub const GETTER_MUST_NOT_HAVE_PARAMETERS: u32 = 1054;
    pub const ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS: u32 = 1094;
    pub const SETTER_CANNOT_HAVE_RETURN_TYPE: u32 = 1095;
    pub const TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY: u32 = 1098;
    pub const EXPRESSION_EXPECTED: u32 = 1109;
    pub const TYPE_EXPECTED: u32 = 1110;
    pub const OBJECT_LITERAL_DUPLICATE_PROPERTY: u32 = 1117; // An object literal cannot have multiple properties with the same name.
    pub const DECLARATION_EXPECTED: u32 = 1146;
    pub const LINE_BREAK_NOT_PERMITTED_HERE: u32 = 1142; // Line break not permitted here.
    pub const EXTENDS_CLAUSE_ALREADY_SEEN: u32 = 1172;
    pub const EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE: u32 = 1173;
    pub const CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS: u32 = 1174;
    pub const IMPLEMENTS_CLAUSE_ALREADY_SEEN: u32 = 1175;
    pub const VARIABLE_DECLARATION_EXPECTED: u32 = 1134;
    pub const PROPERTY_OR_SIGNATURE_EXPECTED: u32 = 1131;
    pub const ENUM_MEMBER_EXPECTED: u32 = 1132;
    pub const STATEMENT_EXPECTED: u32 = 1129;
    pub const CATCH_OR_FINALLY_EXPECTED: u32 = 1472;
    pub const DECORATORS_NOT_VALID_HERE: u32 = 1206;
    pub const IMPLEMENTATION_CANNOT_BE_IN_AMBIENT_CONTEXT: u32 = 1183; // An implementation cannot be declared in ambient contexts.
    pub const MODIFIERS_NOT_ALLOWED_HERE: u32 = 1184;
    pub const CONST_MODIFIER_CANNOT_APPEAR_ON_A_CLASS_ELEMENT: u32 = 1248; // 'const' modifier cannot appear on a class element.
    pub const ABSTRACT_ONLY_IN_ABSTRACT_CLASS: u32 = 1253; // 'abstract' modifier can only appear on a class, method, or property declaration.
    pub const UNEXPECTED_TOKEN_CLASS_MEMBER: u32 = 1068; // Unexpected token. A constructor, method, accessor, or property was expected.
    pub const DECLARATION_OR_STATEMENT_EXPECTED: u32 = 1128; // Declaration or statement expected.
    pub const VAR_DECLARATION_NOT_ALLOWED: u32 = 1440; // Variable declaration not allowed at this location.
    pub const NUMERIC_SEPARATORS_NOT_ALLOWED_HERE: u32 = 6188;
    pub const MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED: u32 = 6189;

    // =========================================================================
    // Type checking errors (2xxx)
    // =========================================================================

    // Basic type errors
    pub const DUPLICATE_IDENTIFIER: u32 = 2300;
    pub const CANNOT_FIND_NAME: u32 = 2304;
    pub const MODULE_HAS_NO_EXPORTED_MEMBER: u32 = 2305;
    pub const GENERIC_TYPE_REQUIRES_TYPE_ARGUMENTS: u32 = 2314;
    pub const TYPE_IS_NOT_GENERIC: u32 = 2315;
    pub const TYPE_NOT_ASSIGNABLE_TO_TYPE: u32 = 2322;
    pub const PROPERTY_MISSING_IN_TYPE: u32 = 2324;
    pub const TYPES_OF_PROPERTY_INCOMPATIBLE: u32 = 2326;
    pub const PROPERTY_DOES_NOT_EXIST_ON_TYPE: u32 = 2339;
    pub const TYPE_HAS_NO_PROPERTY: u32 = 2339;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE: u32 = 2693;
    pub const ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB: u32 = 2585;
    pub const LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS: u32 = 2695;
    pub const ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE: u32 = 2749;

    // Delete expression errors
    pub const DELETE_OPERAND_MUST_BE_OPTIONAL: u32 = 2703; // The operand of a 'delete' operation must be optional.

    // Import/Export errors
    pub const IMPORT_ASSIGNMENT_CANNOT_BE_USED_WITH_ESM: u32 = 1202; // Import assignment cannot be used when targeting ECMAScript modules.

    // Function/call errors
    pub const ARGUMENT_NOT_ASSIGNABLE_TO_PARAMETER: u32 = 2345;
    pub const CANNOT_INVOKE_EXPRESSION_WHOSE_TYPE_LACKS_CALL_SIGNATURE: u32 = 2348;
    pub const CANNOT_INVOKE_NON_FUNCTION: u32 = 2349;
    pub const CANNOT_INVOKE_POSSIBLY_UNDEFINED: u32 = 2722;
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN: u32 = 2552; // Cannot find name '{0}'. Did you mean '{1}'?
    /// TS2583: Cannot find name '{0}'. Do you need to change your target library?
    /// Emitted when an ES2015+ global is referenced but the lib doesn't include it.
    pub const CANNOT_FIND_NAME_CHANGE_LIB: u32 = 2583;
    pub const EXPECTED_ARGUMENTS: u32 = 2554; // Expected {0} arguments, but got {1}
    pub const EXPECTED_AT_LEAST_ARGUMENTS: u32 = 2555;
    pub const NO_OVERLOAD_MATCHES_CALL: u32 = 2769;
    pub const FUNCTION_IMPLEMENTATION_NAME_MUST_BE: u32 = 2389; // Function implementation name must be '{0}'
    pub const CONSTRUCTOR_IMPLEMENTATION_MISSING: u32 = 2390; // Constructor implementation is missing
    pub const FUNCTION_IMPLEMENTATION_MISSING: u32 = 2391; // Function implementation is missing
    pub const MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS: u32 = 2392; // Multiple constructor implementations are not allowed
    pub const NOT_ALL_CODE_PATHS_RETURN_VALUE: u32 = 2366;
    pub const LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER: u32 = 2362; // The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
    pub const RIGHT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER: u32 = 2363; // The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
    pub const OPERATOR_CANNOT_BE_APPLIED_TO_TYPES: u32 = 2365; // Operator '{0}' cannot be applied to types '{1}' and '{2}'.
    pub const FUNCTION_LACKS_RETURN_TYPE: u32 = 2355;
    pub const FUNCTION_RETURN_TYPE_MISMATCH: u32 = 2322;
    pub const ASYNC_FUNCTION_RETURNS_PROMISE: u32 = 2705; // Async function must return Promise
    pub const PARAMETER_PROPERTY_NOT_ALLOWED: u32 = 2369; // A parameter property is only allowed in a constructor implementation.

    // Variable declaration errors
    pub const SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_SAME_TYPE: u32 = 2403; // Subsequent variable declarations must have the same type
    pub const CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE: u32 = 2451; // Cannot redeclare block-scoped variable '{0}'.
    pub const VARIABLE_USED_BEFORE_ASSIGNED: u32 = 2454; // Variable '{0}' is used before being assigned.

    // Null/undefined errors
    pub const OBJECT_IS_POSSIBLY_UNDEFINED: u32 = 2532;
    pub const OBJECT_IS_POSSIBLY_NULL: u32 = 2531;
    pub const OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED: u32 = 2533;
    pub const OBJECT_IS_OF_TYPE_UNKNOWN: u32 = 2571;
    pub const CANNOT_READ_PROPERTY_OF_UNDEFINED: u32 = 2532;

    // Class errors
    pub const CLASS_NAME_CANNOT_BE_ANY: u32 = 2414; // Class name cannot be 'any'.
    pub const CANNOT_CREATE_INSTANCE_OF_ABSTRACT_CLASS: u32 = 2511; // Cannot create an instance of an abstract class.
    pub const CANNOT_FIND_NAME_DID_YOU_MEAN_STATIC: u32 = 2662; // Cannot find name 'X'. Did you mean the static member 'C.X'?
    pub const ABSTRACT_PROPERTY_IN_CONSTRUCTOR: u32 = 2715; // Abstract property 'X' in class 'C' cannot be accessed in the constructor.
    pub const PROPERTY_USED_BEFORE_INITIALIZATION: u32 = 2729; // Property '{0}' is used before its initialization.
    pub const SUPER_ONLY_IN_DERIVED_CLASS: u32 = 2335;
    /// TS2336: 'super' property access is permitted only in a constructor, member function, or member accessor of a derived class.
    pub const SUPER_PROPERTY_ACCESS_INVALID_CONTEXT: u32 = 2336;
    /// TS2337: Super calls are not permitted outside constructors or in nested functions inside constructors.
    pub const SUPER_CALL_NOT_IN_CONSTRUCTOR: u32 = 2337;
    /// TS2376: A 'super' call must be the first statement in the constructor...
    pub const SUPER_MUST_BE_CALLED_BEFORE_THIS: u32 = 2376;
    /// TS17011: 'super' cannot be referenced in a static property initializer.
    pub const SUPER_IN_STATIC_PROPERTY_INITIALIZER: u32 = 17011;
    pub const THIS_CANNOT_BE_REFERENCED: u32 = 2332;
    pub const THIS_IMPLICITLY_HAS_TYPE_ANY: u32 = 2683; // 'this' implicitly has type 'any' because it does not have a type annotation.
    pub const PROPERTY_HAS_NO_INITIALIZER_AND_NOT_DEFINITELY_ASSIGNED: u32 = 2564;
    pub const PROPERTY_HAS_NO_INITIALIZER: u32 = 2564;
    pub const PROPERTY_USED_BEFORE_BEING_ASSIGNED: u32 = 2565;
    pub const ABSTRACT_PROPERTY_IN_NON_ABSTRACT_CLASS: u32 = 2515;
    pub const ABSTRACT_MEMBER_IN_NON_ABSTRACT_CLASS: u32 = 2515; // Same code for methods
    pub const NON_ABSTRACT_CLASS_MISSING_IMPLEMENTATIONS: u32 = 2654; // Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2}.
    pub const CANNOT_ASSIGN_TO_READONLY_PROPERTY: u32 = 2540;
    pub const CANNOT_ASSIGN_TO_PRIVATE_METHOD: u32 = 2803; // Cannot assign to private method 'X'. Private methods are not writable.
    pub const ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NOT: u32 = 2676; // Accessors must both be abstract or non-abstract.
    pub const CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE: u32 = 2420;
    pub const CLASS_INCORRECTLY_EXTENDS_BASE_CLASS: u32 = 2415;
    pub const PROPERTY_NOT_ASSIGNABLE_TO_SAME_IN_BASE: u32 = 2416; // Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}'.
    pub const MEMBER_IS_NOT_ACCESSIBLE: u32 = 2341;
    pub const PROPERTY_IS_PRIVATE: u32 = 2341;
    pub const PROPERTY_IS_PROTECTED: u32 = 2445;
    pub const TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE: u32 = 2507; // Type 'X' is not a constructor function type.
    pub const CANNOT_EXTEND_SEALED_CLASS: u32 = 2509;
    pub const GET_ACCESSOR_MUST_RETURN_VALUE: u32 = 2378; // A 'get' accessor must return a value.
    pub const CONSTRUCTOR_CANNOT_HAVE_RETURN_TYPE: u32 = 2380;
    pub const STATIC_MEMBERS_CANNOT_REFERENCE_TYPE_PARAMETERS: u32 = 2302;
    pub const OVERRIDE_MEMBER_NOT_IN_BASE: u32 = 4114; // This member cannot have an 'override' modifier because it is not declared in the base class
    pub const OVERRIDE_MEMBER_REQUIRED: u32 = 4113; // This member must have an 'override' modifier because it overrides a member in the base class
    pub const PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT: u32 = 2819; // Private identifiers are not allowed in ambient contexts.

    // Interface/type errors
    pub const INTERFACE_NAME_CANNOT_BE: u32 = 2427; // Interface name cannot be '{0}'.
    pub const INTERFACE_CAN_ONLY_EXTEND_INTERFACE: u32 = 2422;
    pub const INTERFACE_INCORRECTLY_EXTENDS_INTERFACE: u32 = 2430; // Interface '{0}' incorrectly extends interface '{1}'.
    pub const TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF: u32 = 2456;
    pub const INTERFACE_DECLARES_CONFLICTING_MEMBER: u32 = 2320;

    // Object literal errors
    pub const OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES: u32 = 2353;
    pub const EXCESS_PROPERTY_CHECK: u32 = 2353;
    pub const PROPERTY_ASSIGNMENT_EXPECTED: u32 = 1136;
    pub const DUPLICATE_PROPERTY: u32 = 2300;

    // Destructuring errors
    pub const TYPE_IS_NOT_AN_ARRAY_TYPE: u32 = 2461; // Type '{0}' is not an array type.
    pub const TYPE_IS_NOT_AN_ARRAY_OR_DOES_NOT_HAVE_ITERATOR: u32 = 2548; // Type '{0}' is not an array type or does not have a '[Symbol.iterator]()' method that returns an iterator.

    // Index signature errors
    pub const INDEX_SIGNATURE_MISSING: u32 = 2329;
    pub const NO_INDEX_SIGNATURE: u32 = 7053;
    pub const INDEX_SIGNATURE_PARAMETER_MUST_BE_STRING_OR_NUMBER: u32 = 1023;
    pub const PROPERTY_ACCESS_FROM_INDEX_SIGNATURE: u32 = 4111; // Property comes from an index signature, so it must be accessed with ['prop']

    // Switch/control flow
    pub const SWITCH_NOT_EXHAUSTIVE: u32 = 2761;
    pub const FALLTHROUGH_CASE: u32 = 7029;
    pub const UNREACHABLE_CODE_DETECTED: u32 = 7027;

    // Module/import errors
    pub const CANNOT_FIND_MODULE_2307: u32 = 2307; // Classic: Cannot find module 'x'.
    pub const MODULE_NOT_FOUND: u32 = 2307;
    pub const CANNOT_FIND_MODULE: u32 = 2307; // Cannot find module '{0}' or its corresponding type declarations.
    pub const INVALID_MODULE_NAME_IN_AUGMENTATION: u32 = 2664; // Invalid module name in augmentation, module '{0}' cannot be found.
    pub const EXPORT_ASSIGNMENT_WITH_OTHER_EXPORTS: u32 = 2309; // An export assignment cannot be used in a module with other exported elements.
    pub const HAS_NO_DEFAULT_EXPORT: u32 = 2613;
    pub const EXPORT_ASSIGNMENT_CANNOT_BE_USED: u32 = 2714;
    pub const AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME: u32 = 5061;
    pub const AMBIENT_MODULES_CANNOT_BE_NESTED: u32 = 2435; // Ambient modules cannot be nested in other modules or namespaces.

    // Promise/async errors
    pub const AWAIT_OUTSIDE_ASYNC: u32 = 1308;
    pub const AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION: u32 = 1359;
    pub const TYPE_IS_NOT_A_PROMISE: u32 = 2345;
    pub const ASYNC_FUNCTION_MUST_RETURN_PROMISE: u32 = 2697; // An async function or method must return a 'Promise'.
    pub const VOID_NOT_AWAITED: u32 = 2801;
    pub const ASYNC_FUNCTION_WITHOUT_AWAIT: u32 = 80006;

    // Type parameter/generic errors
    pub const TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED: u32 = 2344;
    pub const TYPE_PARAMETER_CANNOT_HAVE_VARIANCE_MODIFIER: u32 = 2637;
    pub const CONSTRAINT_OF_TYPE_PARAMETER: u32 = 2313;
    pub const TYPE_INSTANTIATION_EXCESSIVELY_DEEP: u32 = 2589;

    // Definite assignment errors
    pub const PROPERTY_NO_INITIALIZER_NO_DEFINITE_ASSIGNMENT: u32 = 2564;

    // Parameter default value errors
    pub const AWAIT_IN_PARAMETER_DEFAULT: u32 = 2524; // 'await' expressions cannot be used in a parameter default value.
    pub const PARAMETER_CANNOT_REFERENCE_ITSELF: u32 = 2372; // Parameter '{0}' cannot reference itself.

    // Enum errors
    pub const ENUM_MEMBER_MUST_HAVE_INITIALIZER: u32 = 2432;
    pub const CONST_ENUM_MEMBER_MUST_BE_INITIALIZED: u32 = 2474;
    pub const COMPUTED_PROPERTY_NAME_IN_ENUM: u32 = 1164;

    // Spread/rest/iterator errors
    /// TS2488: Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.
    /// Used for for-of loops with non-iterable types and spread operations.
    pub const TYPE_MUST_HAVE_SYMBOL_ITERATOR: u32 = 2488;
    /// Alias for TS2488 for spread argument context
    pub const SPREAD_ARGUMENT_MUST_BE_ARRAY: u32 = 2488;
    /// TS2504: Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.
    /// Used for for-await-of loops with non-async-iterable types.
    pub const TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR: u32 = 2504;
    pub const REST_ELEMENT_MUST_BE_LAST: u32 = 2462;

    // JSX errors
    pub const JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG: u32 = 17002;
    pub const EXPECTED_CORRESPONDING_JSX_CLOSING_TAG: u32 = 17002;
    pub const JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION: u32 = 17000;

    // Decorator errors
    pub const DECORATOR_CAN_ONLY_DECORATE_CLASS_OR_CLASS_MEMBER: u32 = 1249;
    pub const DECORATOR_FUNCTION_RETURN_TYPE_NOT_COMPATIBLE: u32 = 1270;

    // Assertion errors
    pub const ASSERTION_FUNCTIONS_CAN_ONLY_BE_PRESENT_IN_VOID_RETURNING_FUNCTIONS: u32 = 1228;
    pub const TYPE_PREDICATE_MUST_BE_BOOLEAN: u32 = 1228;

    // Mapped type errors
    pub const MAPPED_TYPE_MODIFIER_CAN_ONLY_BE_USED: u32 = 1071;

    // Conditional type errors
    pub const INFER_CAN_ONLY_BE_USED_IN_EXTENDS_CLAUSE: u32 = 1338;

    // Reserved word errors
    pub const AWAIT_IDENTIFIER_ILLEGAL: u32 = 1359; // Identifier expected. 'await' is a reserved word that cannot be used here.

    // Target version errors (18xxx)
    pub const ACCESSOR_MODIFIER_ONLY_ES2015_PLUS: u32 = 18045; // Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher.

    // =========================================================================
    // Warning codes (4xxx - 6xxx)
    // =========================================================================
    pub const UNUSED_VARIABLE: u32 = 6133;
    pub const UNUSED_PARAMETER: u32 = 6133;
    pub const UNUSED_IMPORT: u32 = 6133;
    pub const IMPLICIT_ANY: u32 = 7005;
    pub const IMPLICIT_ANY_PARAMETER: u32 = 7006;
    pub const IMPLICIT_ANY_MEMBER: u32 = 7008;
    pub const IMPLICIT_ANY_RETURN: u32 = 7010;
    pub const IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION: u32 = 7011;
    pub const COULD_NOT_RESOLVE_TYPE: u32 = 7016;
    pub const NOT_ALL_CODE_PATHS_RETURN: u32 = 7030;
}
